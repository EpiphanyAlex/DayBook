use std::{
    path::Component,
    sync::{
        atomic::{AtomicBool, AtomicU8, Ordering},
        Arc, Mutex,
    },
    time::Instant,
};

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use time::{
    format_description::well_known::Rfc3339, macros::format_description, Date, OffsetDateTime,
};
use uuid::Uuid;

use crate::{
    agent::types::{
        CompleteSourceArgs, DraftTransactionArgs, ReadSourceArgs, ReportSourceTotalArgs,
    },
    db::Database,
    error::{AppError, AppResult},
    money::{currency_exponent, validate_triple, MAX_ABS_VALUE},
};

#[derive(Debug, Clone)]
pub struct Assignment {
    pub source_id: String,
    pub attempt_id: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToolContent {
    Text { text: String },
    Image { data: String, mime_type: String },
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolOutput {
    pub content: Vec<ToolContent>,
    pub structured_content: Option<Value>,
}

impl ToolOutput {
    fn json(value: Value) -> Self {
        Self {
            content: vec![ToolContent::Text {
                text: value.to_string(),
            }],
            structured_content: Some(value),
        }
    }
}

#[derive(Debug)]
pub struct DraftStore {
    database: Arc<Database>,
    assignment: Option<Assignment>,
    closed: AtomicBool,
    completion_rejections: AtomicU8,
    total_marker_rejections: AtomicU8,
    trace_events: Mutex<Vec<Value>>,
    debug_events: Mutex<Vec<Value>>,
}

impl DraftStore {
    pub fn for_probe(database: Arc<Database>) -> Self {
        Self {
            database,
            assignment: None,
            closed: AtomicBool::new(false),
            completion_rejections: AtomicU8::new(0),
            total_marker_rejections: AtomicU8::new(0),
            trace_events: Mutex::new(Vec::new()),
            debug_events: Mutex::new(Vec::new()),
        }
    }

    pub fn for_task(database: Arc<Database>, assignment: Assignment) -> Self {
        Self {
            database,
            assignment: Some(assignment),
            closed: AtomicBool::new(false),
            completion_rejections: AtomicU8::new(0),
            total_marker_rejections: AtomicU8::new(0),
            trace_events: Mutex::new(Vec::new()),
            debug_events: Mutex::new(Vec::new()),
        }
    }

    pub fn handle(&self, tool: &str, arguments: Value) -> AppResult<ToolOutput> {
        let started = Instant::now();
        let debug_arguments = arguments.clone();
        let argument_shape = json_shape(&arguments);
        let result = (|| {
            if self.closed.load(Ordering::SeqCst) {
                return Err(tool_rejected("本次来源已经完成，工具面已关闭"));
            }
            match tool {
                "list_pending_sources" => self.list_pending_sources(),
                "read_source" => self.read_source(parse(arguments)?),
                "draft_transaction" => self.draft_transaction(parse(arguments)?),
                "report_source_total" => self.report_source_total(parse(arguments)?),
                "complete_source" => self.complete_source(parse(arguments)?),
                _ => Err(tool_rejected("工具不在 M0 注册表中")),
            }
        })();
        self.record_tool_call(
            tool,
            argument_shape,
            debug_arguments,
            started.elapsed().as_millis(),
            &result,
        );
        result
    }

    pub fn is_complete(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }

    pub fn trace_events(&self) -> Vec<Value> {
        self.trace_events
            .lock()
            .map(|events| events.clone())
            .unwrap_or_default()
    }

    pub fn debug_events(&self) -> Vec<Value> {
        self.debug_events
            .lock()
            .map(|events| events.clone())
            .unwrap_or_default()
    }

    fn record_tool_call(
        &self,
        tool: &str,
        argument_shape: Value,
        arguments: Value,
        duration_ms: u128,
        result: &AppResult<ToolOutput>,
    ) {
        let (ok, error_code) = match result {
            Ok(_) => (true, None),
            Err(error) => (false, Some(error.code.as_str())),
        };
        if let Ok(mut events) = self.trace_events.lock() {
            events.push(json!({
                "kind": "tool_call",
                "tool": tool,
                "argumentShape": argument_shape,
                "durationMs": duration_ms,
                "ok": ok,
                "errorCode": error_code,
            }));
        }
        if let Ok(mut events) = self.debug_events.lock() {
            events.push(json!({
                "kind": "tool_call",
                "tool": tool,
                "arguments": arguments,
                "ok": ok,
                "errorCode": error_code,
            }));
        }
    }

    fn assignment(&self) -> AppResult<&Assignment> {
        self.assignment
            .as_ref()
            .ok_or_else(|| tool_rejected("能力探测会话没有来源指派"))
    }

    fn ensure_assigned(&self, source_id: &str) -> AppResult<&Assignment> {
        let assignment = self.assignment()?;
        if assignment.source_id != source_id {
            return Err(tool_rejected("source_id 不在本次任务的指派范围内"));
        }
        Ok(assignment)
    }

    fn list_pending_sources(&self) -> AppResult<ToolOutput> {
        let Some(assignment) = &self.assignment else {
            return Ok(ToolOutput::json(json!({ "sources": [] })));
        };
        let source = self.database.read(|connection| {
            connection
                .query_row(
                    "SELECT id, kind, original_filename, ext, byte_size, imported_at
                 FROM sources WHERE id = ?1",
                    [&assignment.source_id],
                    |row| {
                        Ok(json!({
                            "sourceId": row.get::<_, String>(0)?,
                            "kind": row.get::<_, String>(1)?,
                            "originalFilename": row.get::<_, Option<String>>(2)?,
                            "ext": row.get::<_, String>(3)?,
                            "byteSize": row.get::<_, i64>(4)?,
                            "importedAt": row.get::<_, String>(5)?,
                        }))
                    },
                )
                .optional()
        })?;
        Ok(ToolOutput::json(
            json!({ "sources": source.into_iter().collect::<Vec<_>>() }),
        ))
    }

    fn read_source(&self, args: ReadSourceArgs) -> AppResult<ToolOutput> {
        self.ensure_assigned(&args.source_id)?;
        let (kind, ext, relative_path) = self.database.read(|connection| {
            connection.query_row(
                "SELECT kind, ext, evidence_relpath FROM sources WHERE id = ?1",
                [&args.source_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
        })?;
        let relative = std::path::Path::new(&relative_path);
        if relative.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        }) {
            return Err(AppError::storage("证据路径越出数据目录"));
        }
        let bytes = std::fs::read(self.database.root().join(relative))?;
        let mut metadata = json!({ "sourceId": args.source_id, "kind": kind, "ext": ext });
        let mut content = vec![ToolContent::Text {
            text: metadata.to_string(),
        }];
        if kind == "utterance" {
            let text = String::from_utf8(bytes)
                .map_err(|error| AppError::storage(format!("口述证据不是 UTF-8：{error}")))?;
            metadata["text"] = Value::String(text.clone());
            content.push(ToolContent::Text { text });
        } else {
            let mime_type = match ext.as_str() {
                "png" => "image/png",
                "jpg" | "jpeg" => "image/jpeg",
                _ => return Err(tool_rejected("当前来源不是 M0 支持的图片格式")),
            };
            content.push(ToolContent::Image {
                data: BASE64.encode(bytes),
                mime_type: mime_type.to_owned(),
            });
        }
        Ok(ToolOutput {
            content,
            structured_content: Some(metadata),
        })
    }

    fn draft_transaction(&self, args: DraftTransactionArgs) -> AppResult<ToolOutput> {
        let assignment = self.ensure_assigned(&args.source_id)?.clone();
        validate_draft_args(&self.database, &args)?;
        let amount_minor = parse_decimal(&args.amount_minor)?;
        let triple = match (
            args.base_amount_minor.as_deref(),
            args.base_currency.as_deref(),
            args.rate_ppm.as_deref(),
        ) {
            (None, None, None) => None,
            (Some(base_amount), Some(base_currency), Some(rate)) => {
                currency_exponent(base_currency).map_err(as_tool_rejection)?;
                let base_amount = parse_decimal(base_amount)?;
                let rate = parse_decimal(rate)?;
                validate_triple(
                    amount_minor,
                    &args.currency,
                    base_amount,
                    base_currency,
                    rate,
                )
                .map_err(as_tool_rejection)?;
                Some((base_amount, base_currency.to_owned(), rate))
            }
            _ => return Err(tool_rejected("本位币金额、币种与汇率必须全填或全空")),
        };
        let id = Uuid::new_v4().to_string();
        let timestamp = now()?;
        let drafted_json = serde_json::to_string(&args)
            .map_err(|error| AppError::storage(format!("起草快照序列化失败：{error}")))?;
        let (span_start, span_end) = args
            .evidence_span
            .as_ref()
            .map_or((None, None), |span| (Some(span.start), Some(span.end)));
        let (base_amount, base_currency, rate) = triple
            .as_ref()
            .map_or((None, None, None), |(amount, currency, rate)| {
                (Some(*amount), Some(currency.as_str()), Some(*rate))
            });
        let audit_id = Uuid::new_v4().to_string();
        let after_json = json!({ "draftId": id, "attemptId": assignment.attempt_id }).to_string();

        self.database.write(|transaction| {
            transaction
                .execute(
                    "INSERT INTO draft_transactions (
                        id, source_id, evidence_text, source_ordinal,
                        evidence_span_start, evidence_span_end, attempt_id, drafted_json,
                        occurred_on, amount_minor, currency, base_amount_minor, base_currency,
                        rate_ppm, direction, merchant, category, channel, confidence, created_at
                     ) VALUES (
                        ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10,
                        ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20
                     )",
                    params![
                        id,
                        args.source_id,
                        args.evidence_text,
                        args.source_ordinal,
                        span_start,
                        span_end,
                        assignment.attempt_id,
                        drafted_json,
                        args.occurred_on,
                        amount_minor,
                        args.currency,
                        base_amount,
                        base_currency,
                        rate,
                        args.direction,
                        args.merchant,
                        args.category,
                        args.channel,
                        args.confidence,
                        timestamp,
                    ],
                )
                .map_err(|error| tool_rejected(format!("草稿参数被数据层拒绝：{error}")))?;
            let session_id: String = transaction.query_row(
                "SELECT agent_session_id FROM parse_attempts WHERE id = ?1",
                [&assignment.attempt_id],
                |row| row.get(0),
            )?;
            transaction.execute(
                "INSERT INTO audit_log (
                    id, actor, at, entity_type, entity_id, action, after_json, agent_session_id
                 ) VALUES (?1, 'agent', ?2, 'draft_transaction', ?3, 'create', ?4, ?5)",
                params![audit_id, timestamp, id, after_json, session_id],
            )?;
            Ok(())
        })?;
        Ok(ToolOutput::json(json!({ "draftId": id })))
    }

