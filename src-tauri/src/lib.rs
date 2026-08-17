pub mod agent;
pub mod db;
pub mod domain;
pub mod error;
pub mod eval;
pub mod ingest;
pub mod mcp;
pub mod money;

use std::sync::Arc;

use agent::{backend::BackendStatus, runtime::AgentRuntime};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use db::{Database, FoundationStatus};
use domain::confirm::{
    BatchConfirmResult, ConfirmResult, DraftPatch, ReviewDraft, ReviewSource, TotalCheck,
    UtteranceAttestation,
};
use error::AppResult;
use ingest::ImportResult;
use serde::Serialize;
use tauri::Manager;

#[derive(Debug)]
struct AppState {
    database: Arc<Database>,
    agent: Arc<AgentRuntime>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct EvidenceContent {
    kind: String,
    mime_type: String,
    data_base64: Option<String>,
    text: Option<String>,
}

#[tauri::command]
fn foundation_status(state: tauri::State<'_, AppState>) -> AppResult<FoundationStatus> {
    state.database.status()
}

#[tauri::command]
fn set_base_currency(
    state: tauri::State<'_, AppState>,
    currency: String,
) -> AppResult<FoundationStatus> {
    state.database.set_base_currency(&currency)?;
    state.database.status()
}

#[tauri::command]
fn set_debug_logging(
    state: tauri::State<'_, AppState>,
    enabled: bool,
) -> AppResult<FoundationStatus> {
    state.database.set_debug_logging(enabled)?;
    state.database.status()
}

#[tauri::command]
fn reveal_data_directory(state: tauri::State<'_, AppState>) -> AppResult<()> {
    let status = std::process::Command::new("/usr/bin/open")
        .arg("-R")
        .arg(state.database.root())
        .status()?;
    if !status.success() {
        return Err(error::AppError::storage(format!(
            "访达未能显示数据目录（open exit status: {status}）"
        )));
    }
    Ok(())
}

#[tauri::command]
fn agent_status(state: tauri::State<'_, AppState>) -> BackendStatus {
    state.agent.status()
}

#[tauri::command]
async fn probe_agent(state: tauri::State<'_, AppState>) -> AppResult<BackendStatus> {
    state.agent.probe(Arc::clone(&state.database)).await?;
    let mut status = state.agent.status();
    status.authenticated = Some(true);
    status.error_code = None;
    Ok(status)
}

#[tauri::command]
fn import_source_file(state: tauri::State<'_, AppState>, path: String) -> AppResult<ImportResult> {
    ingest::import_file(&state.database, path)
}

#[tauri::command]
fn submit_utterance(
    state: tauri::State<'_, AppState>,
    text: String,
    idempotency_key: String,
) -> AppResult<ImportResult> {
    ingest::import_utterance(&state.database, &text, &idempotency_key)
}

#[tauri::command]
async fn parse_source(
    state: tauri::State<'_, AppState>,
    source_id: String,
) -> AppResult<agent::runtime::AttemptSummary> {
    state
        .agent
        .parse_source(Arc::clone(&state.database), source_id)
        .await
}

#[tauri::command]
async fn cancel_parsing(state: tauri::State<'_, AppState>) -> AppResult<bool> {
    state.agent.cancel().await
}

#[tauri::command]
fn recent_agent_logs(state: tauri::State<'_, AppState>) -> AppResult<Vec<String>> {
    agent::runtime::recent_agent_logs(&state.database, 30)
}

#[tauri::command]
fn list_review_sources(state: tauri::State<'_, AppState>) -> AppResult<Vec<ReviewSource>> {
    domain::confirm::list_review_sources(&state.database)
}

#[tauri::command]
fn list_active_drafts(
    state: tauri::State<'_, AppState>,
    source_id: String,
) -> AppResult<Vec<ReviewDraft>> {
    domain::confirm::list_active_drafts(&state.database, &source_id)
}

#[tauri::command]
fn check_source_total(
    state: tauri::State<'_, AppState>,
    attempt_id: String,
) -> AppResult<TotalCheck> {
    domain::confirm::total_check(&state.database, &attempt_id)
}

#[tauri::command]
fn confirm_draft(state: tauri::State<'_, AppState>, draft_id: String) -> AppResult<ConfirmResult> {
    domain::confirm::confirm_one(&state.database, &draft_id)
}

#[tauri::command]
fn confirm_drafts(
    state: tauri::State<'_, AppState>,
    draft_ids: Vec<String>,
    attestation: Option<UtteranceAttestation>,
) -> AppResult<BatchConfirmResult> {
    domain::confirm::confirm_batch(&state.database, &draft_ids, attestation.as_ref())
}

#[tauri::command]
fn update_draft(
    state: tauri::State<'_, AppState>,
    draft_id: String,
    patch: DraftPatch,
) -> AppResult<()> {
    domain::confirm::edit_draft(&state.database, &draft_id, &patch)
}

#[tauri::command]
fn discard_draft(state: tauri::State<'_, AppState>, draft_id: String) -> AppResult<()> {
    domain::confirm::discard_draft(&state.database, &draft_id)
}

#[tauri::command]
fn read_evidence(
    state: tauri::State<'_, AppState>,
    source_id: String,
) -> AppResult<EvidenceContent> {
    let (kind, ext, relative_path) = state.database.read(|connection| {
        connection.query_row(
            "SELECT kind, ext, evidence_relpath FROM sources WHERE id = ?1",
            [&source_id],
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
            std::path::Component::ParentDir
                | std::path::Component::RootDir
                | std::path::Component::Prefix(_)
        )
    }) {
        return Err(error::AppError::storage("证据路径越出数据目录"));
    }
    let bytes = std::fs::read(state.database.root().join(relative))?;
    if kind == "utterance" {
        let text = String::from_utf8(bytes)
            .map_err(|error| error::AppError::storage(format!("口述证据不是 UTF-8：{error}")))?;
        Ok(EvidenceContent {
            kind,
            mime_type: "text/plain; charset=utf-8".to_owned(),
            data_base64: None,
            text: Some(text),
        })
    } else {
        let mime_type = if ext == "png" {
            "image/png"
        } else {
            "image/jpeg"
        };
        Ok(EvidenceContent {
            kind,
            mime_type: mime_type.to_owned(),
            data_base64: Some(BASE64.encode(bytes)),
            text: None,
        })
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .setup(|app| {
            let data_directory = app.path().data_dir()?.join("Daybook");
            let database = Arc::new(Database::open(data_directory)?);
            agent::runtime::cleanup_expired_logs(&database)?;
            ingest::recover_interrupted(&database)?;
            app.manage(AppState {
                database,
                agent: Arc::new(AgentRuntime::claude_default()),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            foundation_status,
            set_base_currency,
            set_debug_logging,
            reveal_data_directory,
            agent_status,
            probe_agent,
            import_source_file,
            submit_utterance,
            parse_source,
            cancel_parsing,
            recent_agent_logs,
            list_review_sources,
            list_active_drafts,
            check_source_total,
            confirm_draft,
            confirm_drafts,
            update_draft,
            discard_draft,
            read_evidence,
        ])
        .build(tauri::generate_context!())
        .expect("Daybook 启动失败");
    app.run(|app_handle, event| {
        if matches!(event, tauri::RunEvent::ExitRequested { .. }) {
            let state = app_handle.state::<AppState>();
            let _ = tauri::async_runtime::block_on(state.agent.shutdown());
        }
    });
}

#[cfg(test)]
mod m0 {
    use std::sync::Arc;

    use async_trait::async_trait;
    use serde_json::json;
    use tempfile::tempdir;
    use tokio::sync::watch;

    use super::*;
    use crate::{
        agent::{
            backend::{AgentBackend, AgentTask, AgentTaskResult, ProbeResult},
            registry::{effective_capability_hash, expected_capabilities},
        },
        domain::draft::{Assignment, DraftStore},
    };

    // `src-tauri` 有两个 bin（daybook / daybook-mcp）。缺 `default-run` 时 `cargo run` 不知道
    // 该起哪个，`npm run tauri dev` 与 `npm run tauri build` 都停在
    // 「could not determine which binary to run」——**桌面应用一次都没起来过，而门禁全绿**。
    #[test]
    fn cargo_manifest_declares_default_run() {
        let manifest = include_str!("../Cargo.toml");
        assert!(
            manifest.contains("default-run = \"daybook\""),
            "两个 bin 并存时必须声明 default-run，否则 tauri dev / tauri build 起不来"
        );
    }

    // `tauri-build` 把 icons/icon.png 解码成裸 RGBA 编进二进制，运行时 tauri 拿字节数除以 4
    // 反推像素数。图标若是 16 位/通道，解码结果是 8 字节/像素，两边差一倍——应用在
    // `did_finish_launching` 里 panic，一个窗口都开不出来。
    // 2026-08-13 M0 人工验收实测：当时的 icon.png 正是 16 位，`npm run tauri dev` 直接 abort，
    // 而全部 cargo test 与七条 CI 门禁都是绿的——它们从不启动应用。
    #[test]
    fn app_icon_decodes_to_eight_bit_rgba() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/icons/icon.png");
        let reader = png::Decoder::new(std::fs::File::open(path).unwrap())
            .read_info()
            .unwrap();
        let info = reader.info();
        assert_eq!(info.color_type, png::ColorType::Rgba, "应用图标必须是 RGBA");
        assert_eq!(
            info.bit_depth,
            png::BitDepth::Eight,
            "应用图标必须是 8 位/通道"
        );
        let expected = info.width as usize * info.height as usize * 4;
        assert_eq!(
            reader.output_buffer_size(),
            expected,
            "解码后必须正好 4 字节/像素，否则 tauri 启动时判定图标无效"
        );
    }

    struct DeterministicBackend;

    #[async_trait]
    impl AgentBackend for DeterministicBackend {
        fn id(&self) -> &'static str {
            "deterministic"
        }

        fn status(&self) -> BackendStatus {
            BackendStatus {
                backend_id: self.id().to_owned(),
                executable: None,
                version: Some("1".to_owned()),
                available: true,
                authenticated: Some(true),
                error_code: None,
            }
        }

        async fn probe(&self, _database: Arc<Database>) -> AppResult<ProbeResult> {
            let manifest = expected_capabilities();
            Ok(ProbeResult {
                backend_id: self.id().to_owned(),
                backend_version: "1".to_owned(),
                effective_capability_hash: effective_capability_hash(&manifest),
                sealed_config_fingerprint: "deterministic-sealed".to_owned(),
                manifest,
            })
        }

        async fn run_task(
            &self,
            database: Arc<Database>,
            task: AgentTask,
            _cancel: watch::Receiver<bool>,
        ) -> AppResult<AgentTaskResult> {
            let store = DraftStore::for_task(
                database,
                Assignment {
                    source_id: task.source_id.clone(),
                    attempt_id: task.attempt_id,
                },
            );
            store.handle("read_source", json!({ "sourceId": task.source_id }))?;
            store.handle(
                "draft_transaction",
                json!({
                    "sourceId": task.source_id,
                    "evidenceText": "咖啡 5 澳元",
                    "sourceOrdinal": 1,
                    "evidenceSpan": { "start": 0, "end": 7 },
                    "occurredOn": "2026-08-12",
                    "amountMinor": "500",
                    "currency": "AUD",
                    "baseAmountMinor": "500",
                    "baseCurrency": "AUD",
                    "ratePpm": "1000000",
                    "direction": "expense",
                    "merchant": "咖啡",
                    "category": "餐饮",
                    "channel": null,
                    "confidence": 96
                }),
            )?;
            store.handle(
                "report_source_total",
                json!({
                    "sourceId": task.source_id,
                    "amountMinor": "500",
                    "currency": "AUD",
                    "kind": "expense_total",
                    "evidenceText": "总共 5"
                }),
            )?;
            store.handle(
                "complete_source",
                json!({
                    "sourceId": task.source_id,
                    "itemCount": 1,
                    "unparsedNote": ""
                }),
            )?;
            Ok(AgentTaskResult {
                success: store.is_complete(),
                model_id: Some("deterministic-model".to_owned()),
                agent_session_id: Some(task.agent_session_id),
                stdout: String::new(),
                stderr: String::new(),
                trace_events: store.trace_events(),
                debug_events: store.debug_events(),
            })
        }
    }

    #[tokio::test]
    async fn deterministic_end_to_end() {
        let directory = tempdir().unwrap();
        let database = Arc::new(Database::open(directory.path()).unwrap());
        database.set_base_currency("AUD").unwrap();
        database.set_debug_logging(false).unwrap();
        let imported = ingest::import_utterance(
            &database,
            "咖啡 5 澳元，总共 5",
            "00000000-0000-4000-8000-000000000001",
        )
        .unwrap();
        let runtime = AgentRuntime::new(Arc::new(DeterministicBackend));
        let summary = runtime
            .parse_source(Arc::clone(&database), imported.source_id.clone())
            .await
            .unwrap();
        assert_eq!(summary.outcome, "completed");
        let check = domain::confirm::total_check(&database, &summary.attempt_id).unwrap();
        assert_eq!(check.reconciliation_status, "passed");
        assert_eq!(check.confirmation_policy, "user_attested_batch");
        let drafts = domain::confirm::list_active_drafts(&database, &imported.source_id).unwrap();
        assert_eq!(drafts.len(), 1);
        let confirmed = domain::confirm::confirm_batch(
            &database,
            &[drafts[0].id.clone()],
            Some(&UtteranceAttestation {
                full_source_visible: true,
                results_adjacent: true,
                item_count_visible: true,
            }),
        )
        .unwrap();
        assert_eq!(confirmed.confirmed.len(), 1);
        assert!(confirmed.rejected.is_empty());
        let (transactions, source_state, evidence, trace): (i64, String, String, i64) = database
            .read(|connection| {
                connection.query_row(
                    "SELECT COUNT(*), MAX(s.state), MAX(t.evidence_text),
                        (SELECT COUNT(*) FROM audit_log a
                         WHERE a.action = 'confirm')
                     FROM transactions t JOIN sources s ON s.id = t.source_id",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                )
            })
            .unwrap();
        assert_eq!(transactions, 1);
        assert_eq!(source_state, "reviewed");
        assert_eq!(evidence, "咖啡 5 澳元");
        assert_eq!(trace, 1);
    }
}
