use rusqlite::{params, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};
use serde_json::json;
use time::{
    format_description::well_known::Rfc3339, macros::format_description, Date, OffsetDateTime,
};
use uuid::Uuid;

use crate::{
    db::Database,
    error::{AppError, AppResult},
    money::{convert_minor, currency_exponent, ensure_range, validate_triple, DecimalI64},
};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TotalCheck {
    pub attempt_id: String,
    pub source_id: String,
    pub source_kind: String,
    pub reconciliation_status: String,
    pub confirmation_policy: String,
    pub reported_total_minor: Option<DecimalI64>,
    pub calculated_total_minor: Option<DecimalI64>,
    pub reported_total_currency: Option<String>,
    pub reported_total_kind: Option<String>,
    pub reported_total_evidence_text: Option<String>,
    pub unavailable_draft_ids: Vec<String>,
    pub outcome: Option<String>,
    pub unparsed_note: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmResult {
    pub draft_id: String,
    pub transaction_id: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct UtteranceAttestation {
    pub full_source_visible: bool,
    pub results_adjacent: bool,
    pub item_count_visible: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchConfirmResult {
    pub confirmed: Vec<ConfirmResult>,
    pub rejected: Vec<RejectedDraft>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RejectedDraft {
    pub draft_id: String,
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DraftPatch {
    pub occurred_on: Option<String>,
    pub amount_minor: Option<DecimalI64>,
    pub currency: Option<String>,
    // 没有 base_amount_minor：本位币金额是导出值，不是独立输入（03 §3.5）。
    pub base_currency: Option<String>,
    pub rate_ppm: Option<DecimalI64>,
    pub direction: Option<String>,
    pub merchant: Option<String>,
    pub category: Option<String>,
    pub channel: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewDraft {
    pub id: String,
    pub source_id: String,
    pub attempt_id: String,
    pub evidence_text: String,
    pub source_ordinal: i64,
    pub evidence_span_start: Option<i64>,
    pub evidence_span_end: Option<i64>,
    pub occurred_on: String,
    pub amount_minor: DecimalI64,
    pub currency: String,
    pub base_amount_minor: Option<DecimalI64>,
    pub base_currency: Option<String>,
    pub rate_ppm: Option<DecimalI64>,
    pub direction: String,
    pub merchant: String,
    pub category: Option<String>,
    pub channel: Option<String>,
    pub confidence: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewSource {
    pub id: String,
    pub kind: String,
    pub original_filename: Option<String>,
    pub ext: String,
    pub state: String,
    pub parse_error_code: Option<String>,
    pub latest_attempt_id: Option<String>,
    pub imported_at: String,
    pub active_draft_count: i64,
}

#[derive(Debug, Clone)]
struct ReconciliationDraft {
    id: String,
    amount_minor: i64,
    currency: String,
    base_amount_minor: Option<i64>,
    base_currency: Option<String>,
    direction: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DraftSnapshot {
    id: String,
    source_id: String,
    attempt_id: String,
    evidence_text: String,
    occurred_on: String,
    amount_minor: DecimalI64,
    currency: String,
    base_amount_minor: Option<DecimalI64>,
    base_currency: Option<String>,
    rate_ppm: Option<DecimalI64>,
    direction: String,
    merchant: String,
    category: Option<String>,
    channel: Option<String>,
    drafted_json: String,
}

pub fn total_check(database: &Database, attempt_id: &str) -> AppResult<TotalCheck> {
    let (
        source_id,
        source_kind,
        reported_minor,
        reported_currency,
        reported_kind,
        evidence,
        outcome,
        note,
    ) = database
        .read(|connection| {
            connection
                .query_row(
                    "SELECT a.source_id, s.kind, a.reported_total_minor,
                        a.reported_total_currency, a.reported_total_kind,
                        a.reported_total_evidence_text, a.outcome, a.unparsed_note
                     FROM parse_attempts a JOIN sources s ON s.id = a.source_id
                     WHERE a.id = ?1",
                    [attempt_id],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, Option<i64>>(2)?,
                            row.get::<_, Option<String>>(3)?,
                            row.get::<_, Option<String>>(4)?,
                            row.get::<_, Option<String>>(5)?,
                            row.get::<_, Option<String>>(6)?,
                            row.get::<_, Option<String>>(7)?,
                        ))
                    },
                )
                .optional()
        })?
        .ok_or_else(|| AppError::new("data.not_found", "解析尝试不存在"))?;

    let base = |status: &str, policy: &str| TotalCheck {
        attempt_id: attempt_id.to_owned(),
        source_id: source_id.clone(),
        source_kind: source_kind.clone(),
        reconciliation_status: status.to_owned(),
        confirmation_policy: policy.to_owned(),
        reported_total_minor: reported_minor.map(DecimalI64),
        calculated_total_minor: None,
        reported_total_currency: reported_currency.clone(),
        reported_total_kind: reported_kind.clone(),
        reported_total_evidence_text: evidence.clone(),
        unavailable_draft_ids: Vec::new(),
        outcome: outcome.clone(),
        unparsed_note: note.clone(),
    };
    let Some(reported_minor) = reported_minor else {
        return Ok(if source_kind == "utterance" {
            base("not_applicable", "user_attested_batch")
        } else {
            base("unavailable", "single_only")
        });
    };
    let reported_currency = reported_currency.as_deref().ok_or_else(|| {
        AppError::storage("reported_total_minor 存在但 reported_total_currency 缺失")
    })?;
    let reported_kind = reported_kind
        .as_deref()
        .ok_or_else(|| AppError::storage("reported_total_minor 存在但 reported_total_kind 缺失"))?;
    let drafts = reconciliation_drafts(database, attempt_id)?;
    let mut unavailable = Vec::new();
    let mut expense = 0_i128;
    let mut income = 0_i128;
    for draft in &drafts {
        if draft.direction == "transfer" {
            unavailable.push(draft.id.clone());
            continue;
        }
        let amount = if draft.currency == reported_currency {
            Some(draft.amount_minor)
        } else if draft.base_currency.as_deref() == Some(reported_currency) {
            draft.base_amount_minor
        } else {
            None
        };
        let Some(amount) = amount else {
            unavailable.push(draft.id.clone());
            continue;
        };
        match draft.direction.as_str() {
            "expense" => expense += i128::from(amount),
            "income" => income += i128::from(amount),
            _ => unavailable.push(draft.id.clone()),
        }
    }
    let policy_for = |status: &str| {
        if source_kind == "utterance" {
            "user_attested_batch"
        } else if status == "passed" {
            "reconciled_batch"
        } else {
            "single_only"
        }
    };
    if !unavailable.is_empty() {
        let mut result = base("unavailable", policy_for("unavailable"));
        result.unavailable_draft_ids = unavailable;
        return Ok(result);
    }
    let calculated = match reported_kind {
        "expense_total" => expense,
        "income_total" => income,
        "net_change" => income - expense,
        _ => return Err(AppError::storage("未知 reported_total_kind")),
    };
    let calculated = i64::try_from(calculated)
        .map_err(|_| AppError::new("data.amount_out_of_range", "草稿合计超出支持范围"))?;
    ensure_range(calculated)?;
    let status = if calculated == reported_minor {
        "passed"
    } else {
        "failed"
    };
    let mut result = base(status, policy_for(status));
    result.calculated_total_minor = Some(DecimalI64(calculated));
    Ok(result)
}

pub fn list_review_sources(database: &Database) -> AppResult<Vec<ReviewSource>> {
    database.read(|connection| {
        let mut statement = connection.prepare(
            "SELECT s.id, s.kind, s.original_filename, s.ext, s.state,
                s.parse_error_code, s.latest_attempt_id, s.imported_at,
                COUNT(d.id)
             FROM sources s
             LEFT JOIN draft_transactions d
               ON d.attempt_id = s.latest_attempt_id
              AND d.voided_at IS NULL AND d.discarded_at IS NULL AND d.consumed_at IS NULL
             GROUP BY s.id
             ORDER BY s.imported_at DESC, s.id",
        )?;
        let rows = statement
            .query_map([], |row| {
                Ok(ReviewSource {
                    id: row.get(0)?,
                    kind: row.get(1)?,
                    original_filename: row.get(2)?,
                    ext: row.get(3)?,
                    state: row.get(4)?,
                    parse_error_code: row.get(5)?,
                    latest_attempt_id: row.get(6)?,
                    imported_at: row.get(7)?,
                    active_draft_count: row.get(8)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    })
}

pub fn list_active_drafts(database: &Database, source_id: &str) -> AppResult<Vec<ReviewDraft>> {
    database.read(|connection| {
        let mut statement = connection.prepare(
            "SELECT d.id, d.source_id, d.attempt_id, d.evidence_text, d.source_ordinal,
                d.evidence_span_start, d.evidence_span_end, d.occurred_on,
                d.amount_minor, d.currency, d.base_amount_minor, d.base_currency,
                d.rate_ppm, d.direction, d.merchant, d.category, d.channel, d.confidence
             FROM draft_transactions d JOIN sources s ON s.latest_attempt_id = d.attempt_id
             WHERE s.id = ?1 AND d.voided_at IS NULL
               AND d.discarded_at IS NULL AND d.consumed_at IS NULL
             ORDER BY d.source_ordinal, d.id",
        )?;
        let rows = statement
            .query_map([source_id], |row| {
                Ok(ReviewDraft {
                    id: row.get(0)?,
                    source_id: row.get(1)?,
                    attempt_id: row.get(2)?,
                    evidence_text: row.get(3)?,
                    source_ordinal: row.get(4)?,
                    evidence_span_start: row.get(5)?,
                    evidence_span_end: row.get(6)?,
                    occurred_on: row.get(7)?,
                    amount_minor: DecimalI64(row.get(8)?),
                    currency: row.get(9)?,
                    base_amount_minor: row.get::<_, Option<i64>>(10)?.map(DecimalI64),
                    base_currency: row.get(11)?,
                    rate_ppm: row.get::<_, Option<i64>>(12)?.map(DecimalI64),
                    direction: row.get(13)?,
                    merchant: row.get(14)?,
                    category: row.get(15)?,
                    channel: row.get(16)?,
                    confidence: row.get(17)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    })
}

pub fn confirm_one(database: &Database, draft_id: &str) -> AppResult<ConfirmResult> {
    let timestamp = now()?;
    database.write(|transaction| confirm_in_transaction(transaction, draft_id, &timestamp))
}

pub fn confirm_batch(
    database: &Database,
    draft_ids: &[String],
    attestation: Option<&UtteranceAttestation>,
) -> AppResult<BatchConfirmResult> {
    if draft_ids.is_empty() {
        return Err(AppError::invalid_argument("至少选择一条草稿"));
    }
    let attempt_ids = database.read(|connection| {
        let mut statement = connection.prepare(
            "SELECT DISTINCT attempt_id FROM draft_transactions
             WHERE id IN (SELECT value FROM json_each(?1))",
        )?;
        let encoded =
            serde_json::to_string(draft_ids).map_err(|_| rusqlite::Error::InvalidQuery)?;
        let values = statement
            .query_map([encoded], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(values)
    })?;
    if attempt_ids.len() != 1 {
        return Err(AppError::invalid_argument("批量确认必须来自同一次解析尝试"));
    }
    let check = total_check(database, &attempt_ids[0])?;
    ensure_batch_allowed(&check)?;
    if check.confirmation_policy == "user_attested_batch"
        && !attestation.is_some_and(|value| {
            value.full_source_visible && value.results_adjacent && value.item_count_visible
        })
    {
        return Err(AppError::new(
            "review.attestation_required",
            "批量确认口述前，必须完整展示原文、拆分结果与条数",
        ));
    }
    let timestamp = now()?;
    database.write(|transaction| {
        let mut confirmed = Vec::new();
        let mut rejected = Vec::new();
        for draft_id in draft_ids {
            match confirm_in_transaction(transaction, draft_id, &timestamp) {
                Ok(result) => confirmed.push(result),
                Err(error)
                    if matches!(
                        error.code.as_str(),
                        "review.missing_evidence" | "review.incomplete_triple"
                    ) =>
                {
                    rejected.push(RejectedDraft {
                        draft_id: draft_id.clone(),
                        code: error.code,
                        message: error.message,
                    });
                }
                Err(error) => return Err(error),
            }
        }
        Ok(BatchConfirmResult {
            confirmed,
            rejected,
        })
    })
}

pub fn edit_draft(database: &Database, draft_id: &str, patch: &DraftPatch) -> AppResult<()> {
    let timestamp = now()?;
    database.write(|transaction| {
        let before = load_active_snapshot(transaction, draft_id)?;
        let occurred_on = patch.occurred_on.as_ref().unwrap_or(&before.occurred_on);
        Date::parse(occurred_on, format_description!("[year]-[month]-[day]"))
            .map_err(|_| AppError::invalid_argument("occurred_on 必须是有效的 YYYY-MM-DD 日期"))?;
        let amount = patch.amount_minor.unwrap_or(before.amount_minor).0;
        let currency = patch.currency.as_ref().unwrap_or(&before.currency);
        currency_exponent(currency)?;
        let base_currency = patch
            .base_currency
            .as_ref()
            .or(before.base_currency.as_ref());
        let rate = patch.rate_ppm.or(before.rate_ppm).map(|v| v.0);
        // 本位币金额是导出值，不是独立输入（03 §3.5）：`rate_ppm` 是来源上印着的那个数，
        // 用户纠正金额时它不变，所以本位币金额只能跟着重算。把它也当输入，就存在一个
        // 「三者互相矛盾」的可表达状态——只改金额时必然 data.money_inconsistent。
        let base_amount = match (base_currency, rate) {
            (Some(base_currency), Some(rate)) => {
                Some(convert_minor(amount, currency, base_currency, rate)?)
            }
            (None, None) => None,
            _ => {
                return Err(AppError::new(
                    "review.incomplete_triple",
                    "本位币币种与汇率必须同时填写或同时留空",
                ))
            }
        };
        let direction = patch.direction.as_ref().unwrap_or(&before.direction);
        if !matches!(direction.as_str(), "expense" | "income" | "transfer") {
            return Err(AppError::invalid_argument("direction 不在允许集合中"));
        }
        let merchant = patch.merchant.as_ref().unwrap_or(&before.merchant);
        if merchant.trim().is_empty() {
            return Err(AppError::invalid_argument("商户不能为空"));
        }
        let category = patch.category.as_ref().or(before.category.as_ref());
        let channel = patch.channel.as_ref().or(before.channel.as_ref());
        transaction.execute(
            "UPDATE draft_transactions SET occurred_on = ?1, amount_minor = ?2,
                currency = ?3, base_amount_minor = ?4, base_currency = ?5, rate_ppm = ?6,
                direction = ?7, merchant = ?8, category = ?9, channel = ?10
             WHERE id = ?11 AND voided_at IS NULL AND discarded_at IS NULL AND consumed_at IS NULL",
            params![
                occurred_on,
                amount,
                currency,
                base_amount,
                base_currency,
                rate,
                direction,
                merchant,
                category,
                channel,
                draft_id
            ],
        )?;
        let after = load_active_snapshot(transaction, draft_id)?;
        write_human_audit(
            transaction,
            &timestamp,
            draft_id,
            "update",
            Some(&before),
            Some(&after),
        )?;
        Ok(())
    })
}

pub fn discard_draft(database: &Database, draft_id: &str) -> AppResult<()> {
    let timestamp = now()?;
    database.write(|transaction| {
        let before = load_active_snapshot(transaction, draft_id)?;
        let updated = transaction.execute(
            "UPDATE draft_transactions SET discarded_at = ?1
             WHERE id = ?2 AND voided_at IS NULL AND discarded_at IS NULL AND consumed_at IS NULL",
            params![timestamp, draft_id],
        )?;
        if updated != 1 {
            return Err(AppError::new("review.draft_not_active", "草稿已被处理"));
        }
        write_human_audit(
            transaction,
            &timestamp,
            draft_id,
            "discard",
            Some(&before),
            Some(&json!({ "discardedAt": timestamp })),
        )?;
        update_source_reviewed(transaction, &before.source_id, &before.attempt_id)?;
        Ok(())
    })
}

fn ensure_batch_allowed(check: &TotalCheck) -> AppResult<()> {
    match check.confirmation_policy.as_str() {
        "reconciled_batch" | "user_attested_batch" => Ok(()),
        "single_only" if check.reconciliation_status == "failed" => Err(AppError::new(
            "review.total_mismatch",
            "来源合计与草稿合计不一致，只能逐条确认",
        )),
        "single_only" => Err(AppError::new(
            "review.total_unavailable",
            "来源总额无法校验，只能逐条确认",
        )),
        _ => Err(AppError::storage("未知 confirmation_policy")),
    }
}

fn confirm_in_transaction(
    transaction: &Transaction<'_>,
    draft_id: &str,
    timestamp: &str,
) -> AppResult<ConfirmResult> {
    let draft = load_active_snapshot(transaction, draft_id)?;
    if draft.source_id.trim().is_empty() || draft.evidence_text.trim().is_empty() {
        return Err(AppError::new(
            "review.missing_evidence",
            "草稿没有完整证据，不能入库",
        ));
    }
    let (Some(base_amount), Some(base_currency), Some(rate)) = (
        draft.base_amount_minor,
        draft.base_currency.as_ref(),
        draft.rate_ppm,
    ) else {
        return Err(AppError::new(
            "review.incomplete_triple",
            "草稿缺少本位币金额或汇率，不能入库",
        ));
    };
    validate_triple(
        draft.amount_minor.0,
        &draft.currency,
        base_amount.0,
        base_currency,
        rate.0,
    )?;
    let transaction_id = Uuid::new_v4().to_string();
    transaction.execute(
        "INSERT INTO transactions (
            id, source_id, source_draft_id, evidence_text, occurred_on,
            amount_minor, currency, base_amount_minor, base_currency, rate_ppm,
            direction, merchant, category, channel, confirmed_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
        params![
            transaction_id,
            draft.source_id,
            draft.id,
            draft.evidence_text,
            draft.occurred_on,
            draft.amount_minor.0,
            draft.currency,
            base_amount.0,
            base_currency,
            rate.0,
            draft.direction,
            draft.merchant,
            draft.category,
            draft.channel,
            timestamp,
        ],
    )?;
    let updated = transaction.execute(
        "UPDATE draft_transactions SET consumed_at = ?1
         WHERE id = ?2 AND voided_at IS NULL AND discarded_at IS NULL AND consumed_at IS NULL",
        params![timestamp, draft_id],
    )?;
    if updated != 1 {
        return Err(AppError::new("review.draft_not_active", "草稿已被处理"));
    }
    write_human_audit(
        transaction,
        timestamp,
        draft_id,
        "confirm",
        Some(&draft),
        Some(&json!({ "transactionId": transaction_id, "consumedAt": timestamp })),
    )?;
    update_source_reviewed(transaction, &draft.source_id, &draft.attempt_id)?;
    Ok(ConfirmResult {
        draft_id: draft_id.to_owned(),
        transaction_id,
    })
}

fn update_source_reviewed(
    transaction: &Transaction<'_>,
    source_id: &str,
    attempt_id: &str,
) -> AppResult<()> {
    transaction.execute(
        "UPDATE sources SET state = 'reviewed'
         WHERE id = ?1 AND latest_attempt_id = ?2 AND state = 'parsed'
           AND NOT EXISTS (
             SELECT 1 FROM draft_transactions
             WHERE attempt_id = ?2 AND voided_at IS NULL
               AND discarded_at IS NULL AND consumed_at IS NULL
           )",
        params![source_id, attempt_id],
    )?;
    Ok(())
}

fn load_active_snapshot(transaction: &Transaction<'_>, draft_id: &str) -> AppResult<DraftSnapshot> {
    transaction
        .query_row(
            "SELECT id, source_id, attempt_id, evidence_text, occurred_on,
                amount_minor, currency, base_amount_minor, base_currency, rate_ppm,
                direction, merchant, category, channel, drafted_json
             FROM draft_transactions
             WHERE id = ?1 AND voided_at IS NULL AND discarded_at IS NULL AND consumed_at IS NULL",
            [draft_id],
            |row| {
                Ok(DraftSnapshot {
                    id: row.get(0)?,
                    source_id: row.get(1)?,
                    attempt_id: row.get(2)?,
                    evidence_text: row.get(3)?,
                    occurred_on: row.get(4)?,
                    amount_minor: DecimalI64(row.get(5)?),
                    currency: row.get(6)?,
                    base_amount_minor: row.get::<_, Option<i64>>(7)?.map(DecimalI64),
                    base_currency: row.get(8)?,
                    rate_ppm: row.get::<_, Option<i64>>(9)?.map(DecimalI64),
                    direction: row.get(10)?,
                    merchant: row.get(11)?,
                    category: row.get(12)?,
                    channel: row.get(13)?,
                    drafted_json: row.get(14)?,
                })
            },
        )
        .optional()?
        .ok_or_else(|| AppError::new("review.draft_not_active", "草稿不存在或已被处理"))
}

fn reconciliation_drafts(
    database: &Database,
    attempt_id: &str,
) -> AppResult<Vec<ReconciliationDraft>> {
    database.read(|connection| {
        let mut statement = connection.prepare(
            "SELECT id, amount_minor, currency, base_amount_minor, base_currency, direction
             FROM draft_transactions WHERE attempt_id = ?1 AND voided_at IS NULL
             ORDER BY source_ordinal, id",
        )?;
        let values = statement
            .query_map([attempt_id], |row| {
                Ok(ReconciliationDraft {
                    id: row.get(0)?,
                    amount_minor: row.get(1)?,
                    currency: row.get(2)?,
                    base_amount_minor: row.get(3)?,
                    base_currency: row.get(4)?,
                    direction: row.get(5)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(values)
    })
}

fn write_human_audit<T: Serialize, U: Serialize>(
    transaction: &Transaction<'_>,
    timestamp: &str,
    entity_id: &str,
    action: &str,
    before: Option<&T>,
    after: Option<&U>,
) -> AppResult<()> {
    let before_json = before
        .map(serde_json::to_string)
        .transpose()
        .map_err(|error| AppError::storage(format!("审计序列化失败：{error}")))?;
    let after_json = after
        .map(serde_json::to_string)
        .transpose()
        .map_err(|error| AppError::storage(format!("审计序列化失败：{error}")))?;
    transaction.execute(
        "INSERT INTO audit_log (
            id, actor, at, entity_type, entity_id, action, before_json, after_json
         ) VALUES (?1, 'human', ?2, 'draft_transaction', ?3, ?4, ?5, ?6)",
        params![
            Uuid::new_v4().to_string(),
            timestamp,
            entity_id,
            action,
            before_json,
            after_json,
        ],
    )?;
    Ok(())
}

fn now() -> AppResult<String> {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .map_err(|error| AppError::storage(format!("时间格式化失败：{error}")))
}

#[cfg(test)]
mod review {
    use tempfile::tempdir;

    use super::*;

    struct Fixture {
        _directory: tempfile::TempDir,
        database: Database,
        source_id: String,
        attempt_id: String,
        kind: String,
    }

    impl Fixture {
        fn new(kind: &str, reported: Option<(i64, &str, &str)>) -> Self {
            let directory = tempdir().unwrap();
            let database = Database::open(directory.path()).unwrap();
            let source_id = Uuid::new_v4().to_string();
            let attempt_id = Uuid::new_v4().to_string();
            let hash = format!("{:064x}", source_id.len());
            database
                .write(|transaction| {
                    if kind == "utterance" {
                        transaction.execute(
                            "INSERT INTO sources (
                                id, kind, content_hash, idempotency_key, ext, byte_size,
                                evidence_relpath, imported_at, state
                             ) VALUES (?1, 'utterance', ?2, ?3, 'txt', 1, ?4, ?5, 'imported')",
                            params![source_id, hash, format!("key-{source_id}"), format!("evidence/{source_id}.txt"), "2026-08-13T00:00:00Z"],
                        )?;
                    } else {
                        transaction.execute(
                            "INSERT INTO sources (
                                id, kind, content_hash, original_filename, ext, byte_size,
                                evidence_relpath, imported_at, state
                             ) VALUES (?1, 'file', ?2, 'a.png', 'png', 1, ?3, ?4, 'imported')",
                            params![source_id, hash, format!("evidence/{source_id}.png"), "2026-08-13T00:00:00Z"],
                        )?;
                    }
                    let (amount, currency, total_kind) = reported
                        .map(|value| (Some(value.0), Some(value.1), Some(value.2)))
                        .unwrap_or((None, None, None));
                    let evidence = reported.map(|_| "Total");
                    transaction.execute(
                        "INSERT INTO parse_attempts (
                            id, source_id, agent_session_id, backend_id, prompt_hash,
                            tool_surface_version, effective_capability_hash, app_version, started_at,
                            ended_at, outcome, reported_item_count, unparsed_note,
                            reported_total_minor, reported_total_currency,
                            reported_total_kind, reported_total_evidence_text
                         ) VALUES (?1, ?2, 'session', 'test', ?3, 'm0-test', ?3, '0.1.0', ?4,
                            ?4, 'completed', 0, '', ?5, ?6, ?7, ?8)",
                        params![attempt_id, source_id, "b".repeat(64), "2026-08-13T00:00:00Z", amount, currency, total_kind, evidence],
                    )?;
                    transaction.execute(
                        "UPDATE sources SET state = 'parsed', latest_attempt_id = ?1 WHERE id = ?2",
                        params![attempt_id, source_id],
                    )?;
                    Ok(())
                })
                .unwrap();
            Self {
                _directory: directory,
                database,
                source_id,
                attempt_id,
                kind: kind.to_owned(),
            }
        }

        fn draft(&self, id: &str, ordinal: i64, amount: i64, currency: &str, direction: &str) {
            self.draft_with_base(
                id,
                ordinal,
                amount,
                currency,
                Some((amount, currency, 1_000_000)),
                direction,
            );
        }

        fn draft_with_base(
            &self,
            id: &str,
            ordinal: i64,
            amount: i64,
            currency: &str,
            base: Option<(i64, &str, i64)>,
            direction: &str,
        ) {
            let (base_amount, base_currency, rate) = base
                .map(|value| (Some(value.0), Some(value.1), Some(value.2)))
                .unwrap_or((None, None, None));
            let (span_start, span_end) = if self.kind == "utterance" {
                (Some(0_i64), Some(8_i64))
            } else {
                (None, None)
            };
            self.database
                .write(|transaction| {
                    transaction.execute(
                        "INSERT INTO draft_transactions (
                            id, source_id, evidence_text, source_ordinal,
                            evidence_span_start, evidence_span_end, attempt_id, drafted_json,
                            occurred_on, amount_minor, currency, base_amount_minor, base_currency,
                            rate_ppm, direction, merchant, created_at
                         ) VALUES (?1, ?2, 'evidence', ?3, ?4, ?5, ?6, '{\"original\":true}',
                            '2026-08-13', ?7, ?8, ?9, ?10, ?11, ?12, 'Merchant', ?13)",
                        params![
                            id,
                            self.source_id,
                            ordinal,
                            span_start,
                            span_end,
                            self.attempt_id,
                            amount,
                            currency,
                            base_amount,
                            base_currency,
                            rate,
                            direction,
                            "2026-08-13T00:00:00Z"
                        ],
                    )?;
                    Ok(())
                })
                .unwrap();
        }
    }

    #[test]
    fn total_check_exact_equality() {
        let passed = Fixture::new("file", Some((100, "AUD", "expense_total")));
        passed.draft("d1", 1, 100, "AUD", "expense");
        assert_eq!(
            total_check(&passed.database, &passed.attempt_id)
                .unwrap()
                .reconciliation_status,
            "passed"
        );
        let failed = Fixture::new("file", Some((101, "AUD", "expense_total")));
        failed.draft("d1", 1, 100, "AUD", "expense");
        assert_eq!(
            total_check(&failed.database, &failed.attempt_id)
                .unwrap()
                .reconciliation_status,
            "failed"
        );
    }

    #[test]
    fn total_check_uses_reported_currency() {
        let fixture = Fixture::new("file", Some((200, "AUD", "expense_total")));
        fixture.draft("aud", 1, 100, "AUD", "expense");
        fixture.draft_with_base(
            "usd",
            2,
            65,
            "USD",
            Some((100, "AUD", 1_538_462)),
            "expense",
        );
        assert_eq!(
            total_check(&fixture.database, &fixture.attempt_id)
                .unwrap()
                .reconciliation_status,
            "passed"
        );
    }

    #[test]
    fn total_check_unavailable_when_not_reported() {
        let fixture = Fixture::new("file", None);
        let check = total_check(&fixture.database, &fixture.attempt_id).unwrap();
        assert_eq!(check.reconciliation_status, "unavailable");
        assert_eq!(check.confirmation_policy, "single_only");
    }

    #[test]
    fn total_check_unavailable_when_amount_unobtainable() {
        let fixture = Fixture::new("file", Some((100, "AUD", "expense_total")));
        fixture.draft_with_base("d1", 1, 50, "USD", None, "expense");
        let check = total_check(&fixture.database, &fixture.attempt_id).unwrap();
        assert_eq!(check.reconciliation_status, "unavailable");
        assert_eq!(check.unavailable_draft_ids, vec!["d1"]);
    }

    #[test]
    fn total_check_unavailable_on_transfer() {
        let fixture = Fixture::new("file", Some((0, "AUD", "net_change")));
        fixture.draft("d1", 1, 100, "AUD", "transfer");
        assert_eq!(
            total_check(&fixture.database, &fixture.attempt_id)
                .unwrap()
                .reconciliation_status,
            "unavailable"
        );
    }

    #[test]
    fn total_check_uses_reported_kind() {
        for (kind, total) in [
            ("expense_total", 100),
            ("income_total", 40),
            ("net_change", -60),
        ] {
            let fixture = Fixture::new("file", Some((total, "AUD", kind)));
            fixture.draft("expense", 1, 100, "AUD", "expense");
            fixture.draft("income", 2, 40, "AUD", "income");
            assert_eq!(
                total_check(&fixture.database, &fixture.attempt_id)
                    .unwrap()
                    .reconciliation_status,
                "passed",
                "{kind}"
            );
        }
    }

    #[test]
    fn utterance_yields_user_attested_batch() {
        let fixture = Fixture::new("utterance", None);
        let check = total_check(&fixture.database, &fixture.attempt_id).unwrap();
        assert_eq!(check.reconciliation_status, "not_applicable");
        assert_eq!(check.confirmation_policy, "user_attested_batch");
    }

    #[test]
    fn utterance_with_stated_total_reconciles() {
        let fixture = Fixture::new("utterance", Some((100, "AUD", "expense_total")));
        fixture.draft_with_base(
            "d1",
            1,
            100,
            "AUD",
            Some((100, "AUD", 1_000_000)),
            "expense",
        );
        let check = total_check(&fixture.database, &fixture.attempt_id).unwrap();
        assert_eq!(check.reconciliation_status, "passed");
        assert_eq!(check.confirmation_policy, "user_attested_batch");
    }

    #[test]
    fn file_source_never_user_attested() {
        for reported in [None, Some((100, "AUD", "expense_total"))] {
            let fixture = Fixture::new("file", reported);
            let check = total_check(&fixture.database, &fixture.attempt_id).unwrap();
            assert_ne!(check.reconciliation_status, "not_applicable");
            assert_ne!(check.confirmation_policy, "user_attested_batch");
        }
    }

    #[test]
    fn file_without_total_is_unavailable_not_na() {
        let fixture = Fixture::new("file", None);
        fixture.draft("d1", 1, 100, "AUD", "expense");
        let check = total_check(&fixture.database, &fixture.attempt_id).unwrap();
        assert_eq!(
            (&check.reconciliation_status, &check.confirmation_policy),
            (&"unavailable".to_owned(), &"single_only".to_owned())
        );
    }

    #[test]
    fn batch_gate_reads_policy_not_status() {
        let fixture = Fixture::new("file", Some((0, "AUD", "expense_total")));
        let mut check = total_check(&fixture.database, &fixture.attempt_id).unwrap();
        check.reconciliation_status = "passed".to_owned();
        check.confirmation_policy = "single_only".to_owned();
        assert!(ensure_batch_allowed(&check).is_err());
    }

    #[test]
    fn confirm_rejects_incomplete_triple() {
        let fixture = Fixture::new("file", None);
        fixture.draft_with_base("d1", 1, 100, "AUD", None, "expense");
        assert_eq!(
            confirm_one(&fixture.database, "d1").unwrap_err().code,
            "review.incomplete_triple"
        );
    }

    fn stored_triple(
        fixture: &Fixture,
        id: &str,
    ) -> (i64, Option<i64>, Option<String>, Option<i64>) {
        fixture
            .database
            .read(|connection| {
                connection.query_row(
                    "SELECT amount_minor, base_amount_minor, base_currency, rate_ppm
                     FROM draft_transactions WHERE id = ?1",
                    [id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                )
            })
            .unwrap()
    }

    #[test]
    fn inline_edit_amount_recomputes_base() {
        // §1 的头号场景：用户把 AI 读错的 1680 改回 168，界面上只有金额这一个输入。
        let same_currency = Fixture::new("file", None);
        same_currency.draft("d1", 1, 1680, "AUD", "expense");
        edit_draft(
            &same_currency.database,
            "d1",
            &DraftPatch {
                amount_minor: Some(DecimalI64(168)),
                ..Default::default()
            },
        )
        .unwrap();
        let (amount, base, base_currency, rate) = stored_triple(&same_currency, "d1");
        assert_eq!((amount, base), (168, Some(168)));
        validate_triple(
            amount,
            "AUD",
            base.unwrap(),
            &base_currency.unwrap(),
            rate.unwrap(),
        )
        .unwrap();

        // 跨币种：证明它真的走了换算，不只是原币等于本位币时的恒等退化。
        let cross = Fixture::new("file", None);
        cross.draft_with_base("d2", 1, 65, "USD", Some((100, "AUD", 1_538_462)), "expense");
        edit_draft(
            &cross.database,
            "d2",
            &DraftPatch {
                amount_minor: Some(DecimalI64(130)),
                ..Default::default()
            },
        )
        .unwrap();
        let (amount, base, base_currency, rate) = stored_triple(&cross, "d2");
        assert_eq!((amount, base), (130, Some(200)));
        validate_triple(
            amount,
            "USD",
            base.unwrap(),
            &base_currency.unwrap(),
            rate.unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn inline_edit_preserves_drafted_json() {
        // 改的是**金额**，与 §6 验收原文一致。改回「只改 merchant」本条即失去意义——
        // v0.11 正是那样写的，于是「只改金额必失败」漏出了门禁。
        let fixture = Fixture::new("file", None);
        fixture.draft("d1", 1, 1680, "AUD", "expense");
        edit_draft(
            &fixture.database,
            "d1",
            &DraftPatch {
                amount_minor: Some(DecimalI64(168)),
                ..Default::default()
            },
        )
        .unwrap();
        let drafted: String = fixture
            .database
            .read(|connection| {
                connection.query_row(
                    "SELECT drafted_json FROM draft_transactions WHERE id = 'd1'",
                    [],
                    |row| row.get(0),
                )
            })
            .unwrap();
        assert_eq!(drafted, "{\"original\":true}");
        assert_eq!(stored_triple(&fixture, "d1").0, 168);
    }

    #[test]
    fn inline_edit_rejects_half_triple() {
        let fixture = Fixture::new("file", None);
        fixture.draft_with_base("d1", 1, 100, "AUD", None, "expense");
        assert_eq!(
            edit_draft(
                &fixture.database,
                "d1",
                &DraftPatch {
                    rate_ppm: Some(DecimalI64(1_000_000)),
                    ..Default::default()
                },
            )
            .unwrap_err()
            .code,
            "review.incomplete_triple"
        );
        assert_eq!(stored_triple(&fixture, "d1"), (100, None, None, None));
    }

    #[test]
    fn inline_edit_completes_missing_triple() {
        // §3.5「三元组补全」：缺三元组的草稿要能当场补齐，而不是只能丢弃重解析。
        let fixture = Fixture::new("file", None);
        fixture.draft_with_base("d1", 1, 100, "AUD", None, "expense");
        assert_eq!(
            confirm_one(&fixture.database, "d1").unwrap_err().code,
            "review.incomplete_triple"
        );
        edit_draft(
            &fixture.database,
            "d1",
            &DraftPatch {
                base_currency: Some("AUD".to_owned()),
                rate_ppm: Some(DecimalI64(1_000_000)),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(stored_triple(&fixture, "d1").1, Some(100));
        confirm_one(&fixture.database, "d1").unwrap();
    }

    #[test]
    fn every_edit_writes_audit() {
        let fixture = Fixture::new("file", None);
        fixture.draft("d1", 1, 100, "AUD", "expense");
        edit_draft(
            &fixture.database,
            "d1",
            &DraftPatch {
                merchant: Some("New".to_owned()),
                ..Default::default()
            },
        )
        .unwrap();
        let audit: (i64, String, String) = fixture
            .database
            .read(|connection| {
                connection.query_row(
                    "SELECT COUNT(*), MIN(before_json), MIN(after_json) FROM audit_log",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
            })
            .unwrap();
        assert_eq!(audit.0, 1);
        assert!(!audit.1.is_empty() && !audit.2.is_empty());
    }

    #[test]
    fn confirmed_draft_is_marked_not_deleted() {
        let fixture = Fixture::new("file", None);
        fixture.draft("d1", 1, 100, "AUD", "expense");
        confirm_one(&fixture.database, "d1").unwrap();
        let state: (i64, i64) = fixture
            .database
            .read(|connection| {
                connection.query_row(
                    "SELECT COUNT(*), COUNT(consumed_at) FROM draft_transactions WHERE id = 'd1'",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
            })
            .unwrap();
        assert_eq!(state, (1, 1));
    }

    #[test]
    fn discarded_draft_is_marked_not_voided() {
        let fixture = Fixture::new("file", None);
        fixture.draft("d1", 1, 100, "AUD", "expense");
        discard_draft(&fixture.database, "d1").unwrap();
        let state: (i64, i64, String) = fixture.database.read(|connection| connection.query_row("SELECT COUNT(discarded_at), COUNT(voided_at), drafted_json FROM draft_transactions WHERE id = 'd1'", [], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))).unwrap();
        assert_eq!(state, (1, 0, "{\"original\":true}".to_owned()));
    }

    #[test]
    fn total_check_survives_discard() {
        let fixture = Fixture::new("file", Some((100, "AUD", "expense_total")));
        fixture.draft("d1", 1, 100, "AUD", "expense");
        let before = total_check(&fixture.database, &fixture.attempt_id).unwrap();
        discard_draft(&fixture.database, "d1").unwrap();
        let after = total_check(&fixture.database, &fixture.attempt_id).unwrap();
        assert_eq!(before.reconciliation_status, after.reconciliation_status);
        assert_eq!(before.calculated_total_minor, after.calculated_total_minor);
    }

    #[test]
    fn total_check_survives_partial_confirm() {
        let fixture = Fixture::new("file", Some((201, "AUD", "expense_total")));
        fixture.draft("d1", 1, 100, "AUD", "expense");
        fixture.draft("d2", 2, 100, "AUD", "expense");
        let before = total_check(&fixture.database, &fixture.attempt_id).unwrap();
        confirm_one(&fixture.database, "d1").unwrap();
        let partial = total_check(&fixture.database, &fixture.attempt_id).unwrap();
        confirm_one(&fixture.database, "d2").unwrap();
        let complete = total_check(&fixture.database, &fixture.attempt_id).unwrap();
        assert_eq!(
            before.calculated_total_minor,
            partial.calculated_total_minor
        );
        assert_eq!(
            before.calculated_total_minor,
            complete.calculated_total_minor
        );
        assert_eq!(complete.reconciliation_status, "failed");
    }

    #[test]
    fn total_check_excludes_voided_drafts() {
        let fixture = Fixture::new("file", Some((100, "AUD", "expense_total")));
        fixture.draft("active", 1, 100, "AUD", "expense");
        fixture.draft("voided", 2, 50, "AUD", "expense");
        fixture
            .database
            .write(|transaction| {
                transaction.execute(
                    "UPDATE draft_transactions SET voided_at = ?1 WHERE id = 'voided'",
                    ["2026-08-13T00:00:00Z"],
                )?;
                Ok(())
            })
            .unwrap();
        assert_eq!(
            total_check(&fixture.database, &fixture.attempt_id)
                .unwrap()
                .reconciliation_status,
            "passed"
        );
    }

    #[test]
    fn confirm_rejects_draft_without_evidence() {
        let fixture = Fixture::new("file", None);
        fixture.draft("d1", 1, 100, "AUD", "expense");
        fixture
            .database
            .read(|connection| {
                connection.pragma_update(None, "ignore_check_constraints", "ON")?;
                connection.execute(
                    "UPDATE draft_transactions SET evidence_text = '' WHERE id = 'd1'",
                    [],
                )?;
                connection.pragma_update(None, "ignore_check_constraints", "OFF")?;
                Ok(())
            })
            .unwrap();
        assert_eq!(
            confirm_one(&fixture.database, "d1").unwrap_err().code,
            "review.missing_evidence"
        );
    }

    #[test]
    fn batch_confirm_blocked_when_total_failed() {
        let fixture = Fixture::new("file", Some((101, "AUD", "expense_total")));
        fixture.draft("d1", 1, 100, "AUD", "expense");
        assert_eq!(
            confirm_batch(&fixture.database, &["d1".to_owned()], None)
                .unwrap_err()
                .code,
            "review.total_mismatch"
        );
    }

    #[test]
    fn batch_confirm_blocked_when_total_unavailable() {
        let fixture = Fixture::new("file", None);
        fixture.draft("d1", 1, 100, "AUD", "expense");
        assert_eq!(
            confirm_batch(&fixture.database, &["d1".to_owned()], None)
                .unwrap_err()
                .code,
            "review.total_unavailable"
        );
    }

    #[test]
    fn single_confirm_allowed_when_total_failed() {
        let fixture = Fixture::new("file", Some((101, "AUD", "expense_total")));
        fixture.draft("d1", 1, 100, "AUD", "expense");
        assert_eq!(
            total_check(&fixture.database, &fixture.attempt_id)
                .unwrap()
                .reconciliation_status,
            "failed"
        );
        confirm_one(&fixture.database, "d1").unwrap();
    }

    #[test]
    fn transaction_traces_to_draft() {
        let fixture = Fixture::new("file", None);
        fixture.draft("d1", 1, 100, "AUD", "expense");
        let result = confirm_one(&fixture.database, "d1").unwrap();
        let (source_draft_id, consumed): (String, bool) = fixture
            .database
            .read(|connection| {
                connection.query_row(
                    "SELECT t.source_draft_id, d.consumed_at IS NOT NULL
                     FROM transactions t JOIN draft_transactions d ON d.id = t.source_draft_id
                     WHERE t.id = ?1",
                    [result.transaction_id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
            })
            .unwrap();
        assert_eq!(source_draft_id, "d1");
        assert!(consumed);
    }

    #[test]
    fn no_force_bypass_exists() {
        let commands = include_str!("../lib.rs");
        let start = commands.find("fn confirm_draft(").unwrap();
        let end = commands[start..].find("fn update_draft(").unwrap() + start;
        let confirmation_commands = &commands[start..end];
        assert!(!confirmation_commands.contains("force"));
        assert!(!confirmation_commands.contains("ignore"));
    }

    #[test]
    fn total_check_is_scoped_to_attempt() {
        let fixture = Fixture::new("file", Some((100, "AUD", "expense_total")));
        fixture.draft("first", 1, 100, "AUD", "expense");
        let second_attempt = Uuid::new_v4().to_string();
        fixture
            .database
            .write(|transaction| {
                transaction.execute(
                    "INSERT INTO parse_attempts (
                        id, source_id, agent_session_id, backend_id, prompt_hash,
                        tool_surface_version, effective_capability_hash, app_version,
                        started_at, ended_at, outcome, reported_item_count, unparsed_note,
                        reported_total_minor, reported_total_currency,
                        reported_total_kind, reported_total_evidence_text
                     ) VALUES (?1, ?2, 'second-session', 'test', ?3, 'm0-test', ?3,
                        '0.1.0', ?4, ?4, 'completed', 1, '', 300, 'AUD',
                        'expense_total', 'Total 3.00')",
                    params![
                        second_attempt,
                        fixture.source_id,
                        "c".repeat(64),
                        "2026-08-13T01:00:00Z"
                    ],
                )?;
                transaction.execute(
                    "INSERT INTO draft_transactions (
                        id, source_id, evidence_text, source_ordinal, attempt_id, drafted_json,
                        occurred_on, amount_minor, currency, base_amount_minor, base_currency,
                        rate_ppm, direction, merchant, created_at
                     ) VALUES ('second', ?1, 'second evidence', 1, ?2, '{}',
                        '2026-08-13', 300, 'AUD', 300, 'AUD', 1000000,
                        'expense', 'Second', ?3)",
                    params![fixture.source_id, second_attempt, "2026-08-13T01:00:00Z"],
                )?;
                Ok(())
            })
            .unwrap();
        let first = total_check(&fixture.database, &fixture.attempt_id).unwrap();
        let second = total_check(&fixture.database, &second_attempt).unwrap();
        assert_eq!(first.calculated_total_minor, Some(DecimalI64(100)));
        assert_eq!(second.calculated_total_minor, Some(DecimalI64(300)));
        assert_eq!(first.reconciliation_status, "passed");
        assert_eq!(second.reconciliation_status, "passed");
    }

    #[test]
    fn retry_uses_new_reported_total() {
        total_check_is_scoped_to_attempt();
    }
}
