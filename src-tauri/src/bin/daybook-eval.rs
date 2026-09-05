//! `daybook-eval` —— 评测评分器的命令行入口（`docs/prd/07-eval.md`）。
//!
//! 算分、正式 manifest 门禁、首轮 / finalize / diagnosis 报告协议都在这里；
//! `scripts/eval.mjs` 只是模式选择与渲染的薄壳（比率格式化在 Node 侧——见
//! `daybook_lib::eval::report` 的说明）。
//!
//! ```text
//! daybook-eval version
//! daybook-eval validate       --manifest fixtures/manifest.json [--root <repo>]
//! daybook-eval replay-score   --manifest fixtures/manifest.json [--root <repo>] [--out <file>]
//! daybook-eval export-fixture --session <agent_session_id> [--data-dir <path>] [--slug <name>]
//! daybook-eval run            --manifest fixtures/manifest.json [--trials N] [--keep-runs <dir>]
//! daybook-eval init-m0        --root <repo> --out fixtures/local/<set>
//! daybook-eval m0-go-no-go    --manifest fixtures/local/<set>/manifest.json --out <first.json>
//! daybook-eval m0-finalize    --report <first.json>
//! daybook-eval m0-diagnose    --report <first.json> --root <repo>
//! ```
//!
//! `run`、`m0-go-no-go` 与 `m0-diagnose` 会起用户自己的 agent CLI；其余子命令零额度。
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
    agent::runtime::AgentRuntime,
    domain::confirm,
    eval::{
        expected::{evaluate_scope, ExpectedSet},
        export, formal,
        init::{self, InitOptions},
        join::{degraded_set_match, ordinal_full_outer_join},
        live, m0,
        manifest::{ensure_m0_manifest_location, Manifest, ValidatedCase},
        metrics::CaseOutcome,
        replay::{
            predictions_from_drafted_json, replay_fixture, scratch_root,
            utterance_substring_violations, FixtureEnv,
        },
        report::{
            build_hard_field_diffs, build_reconciliation_evidence, reported_claim_identity,
            Attribution, CaseReport, Report,
        },
        EvalError, EvalResult,
    },
};

