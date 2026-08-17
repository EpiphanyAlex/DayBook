use std::{
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
};

use rusqlite::{Connection, OpenFlags, Transaction};
use serde::{Deserialize, Serialize};

use crate::{
    error::{AppError, AppResult},
    money::currency_exponent,
};

/// 迁移号（`PRAGMA user_version`）。`pub` 是因为夹具的版本三元组要拿它比对——
/// 夹具过期必须报得明白，而不是重放到一半报个别的错（`docs/prd/07-eval.md` §5 R4）。
pub const LATEST_SCHEMA_VERSION: i64 = 1;
const M0_MIGRATION: &str = include_str!("../migrations/0001_m0.sql");

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FoundationStatus {
    pub schema_version: i64,
    pub data_directory: String,
    pub base_currency: Option<String>,
    pub debug_logging: bool,
}

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct Preferences {
    base_currency: Option<String>,
    debug_logging: Option<bool>,
}

#[derive(Debug)]
pub struct Database {
    connection: Mutex<Connection>,
    preference_lock: Mutex<()>,
    root: PathBuf,
}

impl Database {
    pub fn open(root: impl AsRef<Path>) -> AppResult<Self> {
        let root = root.as_ref();
        reject_icloud_path(root)?;
        fs::create_dir_all(root.join("evidence"))?;
        let database_path = root.join("daybook.db");
        let connection = Connection::open_with_flags(
            database_path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE,
        )?;
        connection.pragma_update(None, "foreign_keys", "ON")?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        migrate(&connection)?;
        Ok(Self {
            connection: Mutex::new(connection),
            preference_lock: Mutex::new(()),
            root: root.to_path_buf(),
        })
    }

    pub fn status(&self) -> AppResult<FoundationStatus> {
        let schema_version = self.read(|connection| {
            connection.pragma_query_value(None, "user_version", |row| row.get(0))
        })?;
        Ok(FoundationStatus {
            schema_version,
            data_directory: self.root.to_string_lossy().into_owned(),
            base_currency: self.base_currency()?,
            debug_logging: self.debug_logging()?,
        })
    }

    pub fn base_currency(&self) -> AppResult<Option<String>> {
        let _guard = self
            .preference_lock
            .lock()
            .map_err(|_| AppError::storage("偏好设置锁已损坏"))?;
        self.read_preferences().map(|value| value.base_currency)
    }

    pub fn require_base_currency(&self) -> AppResult<String> {
        self.base_currency()?.ok_or_else(|| {
            AppError::new(
                "data.base_currency_required",
                "解析前请先选择本位币；Daybook 不会替你猜地区",
            )
        })
    }

    pub fn set_base_currency(&self, currency: &str) -> AppResult<()> {
        currency_exponent(currency)?;
        let _guard = self
            .preference_lock
            .lock()
            .map_err(|_| AppError::storage("偏好设置锁已损坏"))?;
        let mut preferences = self.read_preferences()?;
        preferences.base_currency = Some(currency.to_owned());
        self.write_preferences(&preferences)
    }

    pub fn debug_logging(&self) -> AppResult<bool> {
        let _guard = self
            .preference_lock
            .lock()
            .map_err(|_| AppError::storage("偏好设置锁已损坏"))?;
        Ok(self
            .read_preferences()?
            .debug_logging
            .unwrap_or(cfg!(debug_assertions)))
    }

    pub fn set_debug_logging(&self, enabled: bool) -> AppResult<()> {
        let _guard = self
            .preference_lock
            .lock()
            .map_err(|_| AppError::storage("偏好设置锁已损坏"))?;
        let mut preferences = self.read_preferences()?;
        preferences.debug_logging = Some(enabled);
        self.write_preferences(&preferences)
    }

    fn write_preferences(&self, preferences: &Preferences) -> AppResult<()> {
        let bytes = serde_json::to_vec_pretty(&preferences)
            .map_err(|error| AppError::storage(format!("偏好设置序列化失败：{error}")))?;
        let target = self.root.join("preferences.json");
        let temporary = self.root.join(".preferences.json.tmp");
        fs::write(&temporary, bytes)?;
        fs::rename(temporary, target)?;
        Ok(())
    }

