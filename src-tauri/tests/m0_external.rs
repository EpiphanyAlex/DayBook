use std::{process::Stdio, sync::Arc};

use daybook_lib::{
    agent::{
        registry::m0_tool_registry,
        session::{AgentSession, SessionMode, SOCKET_ENV, TOKEN_ENV},
    },
    db::Database,
    domain::draft::Assignment,
};
use rusqlite::params;
use serde_json::{json, Value};
use tempfile::tempdir;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{ChildStdin, ChildStdout, Command},
};
use uuid::Uuid;

async fn request(
    stdin: &mut ChildStdin,
    stdout: &mut BufReader<ChildStdout>,
    id: i64,
    method: &str,
    params: Value,
) -> Value {
    let mut bytes = serde_json::to_vec(&json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": method,
        "params": params,
    }))
    .unwrap();
    bytes.push(b'\n');
    stdin.write_all(&bytes).await.unwrap();
    stdin.flush().await.unwrap();
    loop {
        let mut line = String::new();
        assert!(stdout.read_line(&mut line).await.unwrap() > 0);
        let message: Value = serde_json::from_str(&line).unwrap();
        if message["id"] == id {
            assert!(message.get("error").is_none(), "{message}");
            return message["result"].clone();
        }
    }
}

async fn notification(stdin: &mut ChildStdin, method: &str) {
    let mut bytes = serde_json::to_vec(&json!({
        "jsonrpc": "2.0",
        "method": method,
    }))
    .unwrap();
    bytes.push(b'\n');
    stdin.write_all(&bytes).await.unwrap();
    stdin.flush().await.unwrap();
}

#[tokio::test]
#[ignore = "由 scripts/verify-m0.mjs 运行；需要绑定本机 Unix socket"]
async fn scripted_stdio_mcp_uds_roundtrip() {
    let directory = tempdir().unwrap();
    let database = Arc::new(Database::open(directory.path()).unwrap());
    let source_id = Uuid::new_v4().to_string();
    let attempt_id = Uuid::new_v4().to_string();
    let relative = format!("evidence/{source_id}.txt");
    std::fs::write(database.root().join(&relative), "咖啡 5 澳元，总共 5").unwrap();
    database
        .write(|transaction| {
            transaction.execute(
                "INSERT INTO sources (
                    id, kind, content_hash, idempotency_key, ext, byte_size,
                    evidence_relpath, imported_at, state
                 ) VALUES (?1, 'utterance', ?2, 'scripted-token', 'txt', 29, ?3,
                    '2026-08-13T00:00:00Z', 'parsing')",
                params![source_id, "a".repeat(64), relative],
            )?;
            transaction.execute(
                "INSERT INTO parse_attempts (
                    id, source_id, agent_session_id, backend_id, prompt_hash,
                    tool_surface_version, effective_capability_hash, app_version, started_at
                 ) VALUES (?1, ?2, 'scripted-session', 'scripted', ?3,
                    'm0-scripted', ?3, '0.1.0', '2026-08-13T00:00:00Z')",
                params![attempt_id, source_id, "b".repeat(64)],
            )?;
            transaction.execute(
                "UPDATE sources SET latest_attempt_id = ?1 WHERE id = ?2",
                params![attempt_id, source_id],
            )?;
            Ok(())
        })
        .unwrap();

    let session = AgentSession::start(
        Arc::clone(&database),
        SessionMode::Task(Assignment {
            source_id: source_id.clone(),
            attempt_id: attempt_id.clone(),
        }),
    )
    .await
    .unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_daybook-mcp"))
        .env(SOCKET_ENV, session.socket_path())
        .env(TOKEN_ENV, session.token())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    let mut stdin = child.stdin.take().unwrap();
    request(
        &mut stdin,
        &mut stdout,
        1,
        "initialize",
        json!({
            "protocolVersion": "2025-11-25",
            "capabilities": {},
            "clientInfo": { "name": "daybook-scripted", "version": "0.1.0" }
        }),
    )
    .await;
    notification(&mut stdin, "notifications/initialized").await;

    let listed = request(&mut stdin, &mut stdout, 2, "tools/list", json!({})).await;
    let actual = listed["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    let expected = m0_tool_registry()
        .iter()
        .map(|tool| tool.name)
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(actual, expected);

    for (index, (name, value)) in [
        ("list_pending_sources", json!({})),
        ("read_source", json!({ "sourceId": source_id })),
        (
            "draft_transaction",
            json!({
                "sourceId": source_id,
                "evidenceText": "咖啡 5 澳元",
                "sourceOrdinal": 1,
                "evidenceSpan": { "start": 0, "end": 7 },
                "occurredOn": "2026-08-13",
                "amountMinor": "500",
                "currency": "AUD",
                "baseAmountMinor": "500",
                "baseCurrency": "AUD",
                "ratePpm": "1000000",
                "direction": "expense",
                "merchant": "咖啡",
                "category": "餐饮",
                "channel": null,
                "confidence": 100
            }),
        ),
        (
            "report_source_total",
            json!({
                "sourceId": source_id,
                "amountMinor": "500",
                "currency": "AUD",
                "kind": "expense_total",
                "evidenceText": "总共 5"
            }),
        ),
        (
            "complete_source",
            json!({ "sourceId": source_id, "itemCount": 1, "unparsedNote": "" }),
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let response = request(
            &mut stdin,
            &mut stdout,
            i64::try_from(index).unwrap() + 3,
            "tools/call",
            json!({ "name": name, "arguments": value }),
        )
        .await;
        assert_ne!(response["isError"], true, "{name}: {response}");
    }
    assert!(session.is_complete());
    stdin.shutdown().await.unwrap();
    drop(stdin);
    let _ = child.wait().await;

    let (drafts, audits, transactions, reported_count): (i64, i64, i64, i64) = database
        .read(|connection| {
            connection.query_row(
                "SELECT
                    (SELECT COUNT(*) FROM draft_transactions WHERE attempt_id = ?1),
                    (SELECT COUNT(*) FROM audit_log WHERE actor = 'agent'),
                    (SELECT COUNT(*) FROM transactions),
                    (SELECT reported_item_count FROM parse_attempts WHERE id = ?1)",
                [&attempt_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
        })
        .unwrap();
    assert_eq!((drafts, audits, transactions, reported_count), (1, 2, 0, 1));
}