fn main() -> ExitCode {
    match run() {
        Ok(code) => ExitCode::from(code),
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

fn run() -> EvalResult<u8> {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let Some(command) = arguments.first() else {
        return Err(usage());
    };
    if matches!(command.as_str(), "help" | "--help" | "-h") {
        println!("{USAGE}");
        return Ok(0);
    }
    if command == "version" {
        print_version()?;
        return Ok(0);
    }
    let flags = parse_flags(&arguments[1..])?;
    match command.as_str() {
        "validate" => validate(&manifest_options(&flags)?),
        "replay-score" => replay_score(&manifest_options(&flags)?),
        "export-fixture" => export(&flags),
        "run" => live_run(&flags),
        "init-m0" => init_m0(&flags),
        "m0-go-no-go" => return m0_go_no_go(&flags),
        "m0-finalize" => return m0_finalize(&flags),
        "m0-diagnose" => return m0_diagnose(&flags),
        other => {
            return Err(EvalError::Usage(format!(
                "不认识的子命令 `{other}`。\n{USAGE}"
            )))
        }
    }?;
    Ok(0)
}

const USAGE: &str = "用法：\n  \
    daybook-eval version\n  \
    daybook-eval validate       --manifest <path> [--root <path>]\n  \
    daybook-eval replay-score   --manifest <path> [--root <path>] [--out <path>]\n  \
    daybook-eval export-fixture --session <agent_session_id> [--data-dir <path>] [--root <path>] [--slug <name>] [--out <path>]\n  \
    daybook-eval run            --manifest <path> [--root <path>] [--trials <n>] [--keep-runs <dir>] [--out <path>]\n  \
    daybook-eval init-m0        --root <repo> --out <fixtures/local/...> [--screenshots <n>] [--controls <n>]\n  \
    daybook-eval m0-go-no-go    --manifest <fixtures/local/.../manifest.json> --root <repo> --out <first.json>\n  \
    daybook-eval m0-finalize    --report <first.json> [--out <final.json>]\n  \
    daybook-eval m0-diagnose    --report <first.json> --root <repo> [--out <diagnosis.json>]\n\
\n  \
    `run` 与 `m0-go-no-go` / `m0-diagnose` 会烧订阅额度；validate、replay-score、\n  \
    export-fixture、init-m0、m0-finalize 都是零额度。";

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

fn timestamp() -> EvalResult<String> {
    OffsetDateTime::now_utc()
        .format(&format_description!(
            "[year][month][day]-[hour][minute][second]"
        ))
        .map_err(|error| EvalError::Usage(format!("时间格式化失败：{error}")))
}

/// 零 backend 的 M0 清单初始化器。只建中性 manifest / 空目录，不写真实输入路径。
fn init_m0(flags: &Flags) -> EvalResult<()> {
    let root = flags
        .get("root")
        .map_or_else(|| PathBuf::from("."), PathBuf::from);
    let out = flags
        .get("out")
        .map(PathBuf::from)
        .ok_or_else(|| EvalError::Usage(format!("缺 --out。\n{USAGE}")))?;
    let value = |name: &str, default: usize| -> EvalResult<usize> {
        flags.get(name).map_or(Ok(default), |raw| {
            raw.parse()
                .map_err(|_| EvalError::Usage(format!("--{name} 必须是正整数，收到 `{raw}`")))
        })
    };
    let options = InitOptions {
        screenshot_count: value("screenshots", 22)?,
        control_count: value("controls", 4)?,
        utterance_single: value("single", 4)?,
        utterance_two_to_three: value("two-to-three", 9)?,
        utterance_four_plus: value("four-plus", 7)?,
        added_on: today()?,
    };
    let manifest = init::initialize(&root, &out, &options)?;
    println!(
        "✓ M0 正式清单骨架已创建：{}\n  未复制、未记录任何真实 input 路径；请只在本机补齐各 case 的 input.* / expected.json / env.json 与中性 layout 标签。",
        manifest.display()
    );
    Ok(())
}

/// 唯一能产生 docs/PRD.md §9.4 正式 verdict 的首轮入口。
fn m0_go_no_go(flags: &Flags) -> EvalResult<u8> {
    if flags.contains_key("trials") || flags.contains_key("keep-runs") {
        return Err(EvalError::Usage(
            "M0 正式首轮每 case 恰好 1 轮；不得传 --trials / --keep-runs。三轮只经 --m0-diagnose"
                .to_owned(),
        ));
    }
    let options = manifest_options(flags)?;
    let output = options
        .out
        .as_ref()
        .ok_or_else(|| EvalError::Usage("M0 正式首轮缺 --out（报告必须永久保存）".to_owned()))?;
    let sidecar = m0::adjudications_path(output);
    if output.exists() || sidecar.exists() {
        return Err(EvalError::Usage(format!(
            "M0 正式首轮拒绝覆盖已有 report / adjudications：{} / {}",
            output.display(),
            sidecar.display()
        )));
    }
    m0::ensure_private_report_path(&options.root, output)?;
    ensure_m0_manifest_location(&options.root, &options.manifest)?;
    let manifest_bytes = std::fs::read(&options.manifest)?;
    let manifest = Manifest::from_bytes(&manifest_bytes)?;
    let cases = manifest.validate_m0(&options.root, &options.manifest)?;
    let fixture_set = m0::snapshot_fixture_set_from_manifest(
        &options.root,
        &options.manifest,
        &cases,
        &manifest_bytes,
    )?;
    let async_runtime = tokio::runtime::Runtime::new()
        .map_err(|error| EvalError::Fixture(format!("无法启动异步运行时：{error}")))?;
    let report = async_runtime.block_on(async {
        let agent = AgentRuntime::claude_default();
        formal::run_first(&agent, &cases).await
    })?;
    let written = m0::save_first(
        &report,
        &options.root,
        &options.manifest,
        &cases,
        &fixture_set,
        output,
    )?;
    println!("✓ M0 首轮报告已永久保存：{}", written.report_path.display());
    if let Some(path) = &written.adjudications_path {
        println!(
            "? 指标 5 待人工裁定：{}\n  填完后运行 --m0-finalize；首轮报告不会被改写。",
            path.display()
        );
    }
    Ok(written.exit.code())
}

/// 零额度 finalize：不加载 backend，只读首轮报告与相邻 adjudications。
fn m0_finalize(flags: &Flags) -> EvalResult<u8> {
    let report = flags
        .get("report")
        .map(PathBuf::from)
        .ok_or_else(|| EvalError::Usage(format!("缺 --report。\n{USAGE}")))?;
    let output = flags
        .get("out")
        .map_or_else(|| m0::final_report_path(&report), PathBuf::from);
    match m0::finalize(&report, &output)? {
        m0::FinalizeResult::Complete {
            report_path,
            envelope,
            exit,
        } => {
            println!(
                "✓ M0 final 报告已生成：{}\n  verdict: {}",
                report_path.display(),
                envelope.verdict
            );
            Ok(exit.code())
        }
        m0::FinalizeResult::Incomplete {
            missing_case_ids,
            adjudications_path,
        } => {
            eprintln!(
                "[eval] incomplete：{} 仍有 {} 条未裁定：{}",
                adjudications_path.display(),
                missing_case_ids.len(),
                missing_case_ids.join(", ")
            );
            Ok(m0::FormalExit::Incomplete.code())
        }
    }
}

/// 对首轮失败与预标 flaky case 的并集各追加 3 轮，单写诊断报告。
fn m0_diagnose(flags: &Flags) -> EvalResult<u8> {
    let first_path = flags
        .get("report")
        .map(PathBuf::from)
        .ok_or_else(|| EvalError::Usage(format!("缺 --report。\n{USAGE}")))?;
    let root = flags
        .get("root")
        .map_or_else(|| PathBuf::from("."), PathBuf::from);
    let first_bytes = std::fs::read(&first_path)?;
    let first = m0::read_first(&first_path)?;
    m0::ensure_private_report_path(&root, &first_path)?;
    let targets = m0::diagnosis_targets(&first);
    if targets.is_empty() {
        return Err(EvalError::Usage(
            "首轮没有失败或预标 flaky case，无需启动三轮诊断".to_owned(),
        ));
    }
    let output = match flags.get("out") {
        Some(path) => PathBuf::from(path),
        None => m0::diagnosis_report_path(&first_path, &timestamp()?),
    };
    if output.exists() {
        return Err(EvalError::Usage(format!(
            "M0 诊断报告拒绝覆盖已有文件：{}",
            output.display()
        )));
    }
    m0::ensure_private_report_path(&root, &output)?;
    let manifest_path = root.join(&first.manifest_path);
    ensure_m0_manifest_location(&root, &manifest_path)?;
    let manifest_bytes = std::fs::read(&manifest_path)?;
    let manifest = Manifest::from_bytes(&manifest_bytes)?;
    let cases = manifest.validate_m0(&root, &manifest_path)?;
    m0::snapshot_fixture_set_from_manifest(&root, &manifest_path, &cases, &manifest_bytes)?;
    let fixture_set = m0::fixture_set_from_first(&first)?;
    m0::ensure_fixture_set_for_first(&root, &first, &cases)?;
    let async_runtime = tokio::runtime::Runtime::new()
        .map_err(|error| EvalError::Fixture(format!("无法启动异步运行时：{error}")))?;
    let diagnosis = async_runtime.block_on(async {
        let agent = AgentRuntime::claude_default();
        formal::run_diagnosis(&agent, &cases, &targets, &first.report_id, &fixture_set).await
    })?;
    m0::ensure_fixture_set_for_first(&root, &first, &cases)?;
    m0::write_diagnosis_new(&output, &diagnosis)?;
    if std::fs::read(&first_path)? != first_bytes {
        return Err(EvalError::Fixture(
            "诊断检测到首轮报告被改写，拒绝接受".to_owned(),
        ));
    }
    println!(
        "✓ M0 三轮诊断已保存：{}（{} 个目标 × 追加 3 轮；不覆盖首轮）",
        output.display(),
        targets.len()
    );
    Ok(0)
}

/// **真跑 agent 的 eval 轮次**（07 §3.1）。走生产同一条路径：起 MCP server、spawn 用户
/// 自己的 CLI、落进临时数据目录、然后查表打分。
///
/// **每跑一轮就烧一轮订阅额度**，所以它不进 CI，也不在 `verify-m0.mjs` 里。
///
/// - `--trials N`：只对标记 `flaky` 的用例生效，且**第 2 轮起只进诊断栏**（§9.4 口径③）
/// - `--keep-runs <dir>`：把每一轮的数据目录留下来。**留着是为了让一次跑砸的 eval 能直接
///   变成回归夹具**——`daybook-eval export-fixture --data-dir <那一轮的目录> --session <id>`。
///   不给这个参数就跑完即删，因为那里面是真实解析产物。
fn live_run(flags: &Flags) -> EvalResult<()> {
    let options = manifest_options(flags)?;
    let trials: usize = match flags.get("trials") {
        Some(value) => value
            .parse()
            .map_err(|_| EvalError::Usage(format!("--trials 必须是正整数，收到 `{value}`")))?,
        None => 1,
    };
    if trials == 0 {
        return Err(EvalError::Usage("--trials 至少是 1".to_owned()));
    }
    let keep_runs = flags.get("keep-runs").map(PathBuf::from);

    let manifest = Manifest::load(&options.manifest)?;
    let cases = manifest.validate(&options.root)?;
    let async_runtime = tokio::runtime::Runtime::new()
        .map_err(|error| EvalError::Fixture(format!("无法启动异步运行时：{error}")))?;

    async_runtime.block_on(async {
        let agent = AgentRuntime::claude_default();

        // **先探测，再跑任何一条用例**：检测不到可用 CLI 就非零退出（§6），
        // 顺带省得烧了一半额度才发现没登录。
        let probe_root = scratch_root()?;
        let probed = live::ensure_backend_ready(&agent, &probe_root).await;
        let _ = std::fs::remove_dir_all(&probe_root);
        probed?;

        let mut reports = Vec::new();
        let mut outcomes = Vec::new();
        for case in &cases {
            eprintln!("[eval] 跑 {} …", case.id);
            let (report, outcome) = run_case(&agent, case, trials, keep_runs.as_deref()).await?;
            reports.push(report);
            outcomes.push(outcome);
        }

        let report = Report::with_trials("live", trials as u32, reports, &outcomes);
        let json = serde_json::to_string_pretty(&report)?;
        match &options.out {
            Some(path) => std::fs::write(path, json)?,
            None => println!("{json}"),
        }
        Ok(())
    })
}

async fn run_case(
    agent: &AgentRuntime,
    case: &ValidatedCase,
    trials: usize,
    keep_runs: Option<&Path>,
) -> EvalResult<(CaseReport, CaseOutcome)> {
    let expected = ExpectedSet::load(&case.expected_path)?;
    let env = FixtureEnv::load(&case.env_path)?;
    // **只有标 `flaky` 的用例跑多轮**——额度是真实约束（§3.1），而 §3.4 那条说的就是
    // 「标记为 flaky 或曾经出过错的用例跑 3 轮」。
    let rounds = if case.flaky { trials } else { 1 };

    let mut official = None;
    let mut passes = Vec::new();
    for round in 0..rounds {
        let scratch = match keep_runs {
            Some(root) => {
                let path = root.join(format!("{}-trial{}", case.id, round + 1));
                std::fs::create_dir_all(&path)?;
                path
            }
            None => scratch_root()?,
        };
        let outcome = live::run_trial(agent, &case.dir, &env, &expected, &scratch).await;
        if keep_runs.is_none() {
            // 真实解析产物，跑完即删。
            let _ = std::fs::remove_dir_all(&scratch);
        }
        let outcome = outcome?;
        passes.push(outcome.passed());
        // **第 1 轮出正式数**，之后的只进诊断栏（§9.4 口径③）。
        if official.is_none() {
            official = Some((outcome, expected.clone()));
        }
    }

    let (outcome, expected) = official.expect("rounds ≥ 1");
    let degraded = live::degraded_for(&expected, &outcome)?;
    let predicted = predictions_from_drafted_json(&outcome.database, &outcome.attempt_id)?;
    let hard_field_diffs = build_hard_field_diffs(&expected.items, &predicted, &outcome.join);
    let reconciliation_evidence =
        build_reconciliation_evidence(expected.reconciliation_scope.as_ref(), &outcome.check);
    let scope_evaluation = evaluate_scope(
        expected.reconciliation_scope.as_ref(),
        reported_claim_identity(&outcome.check).as_ref(),
    );
    let attribution = attribution_of(&outcome.database, &outcome.attempt_id)?;
    let unparsed_note = outcome.check.unparsed_note.clone().unwrap_or_default();
    let join = outcome.join.clone();

    let case_outcome = CaseOutcome {
        id: case.id.clone(),
        pool: case.pool,
        source_kind: expected.source_kind.clone(),
        join: join.clone(),
        degraded: degraded.clone(),
        reconciliation_status: outcome.check.reconciliation_status.clone(),
        confirmation_policy: outcome.check.confirmation_policy.clone(),
        unparsed_note: unparsed_note.clone(),
        scope_evaluation,
        stated_item_count: expected.stated_item_count(),
        duration_ms: Some(outcome.duration_ms),
        usage: outcome.usage.clone(),
    };
    let report = CaseReport {
        id: case.id.clone(),
        pool: case.pool.as_str(),
        judged: case.pool.is_judged(),
        flaky: case.flaky,
        source_kind: expected.source_kind.clone(),
        attribution,
        reconciliation_status: outcome.check.reconciliation_status.clone(),
        confirmation_policy: outcome.check.confirmation_policy.clone(),
        unparsed_note,
        hard_field_diffs,
        reconciliation_evidence,
        matched: join.matched_count(),
        missed: join.missed_count(),
        extra: join.extra_count(),
        join,
        degraded,
        calls: Vec::new(),
        substring_violations: outcome.substring_violations.clone(),
        case_passed: outcome.passed(),
        execution_error: outcome.execution_error.clone(),
        duration_ms: Some(outcome.duration_ms),
        usage: outcome.usage.clone(),
        trial_diagnostics: (passes.len() > 1)
            .then(|| live::TrialDiagnostics::new(passes))
            .flatten(),
    };
    Ok((report, case_outcome))
}

fn attribution_of(
    database: &daybook_lib::db::Database,
    attempt_id: &str,
) -> EvalResult<Attribution> {
    Ok(database.read(|connection| {
        connection.query_row(
            "SELECT backend_id, backend_version, model_id, prompt_hash,
                    tool_surface_version, app_version
             FROM parse_attempts WHERE id = ?1",
            [attempt_id],
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
    })?)
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
    let hard_field_diffs = build_hard_field_diffs(&expected.items, &predicted, &join);
    let reconciliation_evidence =
        build_reconciliation_evidence(expected.reconciliation_scope.as_ref(), &check);
    let scope_evaluation = evaluate_scope(
        expected.reconciliation_scope.as_ref(),
        reported_claim_identity(&check).as_ref(),
    );
    let case_passed = join.is_clean_source();
    let outcome = CaseOutcome {
        id: case.id.clone(),
        pool: case.pool,
        source_kind: expected.source_kind.clone(),
        join: join.clone(),
        degraded: degraded.clone(),
        reconciliation_status: check.reconciliation_status.clone(),
        confirmation_policy: check.confirmation_policy.clone(),
        unparsed_note: unparsed_note.clone(),
        scope_evaluation,
        stated_item_count: expected.stated_item_count(),
        duration_ms: None,
        usage: None,
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
        hard_field_diffs,
        reconciliation_evidence,
        matched: join.matched_count(),
        missed: join.missed_count(),
        extra: join.extra_count(),
        join,
        degraded,
        calls: replayed.calls.clone(),
        substring_violations,
        case_passed,
        execution_error: None,
        duration_ms: None,
        usage: None,
        trial_diagnostics: None,
    };
    Ok((report, outcome))
}

#[cfg(test)]
mod eval {
    use super::*;

    #[test]
    fn synthetic_eligible_decoy_correct_claim_is_exact() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .to_path_buf();
        let source = root.join("fixtures/ci/2026-08-30-total-scope/eligible-decoy");
        let directory = tempfile::tempdir().unwrap();
        for name in ["input.png", "expected.json", "env.json", "tool-calls.json"] {
            std::fs::copy(source.join(name), directory.path().join(name)).unwrap();
        }
        let case = ValidatedCase {
            id: "synthetic-correct-total".to_owned(),
            pool: daybook_lib::eval::manifest::Pool::Screenshot,
            flaky: false,
            sample: None,
            dir: directory.path().to_path_buf(),
            expected_path: directory.path().join("expected.json"),
            env_path: directory.path().join("env.json"),
            tool_calls_path: Some(directory.path().join("tool-calls.json")),
        };
        let (_, wrong) = score_one(&case).unwrap();
        assert!(wrong.scope_evaluation.as_ref().unwrap().scope_violation);
        let calls_path = case.tool_calls_path.as_ref().unwrap();
        let mut calls: serde_json::Value =
            serde_json::from_slice(&std::fs::read(calls_path).unwrap()).unwrap();
        let claim = calls["calls"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|call| call["tool"] == "report_source_total")
            .unwrap();
        claim["arguments"]["amountMinor"] = serde_json::json!("300");
        claim["arguments"]["evidenceText"] = serde_json::json!("VIEWPORT TOTAL 3.00 AUD");
        std::fs::write(calls_path, serde_json::to_vec(&calls).unwrap()).unwrap();
        let (case_report, correct) = score_one(&case).unwrap();
        let evidence = case_report.reconciliation_evidence.as_ref().unwrap();
        assert_eq!(evidence.reported_claim_matches_expected, Some(true));
        assert!(!evidence.scope_violation);
        let before = daybook_lib::eval::metrics::total_availability(&[&wrong]);
        let after = daybook_lib::eval::metrics::total_availability(&[&correct]);
        assert_eq!((before.num, before.den), (Some(0), 1));
        assert_eq!((after.num, after.den), (Some(1), 1));
        let report = Report::build("replay", vec![case_report], &[correct]);
        assert_eq!(report.scope_invalid_total_reports, 0);
        assert_eq!(report.verdict, "go");
    }

    #[test]
    fn synthetic_total_scope_fixture_catches_no_go() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .to_path_buf();
        let manifest_path = root.join("fixtures/manifest.json");
        let manifest = Manifest::load(&manifest_path).unwrap();
        let cases = manifest.validate(&root).unwrap();
        let selected = cases
            .iter()
            .filter(|case| case.id.starts_with("synthetic-"))
            .collect::<Vec<_>>();
        assert_eq!(selected.len(), 4);

        let mut reports = Vec::new();
        let mut outcomes = Vec::new();
        for case in selected {
            let expected = ExpectedSet::load(&case.expected_path).unwrap();
            let input = FixtureEnv::load(&case.env_path)
                .map(|env| case.dir.join(env.source.input))
                .unwrap();
            for path in [
                case.expected_path.clone(),
                case.env_path.clone(),
                case.tool_calls_path.clone().unwrap(),
                input.clone(),
            ] {
                let bytes = std::fs::read(path).unwrap();
                let content = String::from_utf8_lossy(&bytes);
                assert!(!content.contains("fixtures/local/"));
                assert!(!content.contains("output/m0-eval/"));
            }
            expected
                .validate_formal(&case.expected_path, &input)
                .unwrap();
            let (report, outcome) = score_one(case).unwrap();
            reports.push(report);
            outcomes.push(outcome);
        }
        let report = Report::build("replay", reports, &outcomes);
        assert_eq!(report.scope_invalid_total_reports, 1);
        assert_eq!(
            report
                .cases
                .iter()
                .filter(|case| case
                    .reconciliation_evidence
                    .as_ref()
                    .is_some_and(|evidence| evidence.scope_violation))
                .count(),
            3,
            "对照栏仍保留逐例 violation 证据"
        );
        assert_eq!(report.verdict, "no_go");

        let keyword = report
            .cases
            .iter()
            .find(|case| case.id == "synthetic-keyword-not-gate")
            .unwrap();
        assert_eq!(keyword.reconciliation_status, "not_applicable");
        assert!(keyword.case_passed);
        assert!(
            keyword
                .reconciliation_evidence
                .as_ref()
                .unwrap()
                .reported
                .is_none(),
            "仅有合计关键词不能强制 report_source_total"
        );
    }
}
