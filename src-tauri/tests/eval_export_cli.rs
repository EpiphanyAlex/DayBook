//! `daybook-eval export-fixture` 的命令行级验收（`docs/prd/07-eval.md` §3.6 / §6）。
//!
//! 库里的 `eval::export::eval::*` 测的是打包逻辑；**这一份测的是子命令真的接上了**——
//! 参数解析、默认输出路径、退出码。放在 `tests/` 是因为只有集成测试拿得到
//! `CARGO_BIN_EXE_daybook-eval`（同 `tests/m0_external.rs` 用 helper 二进制的理由）。

use std::{path::Path, process::Command, sync::Arc};

use daybook_lib::{
    agent::registry::tool_surface_version,
    db::Database,
    domain::draft::{Assignment, DraftStore},
    ingest,
};
use serde_json::{json, Value};
use uuid::Uuid;

/// 造一个「用过一次」的数据目录：一条口述来源、一次解析尝试、两条草稿、一份 debug 日志。
fn seed(data_root: &Path) -> String {
    let database = Arc::new(Database::open(data_root).unwrap());
    database.set_base_currency("AUD").unwrap();
    let transcript = "咖啡 5 澳元，三明治 12 澳元，总共 17 澳元";
    let imported = ingest::import_utterance(&database, transcript, "cli-export").unwrap();

    let attempt_id = Uuid::new_v4().to_string();
    let agent_session_id = Uuid::new_v4().to_string();
    database
        .write(|transaction| {
            ingest::apply_transition(
                transaction,
                &imported.source_id,
                ingest::SourceState::Parsing,
                None,
            )?;
            transaction.execute(
                "INSERT INTO parse_attempts (
                    id, source_id, agent_session_id, backend_id, backend_version, model_id,
                    prompt_hash, tool_surface_version, effective_capability_hash,
                    app_version, started_at
                 ) VALUES (?1, ?2, ?3, 'claude-code', '1.2.3', 'test-model',
                    ?4, ?5, ?4, '0.1.0', '2026-08-17T00:00:00Z')",
                rusqlite::params![
                    attempt_id,
                    imported.source_id,
                    agent_session_id,
                    "0".repeat(64),
                    tool_surface_version(),
                ],
            )?;
            transaction.execute(
                "UPDATE sources SET latest_attempt_id = ?1 WHERE id = ?2",
                rusqlite::params![attempt_id, imported.source_id],
            )?;
            Ok(())
        })
        .unwrap();

    let store = DraftStore::for_task(
        Arc::clone(&database),
        Assignment {
            source_id: imported.source_id.clone(),
            attempt_id: attempt_id.clone(),
        },
    );
    let span_of = |needle: &str| -> (i64, i64) {
        let byte_offset = transcript.find(needle).unwrap();
        let start = transcript[..byte_offset].chars().count() as i64;
        (start, start + needle.chars().count() as i64)
    };
    for (ordinal, claim, amount) in [(1, "咖啡 5 澳元", "500"), (2, "三明治 12 澳元", "1200")]
    {
        let (start, end) = span_of(claim);
        store
            .handle(
                "draft_transaction",
                json!({
                    "sourceId": imported.source_id,
                    "evidenceText": claim,
                    "sourceOrdinal": ordinal,
                    "evidenceSpan": { "start": start, "end": end },
                    "occurredOn": "2026-08-17",
                    "amountMinor": amount,
                    "currency": "AUD",
                    "baseAmountMinor": amount,
                    "baseCurrency": "AUD",
                    "ratePpm": "1000000",
                    "direction": "expense",
                    "merchant": "SHOP",
                    "category": null,
                    "channel": null,
                    "confidence": 90
                }),
            )
            .unwrap();
    }
    // 原文里有「总共 17 澳元」，01 §3.2 的合计词闸门要求先报合计再完成。
    store
        .handle(
            "report_source_total",
            json!({
                "sourceId": imported.source_id,
                "amountMinor": "1700",
                "currency": "AUD",
                "kind": "expense_total",
                "evidenceText": "总共 17 澳元"
            }),
        )
        .unwrap();
    store
        .handle(
            "complete_source",
            json!({ "sourceId": imported.source_id, "itemCount": 2, "unparsedNote": "" }),
        )
        .unwrap();
    database
        .write(|transaction| {
            transaction.execute(
                "UPDATE parse_attempts SET ended_at = '2026-08-17T00:01:00Z', outcome = 'completed'
                 WHERE id = ?1",
                [&attempt_id],
            )?;
            ingest::apply_transition(
                transaction,
                &imported.source_id,
                ingest::SourceState::Parsed,
                None,
            )?;
            Ok(())
        })
        .unwrap();

    let logs = data_root.join("logs");
    std::fs::create_dir_all(&logs).unwrap();
    let mut lines = vec![json!({
        "kind": "debug",
        "attemptId": attempt_id,
        "agentSessionId": agent_session_id,
        "prompt": "……",
    })
    .to_string()];
    lines.extend(
        store
            .debug_events()
            .iter()
            .map(std::string::ToString::to_string),
    );
    std::fs::write(
        logs.join(format!("{agent_session_id}.debug.jsonl")),
        format!("{}\n", lines.join("\n")),
    )
    .unwrap();

    agent_session_id
}

