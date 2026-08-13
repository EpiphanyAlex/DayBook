use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
};

use rusqlite::{params, OptionalExtension};
use serde::Serialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};
use uuid::Uuid;

use crate::{
    db::Database,
    error::{AppError, AppResult},
};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub source_id: String,
    pub deduplicated: bool,
    pub matching_utterance_source_ids: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceState {
    Imported,
    Parsing,
    Parsed,
    Failed,
    Reviewed,
}

impl SourceState {
    pub fn parse(value: &str) -> AppResult<Self> {
        match value {
            "imported" => Ok(Self::Imported),
            "parsing" => Ok(Self::Parsing),
            "parsed" => Ok(Self::Parsed),
            "failed" => Ok(Self::Failed),
            "reviewed" => Ok(Self::Reviewed),
            _ => Err(AppError::new(
                "data.invalid_source_state",
                format!("未知来源状态：{value}"),
            )),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Imported => "imported",
            Self::Parsing => "parsing",
            Self::Parsed => "parsed",
            Self::Failed => "failed",
            Self::Reviewed => "reviewed",
        }
    }

    pub fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Imported, Self::Parsing)
                | (Self::Parsing, Self::Parsed | Self::Failed)
                | (Self::Failed, Self::Parsing)
                | (Self::Parsed, Self::Parsing | Self::Reviewed)
                | (Self::Failed, Self::Failed)
        )
    }
}

pub fn import_file(database: &Database, input_path: impl AsRef<Path>) -> AppResult<ImportResult> {
    let input_path = input_path.as_ref();
    let bytes = fs::read(input_path)?;
    let original_filename = input_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| AppError::invalid_argument("文件名不是有效的 UTF-8"))?;
    import_file_bytes(database, original_filename, &bytes, false)
}

pub fn import_utterance(
    database: &Database,
    text: &str,
    idempotency_key: &str,
) -> AppResult<ImportResult> {
    if text.trim().is_empty() {
        return Err(AppError::invalid_argument("口述文本不能为空"));
    }
    if idempotency_key.trim().is_empty() || idempotency_key.len() > 200 {
        return Err(AppError::invalid_argument(
            "idempotency_key 必须是 1–200 个字符",
        ));
    }
    if let Some(source_id) = database.read(|connection| {
        connection
            .query_row(
                "SELECT id FROM sources WHERE kind = 'utterance' AND idempotency_key = ?1",
                [idempotency_key],
                |row| row.get(0),
            )
            .optional()
    })? {
        return Ok(ImportResult {
            source_id,
            deduplicated: true,
            matching_utterance_source_ids: Vec::new(),
        });
    }

    let content_hash = sha256_hex(text.as_bytes());
    let matching_utterance_source_ids = database.read(|connection| {
        let mut statement = connection.prepare(
            "SELECT id FROM sources
             WHERE kind = 'utterance' AND content_hash = ?1
             ORDER BY imported_at, id",
        )?;
        let rows = statement
            .query_map([&content_hash], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    })?;
    let source_id = Uuid::new_v4().to_string();
    let relative_path = evidence_relative_path(&source_id, "txt")?;
    let evidence_path = database.root().join(&relative_path);
    write_new_evidence(&evidence_path, text.as_bytes())?;
    let timestamp = now()?;
    let insert_result = database.write(|transaction| {
        transaction.execute(
            "INSERT INTO sources (
                id, kind, content_hash, idempotency_key, original_filename, ext,
                byte_size, evidence_relpath, imported_at, state
             ) VALUES (?1, 'utterance', ?2, ?3, NULL, 'txt', ?4, ?5, ?6, 'imported')",
            params![
                source_id,
                content_hash,
                idempotency_key,
                i64::try_from(text.len())
                    .map_err(|_| AppError::invalid_argument("口述文本过大"))?,
                relative_path,
                timestamp,
            ],
        )?;
        Ok(())
    });
    if let Err(error) = insert_result {
        cleanup_unreferenced_evidence(&evidence_path);
        if let Some(existing) = database.read(|connection| {
            connection
                .query_row(
                    "SELECT id FROM sources WHERE kind = 'utterance' AND idempotency_key = ?1",
                    [idempotency_key],
                    |row| row.get::<_, String>(0),
                )
                .optional()
        })? {
            return Ok(ImportResult {
                source_id: existing,
                deduplicated: true,
                matching_utterance_source_ids: Vec::new(),
            });
        }
        return Err(error);
    }
    Ok(ImportResult {
        source_id,
        deduplicated: false,
        matching_utterance_source_ids,
    })
}

