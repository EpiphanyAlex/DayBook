//! `daybook-eval` —— 评测评分器的命令行入口（`docs/prd/07-eval.md`）。
//!
//! **只做零额度的那一半**：校验用例清单、重放夹具、算计数、吐一份结构化 JSON。
//! `scripts/eval.mjs` 是它的薄壳，负责渲染 diff 表（比率的格式化在那边——见
//! `daybook_lib::eval::report` 的说明）。
//!
//! ```text
//! daybook-eval version
//! daybook-eval validate       --manifest fixtures/manifest.json [--root <repo>]
//! daybook-eval replay-score   --manifest fixtures/manifest.json [--root <repo>] [--out <file>]
//! daybook-eval export-fixture --session <agent_session_id> [--data-dir <path>] [--slug <name>]
//! ```
//!
//! 参数是手搓的：仓库现在没有 CLI 解析依赖，为几个子命令引一个会连带动
//! `Cargo.lock`，而 CI 的每一步 cargo 都带 `--offline`。

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    process::ExitCode,
};

use time::{macros::format_description, OffsetDateTime};

use daybook_lib::{
    domain::confirm,
    eval::{
        expected::ExpectedSet,
        export,
        join::{degraded_set_match, ordinal_full_outer_join},
        manifest::{Manifest, ValidatedCase},
        metrics::CaseOutcome,
        replay::{
            predictions_from_drafted_json, replay_fixture, scratch_root,
            utterance_substring_violations,
        },
        report::{Attribution, CaseReport, Report},
        EvalError, EvalResult,
    },
};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("[eval] {error}");
            ExitCode::FAILURE
        }
    }
}

/// 手搓的参数表：`--flag value` 收进一个 map，各子命令自己取需要的那几个。
type Flags = BTreeMap<String, String>;

struct Options {
    manifest: PathBuf,
    root: PathBuf,
    out: Option<PathBuf>,
}

fn run() -> EvalResult<()> {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let Some(command) = arguments.first() else {
        return Err(usage());
    };
    if command == "version" {
        return print_version();
    }
    let flags = parse_flags(&arguments[1..])?;
    match command.as_str() {
        "validate" => validate(&manifest_options(&flags)?),
        "replay-score" => replay_score(&manifest_options(&flags)?),
        "export-fixture" => export(&flags),
        other => Err(EvalError::Usage(format!(
            "不认识的子命令 `{other}`。\n{USAGE}"
        ))),
    }
}

const USAGE: &str = "用法：\n  \
    daybook-eval version\n  \
    daybook-eval validate       --manifest <path> [--root <path>]\n  \
    daybook-eval replay-score   --manifest <path> [--root <path>] [--out <path>]\n  \
    daybook-eval export-fixture --session <agent_session_id> [--data-dir <path>] [--root <path>] [--slug <name>] [--out <path>]";

/// 打印当前的版本三元组。
///
/// 夹具过期时（07 §5 R4）第一件要知道的事就是「现在应该是多少」——重新导出一条夹具
/// 之前先看这个，比去源码里翻 `tool_surface_version()` 快。
fn print_version() -> EvalResult<()> {
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "toolSurfaceVersion": daybook_lib::agent::registry::tool_surface_version(),
            "appVersion": env!("CARGO_PKG_VERSION"),
            "schemaVersion": daybook_lib::db::LATEST_SCHEMA_VERSION,
            "effectiveCapabilityHash": daybook_lib::agent::registry::effective_capability_hash(
                &daybook_lib::agent::registry::expected_capabilities(),
            ),
        }))?
    );
    Ok(())
}

fn usage() -> EvalError {
    EvalError::Usage(format!("缺子命令。\n{USAGE}"))
}