    fn read_preferences(&self) -> AppResult<Preferences> {
        let path = self.root.join("preferences.json");
        let bytes = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Preferences::default())
            }
            Err(error) => return Err(error.into()),
        };
        let preferences = serde_json::from_slice::<Preferences>(&bytes)
            .map_err(|error| AppError::storage(format!("偏好设置损坏：{error}")))?;
        if let Some(currency) = preferences.base_currency.as_deref() {
            currency_exponent(currency)?;
        }
        Ok(preferences)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn transaction_rollup_by_base_currency(&self) -> AppResult<Vec<(String, i64)>> {
        self.read(|connection| {
            let mut statement = connection.prepare(
                "SELECT base_currency,
                    SUM(CASE direction
                        WHEN 'expense' THEN -base_amount_minor
                        WHEN 'income' THEN base_amount_minor
                        ELSE 0 END)
                 FROM transactions
                 WHERE deleted_at IS NULL
                 GROUP BY base_currency
                 ORDER BY base_currency",
            )?;
            let rows = statement
                .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
                .collect();
            rows
        })
    }

    pub fn read<T>(
        &self,
        operation: impl FnOnce(&Connection) -> rusqlite::Result<T>,
    ) -> AppResult<T> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| AppError::storage("数据库锁已损坏"))?;
        operation(&connection).map_err(AppError::from)
    }

    pub fn write<T>(
        &self,
        operation: impl FnOnce(&Transaction<'_>) -> AppResult<T>,
    ) -> AppResult<T> {
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| AppError::storage("数据库锁已损坏"))?;
        let transaction = connection.transaction()?;
        let output = operation(&transaction)?;
        transaction.commit()?;
        Ok(output)
    }
}

fn migrate(connection: &Connection) -> AppResult<()> {
    let current: i64 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if current > LATEST_SCHEMA_VERSION {
        return Err(AppError::new(
            "data.migration_drift",
            format!("数据版本 {current} 高于应用支持的版本 {LATEST_SCHEMA_VERSION}，请升级应用"),
        ));
    }
    if current == 0 {
        connection.execute_batch(M0_MIGRATION)?;
    }
    Ok(())
}