/// **`sources.state` 的唯一写入点**（02 §3.4）。
///
/// 本函数收在同一个事务里，好让调用方把「推进状态」和它自己的写入（插 `parse_attempts`、
/// 作废草稿、写审计）放在一次提交里。`source_state_has_one_writer` 守着「别处不许再写」。
///
/// **为什么必须只有一处**：M0 曾经有两份状态机——本文件这份（有测试、没人用）和
/// `agent/runtime.rs` 里内联的 `matches!`（真正生效）。两份对 `parsed → parsing`
/// 的答案还相反，而穷举 25 种转移的测试全绿，因为它测的是没人调用的那份。
pub fn apply_transition(
    transaction: &rusqlite::Transaction<'_>,
    source_id: &str,
    next: SourceState,
    error_code: Option<&str>,
) -> AppResult<()> {
    let current = transaction
        .query_row(
            "SELECT state FROM sources WHERE id = ?1",
            [source_id],
            |row| row.get::<_, String>(0),
        )
        .map_err(|error| match error {
            rusqlite::Error::QueryReturnedNoRows => AppError::new("data.not_found", "来源不存在"),
            other => other.into(),
        })?;
    let current = SourceState::parse(&current)?;
    if !current.can_transition_to(next) {
        return Err(AppError::new(
            "ingest.invalid_state_transition",
            format!("不能从 {} 转到 {}", current.as_str(), next.as_str()),
        ));
    }
    if (next == SourceState::Failed) != error_code.is_some() {
        return Err(AppError::invalid_argument(
            "failed 状态必须且只能带 parse_error_code",
        ));
    }
    transaction.execute(
        "UPDATE sources SET state = ?1, parse_error_code = ?2 WHERE id = ?3",
        params![next.as_str(), error_code, source_id],
    )?;
    Ok(())
}

pub fn transition_source(
    database: &Database,
    source_id: &str,
    next: SourceState,
    error_code: Option<&str>,
) -> AppResult<()> {
    database.write(|transaction| apply_transition(transaction, source_id, next, error_code))
}

pub fn recover_interrupted(database: &Database) -> AppResult<usize> {
    let timestamp = now()?;
    database.write(|transaction| {
        let open_attempts = {
            let mut statement = transaction.prepare(
                "SELECT id, source_id FROM parse_attempts WHERE ended_at IS NULL ORDER BY started_at",
            )?;
            let rows = statement
                .query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            rows
        };
        for (attempt_id, source_id) in &open_attempts {
            transaction.execute(
                "UPDATE parse_attempts
                 SET ended_at = ?1, outcome = 'interrupted', error_code = 'agent.interrupted'
                 WHERE id = ?2 AND ended_at IS NULL",
                params![timestamp, attempt_id],
            )?;
            apply_transition(
                transaction,
                source_id,
                SourceState::Failed,
                Some("agent.interrupted"),
            )?;
            let draft_ids = {
                let mut statement = transaction.prepare(
                    "SELECT id FROM draft_transactions
                     WHERE attempt_id = ?1
                       AND voided_at IS NULL AND discarded_at IS NULL AND consumed_at IS NULL",
                )?;
                let rows = statement
                    .query_map([attempt_id], |row| row.get::<_, String>(0))?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                rows
            };
            for draft_id in draft_ids {
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
                        json!({ "voidedAt": serde_json::Value::Null }).to_string(),
                        json!({ "voidedAt": timestamp }).to_string(),
                    ],
                )?;
            }
        }
        // 「尝试行已插入但来源状态还没推进」的反面：来源卡在 parsing 而尝试行已收束。
        let dangling = {
            let mut statement =
                transaction.prepare("SELECT id FROM sources WHERE state = 'parsing'")?;
            let rows = statement
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            rows
        };
        for source_id in &dangling {
            apply_transition(
                transaction,
                source_id,
                SourceState::Failed,
                Some("agent.interrupted"),
            )?;
        }
        Ok(open_attempts.len().max(dangling.len()))
    })
}