fn parse_flags(arguments: &[String]) -> EvalResult<Flags> {
    let mut flags = Flags::new();
    let mut index = 0;
    while index < arguments.len() {
        let flag = arguments[index].as_str();
        let Some(name) = flag.strip_prefix("--") else {
            return Err(EvalError::Usage(format!(
                "不认识的参数 `{flag}`。\n{USAGE}"
            )));
        };
        let value = arguments
            .get(index + 1)
            .ok_or_else(|| EvalError::Usage(format!("{flag} 缺参数值")))?;
        flags.insert(name.to_owned(), value.clone());
        index += 2;
    }
    Ok(flags)
}

fn manifest_options(flags: &Flags) -> EvalResult<Options> {
    let manifest = flags
        .get("manifest")
        .map(PathBuf::from)
        .ok_or_else(|| EvalError::Usage(format!("缺 --manifest。\n{USAGE}")))?;
    // 用例里的 `dir` 是相对仓库根的，而 manifest 住在 `<repo>/fixtures/manifest.json`。
    let root = flags.get("root").map(PathBuf::from).unwrap_or_else(|| {
        manifest
            .parent()
            .and_then(Path::parent)
            .unwrap_or(Path::new("."))
            .to_path_buf()
    });
    Ok(Options {
        manifest,
        root,
        out: flags.get("out").map(PathBuf::from),
    })
}

/// 07 §3.6 的夹具导出器。
///
/// **默认写 `fixtures/local/<date>-<slug>/`**——导出的是真实截图与真实金额，而
/// `fixtures/ci/` 那一支要进 git（§3.7）。`export_fixture` 自己也会拒绝写进 CI 集，
/// 不靠这里的默认值兜底。
fn export(flags: &Flags) -> EvalResult<()> {
    let session = flags
        .get("session")
        .ok_or_else(|| EvalError::Usage(format!("缺 --session。\n{USAGE}")))?;
    let data_root = match flags.get("data-dir") {
        Some(value) => PathBuf::from(value),
        None => export::default_data_root()?,
    };
    let out_dir = match flags.get("out") {
        Some(value) => PathBuf::from(value),
        None => export::default_out_dir(
            &flags
                .get("root")
                .map_or_else(|| PathBuf::from("."), PathBuf::from),
            &today()?,
            flags.get("slug").map_or("session", String::as_str),
        ),
    };
    let summary = export::export_fixture(&data_root, session, &out_dir)?;
    println!(
        "✓ 夹具已导出：{}\n  尝试 {} · 来源 {} · {} 次工具调用 · 预填 {} 条",
        summary.out_dir.display(),
        summary.attempt_id,
        summary.source_id,
        summary.tool_call_count,
        summary.prefilled_item_count,
    );
    println!(
        "\n⚠️  expected.json 里的条目是**用 agent 那次的输出预填的**，`annotated` 现在是 false。\n   \
         逐条对着 input.* 核对、改对之后把它改成 true，评分器才会接受这条用例——\n   \
         直接拿预填的跑分等于让模型给自己判卷（07 §3.2）。"
    );
    Ok(())
}

fn today() -> EvalResult<String> {
    OffsetDateTime::now_utc()
        .format(&format_description!("[year]-[month]-[day]"))
        .map_err(|error| EvalError::Usage(format!("时间格式化失败：{error}")))
}

/// `--dry-run` 的落点：**不调用 agent 的情况下校验 eval 集完整性**（07 §6）。
///
/// 每个启用的 case 都有输入、期望集合与 `env.json` 且路径都存在；**缺分池标记直接
/// 非零退出**——分池是判定口径的一部分，缺了不该跑。
fn validate(options: &Options) -> EvalResult<()> {
    let manifest = Manifest::load(&options.manifest)?;
    let cases = manifest.validate(&options.root)?;
    for case in &cases {
        // 期望集合能不能解析也算「完整性」的一部分：一份 ordinal 重复的真值，
        // 等到跑分那一刻才炸就白等了一轮。
        ExpectedSet::load(&case.expected_path)?;
    }
    println!(
        "✓ eval 集完整：{} 条启用用例（截图池 {} · 口述池 {} · 对照栏 {}）",
        cases.len(),
        count(&cases, "screenshot"),
        count(&cases, "utterance"),
        count(&cases, "control"),
    );
    Ok(())
}