fn run(arguments: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_daybook-eval"))
        .args(arguments)
        .output()
        .expect("daybook-eval 必须能启动")
}

#[test]
fn export_fixture_subcommand_writes_a_replayable_directory() {
    let workspace = tempfile::tempdir().unwrap();
    let data_root = workspace.path().join("data");
    std::fs::create_dir_all(&data_root).unwrap();
    let session = seed(&data_root);

    let repository = workspace.path().join("repo");
    let output = run(&[
        "export-fixture",
        "--session",
        &session,
        "--data-dir",
        data_root.to_str().unwrap(),
        "--root",
        repository.to_str().unwrap(),
        "--slug",
        "cli-smoke",
    ]);
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    // 默认落 fixtures/local/<date>-<slug>/ —— 那一支不进 git（§3.7）。
    let local = repository.join("fixtures").join("local");
    let exported = std::fs::read_dir(&local)
        .unwrap()
        .next()
        .expect("应当建出一个夹具目录")
        .unwrap()
        .path();
    assert!(
        exported
            .file_name()
            .unwrap()
            .to_string_lossy()
            .ends_with("-cli-smoke"),
        "目录名应当是 <date>-<slug>：{}",
        exported.display()
    );
    for name in ["input.txt", "tool-calls.json", "expected.json", "env.json"] {
        assert!(exported.join(name).exists(), "缺 {name}");
    }

    // 预填的真值必须带 annotated: false，且提示要出现在 stdout 上——
    // 一条使用者看不见的警告等于没有。
    let expected: Value =
        serde_json::from_str(&std::fs::read_to_string(exported.join("expected.json")).unwrap())
            .unwrap();
    assert_eq!(expected["annotated"], json!(false));
    assert_eq!(expected["items"].as_array().unwrap().len(), 2);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("annotated"), "{stdout}");
    assert!(stdout.contains("模型给自己判卷"), "{stdout}");
}

#[test]
fn export_fixture_subcommand_refuses_the_committed_set() {
    let workspace = tempfile::tempdir().unwrap();
    let data_root = workspace.path().join("data");
    std::fs::create_dir_all(&data_root).unwrap();
    let session = seed(&data_root);

    let output = run(&[
        "export-fixture",
        "--session",
        &session,
        "--data-dir",
        data_root.to_str().unwrap(),
        "--out",
        workspace
            .path()
            .join("fixtures/ci/should-not-happen")
            .to_str()
            .unwrap(),
    ]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("fixtures/local/"), "{stderr}");
}

#[test]
fn export_fixture_subcommand_requires_a_session() {
    let output = run(&["export-fixture"]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("缺 --session"), "{stderr}");
    // 用法错误不该被冠上「manifest 不合法」——那会把人引到一个没问题的文件上。
    assert!(!stderr.contains("manifest 不合法"), "{stderr}");
}