    fn report_source_total(&self, args: ReportSourceTotalArgs) -> AppResult<ToolOutput> {
        let assignment = self.ensure_assigned(&args.source_id)?.clone();
        if args.evidence_text.trim().is_empty() {
            return Err(tool_rejected("合计必须附来源上的原文片段"));
        }
        currency_exponent(&args.currency).map_err(as_tool_rejection)?;
        if !matches!(
            args.kind.as_str(),
            "expense_total" | "income_total" | "net_change"
        ) {
            return Err(tool_rejected("合计类型不在允许集合中"));
        }
        let amount = parse_decimal(&args.amount_minor)?;
        let timestamp = now()?;
        let audit_id = Uuid::new_v4().to_string();
        let after_json = serde_json::to_string(&args)
            .map_err(|error| AppError::storage(format!("合计审计序列化失败：{error}")))?;
        self.database.write(|transaction| {
            let updated = transaction.execute(
                "UPDATE parse_attempts SET
                    reported_total_minor = ?1, reported_total_currency = ?2,
                    reported_total_kind = ?3, reported_total_evidence_text = ?4
                 WHERE id = ?5 AND source_id = ?6 AND reported_total_minor IS NULL",
                params![
                    amount,
                    args.currency,
                    args.kind,
                    args.evidence_text,
                    assignment.attempt_id,
                    assignment.source_id,
                ],
            )?;
            if updated != 1 {
                return Err(tool_rejected("本次尝试已经成功报告过合计"));
            }
            let session_id: String = transaction.query_row(
                "SELECT agent_session_id FROM parse_attempts WHERE id = ?1",
                [&assignment.attempt_id],
                |row| row.get(0),
            )?;
            transaction.execute(
                "INSERT INTO audit_log (
                    id, actor, at, entity_type, entity_id, action, after_json, agent_session_id
                 ) VALUES (?1, 'agent', ?2, 'parse_attempt', ?3, 'update', ?4, ?5)",
                params![
                    audit_id,
                    timestamp,
                    assignment.attempt_id,
                    after_json,
                    session_id
                ],
            )?;
            Ok(())
        })?;
        Ok(ToolOutput::json(json!({ "reported": true })))
    }

    fn complete_source(&self, args: CompleteSourceArgs) -> AppResult<ToolOutput> {
        let assignment = self.ensure_assigned(&args.source_id)?.clone();
        if args.item_count < 0 {
            return Err(tool_rejected("item_count 不能为负数"));
        }
        // 合计词闸门（01 §3.2 可信性要求第 6 条）。词表只认字面量，认不出「一共去了三个地方」
        // 这类非金额用法——所以 `unparsed_note` 是它的出口：说明了就放行，产出
        // `completed_with_gaps`，审核界面显眼呈现那段说明。它挡的是**静默**漏报，写了说明就不静默。
        if args.unparsed_note.trim().is_empty()
            && self.explicit_utterance_total_is_unreported(&assignment)?
        {
            // 这条此前不计数、可无限拒绝：agent 找不到合计时只能挂到 180 秒硬超时。
            let rejection = self.total_marker_rejections.fetch_add(1, Ordering::SeqCst) + 1;
            if rejection >= 2 {
                self.fail_protocol(&assignment)?;
                return Err(AppError::new(
                    "agent.protocol_violation",
                    "口述含明显合计词，两次完成尝试都既未回报合计也未说明原因",
                ));
            }
            return Err(tool_rejected(
                "口述原文含明显合计词。若它确实是金额合计，先调用 report_source_total 再重试完成；\
                 若不是（例如「一共去了三个地方」），在 unparsed_note 里说明后重试完成。",
            ));
        }
        let (ids, ordinals) = self.database.read(|connection| {
            let mut statement = connection.prepare(
                "SELECT id, source_ordinal FROM draft_transactions
                 WHERE attempt_id = ?1 AND voided_at IS NULL ORDER BY source_ordinal",
            )?;
            let rows = statement.query_map([&assignment.attempt_id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })?;
            let values = rows.collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(values.into_iter().unzip::<_, _, Vec<_>, Vec<_>>())
        })?;
        let count_matches = usize::try_from(args.item_count).ok() == Some(ids.len());
        let has_gap = ordinals
            .iter()
            .enumerate()
            .any(|(index, ordinal)| *ordinal != i64::try_from(index + 1).unwrap_or(i64::MAX));
        let gap_explained = !has_gap || !args.unparsed_note.trim().is_empty();
        if !count_matches || !gap_explained {
            let rejection = self.completion_rejections.fetch_add(1, Ordering::SeqCst) + 1;
            if rejection >= 2 {
                self.fail_protocol(&assignment)?;
                return Err(AppError::new(
                    "agent.protocol_violation",
                    "完成协议连续两次不一致，本次解析已失败",
                ));
            }
            if !count_matches {
                return Err(AppError::new(
                    "agent.completion_mismatch",
                    "agent 自报条目数与已写入草稿数不一致",
                )
                .with_detail(json!({
                    "reportedCount": args.item_count,
                    "actualCount": ids.len(),
                    "draftIds": ids,
                })));
            }
            return Err(AppError::new(
                "agent.unexplained_gap",
                "来源序号存在跳号，但未说明未解析区域",
            ));
        }
        self.database.write(|transaction| {
            let updated = transaction.execute(
                "UPDATE parse_attempts SET reported_item_count = ?1, unparsed_note = ?2
                 WHERE id = ?3 AND source_id = ?4 AND reported_item_count IS NULL",
                params![
                    args.item_count,
                    args.unparsed_note,
                    assignment.attempt_id,
                    assignment.source_id
                ],
            )?;
            if updated != 1 {
                return Err(tool_rejected("complete_source 已成功调用过"));
            }
            Ok(())
        })?;
        self.closed.store(true, Ordering::SeqCst);
        let outcome = if args.unparsed_note.is_empty() {
            "completed"
        } else {
            "completed_with_gaps"
        };
        Ok(ToolOutput::json(json!({ "outcome": outcome })))
    }

    fn explicit_utterance_total_is_unreported(&self, assignment: &Assignment) -> AppResult<bool> {
        let (kind, evidence_relpath, reported_total): (String, String, Option<i64>) =
            self.database.read(|connection| {
                connection.query_row(
                    "SELECT s.kind, s.evidence_relpath, p.reported_total_minor
                     FROM sources s JOIN parse_attempts p ON p.source_id = s.id
                     WHERE s.id = ?1 AND p.id = ?2",
                    params![assignment.source_id, assignment.attempt_id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
            })?;
        if kind != "utterance" || reported_total.is_some() {
            return Ok(false);
        }
        let relative = std::path::Path::new(&evidence_relpath);
        if relative.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        }) {
            return Err(AppError::storage("证据路径越出数据目录"));
        }
        let text = std::fs::read_to_string(self.database.root().join(relative))?;
        Ok(has_explicit_total_marker(&text))
    }

    fn fail_protocol(&self, assignment: &Assignment) -> AppResult<()> {
        void_attempt(
            &self.database,
            assignment,
            "protocol_violation",
            "agent.protocol_violation",
        )?;
        self.closed.store(true, Ordering::SeqCst);
        Ok(())
    }
}