fn count(cases: &[ValidatedCase], pool: &str) -> usize {
    cases
        .iter()
        .filter(|case| case.pool.as_str() == pool)
        .count()
}

/// 重放全部用例并出报告。**零额度、确定性**——它跑的是录下来的工具调用，不是模型。
fn replay_score(options: &Options) -> EvalResult<()> {
    let manifest = Manifest::load(&options.manifest)?;
    let cases = manifest.validate(&options.root)?;

    let mut reports = Vec::new();
    let mut outcomes = Vec::new();
    for case in &cases {
        let (report, outcome) = score_one(case)?;
        reports.push(report);
        outcomes.push(outcome);
    }

    let report = Report::build("replay", reports, &outcomes);
    let json = serde_json::to_string_pretty(&report)?;
    match &options.out {
        Some(path) => std::fs::write(path, json)?,
        None => println!("{json}"),
    }
    Ok(())
}

fn score_one(case: &ValidatedCase) -> EvalResult<(CaseReport, CaseOutcome)> {
    let expected = ExpectedSet::load(&case.expected_path)?;
    let scratch = scratch_root()?;
    let result = score_in_scratch(case, &expected, &scratch);
    // 重放数据是一次性的：它含夹具里的金额，没有留在磁盘上的理由。
    let _ = std::fs::remove_dir_all(&scratch);
    result
}

fn score_in_scratch(
    case: &ValidatedCase,
    expected: &ExpectedSet,
    scratch: &Path,
) -> EvalResult<(CaseReport, CaseOutcome)> {
    let replayed = replay_fixture(&case.dir, scratch)?;
    let database = &replayed.database;

    let predicted = predictions_from_drafted_json(database, &replayed.attempt_id)?;
    let join = ordinal_full_outer_join(&expected.items, &predicted);
    let degraded = degraded_set_match(&expected.items, &predicted);
    let check = confirm::total_check(database, &replayed.attempt_id)?;
    let substring_violations = utterance_substring_violations(database, &replayed.attempt_id)?;

    let attribution: Attribution = database.read(|connection| {
        connection.query_row(
            "SELECT backend_id, backend_version, model_id, prompt_hash,
                    tool_surface_version, app_version
             FROM parse_attempts WHERE id = ?1",
            [&replayed.attempt_id],
            |row| {
                Ok(Attribution {
                    backend_id: row.get(0)?,
                    backend_version: row.get(1)?,
                    model_id: row.get(2)?,
                    prompt_hash: row.get(3)?,
                    tool_surface_version: row.get(4)?,
                    app_version: row.get(5)?,
                })
            },
        )
    })?;

    let unparsed_note = check.unparsed_note.clone().unwrap_or_default();
    let outcome = CaseOutcome {
        id: case.id.clone(),
        pool: case.pool,
        source_kind: expected.source_kind.clone(),
        join: join.clone(),
        degraded: degraded.clone(),
        reconciliation_status: check.reconciliation_status.clone(),
        confirmation_policy: check.confirmation_policy.clone(),
        unparsed_note: unparsed_note.clone(),
        stated_item_count: expected.stated_item_count(),
    };
    let report = CaseReport {
        id: case.id.clone(),
        pool: case.pool.as_str(),
        judged: case.pool.is_judged(),
        flaky: case.flaky,
        source_kind: expected.source_kind.clone(),
        attribution,
        reconciliation_status: check.reconciliation_status,
        confirmation_policy: check.confirmation_policy,
        unparsed_note,
        matched: join.matched_count(),
        missed: join.missed_count(),
        extra: join.extra_count(),
        join,
        degraded,
        calls: replayed.calls.clone(),
        substring_violations,
    };
    Ok((report, outcome))
}