fn import_file_bytes(
    database: &Database,
    original_filename: &str,
    bytes: &[u8],
    fail_before_insert: bool,
) -> AppResult<ImportResult> {
    let ext = supported_image_extension(bytes)?;
    let content_hash = sha256_hex(bytes);
    if let Some(source_id) = database.read(|connection| {
        connection
            .query_row(
                "SELECT id FROM sources WHERE kind = 'file' AND content_hash = ?1",
                [&content_hash],
                |row| row.get(0),
            )
            .optional()
    })? {
        return Ok(ImportResult {
            source_id,
            deduplicated: true,
            matching_utterance_source_ids: Vec::new(),
        });
    }
    let source_id = Uuid::new_v4().to_string();
    let relative_path = evidence_relative_path(&source_id, ext)?;
    let evidence_path = database.root().join(&relative_path);
    write_new_evidence(&evidence_path, bytes)?;
    let timestamp = now()?;
    let insert_result = if fail_before_insert {
        Err(AppError::storage("injected row failure"))
    } else {
        database.write(|transaction| {
            transaction.execute(
                "INSERT INTO sources (
                    id, kind, content_hash, idempotency_key, original_filename, ext,
                    byte_size, evidence_relpath, imported_at, state
                 ) VALUES (?1, 'file', ?2, NULL, ?3, ?4, ?5, ?6, ?7, 'imported')",
                params![
                    source_id,
                    content_hash,
                    original_filename,
                    ext,
                    i64::try_from(bytes.len())
                        .map_err(|_| AppError::invalid_argument("文件过大"))?,
                    relative_path,
                    timestamp,
                ],
            )?;
            Ok(())
        })
    };
    if let Err(error) = insert_result {
        cleanup_unreferenced_evidence(&evidence_path);
        if let Some(existing) = database.read(|connection| {
            connection
                .query_row(
                    "SELECT id FROM sources WHERE kind = 'file' AND content_hash = ?1",
                    [&content_hash],
                    |row| row.get::<_, String>(0),
                )
                .optional()
        })? {
            return Ok(ImportResult {
                source_id: existing,
                deduplicated: true,
                matching_utterance_source_ids: Vec::new(),
            });
        }
        return Err(error);
    }
    Ok(ImportResult {
        source_id,
        deduplicated: false,
        matching_utterance_source_ids: Vec::new(),
    })
}

fn supported_image_extension(bytes: &[u8]) -> AppResult<&'static str> {
    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n";
    if bytes.starts_with(PNG) {
        return Ok("png");
    }
    if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        return Ok("jpg");
    }
    Err(AppError::new(
        "ingest.unsupported_format",
        "M0 只支持 PNG 与 JPEG 图片",
    ))
}

fn evidence_relative_path(source_id: &str, ext: &str) -> AppResult<String> {
    let now = OffsetDateTime::now_utc();
    Ok(format!(
        "evidence/{}/{:02}/{source_id}.{ext}",
        now.year(),
        u8::from(now.month())
    ))
}