fn has_explicit_total_marker(text: &str) -> bool {
    ["总共", "一共", "合计", "总计"]
        .iter()
        .any(|marker| text.contains(marker))
        || text
            .split(|character: char| !character.is_ascii_alphanumeric())
            .any(|token| token.eq_ignore_ascii_case("total"))
}

fn json_shape(value: &Value) -> Value {
    match value {
        Value::Null => json!("null"),
        Value::Bool(_) => json!("boolean"),
        Value::Number(_) => json!("number"),
        Value::String(_) => json!("string"),
        Value::Array(items) => json!({
            "type": "array",
            "items": items.first().map(json_shape).unwrap_or_else(|| json!("unknown")),
        }),
        Value::Object(entries) => Value::Object(
            entries
                .iter()
                .map(|(key, value)| (key.clone(), json_shape(value)))
                .collect(),
        ),
    }
}

pub fn void_attempt(
    database: &Database,
    assignment: &Assignment,
    outcome: &str,
    error_code: &str,
) -> AppResult<()> {
    let timestamp = now()?;
    database.write(|transaction| {
        transaction.execute(
            "UPDATE parse_attempts SET ended_at = ?1, outcome = ?2, error_code = ?3
             WHERE id = ?4 AND ended_at IS NULL",
            params![timestamp, outcome, error_code, assignment.attempt_id],
        )?;
        // `failed → failed` 是合法的幂等重入：工具面先 fail_protocol 一次，
        // runtime 收到协议失败后还会兜底再作废一次（见 02 §3.4）。
        crate::ingest::apply_transition(
            transaction,
            &assignment.source_id,
            crate::ingest::SourceState::Failed,
            Some(error_code),
        )?;
        let mut statement = transaction.prepare(
            "SELECT id FROM draft_transactions WHERE attempt_id = ?1 AND voided_at IS NULL",
        )?;
        let ids = statement
            .query_map([&assignment.attempt_id], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        drop(statement);
        for draft_id in ids {
            transaction.execute(
                "UPDATE draft_transactions SET voided_at = ?1 WHERE id = ?2",
                params![timestamp, draft_id],
            )?;
            transaction.execute(
                "INSERT INTO audit_log (
                    id, actor, at, entity_type, entity_id, action, before_json, after_json
                 ) VALUES (?1, 'system', ?2, 'draft_transaction', ?3, 'void', ?4, ?5)",
                params![
                    Uuid::new_v4().to_string(),
                    timestamp,
                    draft_id,
                    json!({ "voidedAt": Value::Null }).to_string(),
                    json!({ "voidedAt": timestamp }).to_string(),
                ],
            )?;
        }
        Ok(())
    })
}