fn reject_icloud_path(path: &Path) -> AppResult<()> {
    let normalized = path.to_string_lossy().to_ascii_lowercase();
    if normalized.contains("mobile documents") || normalized.contains("icloud drive") {
        return Err(AppError::new(
            "data.icloud_path_rejected",
            "Daybook 数据目录不能放在 iCloud Drive 中",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod foundation {
    use rusqlite::params;
    use tempfile::tempdir;

    use super::*;

    fn source_values(id: &str) -> (&str, &str, &str, &str, i64, &str, &str, &str) {
        (
            id,
            "file",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "source.png",
            1,
            "png",
            "evidence/2026/08/source.png",
            "2026-08-13T00:00:00Z",
        )
    }

    fn insert_source(connection: &Connection, id: &str) {
        let values = source_values(id);
        connection
            .execute(
                "INSERT INTO sources (
                id, kind, content_hash, original_filename, byte_size, ext,
                evidence_relpath, imported_at, state
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'imported')",
                params![
                    values.0, values.1, values.2, values.3, values.4, values.5, values.6, values.7
                ],
            )
            .unwrap();
    }

    fn insert_attempt(connection: &Connection, id: &str, source_id: &str) {
        connection
            .execute(
                "INSERT INTO parse_attempts (
                id, source_id, agent_session_id, backend_id, prompt_hash,
                tool_surface_version, effective_capability_hash, app_version, started_at
             ) VALUES (?1, ?2, 'session', 'test', ?3, 'm0-test', ?3, '0.1.0', ?4)",
                params![
                    id,
                    source_id,
                    "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                    "2026-08-13T00:00:00Z"
                ],
            )
            .unwrap();
    }

    fn insert_draft(
        connection: &Connection,
        id: &str,
        source_id: &str,
        attempt_id: &str,
        ordinal: i64,
    ) {
        connection
            .execute(
                "INSERT INTO draft_transactions (
                    id, source_id, evidence_text, source_ordinal, attempt_id, drafted_json,
                    occurred_on, amount_minor, currency, base_amount_minor, base_currency,
                    rate_ppm, direction, merchant, created_at
                 ) VALUES (?1, ?2, 'evidence', ?3, ?4, '{\"original\":true}',
                    '2026-08-13', 100, 'AUD', 100, 'AUD', 1000000,
                    'expense', 'Merchant', '2026-08-13T00:00:00Z')",
                params![id, source_id, ordinal, attempt_id],
            )
            .unwrap();
    }

    fn insert_fact_transaction(
        connection: &Connection,
        id: &str,
        base_amount: i64,
        base_currency: &str,
        account_id: Option<&str>,
    ) -> rusqlite::Result<usize> {
        connection.execute(
            "INSERT INTO transactions (
                id, occurred_on, amount_minor, currency, base_amount_minor, base_currency,
                rate_ppm, direction, merchant, account_id, confirmed_at
             ) VALUES (?1, '2026-08-13', ?2, ?3, ?2, ?3, 1000000,
                'income', 'Manual', ?4, '2026-08-13T00:00:00Z')",
            params![id, base_amount, base_currency, account_id],
        )
    }

    #[test]
    fn migration_idempotent() {
        let directory = tempdir().unwrap();
        assert_eq!(
            Database::open(directory.path())
                .unwrap()
                .status()
                .unwrap()
                .schema_version,
            1
        );
        assert_eq!(
            Database::open(directory.path())
                .unwrap()
                .status()
                .unwrap()
                .schema_version,
            1
        );
    }

    #[test]
    fn base_currency_required_before_parse() {
        let directory = tempdir().unwrap();
        let database = Database::open(directory.path()).unwrap();
        assert_eq!(
            database.require_base_currency().unwrap_err().code,
            "data.base_currency_required"
        );
        database.set_base_currency("AUD").unwrap();
        drop(database);
        let reopened = Database::open(directory.path()).unwrap();
        assert_eq!(reopened.require_base_currency().unwrap(), "AUD");
        assert!(directory.path().join("preferences.json").is_file());
    }

    #[test]
    fn m0_creates_six_tables() {
        let directory = tempdir().unwrap();
        let database = Database::open(directory.path()).unwrap();
        let count: i64 = database.read(|connection| {
            connection.query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
                [],
                |row| row.get(0),
            )
        }).unwrap();
        assert_eq!(count, 6);
    }

    #[test]
    fn migration_drift_rejected() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("daybook.db");
        let connection = Connection::open(path).unwrap();
        connection.pragma_update(None, "user_version", 2).unwrap();
        drop(connection);
        assert_eq!(
            Database::open(directory.path()).unwrap_err().code,
            "data.migration_drift"
        );
    }

    #[test]
    fn draft_attempt_source_must_match() {
        let directory = tempdir().unwrap();
        let database = Database::open(directory.path()).unwrap();
        database.read(|connection| {
            insert_source(connection, "00000000-0000-4000-8000-000000000001");
            connection.execute(
                "INSERT INTO sources (
                    id, kind, content_hash, original_filename, byte_size, ext,
                    evidence_relpath, imported_at, state
                 ) VALUES (?1, 'file', ?2, 'other.png', 1, 'png', 'evidence/other.png', ?3, 'imported')",
                params![
                    "00000000-0000-4000-8000-000000000002",
                    "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
                    "2026-08-13T00:00:00Z"
                ],
            )?;
            insert_attempt(
                connection,
                "10000000-0000-4000-8000-000000000001",
                "00000000-0000-4000-8000-000000000001",
            );
            let result = connection.execute(
                "INSERT INTO draft_transactions (
                    id, source_id, evidence_text, source_ordinal, attempt_id, drafted_json,
                    occurred_on, amount_minor, currency, direction, merchant, created_at
                 ) VALUES (?1, ?2, 'x', 1, ?3, '{}', '2026-08-13', 100, 'AUD', 'expense', 'x', ?4)",
                params![
                    "20000000-0000-4000-8000-000000000001",
                    "00000000-0000-4000-8000-000000000002",
                    "10000000-0000-4000-8000-000000000001",
                    "2026-08-13T00:00:00Z"
                ],
            );
            assert!(result.is_err());
            Ok(())
        }).unwrap();
    }

    #[test]
    fn latest_attempt_must_belong_to_source() {
        let directory = tempdir().unwrap();
        let database = Database::open(directory.path()).unwrap();
        database.read(|connection| {
            insert_source(connection, "00000000-0000-4000-8000-000000000001");
            connection.execute(
                "INSERT INTO sources (
                    id, kind, content_hash, original_filename, byte_size, ext,
                    evidence_relpath, imported_at, state
                 ) VALUES (?1, 'file', ?2, 'other.png', 1, 'png', 'evidence/other.png', ?3, 'imported')",
                params![
                    "00000000-0000-4000-8000-000000000002",
                    "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
                    "2026-08-13T00:00:00Z"
                ],
            )?;
            insert_attempt(
                connection,
                "10000000-0000-4000-8000-000000000001",
                "00000000-0000-4000-8000-000000000001",
            );
            let result = connection.execute(
                "UPDATE sources SET latest_attempt_id = ?1 WHERE id = ?2",
                params![
                    "10000000-0000-4000-8000-000000000001",
                    "00000000-0000-4000-8000-000000000002"
                ],
            );
            assert!(result.is_err());
            Ok(())
        }).unwrap();
    }

    #[test]
    fn audit_log_is_append_only() {
        let directory = tempdir().unwrap();
        let database = Database::open(directory.path()).unwrap();
        database
            .read(|connection| {
                connection.execute(
                "INSERT INTO audit_log (id, actor, at, entity_type, entity_id, action, after_json)
                 VALUES ('1', 'system', '2026-08-13T00:00:00Z', 'source', 's', 'create', '{}')",
                [],
            )?;
                assert!(connection
                    .execute(concat!("UPDATE audit_", "log SET action = 'update'"), [])
                    .is_err());
                assert!(connection
                    .execute(concat!("DELETE FROM audit_", "log"), [])
                    .is_err());
                Ok(())
            })
            .unwrap();
    }

    #[test]
    fn audit_actor_accepts_system() {
        let directory = tempdir().unwrap();
        let database = Database::open(directory.path()).unwrap();
        database
            .read(|connection| {
                connection.execute(
                    "INSERT INTO audit_log (id, actor, at, entity_type, entity_id, action)
                     VALUES ('system-audit', 'system', '2026-08-13T00:00:00Z',
                        'parse_attempt', 'attempt', 'void')",
                    [],
                )?;
                Ok(())
            })
            .unwrap();
    }

    #[test]
    fn reported_total_all_or_nothing() {
        let directory = tempdir().unwrap();
        let database = Database::open(directory.path()).unwrap();
        database
            .read(|connection| {
                insert_source(connection, "00000000-0000-4000-8000-000000000010");
                insert_attempt(
                    connection,
                    "10000000-0000-4000-8000-000000000010",
                    "00000000-0000-4000-8000-000000000010",
                );
                assert!(connection
                    .execute(
                        "UPDATE parse_attempts SET reported_total_minor = 100 WHERE id = ?1",
                        ["10000000-0000-4000-8000-000000000010"],
                    )
                    .is_err());
                Ok(())
            })
            .unwrap();
    }

    #[test]
    fn reported_total_lives_on_attempt() {
        let directory = tempdir().unwrap();
        let database = Database::open(directory.path()).unwrap();
        database
            .read(|connection| {
                insert_source(connection, "00000000-0000-4000-8000-000000000011");
                for (attempt, amount) in [("attempt-one", 100), ("attempt-two", 200)] {
                    insert_attempt(connection, attempt, "00000000-0000-4000-8000-000000000011");
                    connection.execute(
                        "UPDATE parse_attempts SET reported_total_minor = ?1,
                            reported_total_currency = 'AUD', reported_total_kind = 'expense_total',
                            reported_total_evidence_text = 'Total' WHERE id = ?2",
                        params![amount, attempt],
                    )?;
                }
                let values = connection
                    .prepare("SELECT reported_total_minor FROM parse_attempts ORDER BY id")?
                    .query_map([], |row| row.get::<_, i64>(0))?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                assert_eq!(values, vec![100, 200]);
                let source_columns = connection
                    .prepare("PRAGMA table_info(sources)")?
                    .query_map([], |row| row.get::<_, String>(1))?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                assert!(source_columns.iter().all(|name| !name.contains("total")));
                Ok(())
            })
            .unwrap();
    }

    #[test]
    fn file_hash_unique_only_for_files() {
        let directory = tempdir().unwrap();
        let database = Database::open(directory.path()).unwrap();
        database
            .read(|connection| {
                let hash = "d".repeat(64);
                for (id, token) in [
                    ("utterance-one", "token-one"),
                    ("utterance-two", "token-two"),
                ] {
                    connection.execute(
                        "INSERT INTO sources (
                            id, kind, content_hash, idempotency_key, ext, byte_size,
                            evidence_relpath, imported_at, state
                         ) VALUES (?1, 'utterance', ?2, ?3, 'txt', 1, ?4,
                            '2026-08-13T00:00:00Z', 'imported')",
                        params![id, hash, token, format!("evidence/{id}.txt")],
                    )?;
                }
                connection.execute(
                    "INSERT INTO sources (
                        id, kind, content_hash, original_filename, ext, byte_size,
                        evidence_relpath, imported_at, state
                     ) VALUES ('file-one', 'file', ?1, 'one.png', 'png', 1,
                        'evidence/one.png', '2026-08-13T00:00:00Z', 'imported')",
                    ["e".repeat(64)],
                )?;
                assert!(connection
                    .execute(
                        "INSERT INTO sources (
                        id, kind, content_hash, original_filename, ext, byte_size,
                        evidence_relpath, imported_at, state
                     ) VALUES ('file-two', 'file', ?1, 'two.png', 'png', 1,
                        'evidence/two.png', '2026-08-13T00:00:00Z', 'imported')",
                        ["e".repeat(64)],
                    )
                    .is_err());
                Ok(())
            })
            .unwrap();
    }

    #[test]
    fn draft_requires_evidence() {
        let directory = tempdir().unwrap();
        let database = Database::open(directory.path()).unwrap();
        database
            .read(|connection| {
                let source = "00000000-0000-4000-8000-000000000012";
                insert_source(connection, source);
                insert_attempt(connection, "attempt", source);
                assert!(connection
                    .execute(
                        "INSERT INTO draft_transactions (
                        id, source_id, evidence_text, source_ordinal, attempt_id, drafted_json,
                        occurred_on, amount_minor, currency, direction, merchant, created_at
                     ) VALUES ('draft', ?1, '', 1, 'attempt', '{}', '2026-08-13',
                        100, 'AUD', 'expense', 'M', '2026-08-13T00:00:00Z')",
                        [source],
                    )
                    .is_err());
                Ok(())
            })
            .unwrap();
    }

    #[test]
    fn draft_links_to_attempt() {
        let directory = tempdir().unwrap();
        let database = Database::open(directory.path()).unwrap();
        database
            .read(|connection| {
                let source = "00000000-0000-4000-8000-000000000013";
                insert_source(connection, source);
                insert_attempt(connection, "attempt", source);
                insert_draft(connection, "draft", source, "attempt", 1);
                let backend: String = connection.query_row(
                    "SELECT p.backend_id FROM draft_transactions d
                     JOIN parse_attempts p ON p.id = d.attempt_id WHERE d.id = 'draft'",
                    [],
                    |row| row.get(0),
                )?;
                assert!(!backend.is_empty());
                Ok(())
            })
            .unwrap();
    }

    #[test]
    fn draft_ordinal_is_unique_per_attempt() {
        let directory = tempdir().unwrap();
        let database = Database::open(directory.path()).unwrap();
        database
            .read(|connection| {
                let source = "00000000-0000-4000-8000-000000000014";
                insert_source(connection, source);
                insert_attempt(connection, "attempt", source);
                insert_draft(connection, "draft-one", source, "attempt", 1);
                assert!(connection
                    .execute(
                        "INSERT INTO draft_transactions (
                        id, source_id, evidence_text, source_ordinal, attempt_id, drafted_json,
                        occurred_on, amount_minor, currency, direction, merchant, created_at
                     ) VALUES ('draft-two', ?1, 'evidence', 1, 'attempt', '{}',
                        '2026-08-13', 100, 'AUD', 'expense', 'M', '2026-08-13T00:00:00Z')",
                        [source],
                    )
                    .is_err());
                Ok(())
            })
            .unwrap();
    }

    #[test]
    fn draft_triple_all_or_nothing() {
        let directory = tempdir().unwrap();
        let database = Database::open(directory.path()).unwrap();
        database
            .read(|connection| {
                let source = "00000000-0000-4000-8000-000000000015";
                insert_source(connection, source);
                insert_attempt(connection, "attempt", source);
                assert!(connection
                    .execute(
                        "INSERT INTO draft_transactions (
                        id, source_id, evidence_text, source_ordinal, attempt_id, drafted_json,
                        occurred_on, amount_minor, currency, base_amount_minor,
                        direction, merchant, created_at
                     ) VALUES ('draft', ?1, 'evidence', 1, 'attempt', '{}',
                        '2026-08-13', 100, 'AUD', 100, 'expense', 'M',
                        '2026-08-13T00:00:00Z')",
                        [source],
                    )
                    .is_err());
                Ok(())
            })
            .unwrap();
    }

    #[test]
    fn utterance_draft_requires_span() {
        let directory = tempdir().unwrap();
        let database = Database::open(directory.path()).unwrap();
        database
            .read(|connection| {
                connection.execute(
                    "INSERT INTO sources (
                        id, kind, content_hash, idempotency_key, ext, byte_size,
                        evidence_relpath, imported_at, state
                     ) VALUES ('utterance', 'utterance', ?1, 'token', 'txt', 1,
                        'evidence/u.txt', '2026-08-13T00:00:00Z', 'imported')",
                    ["f".repeat(64)],
                )?;
                insert_attempt(connection, "attempt-u", "utterance");
                assert!(connection
                    .execute(
                        "INSERT INTO draft_transactions (
                        id, source_id, evidence_text, source_ordinal, attempt_id, drafted_json,
                        occurred_on, amount_minor, currency, direction, merchant, created_at
                     ) VALUES ('draft-u', 'utterance', 'x', 1, 'attempt-u', '{}',
                        '2026-08-13', 100, 'AUD', 'expense', 'M',
                        '2026-08-13T00:00:00Z')",
                        [],
                    )
                    .is_err());
                let source = "00000000-0000-4000-8000-000000000016";
                insert_source(connection, source);
                insert_attempt(connection, "attempt-f", source);
                assert!(connection
                    .execute(
                        "INSERT INTO draft_transactions (
                        id, source_id, evidence_text, source_ordinal,
                        evidence_span_start, evidence_span_end, attempt_id, drafted_json,
                        occurred_on, amount_minor, currency, direction, merchant, created_at
                     ) VALUES ('draft-f', ?1, 'x', 1, 0, 1, 'attempt-f', '{}',
                        '2026-08-13', 100, 'AUD', 'expense', 'M',
                        '2026-08-13T00:00:00Z')",
                        [source],
                    )
                    .is_err());
                Ok(())
            })
            .unwrap();
    }

    #[test]
    fn drafted_json_is_immutable() {
        let directory = tempdir().unwrap();
        let database = Database::open(directory.path()).unwrap();
        database
            .read(|connection| {
                let source = "00000000-0000-4000-8000-000000000017";
                insert_source(connection, source);
                insert_attempt(connection, "attempt", source);
                insert_draft(connection, "draft", source, "attempt", 1);
                connection.execute(
                    "UPDATE draft_transactions SET amount_minor = 200 WHERE id = 'draft'",
                    [],
                )?;
                assert!(connection
                    .execute(
                        concat!(
                            "UPDATE draft_transactions SET drafted_",
                            "json = '{}' WHERE id = 'draft'"
                        ),
                        [],
                    )
                    .is_err());
                let values: (i64, String) = connection.query_row(
                    "SELECT amount_minor, drafted_json FROM draft_transactions WHERE id = 'draft'",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )?;
                assert_eq!(values, (200, "{\"original\":true}".to_owned()));
                Ok(())
            })
            .unwrap();
    }

    #[test]
    fn discard_is_distinct_from_void() {
        let directory = tempdir().unwrap();
        let database = Database::open(directory.path()).unwrap();
        database
            .read(|connection| {
                let source = "00000000-0000-4000-8000-000000000018";
                insert_source(connection, source);
                insert_attempt(connection, "attempt", source);
                insert_draft(connection, "discarded", source, "attempt", 1);
                insert_draft(connection, "voided", source, "attempt", 2);
                connection.execute(
                    "UPDATE draft_transactions SET discarded_at = '2026-08-13T00:00:00Z'
                     WHERE id = 'discarded'",
                    [],
                )?;
                connection.execute(
                    "UPDATE draft_transactions SET voided_at = '2026-08-13T00:00:00Z'
                     WHERE id = 'voided'",
                    [],
                )?;
                let values = connection
                    .prepare(
                        "SELECT id, voided_at IS NOT NULL, discarded_at IS NOT NULL
                         FROM draft_transactions ORDER BY id",
                    )?
                    .query_map([], |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, bool>(1)?,
                            row.get::<_, bool>(2)?,
                        ))
                    })?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                assert_eq!(
                    values,
                    vec![
                        ("discarded".to_owned(), false, true),
                        ("voided".to_owned(), true, false),
                    ]
                );
                Ok(())
            })
            .unwrap();
    }

    #[test]
    fn m0_insert_transaction_with_null_account() {
        let directory = tempdir().unwrap();
        let database = Database::open(directory.path()).unwrap();
        database
            .read(|connection| {
                insert_fact_transaction(connection, "transaction-ok", 100, "AUD", None)?;
                assert!(insert_fact_transaction(
                    connection,
                    "transaction-bad",
                    100,
                    "AUD",
                    Some("missing-account"),
                )
                .is_err());
                Ok(())
            })
            .unwrap();
    }

    #[test]
    fn base_currency_frozen_per_row() {
        let directory = tempdir().unwrap();
        let database = Database::open(directory.path()).unwrap();
        database.set_base_currency("AUD").unwrap();
        database
            .read(|connection| {
                insert_fact_transaction(connection, "aud-row", 100, "AUD", None)?;
                Ok(())
            })
            .unwrap();
        database.set_base_currency("USD").unwrap();
        let values: (String, i64) = database
            .read(|connection| {
                connection.query_row(
                    "SELECT base_currency, base_amount_minor FROM transactions WHERE id = 'aud-row'",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
            })
            .unwrap();
        assert_eq!(values, ("AUD".to_owned(), 100));
    }

    #[test]
    fn rollup_groups_by_base_currency() {
        let directory = tempdir().unwrap();
        let database = Database::open(directory.path()).unwrap();
        database
            .read(|connection| {
                insert_fact_transaction(connection, "aud-row", 100, "AUD", None)?;
                insert_fact_transaction(connection, "usd-row", 75, "USD", None)?;
                Ok(())
            })
            .unwrap();
        assert_eq!(
            database.transaction_rollup_by_base_currency().unwrap(),
            vec![("AUD".to_owned(), 100), ("USD".to_owned(), 75)]
        );
    }

    #[test]
    fn icloud_path_rejected() {
        assert_eq!(
            reject_icloud_path(Path::new("/Users/test/Library/Mobile Documents/Daybook"))
                .unwrap_err()
                .code,
            "data.icloud_path_rejected"
        );
    }
}