fn write_new_evidence(path: &Path, bytes: &[u8]) -> AppResult<()> {
    let parent = path
        .parent()
        .ok_or_else(|| AppError::storage("证据文件没有父目录"))?;
    fs::create_dir_all(parent)?;
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

fn cleanup_unreferenced_evidence(path: &Path) {
    let _ = fs::remove_file(path);
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn now() -> AppResult<String> {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .map_err(|error| AppError::storage(format!("时间格式化失败：{error}")))
}

#[cfg(test)]
#[allow(clippy::module_inception)] // 保持 PRD 中 `cargo test ingest::<case>` 的验收选择器稳定。
mod ingest {
    use tempfile::tempdir;

    use super::*;
    use crate::agent::registry::m0_tool_registry;

    const PNG: &[u8] = b"\x89PNG\r\n\x1a\noriginal";

    fn database() -> (tempfile::TempDir, Database) {
        let directory = tempdir().unwrap();
        let database = Database::open(directory.path()).unwrap();
        (directory, database)
    }

    fn evidence_path(database: &Database, source_id: &str) -> std::path::PathBuf {
        let relative: String = database
            .read(|connection| {
                connection.query_row(
                    "SELECT evidence_relpath FROM sources WHERE id = ?1",
                    [source_id],
                    |row| row.get(0),
                )
            })
            .unwrap();
        database.root().join(relative)
    }

    #[test]
    fn idempotent_source() {
        let (_directory, database) = database();
        let first = import_file_bytes(&database, "a.png", PNG, false).unwrap();
        let second = import_file_bytes(&database, "a.png", PNG, false).unwrap();
        assert_eq!(first.source_id, second.source_id);
        assert_eq!(
            database
                .read(|connection| connection.query_row(
                    "SELECT COUNT(*) FROM sources",
                    [],
                    |row| row.get::<_, i64>(0)
                ))
                .unwrap(),
            1
        );
    }

    #[test]
    fn idempotent_returns_ok_with_flag() {
        let (_directory, database) = database();
        assert!(
            !import_file_bytes(&database, "a.png", PNG, false)
                .unwrap()
                .deduplicated
        );
        assert!(
            import_file_bytes(&database, "a.png", PNG, false)
                .unwrap()
                .deduplicated
        );
    }

    #[test]
    fn idempotent_ignores_filename() {
        let (_directory, database) = database();
        let first = import_file_bytes(&database, "bank.png", PNG, false).unwrap();
        let second = import_file_bytes(&database, "renamed.jpeg", PNG, false).unwrap();
        assert_eq!(first.source_id, second.source_id);
    }

    #[test]
    fn unsupported_format_rejected() {
        let (_directory, database) = database();
        assert_eq!(
            import_file_bytes(&database, "invoice.pdf", b"%PDF", false)
                .unwrap_err()
                .code,
            "ingest.unsupported_format"
        );
        assert_eq!(
            database
                .read(|connection| connection.query_row(
                    "SELECT COUNT(*) FROM sources",
                    [],
                    |row| row.get::<_, i64>(0)
                ))
                .unwrap(),
            0
        );
    }

    #[test]
    fn evidence_written_before_row() {
        let (_directory, database) = database();
        assert!(import_file_bytes(&database, "a.png", PNG, true).is_err());
        assert_eq!(
            database
                .read(|connection| connection.query_row(
                    "SELECT COUNT(*) FROM sources",
                    [],
                    |row| row.get::<_, i64>(0)
                ))
                .unwrap(),
            0
        );
        let files = walkdir(&database.root().join("evidence"));
        assert!(files.is_empty());
    }

    #[test]
    fn original_bytes_preserved() {
        let (_directory, database) = database();
        let imported = import_file_bytes(&database, "a.png", PNG, false).unwrap();
        assert_eq!(
            fs::read(evidence_path(&database, &imported.source_id)).unwrap(),
            PNG
        );
    }

    #[test]
    fn state_machine_transitions() {
        let states = [
            SourceState::Imported,
            SourceState::Parsing,
            SourceState::Parsed,
            SourceState::Failed,
            SourceState::Reviewed,
        ];
        for from in states {
            for to in states {
                let expected = matches!(
                    (from, to),
                    (SourceState::Imported, SourceState::Parsing)
                        | (
                            SourceState::Parsing,
                            SourceState::Parsed | SourceState::Failed
                        )
                        | (
                            SourceState::Failed,
                            SourceState::Parsing | SourceState::Failed
                        )
                        | (
                            SourceState::Parsed,
                            SourceState::Parsing | SourceState::Reviewed
                        )
                );
                assert_eq!(from.can_transition_to(to), expected, "{from:?} -> {to:?}");
            }
        }
    }

    #[test]
    fn source_state_has_one_writer() {
        // M0 曾有两份状态机：本文件这份有测试没人用，`agent/runtime.rs` 内联的那份真正生效，
        // 两份对 `parsed → parsing` 的答案还相反。这条守住「别处不许再写 sources.state」。
        for (name, source) in [
            ("agent/runtime.rs", include_str!("agent/runtime.rs")),
            ("domain/draft.rs", include_str!("domain/draft.rs")),
            ("domain/confirm.rs", include_str!("domain/confirm.rs")),
            ("lib.rs", include_str!("lib.rs")),
        ] {
            // 只查生产代码：夹具直接摆状态是允许的，它们在造前置条件，不是在走状态机。
            let production = source.split("#[cfg(test)]").next().unwrap_or_default();
            let needle = concat!("UPDATE sources SET ", "state");
            assert!(
                !production.contains(needle),
                "{name} 绕过了 ingest::apply_transition 直接改 sources.state",
            );
        }
    }

    #[test]
    fn reparse_of_a_parsed_source_is_legal() {
        // 03 §6 的 `total_check_is_scoped_to_attempt` 要求「同一来源成功解析两次」，
        // 所以 parsed → parsing 必须合法；旧的 can_transition_to 少了这一条。
        let (_directory, database) = database();
        let imported = import_file_bytes(&database, "a.png", PNG, false).unwrap();
        transition_source(&database, &imported.source_id, SourceState::Parsing, None).unwrap();
        transition_source(&database, &imported.source_id, SourceState::Parsed, None).unwrap();
        transition_source(&database, &imported.source_id, SourceState::Parsing, None).unwrap();
        transition_source(&database, &imported.source_id, SourceState::Parsed, None).unwrap();
        transition_source(&database, &imported.source_id, SourceState::Reviewed, None).unwrap();
        assert_eq!(
            transition_source(&database, &imported.source_id, SourceState::Parsing, None)
                .unwrap_err()
                .code,
            "ingest.invalid_state_transition"
        );
    }

    #[test]
    fn agent_cannot_change_source_state() {
        assert!(m0_tool_registry().iter().all(|tool| !tool
            .write_targets
            .iter()
            .any(|target| target.starts_with("sources"))));
    }

    #[test]
    fn agent_cannot_write_source_columns() {
        assert!(m0_tool_registry()
            .iter()
            .flat_map(|tool| tool.write_targets.iter())
            .all(|target| !target.starts_with("sources")));
    }

    #[test]
    fn utterance_idempotent_by_token() {
        let (_directory, database) = database();
        let first = import_utterance(&database, "今天咖啡 5 元", "submit-1").unwrap();
        let retry = import_utterance(&database, "今天咖啡 5 元", "submit-1").unwrap();
        let tomorrow = import_utterance(&database, "今天咖啡 5 元", "submit-2").unwrap();
        assert_eq!(first.source_id, retry.source_id);
        assert!(retry.deduplicated);
        assert_ne!(first.source_id, tomorrow.source_id);
        assert!(!tomorrow.deduplicated);
        assert_eq!(
            tomorrow.matching_utterance_source_ids,
            vec![first.source_id]
        );
    }

    #[test]
    fn utterance_source_roundtrip() {
        let (_directory, database) = database();
        let imported = import_utterance(&database, "午饭 18 元", "submit-1").unwrap();
        let (kind, ext, path): (String, String, String) = database
            .read(|connection| {
                connection.query_row(
                    "SELECT kind, ext, evidence_relpath FROM sources WHERE id = ?1",
                    [&imported.source_id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
            })
            .unwrap();
        assert_eq!(kind, "utterance");
        assert_eq!(ext, "txt");
        assert_eq!(
            fs::read_to_string(database.root().join(path)).unwrap(),
            "午饭 18 元"
        );
    }

    #[test]
    fn startup_scan_closes_open_attempts() {
        let (_directory, database) = database();
        seed_open_attempt(&database, false);
        assert_eq!(recover_interrupted(&database).unwrap(), 1);
        let outcome: String = database
            .read(|connection| {
                connection.query_row("SELECT outcome FROM parse_attempts", [], |row| row.get(0))
            })
            .unwrap();
        assert_eq!(outcome, "interrupted");
    }

    #[test]
    fn startup_scan_clears_stuck_parsing() {
        let (_directory, database) = database();
        seed_open_attempt(&database, true);
        recover_interrupted(&database).unwrap();
        let (state, code, active): (String, String, i64) = database
            .read(|connection| {
                connection.query_row(
                    "SELECT s.state, s.parse_error_code,
                        (SELECT COUNT(*) FROM draft_transactions d
                         WHERE d.source_id = s.id AND d.voided_at IS NULL)
                     FROM sources s",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
            })
            .unwrap();
        assert_eq!(state, "failed");
        assert_eq!(code, "agent.interrupted");
        assert_eq!(active, 0);
    }

    #[test]
    fn failed_source_has_no_active_drafts() {
        let (_directory, database) = database();
        seed_open_attempt(&database, true);
        recover_interrupted(&database).unwrap();
        let (voided, code): (i64, String) = database
            .read(|connection| {
                connection.query_row(
                    "SELECT COUNT(d.voided_at), s.parse_error_code
                     FROM sources s JOIN draft_transactions d ON d.source_id = s.id",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
            })
            .unwrap();
        assert_eq!(voided, 1);
        assert_eq!(code, "agent.interrupted");
    }

    fn seed_open_attempt(database: &Database, with_draft: bool) {
        let source = import_file_bytes(database, "a.png", PNG, false).unwrap();
        database
            .write(|transaction| {
                transaction.execute(
                    "INSERT INTO parse_attempts (
                        id, source_id, agent_session_id, backend_id, prompt_hash,
                        tool_surface_version, effective_capability_hash, app_version, started_at
                     ) VALUES ('attempt', ?1, 'session', 'test', ?2, 'm0-test', ?2, '0.1.0', ?3)",
                    params![source.source_id, "a".repeat(64), "2026-08-13T00:00:00Z"],
                )?;
                transaction.execute(
                    "UPDATE sources SET state = 'parsing', latest_attempt_id = 'attempt'
                     WHERE id = ?1",
                    [&source.source_id],
                )?;
                if with_draft {
                    transaction.execute(
                        "INSERT INTO draft_transactions (
                            id, source_id, evidence_text, source_ordinal, attempt_id, drafted_json,
                            occurred_on, amount_minor, currency, direction, merchant, created_at
                         ) VALUES ('draft', ?1, '18.00', 1, 'attempt', '{}',
                            '2026-08-13', 1800, 'AUD', 'expense', 'Cafe', ?2)",
                        params![source.source_id, "2026-08-13T00:00:00Z"],
                    )?;
                }
                Ok(())
            })
            .unwrap();
    }

    fn walkdir(root: &Path) -> Vec<std::path::PathBuf> {
        let mut files = Vec::new();
        if let Ok(entries) = fs::read_dir(root) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    files.extend(walkdir(&path));
                } else {
                    files.push(path);
                }
            }
        }
        files
    }
}