fn validate_draft_args(database: &Database, args: &DraftTransactionArgs) -> AppResult<()> {
    if args.evidence_text.trim().is_empty() || args.merchant.trim().is_empty() {
        return Err(tool_rejected("草稿必须带 evidence_text 与 merchant"));
    }
    if args.source_ordinal <= 0 {
        return Err(tool_rejected("source_ordinal 必须从 1 开始"));
    }
    if !matches!(args.direction.as_str(), "expense" | "income" | "transfer") {
        return Err(tool_rejected("direction 不在允许集合中"));
    }
    if args
        .confidence
        .is_some_and(|value| !(0..=100).contains(&value))
    {
        return Err(tool_rejected("confidence 必须在 0–100 之间"));
    }
    Date::parse(
        &args.occurred_on,
        format_description!("[year]-[month]-[day]"),
    )
    .map_err(|_| tool_rejected("occurred_on 必须是有效的 YYYY-MM-DD 日期"))?;
    currency_exponent(&args.currency).map_err(as_tool_rejection)?;
    let (kind, path) = database.read(|connection| {
        connection.query_row(
            "SELECT kind, evidence_relpath FROM sources WHERE id = ?1",
            [&args.source_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
    })?;
    match (kind.as_str(), &args.evidence_span) {
        ("file", None) => {}
        ("file", Some(_)) => return Err(tool_rejected("图片来源不接受 evidence_span")),
        ("utterance", Some(span)) => {
            let text = std::fs::read_to_string(database.root().join(path))?;
            let start = usize::try_from(span.start)
                .map_err(|_| tool_rejected("evidence_span.start 不能为负数"))?;
            let end = usize::try_from(span.end)
                .map_err(|_| tool_rejected("evidence_span.end 不能为负数"))?;
            if start >= end {
                return Err(tool_rejected("evidence_span 必须满足 start < end"));
            }
            let points = text.chars().collect::<Vec<_>>();
            if end > points.len() {
                return Err(tool_rejected("evidence_span 超出转写文本"));
            }
            let excerpt = points[start..end].iter().collect::<String>();
            if excerpt != args.evidence_text {
                return Err(tool_rejected(
                    "evidence_span 对应原文与 evidence_text 不一致",
                ));
            }
        }
        ("utterance", None) => return Err(tool_rejected("口述来源必须带 evidence_span")),
        _ => return Err(tool_rejected("未知来源类型")),
    }
    Ok(())
}

fn parse<T: serde::de::DeserializeOwned>(value: Value) -> AppResult<T> {
    serde_json::from_value(value)
        .map_err(|error| tool_rejected(format!("工具参数形状不合法：{error}")))
}

fn parse_decimal(raw: &str) -> AppResult<i64> {
    if raw.is_empty()
        || raw.starts_with('+')
        || (raw.starts_with('0') && raw.len() > 1)
        || (raw.starts_with("-0") && raw.len() > 2)
        || !raw
            .strip_prefix('-')
            .unwrap_or(raw)
            .chars()
            .all(|character| character.is_ascii_digit())
    {
        return Err(tool_rejected("金额必须是规范十进制整数字符串"));
    }
    let value = raw
        .parse::<i64>()
        .map_err(|_| tool_rejected("金额超出 i64 范围"))?;
    if value.unsigned_abs() > MAX_ABS_VALUE as u64 {
        return Err(as_tool_rejection(AppError::new(
            "data.amount_out_of_range",
            "金额或汇率超出支持范围",
        )));
    }
    Ok(value)
}

fn now() -> AppResult<String> {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .map_err(|error| AppError::storage(format!("时间格式化失败：{error}")))
}

fn tool_rejected(message: impl Into<String>) -> AppError {
    AppError::new("agent.tool_rejected", message)
}

fn as_tool_rejection(error: AppError) -> AppError {
    AppError::new("agent.tool_rejected", error.message)
        .with_detail(json!({ "causeCode": error.code }))
}

#[cfg(test)]
mod agent {
    use tempfile::tempdir;

    use super::*;

    struct Fixture {
        _directory: tempfile::TempDir,
        database: Arc<Database>,
        store: DraftStore,
        source_id: String,
        attempt_id: String,
        text: String,
    }

    impl Fixture {
        fn new(kind: &str, text: &str) -> Self {
            let directory = tempdir().unwrap();
            let database = Arc::new(Database::open(directory.path()).unwrap());
            let source_id = Uuid::new_v4().to_string();
            let attempt_id = Uuid::new_v4().to_string();
            let relative = if kind == "utterance" {
                format!("evidence/{source_id}.txt")
            } else {
                format!("evidence/{source_id}.png")
            };
            std::fs::write(database.root().join(&relative), text.as_bytes()).unwrap();
            database
                .write(|transaction| {
                    if kind == "utterance" {
                        transaction.execute(
                            "INSERT INTO sources (
                                id, kind, content_hash, idempotency_key, ext, byte_size,
                                evidence_relpath, imported_at, state
                             ) VALUES (?1, 'utterance', ?2, ?3, 'txt', ?4, ?5,
                                '2026-08-13T00:00:00Z', 'parsing')",
                            params![
                                source_id,
                                "a".repeat(64),
                                format!("token-{source_id}"),
                                text.len(),
                                relative
                            ],
                        )?;
                    } else {
                        transaction.execute(
                            "INSERT INTO sources (
                                id, kind, content_hash, original_filename, ext, byte_size,
                                evidence_relpath, imported_at, state
                             ) VALUES (?1, 'file', ?2, 'source.png', 'png', ?3, ?4,
                                '2026-08-13T00:00:00Z', 'parsing')",
                            params![source_id, "a".repeat(64), text.len(), relative],
                        )?;
                    }
                    transaction.execute(
                        "INSERT INTO parse_attempts (
                            id, source_id, agent_session_id, backend_id, prompt_hash,
                            tool_surface_version, effective_capability_hash, app_version, started_at
                         ) VALUES (?1, ?2, 'session', 'test', ?3, 'm0-test', ?3,
                            '0.1.0', '2026-08-13T00:00:00Z')",
                        params![attempt_id, source_id, "b".repeat(64)],
                    )?;
                    transaction.execute(
                        "UPDATE sources SET latest_attempt_id = ?1 WHERE id = ?2",
                        params![attempt_id, source_id],
                    )?;
                    Ok(())
                })
                .unwrap();
            let store = DraftStore::for_task(
                Arc::clone(&database),
                Assignment {
                    source_id: source_id.clone(),
                    attempt_id: attempt_id.clone(),
                },
            );
            Self {
                _directory: directory,
                database,
                store,
                source_id,
                attempt_id,
                text: text.to_owned(),
            }
        }

        fn draft(&self, ordinal: i64, evidence_text: &str, span: Option<(i64, i64)>) -> Value {
            json!({
                "sourceId": self.source_id,
                "evidenceText": evidence_text,
                "sourceOrdinal": ordinal,
                "evidenceSpan": span.map(|(start, end)| json!({ "start": start, "end": end })),
                "occurredOn": "2026-08-13",
                "amountMinor": "100",
                "currency": "AUD",
                "baseAmountMinor": "100",
                "baseCurrency": "AUD",
                "ratePpm": "1000000",
                "direction": "expense",
                "merchant": "Merchant",
                "category": null,
                "channel": null,
                "confidence": 90
            })
        }

        fn complete(&self, count: i64, note: &str) -> AppResult<ToolOutput> {
            self.store.handle(
                "complete_source",
                json!({
                    "sourceId": self.source_id,
                    "itemCount": count,
                    "unparsedNote": note,
                }),
            )
        }
    }

    #[test]
    fn read_source_rejects_unassigned() {
        let fixture = Fixture::new("file", "png");
        assert_eq!(
            fixture
                .store
                .handle("read_source", json!({ "sourceId": "another-source" }))
                .unwrap_err()
                .code,
            "agent.tool_rejected"
        );
    }

    #[test]
    fn read_utterance_exposes_text_as_structured_content() {
        let fixture = Fixture::new("utterance", "咖啡 5 元");
        let output = fixture
            .store
            .handle("read_source", json!({ "sourceId": fixture.source_id }))
            .unwrap();
        assert_eq!(output.structured_content.unwrap()["text"], "咖啡 5 元");
    }

    #[test]
    fn draft_requires_evidence_args() {
        let fixture = Fixture::new("file", "png");
        let mut missing_source = fixture.draft(1, "evidence", None);
        missing_source.as_object_mut().unwrap().remove("sourceId");
        assert_eq!(
            fixture
                .store
                .handle("draft_transaction", missing_source)
                .unwrap_err()
                .code,
            "agent.tool_rejected"
        );
        assert!(fixture
            .store
            .handle("draft_transaction", fixture.draft(1, "", None))
            .is_err());
        let count: i64 = fixture
            .database
            .read(|connection| {
                connection.query_row("SELECT COUNT(*) FROM draft_transactions", [], |row| {
                    row.get(0)
                })
            })
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn draft_requires_ordinal() {
        let fixture = Fixture::new("file", "png");
        let mut missing = fixture.draft(1, "first", None);
        missing.as_object_mut().unwrap().remove("sourceOrdinal");
        assert!(fixture.store.handle("draft_transaction", missing).is_err());
        fixture
            .store
            .handle("draft_transaction", fixture.draft(1, "first", None))
            .unwrap();
        assert!(fixture
            .store
            .handle("draft_transaction", fixture.draft(1, "duplicate", None))
            .is_err());
        fixture
            .store
            .handle("draft_transaction", fixture.draft(3, "gap allowed", None))
            .unwrap();
    }

    #[test]
    fn draft_span_required_only_for_utterance() {
        let utterance = Fixture::new("utterance", "咖啡 5 元");
        assert!(utterance
            .store
            .handle("draft_transaction", utterance.draft(1, "咖啡", None))
            .is_err());
        let file = Fixture::new("file", "png");
        assert!(file
            .store
            .handle("draft_transaction", file.draft(1, "png", Some((0, 1))))
            .is_err());
    }

    #[test]
    fn draft_span_bounds_are_checked() {
        let fixture = Fixture::new("utterance", "甲乙丙");
        for span in [(1, 1), (2, 1), (0, 4), (-1, 1)] {
            assert!(
                fixture
                    .store
                    .handle("draft_transaction", fixture.draft(1, "甲", Some(span)))
                    .is_err(),
                "{span:?}"
            );
        }
    }

    #[test]
    fn draft_span_must_match_text() {
        let fixture = Fixture::new("utterance", "A😀中B");
        assert_eq!(
            fixture.text.chars().skip(1).take(2).collect::<String>(),
            "😀中"
        );
        fixture
            .store
            .handle("draft_transaction", fixture.draft(1, "😀中", Some((1, 3))))
            .unwrap();
        assert!(fixture
            .store
            .handle("draft_transaction", fixture.draft(2, "😀", Some((1, 3))),)
            .is_err());
    }

    #[test]
    fn complete_source_is_terminal_only_on_success() {
        let success = Fixture::new("file", "png");
        success.complete(0, "").unwrap();
        assert!(success
            .store
            .handle("list_pending_sources", json!({}))
            .is_err());
        let rejected = Fixture::new("file", "png");
        assert_eq!(
            rejected.complete(1, "").unwrap_err().code,
            "agent.completion_mismatch"
        );
        rejected
            .store
            .handle("draft_transaction", rejected.draft(1, "later", None))
            .unwrap();
    }

    #[test]
    fn completion_mismatch_is_recoverable() {
        let fixture = Fixture::new("file", "png");
        fixture
            .store
            .handle("draft_transaction", fixture.draft(1, "first", None))
            .unwrap();
        let error = fixture.complete(2, "").unwrap_err();
        assert_eq!(error.code, "agent.completion_mismatch");
        assert_eq!(error.detail.unwrap()["actualCount"], 1);
        fixture
            .store
            .handle("draft_transaction", fixture.draft(2, "second", None))
            .unwrap();
        fixture.complete(2, "").unwrap();
        assert!(fixture.store.is_complete());
    }

    #[test]
    fn persistent_mismatch_is_protocol_violation() {
        let fixture = Fixture::new("file", "png");
        fixture
            .store
            .handle("draft_transaction", fixture.draft(1, "first", None))
            .unwrap();
        assert_eq!(
            fixture.complete(2, "").unwrap_err().code,
            "agent.completion_mismatch"
        );
        assert_eq!(
            fixture.complete(3, "").unwrap_err().code,
            "agent.protocol_violation"
        );
        let (outcome, state, active): (String, String, i64) = fixture
            .database
            .read(|connection| {
                connection.query_row(
                    "SELECT p.outcome, s.state,
                        (SELECT COUNT(*) FROM draft_transactions d
                         WHERE d.attempt_id = p.id AND d.voided_at IS NULL)
                     FROM parse_attempts p JOIN sources s ON s.id = p.source_id
                     WHERE p.id = ?1",
                    [&fixture.attempt_id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
            })
            .unwrap();
        assert_eq!(
            (outcome.as_str(), state.as_str(), active),
            ("protocol_violation", "failed", 0)
        );
    }

    #[test]
    fn gap_without_note_is_rejected() {
        let fixture = Fixture::new("file", "png");
        for ordinal in [1, 3] {
            fixture
                .store
                .handle(
                    "draft_transaction",
                    fixture.draft(ordinal, &format!("item {ordinal}"), None),
                )
                .unwrap();
        }
        assert_eq!(
            fixture.complete(2, "").unwrap_err().code,
            "agent.unexplained_gap"
        );
        let completed = fixture.complete(2, "第 2 行不是交易").unwrap();
        assert_eq!(
            completed.structured_content.unwrap()["outcome"],
            "completed_with_gaps"
        );
    }

    #[test]
    fn unparsed_note_yields_completed_with_gaps() {
        let fixture = Fixture::new("file", "png");
        fixture
            .store
            .handle("draft_transaction", fixture.draft(1, "first", None))
            .unwrap();
        let completed = fixture.complete(1, "底部一行模糊，未起草").unwrap();
        assert_eq!(
            completed.structured_content.unwrap()["outcome"],
            "completed_with_gaps"
        );
    }

    #[test]
    fn draft_rejects_unknown_currency() {
        let fixture = Fixture::new("file", "png");
        let mut args = fixture.draft(1, "evidence", None);
        args["currency"] = json!("ZZZ");
        args["baseCurrency"] = json!("ZZZ");
        assert_eq!(
            fixture
                .store
                .handle("draft_transaction", args)
                .unwrap_err()
                .code,
            "agent.tool_rejected"
        );
    }

    #[test]
    fn complete_source_writes_no_audit() {
        let fixture = Fixture::new("file", "png");
        fixture.complete(0, "").unwrap();
        let (audit, count): (i64, i64) = fixture
            .database
            .read(|connection| {
                connection.query_row(
                    "SELECT (SELECT COUNT(*) FROM audit_log), reported_item_count
                     FROM parse_attempts WHERE id = ?1",
                    [&fixture.attempt_id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
            })
            .unwrap();
        assert_eq!((audit, count), (0, 0));
    }

    #[test]
    fn report_total_requires_evidence_currency_and_kind() {
        for missing in ["currency", "kind", "evidenceText"] {
            let fixture = Fixture::new("file", "png");
            let mut args = json!({
                "sourceId": fixture.source_id,
                "amountMinor": "100",
                "currency": "AUD",
                "kind": "expense_total",
                "evidenceText": "Total 1.00"
            });
            args.as_object_mut().unwrap().remove(missing);
            assert!(fixture.store.handle("report_source_total", args).is_err());
            let total: Option<i64> = fixture
                .database
                .read(|connection| {
                    connection.query_row(
                        "SELECT reported_total_minor FROM parse_attempts WHERE id = ?1",
                        [&fixture.attempt_id],
                        |row| row.get(0),
                    )
                })
                .unwrap();
            assert_eq!(total, None);
        }
    }

    #[test]
    fn report_total_accepts_only_once_per_attempt() {
        let fixture = Fixture::new("file", "png");
        let args = json!({
            "sourceId": fixture.source_id,
            "amountMinor": "100",
            "currency": "AUD",
            "kind": "expense_total",
            "evidenceText": "Total 1.00"
        });
        fixture
            .store
            .handle("report_source_total", args.clone())
            .unwrap();
        assert!(fixture.store.handle("report_source_total", args).is_err());
        let amount: i64 = fixture
            .database
            .read(|connection| {
                connection.query_row(
                    "SELECT reported_total_minor FROM parse_attempts WHERE id = ?1",
                    [&fixture.attempt_id],
                    |row| row.get(0),
                )
            })
            .unwrap();
        assert_eq!(amount, 100);
    }

    #[test]
    fn explicit_utterance_total_must_be_reported() {
        let fixture = Fixture::new("utterance", "昨天咖啡 5 澳元，总共 5 澳元");
        let evidence = "昨天咖啡 5 澳元";
        fixture
            .store
            .handle(
                "draft_transaction",
                fixture.draft(1, evidence, Some((0, evidence.chars().count() as i64))),
            )
            .unwrap();

        let error = fixture.complete(1, "").unwrap_err();
        assert_eq!(error.code, "agent.tool_rejected");
        assert!(!fixture.store.is_complete());

        fixture
            .store
            .handle(
                "report_source_total",
                json!({
                    "sourceId": fixture.source_id,
                    "amountMinor": "100",
                    "currency": "AUD",
                    "kind": "expense_total",
                    "evidenceText": "总共 5 澳元"
                }),
            )
            .unwrap();
        fixture.complete(1, "").unwrap();
        assert!(fixture.store.is_complete());
    }

    #[test]
    fn explicit_total_marker_is_escapable_with_note() {
        // 「一共」说的是地点数量，不是金额合计——词表分辨不了，agent 必须有路可走。
        let fixture = Fixture::new("utterance", "昨天一共去了三个地方，咖啡 5 元");
        let evidence = "咖啡 5 元";
        assert_eq!(
            fixture.text.chars().skip(11).take(6).collect::<String>(),
            evidence
        );
        fixture
            .store
            .handle(
                "draft_transaction",
                fixture.draft(1, evidence, Some((11, 17))),
            )
            .unwrap();

        assert_eq!(
            fixture.complete(1, "").unwrap_err().code,
            "agent.tool_rejected"
        );
        assert!(!fixture.store.is_complete());

        let completed = fixture
            .complete(
                1,
                "原文的「一共」指地点数量，不是金额合计，来源没有可回报的合计",
            )
            .unwrap();
        assert!(fixture.store.is_complete());
        assert_eq!(
            completed.structured_content.unwrap()["outcome"],
            "completed_with_gaps"
        );
    }

    #[test]
    fn explicit_total_marker_gate_is_bounded() {
        // 此前这条不计数：agent 交不出合计也说不出理由时，只能一直被拒到 180 秒硬超时。
        let fixture = Fixture::new("utterance", "总共花了很多");
        assert_eq!(
            fixture.complete(0, "").unwrap_err().code,
            "agent.tool_rejected"
        );
        assert_eq!(
            fixture.complete(0, "").unwrap_err().code,
            "agent.protocol_violation"
        );
        let (outcome, state): (String, String) = fixture
            .database
            .read(|connection| {
                connection.query_row(
                    "SELECT p.outcome, s.state FROM parse_attempts p
                     JOIN sources s ON s.id = p.source_id WHERE p.id = ?1",
                    [&fixture.attempt_id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
            })
            .unwrap();
        assert_eq!(
            (outcome.as_str(), state.as_str()),
            ("protocol_violation", "failed")
        );
    }

    #[test]
    fn every_ledger_write_tool_writes_audit() {
        let fixture = Fixture::new("file", "png");
        fixture
            .store
            .handle("draft_transaction", fixture.draft(1, "evidence", None))
            .unwrap();
        fixture
            .store
            .handle(
                "report_source_total",
                json!({
                    "sourceId": fixture.source_id,
                    "amountMinor": "100",
                    "currency": "AUD",
                    "kind": "expense_total",
                    "evidenceText": "Total"
                }),
            )
            .unwrap();
        fixture.complete(1, "").unwrap();
        let actors = fixture
            .database
            .read(|connection| {
                connection
                    .prepare("SELECT actor FROM audit_log ORDER BY rowid")?
                    .query_map([], |row| row.get::<_, String>(0))?
                    .collect::<rusqlite::Result<Vec<_>>>()
            })
            .unwrap();
        assert_eq!(actors, vec!["agent", "agent"]);
    }

    #[test]
    fn void_marks_not_deletes_and_is_audited_as_system() {
        let fixture = Fixture::new("file", "png");
        fixture
            .store
            .handle("draft_transaction", fixture.draft(1, "evidence", None))
            .unwrap();
        void_attempt(
            &fixture.database,
            &Assignment {
                source_id: fixture.source_id.clone(),
                attempt_id: fixture.attempt_id.clone(),
            },
            "timeout",
            "agent.timeout",
        )
        .unwrap();
        let (rows, voided, json, agents, systems): (i64, i64, String, i64, i64) = fixture
            .database
            .read(|connection| {
                connection.query_row(
                    "SELECT COUNT(*), COUNT(voided_at), MAX(drafted_json),
                        (SELECT COUNT(*) FROM audit_log WHERE actor = 'agent'),
                        (SELECT COUNT(*) FROM audit_log WHERE actor = 'system' AND action = 'void')
                     FROM draft_transactions",
                    [],
                    |row| {
                        Ok((
                            row.get(0)?,
                            row.get(1)?,
                            row.get(2)?,
                            row.get(3)?,
                            row.get(4)?,
                        ))
                    },
                )
            })
            .unwrap();
        assert_eq!((rows, voided, agents, systems), (1, 1, 1, 1));
        assert!(json.contains("evidence"));
    }

    #[test]
    fn void_is_audited_as_system() {
        let fixture = Fixture::new("file", "png");
        fixture
            .store
            .handle("draft_transaction", fixture.draft(1, "evidence", None))
            .unwrap();
        void_attempt(
            &fixture.database,
            &Assignment {
                source_id: fixture.source_id.clone(),
                attempt_id: fixture.attempt_id.clone(),
            },
            "timeout",
            "agent.timeout",
        )
        .unwrap();
        let count: i64 = fixture
            .database
            .read(|connection| {
                connection.query_row(
                    "SELECT COUNT(*) FROM audit_log
                     WHERE actor = 'system' AND action = 'void'",
                    [],
                    |row| row.get(0),
                )
            })
            .unwrap();
        assert_eq!(count, 1);
    }
}
