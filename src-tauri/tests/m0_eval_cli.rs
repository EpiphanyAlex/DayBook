//! M0 正式流程的命令行退出码回归。全部只读写临时 JSON，不加载 backend、不调用 agent。

use std::{path::Path, process::Command};

use daybook_lib::eval::m0::{Adjudication, AdjudicationFile, FormalEnvelope};
use serde_json::json;

fn run(arguments: &[&str]) -> std::process::Output {
    run_with(arguments, None)
}

fn run_with(arguments: &[&str], path: Option<&str>) -> std::process::Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_daybook-eval"));
    command.args(arguments);
    if let Some(path) = path {
        command.env("PATH", path);
    }
    command.output().expect("daybook-eval 必须能启动")
}

fn first(path: &Path, base_verdict: &str) {
    let envelope = FormalEnvelope {
        format_version: 1,
        mode: "m0_go_no_go".to_owned(),
        stage: "first".to_owned(),
        status: "incomplete".to_owned(),
        verdict: "incomplete".to_owned(),
        exit_code: 2,
        report_id: "report-1".to_owned(),
        created_at: "2026-08-24T00:00:00Z".to_owned(),
        manifest_path: "fixtures/local/m0/manifest.json".to_owned(),
        manifest_sha256: "0".repeat(64),
        adjudications_file: Some("first.adjudications.json".to_owned()),
        first_report_id: None,
        failed_reconciliation_case_ids: vec!["m0-screenshot-001".to_owned()],
        failed_case_ids: Vec::new(),
        flaky_case_ids: Vec::new(),
        base_verdict_without_manual: base_verdict.to_owned(),
        evaluation: json!({
            "decisionMetrics": [{
                "index": 5,
                "key": "false_alarm_rate",
                "label": "总额校验假警报率",
                "ratio": {"num": null, "den": 1},
                "threshold": {"at_most": 50},
                "verdict": "pending_manual"
            }]
        }),
        adjudication_summary: None,
    };
    std::fs::write(path, serde_json::to_vec_pretty(&envelope).unwrap()).unwrap();
    let adjudications = AdjudicationFile {
        version: 1,
        report_id: "report-1".to_owned(),
        adjudications: vec![Adjudication {
            case_id: "m0-screenshot-001".to_owned(),
            false_alarm: None,
            note: String::new(),
        }],
    };
    let stem = path.file_stem().unwrap().to_string_lossy();
    std::fs::write(
        path.with_file_name(format!("{stem}.adjudications.json")),
        serde_json::to_vec_pretty(&adjudications).unwrap(),
    )
    .unwrap();
}

#[test]
fn initializer_cli_needs_no_backend_and_writes_no_input_path() {
    let repository = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(repository.path().join("fixtures/local")).unwrap();
    let output = run_with(
        &[
            "init-m0",
            "--root",
            repository.path().to_str().unwrap(),
            "--out",
            "fixtures/local/m0-a",
        ],
        Some("/definitely/no-agent-cli"),
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let manifest_path = repository.path().join("fixtures/local/m0-a/manifest.json");
    let raw = std::fs::read_to_string(manifest_path).unwrap();
    assert!(!raw.contains("input"));
    assert!(!raw.contains(repository.path().to_str().unwrap()));
}

#[test]
fn formal_first_rejects_existing_output_before_backend() {
    let workspace = tempfile::tempdir().unwrap();
    let output_path = workspace.path().join("first.json");
    std::fs::write(&output_path, "immutable").unwrap();
    let output = run_with(
        &[
            "m0-go-no-go",
            "--manifest",
            workspace.path().join("missing.json").to_str().unwrap(),
            "--root",
            workspace.path().to_str().unwrap(),
            "--out",
            output_path.to_str().unwrap(),
        ],
        Some("/definitely/no-agent-cli"),
    );
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("拒绝覆盖"));
    assert_eq!(std::fs::read_to_string(output_path).unwrap(), "immutable");
}

#[test]
fn formal_cli_uses_fixed_exit_codes() {
    let workspace = tempfile::tempdir().unwrap();
    let first_path = workspace.path().join("first.json");
    first(&first_path, "go");
    let final_path = workspace.path().join("final.json");

    let incomplete = run(&[
        "m0-finalize",
        "--report",
        first_path.to_str().unwrap(),
        "--out",
        final_path.to_str().unwrap(),
    ]);
    assert_eq!(incomplete.status.code(), Some(2));
    assert!(!final_path.exists());

    let adjudications_path = workspace.path().join("first.adjudications.json");
    let mut adjudications: AdjudicationFile =
        serde_json::from_slice(&std::fs::read(&adjudications_path).unwrap()).unwrap();
    adjudications.adjudications[0].false_alarm = Some(false);
    std::fs::write(
        &adjudications_path,
        serde_json::to_vec_pretty(&adjudications).unwrap(),
    )
    .unwrap();
    let go = run(&[
        "m0-finalize",
        "--report",
        first_path.to_str().unwrap(),
        "--out",
        final_path.to_str().unwrap(),
    ]);
    assert_eq!(go.status.code(), Some(0));

    let no_go_first = workspace.path().join("no-go-first.json");
    first(&no_go_first, "no_go");
    let no_go_sidecar = workspace.path().join("no-go-first.adjudications.json");
    let mut no_go_adjudications: AdjudicationFile =
        serde_json::from_slice(&std::fs::read(&no_go_sidecar).unwrap()).unwrap();
    no_go_adjudications.adjudications[0].false_alarm = Some(false);
    std::fs::write(
        &no_go_sidecar,
        serde_json::to_vec_pretty(&no_go_adjudications).unwrap(),
    )
    .unwrap();
    let no_go_final = workspace.path().join("no-go-final.json");
    let no_go = run(&[
        "m0-finalize",
        "--report",
        no_go_first.to_str().unwrap(),
        "--out",
        no_go_final.to_str().unwrap(),
    ]);
    assert_eq!(no_go.status.code(), Some(3));

    let infrastructure = run(&["m0-finalize", "--report", "/definitely/missing.json"]);
    assert_eq!(infrastructure.status.code(), Some(1));
}
