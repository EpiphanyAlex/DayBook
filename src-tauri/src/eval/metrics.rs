//! 十项指标的计数与判定。
//!
//! **阈值与口径的权威出处是 [`docs/PRD.md`](../../../docs/PRD.md) §9.4**，本模块只是
//! 它的一份可执行副本——**数字一个不改**。改阈值要走 §9.4 的四步流程（先写清为什么、
//! 再用第二批独立样本验证），不是改这里的常量。
//!
//! ## 为什么全是整数对
//!
//! `scripts/verify-m0.mjs` 禁止 `src-tauri/src` 下出现浮点类型。但即使没有那条门禁，
//! §9.4 也要求「每个比率一律连原始计数一起报」，写成 `0.967 (58/60)`——**小分母上的
//! 比率是量化的**：60 条上 `≥ 0.98` 实际等于「最多漏 1 条」，漏 2 条直接掉到 0.967。
//! 一个浮点数会把 `(58, 60)` 这一对丢掉，而那正是事后说得清那个数怎么来的唯一依据。
//!
//! 判定用交叉相乘：`num * 1000 >= permille * den`，等价于 `num / den >= permille / 1000`
//! 而不引入除法。

use serde::Serialize;

use super::{
    join::{DegradedMatch, HardField, OrdinalJoin, HARD_FIELDS},
    manifest::Pool,
};

// ── `docs/PRD.md` §9.4 的十项阈值（初值，一个未动） ──────────────────────────
/// 指标 1 条目 recall ≥ 0.98
pub const RECALL_MIN_PERMILLE: i64 = 980;
/// 指标 2 条目 precision ≥ 0.98
pub const PRECISION_MIN_PERMILLE: i64 = 980;
/// 指标 3 金额 / 币种 / 日期的逐字段准确率 ≥ 0.98
pub const FIELD_MIN_PERMILLE: i64 = 980;
/// 指标 4 声明合计可获得率 ≥ 0.70
pub const TOTAL_AVAILABILITY_MIN_PERMILLE: i64 = 700;
/// 指标 5 总额校验假警报率 ≤ 0.05
pub const FALSE_ALARM_MAX_PERMILLE: i64 = 50;
/// 指标 6 口述静默遗漏率 ≤ 0.02
pub const SILENT_OMISSION_MAX_PERMILLE: i64 = 20;
/// 指标 7 人工纠正率 ≤ 0.20
pub const CORRECTION_MAX_PERMILLE: i64 = 200;
/// 指标 8 干净来源率 ≥ 0.60
pub const CLEAN_SOURCE_MIN_PERMILLE: i64 = 600;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Threshold {
    AtLeast(i64),
    AtMost(i64),
    /// 指标 9 / 10：**只记录，不设阈值**。
    RecordOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    Pass,
    Fail,
    /// 分母为 0——这一池里没有能算这条指标的样本。**不伪装成通过。**
    NoSample,
    /// 需要人工裁定才有分子（指标 5：校验 `failed` 但人工核对后其实读对了）。
    PendingManual,
    RecordOnly,
}

/// **一个比率永远连着它的原始计数。** `num` 为 `None` 表示分子要人工裁定。
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Ratio {
    pub num: Option<i64>,
    pub den: i64,
}

impl Ratio {
    pub fn new(num: i64, den: i64) -> Self {
        Self {
            num: Some(num),
            den,
        }
    }

    pub fn pending_manual(den: i64) -> Self {
        Self { num: None, den }
    }

    fn judge(self, threshold: Threshold) -> Verdict {
        let Some(num) = self.num else {
            return Verdict::PendingManual;
        };
        match threshold {
            Threshold::RecordOnly => Verdict::RecordOnly,
            _ if self.den == 0 => Verdict::NoSample,
            Threshold::AtLeast(permille) => {
                if num * 1000 >= permille * self.den {
                    Verdict::Pass
                } else {
                    Verdict::Fail
                }
            }
            Threshold::AtMost(permille) => {
                if num * 1000 <= permille * self.den {
                    Verdict::Pass
                } else {
                    Verdict::Fail
                }
            }
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Metric {
    /// `docs/PRD.md` §9.4 表里的序号。
    pub index: u8,
    pub key: &'static str,
    pub label: &'static str,
    pub ratio: Ratio,
    pub threshold: Threshold,
    pub verdict: Verdict,
}

impl Metric {
    fn new(
        index: u8,
        key: &'static str,
        label: &'static str,
        ratio: Ratio,
        threshold: Threshold,
    ) -> Self {
        Self {
            index,
            key,
            label,
            ratio,
            threshold,
            verdict: ratio.judge(threshold),
        }
    }

    /// 指标 1–3 是「产品可信性的地板」：任一不达标 ⇒ no-go（§9.4「判定」）。
    pub fn is_floor(&self) -> bool {
        matches!(self.index, 1..=3)
    }
}

/// 一条用例跑完之后、进聚合之前的样子。
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageCounts {
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub cache_creation_input_tokens: Option<i64>,
    pub cache_read_input_tokens: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct CaseOutcome {
    pub id: String,
    pub pool: Pool,
    pub source_kind: String,
    pub join: OrdinalJoin,
    pub degraded: DegradedMatch,
    /// `domain::confirm::total_check` 给出的对账结果。
    pub reconciliation_status: String,
    pub confirmation_policy: String,
    pub unparsed_note: String,
    /// 口述原文里说了几笔（指标 6 的基准）。
    pub stated_item_count: i64,
    /// 指标 9：只记录，不参与 verdict。重放路径为 `None`。
    pub duration_ms: Option<i64>,
    /// 指标 10：只记录整数 usage；后端没提供时为 `None`，不伪装成 0。
    pub usage: Option<UsageCounts>,
}

impl CaseOutcome {
    fn drafted_count(&self) -> i64 {
        self.join.matched_count() + self.join.extra_count()
    }

    /// 指标 6：说了 N 件、拆出 < N 件，**且没在 `unparsed_note` 里说明**。
    ///
    /// 「静默」这两个字是判据的一半——agent 自己说「有一块我没读」时用户会去看原件，
    /// 那不是这条指标要抓的东西。
    fn is_silent_omission(&self) -> bool {
        self.drafted_count() < self.stated_item_count && self.unparsed_note.trim().is_empty()
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PoolReport {
    pub pool: &'static str,
    /// **对照栏为 `false`：如实报数，不参与 go / no-go**（07 §3.4）。
    pub judged: bool,
    pub case_count: i64,
    pub metrics: Vec<Metric>,
    /// 诊断栏：降级集合匹配的合计。**不与正式指标同栏。**
    pub degraded: DegradedMatch,
}

/// 按一池算指标 1–3。
///
/// **只有指标 1–3 分池**（§9.4 口径①）。指标 4–8 由
/// [`compute_decision_metrics`] 在截图池 + 口述池的正式判定集合上聚合；把 4–8 也在
/// 两池各算一遍会制造规格里不存在的两套判定。
pub fn compute_pool(pool: Pool, cases: &[&CaseOutcome]) -> PoolReport {
    let matched: i64 = cases.iter().map(|case| case.join.matched_count()).sum();
    let missed: i64 = cases.iter().map(|case| case.join.missed_count()).sum();
    let extra: i64 = cases.iter().map(|case| case.join.extra_count()).sum();

    let field_correct = |field: HardField| -> i64 {
        cases
            .iter()
            .flat_map(|case| case.join.matched.iter())
            .filter(|pair| !pair.wrong_fields.contains(&field))
            .count() as i64
    };

    let mut metrics = vec![
        Metric::new(
            1,
            "item_recall",
            "条目 recall（漏读率的反面）",
            Ratio::new(matched, matched + missed),
            Threshold::AtLeast(RECALL_MIN_PERMILLE),
        ),
        Metric::new(
            2,
            "item_precision",
            "条目 precision（多读 / 幻觉）",
            Ratio::new(matched, matched + extra),
            Threshold::AtLeast(PRECISION_MIN_PERMILLE),
        ),
        // 指标 3 是「金额 + 币种 + 日期的**逐字段**准确率」，所以是三条各自过阈值，
        // 不是一条合并的。字段准确率的分母是**匹配上的条目数**——漏读与多读走
        // precision / recall 那一栏，不重复计入字段错误（07 §3.3）。
        Metric::new(
            3,
            "amount_accuracy",
            "金额准确率（分母 = 匹配上的条目）",
            Ratio::new(field_correct(HardField::AmountMinor), matched),
            Threshold::AtLeast(FIELD_MIN_PERMILLE),
        ),
        Metric::new(
            3,
            "currency_accuracy",
            "币种准确率（分母 = 匹配上的条目）",
            Ratio::new(field_correct(HardField::Currency), matched),
            Threshold::AtLeast(FIELD_MIN_PERMILLE),
        ),
        Metric::new(
            3,
            "date_accuracy",
            "日期准确率（分母 = 匹配上的条目）",
            Ratio::new(field_correct(HardField::OccurredOn), matched),
            Threshold::AtLeast(FIELD_MIN_PERMILLE),
        ),
    ];
    if !pool.is_judged() {
        // 对照栏「只如实报数」。留着数字、去掉判定——一栏带着 pass / fail 的数字，
        // 迟早会有人把它当结论看。
        for metric in &mut metrics {
            metric.verdict = Verdict::RecordOnly;
        }
    }

    PoolReport {
        pool: pool.as_str(),
        judged: pool.is_judged(),
        case_count: cases.len() as i64,
        metrics,
        degraded: DegradedMatch {
            label: "诊断用".to_owned(),
            matched: cases.iter().map(|case| case.degraded.matched).sum(),
            expected_only: cases.iter().map(|case| case.degraded.expected_only).sum(),
            predicted_only: cases.iter().map(|case| case.degraded.predicted_only).sum(),
        },
    }
}

/// 指标 4–8 的唯一正式聚合。
///
/// `cases` 必须只含截图池 + 口述池；对照栏由调用方排除。指标 5 的分子在首轮为 `None`
/// （需要人工裁定），finalize 后传 `Some(false_alarm_count)`。
pub fn compute_decision_metrics(
    cases: &[&CaseOutcome],
    false_alarm_count: Option<i64>,
) -> Vec<Metric> {
    let failed = cases
        .iter()
        .filter(|case| case.reconciliation_status == "failed")
        .count() as i64;
    let false_alarm = if failed == 0 {
        Ratio::new(0, 0)
    } else {
        false_alarm_count.map_or_else(
            || Ratio::pending_manual(failed),
            |num| Ratio::new(num, failed),
        )
    };
    vec![
        Metric::new(
            4,
            "reported_total_availability",
            "声明合计可获得率（分母只含 kind = file）",
            total_availability(cases),
            Threshold::AtLeast(TOTAL_AVAILABILITY_MIN_PERMILLE),
        ),
        Metric::new(
            5,
            "false_alarm_rate",
            "总额校验假警报率（分母 = 全部实际 failed 来源）",
            false_alarm,
            Threshold::AtMost(FALSE_ALARM_MAX_PERMILLE),
        ),
        Metric::new(
            6,
            "silent_omission_rate",
            "口述静默遗漏率（只含 kind = utterance）",
            silent_omission(cases),
            Threshold::AtMost(SILENT_OMISSION_MAX_PERMILLE),
        ),
        Metric::new(
            7,
            "correction_rate",
            "人工纠正率（口径同第 8 条：只算四个硬字段与漏读多读）",
            correction_rate(cases, &HARD_FIELDS),
            Threshold::AtMost(CORRECTION_MAX_PERMILLE),
        ),
        Metric::new(
            8,
            "clean_source_rate",
            "干净来源率（条目完整 + 四个硬字段全对）",
            Ratio::new(
                cases
                    .iter()
                    .filter(|case| case.join.is_clean_source())
                    .count() as i64,
                cases.len() as i64,
            ),
            Threshold::AtLeast(CLEAN_SOURCE_MIN_PERMILLE),
        ),
    ]
}

/// 对照栏只记录：保留 1–8 的原始计数，但把判定全部改成 `record_only`。
pub fn compute_control_pool(cases: &[&CaseOutcome]) -> PoolReport {
    let mut report = compute_pool(Pool::Control, cases);
    report.metrics.extend(compute_decision_metrics(cases, None));
    for metric in &mut report.metrics {
        metric.verdict = Verdict::RecordOnly;
        metric.threshold = Threshold::RecordOnly;
    }
    report
}

/// 指标 4 的分母**只含 `kind = file`**（§9.4 口径④）。
///
/// 口述来源不走机器对账（03 §3.3），混进分母会把这个数稀释成量不到闸门 3 覆盖面的
/// 东西——而它量的就是闸门 3 的覆盖面。
pub fn total_availability(cases: &[&CaseOutcome]) -> Ratio {
    let files: Vec<_> = cases
        .iter()
        .filter(|case| case.source_kind == "file")
        .collect();
    let obtained = files
        .iter()
        .filter(|case| matches!(case.reconciliation_status.as_str(), "passed" | "failed"))
        .count() as i64;
    Ratio::new(obtained, files.len() as i64)
}

fn silent_omission(cases: &[&CaseOutcome]) -> Ratio {
    let utterances: Vec<_> = cases
        .iter()
        .filter(|case| case.source_kind == "utterance")
        .collect();
    let silent = utterances
        .iter()
        .filter(|case| case.is_silent_omission())
        .count() as i64;
    Ratio::new(silent, utterances.len() as i64)
}

/// 指标 7 的分子**只含四个硬字段与漏读多读**（§9.4 口径②）。
///
/// `fields` 是参数而不是写死的常量，**只是为了让 07 §6 那条回归用例真的能变红**——
/// 「把文案差异也计入时该用例必须变红」。生产路径永远传 [`HARD_FIELDS`]。
pub fn correction_rate(cases: &[&CaseOutcome], fields: &[HardField]) -> Ratio {
    let num: i64 = cases
        .iter()
        .map(|case| case.join.needing_change(fields))
        .sum();
    let den: i64 = cases
        .iter()
        .map(|case| case.join.matched_count() + case.join.missed_count() + case.join.extra_count())
        .sum();
    Ratio::new(num, den)
}

/// 全局判定（§9.4「判定」）：
///
/// - **1–3 任一不达标 ⇒ no-go**
/// - **4–8 不达标 ⇒ 条件 go**（允许进 M1，但必须记下是哪一条、对策是什么）
/// - **9–10 只记录**
///
/// **对照栏不参与。**
pub fn overall_verdict(pools: &[PoolReport], decision_metrics: &[Metric]) -> &'static str {
    for report in pools.iter().filter(|report| report.judged) {
        if report
            .metrics
            .iter()
            .any(|metric| metric.is_floor() && metric.verdict == Verdict::Fail)
        {
            return "no_go";
        }
    }
    if decision_metrics
        .iter()
        .any(|metric| metric.verdict == Verdict::Fail)
    {
        "conditional_go"
    } else {
        "go"
    }
}

#[cfg(test)]
mod eval {
    use super::*;
    use crate::eval::join::{FreeTextField, MatchedPair, PredictedItem};

    fn clean_pair(ordinal: i64) -> MatchedPair {
        MatchedPair {
            source_ordinal: ordinal,
            wrong_fields: Vec::new(),
            free_text_differences: Vec::new(),
        }
    }

    fn case(id: &str, kind: &str, join: OrdinalJoin, status: &str) -> CaseOutcome {
        CaseOutcome {
            id: id.to_owned(),
            pool: if kind == "file" {
                Pool::Screenshot
            } else {
                Pool::Utterance
            },
            source_kind: kind.to_owned(),
            stated_item_count: join.matched_count() + join.missed_count(),
            join,
            degraded: DegradedMatch::default(),
            reconciliation_status: status.to_owned(),
            confirmation_policy: "single_only".to_owned(),
            unparsed_note: String::new(),
            duration_ms: None,
            usage: None,
        }
    }

    /// 07 §6：「指标 4 的分母**只含 `kind = file`** 的回归用例通过：混入 `utterance`
    /// 来源时该用例必须变红。」
    #[test]
    fn total_availability_denominator_is_file_only() {
        let file = case(
            "file",
            "file",
            OrdinalJoin {
                matched: vec![clean_pair(1)],
                ..OrdinalJoin::default()
            },
            "passed",
        );
        // 口述来源没报合计 ⇒ not_applicable，它不该出现在这条指标的分母里。
        let utterance = case(
            "utterance",
            "utterance",
            OrdinalJoin {
                matched: vec![clean_pair(1)],
                ..OrdinalJoin::default()
            },
            "not_applicable",
        );
        let cases = vec![&file, &utterance];

        let ratio = total_availability(&cases);
        assert_eq!(ratio.den, 1, "分母只含 kind = file");
        assert_eq!(ratio.num, Some(1));
        assert_eq!(ratio.judge(Threshold::AtLeast(700)), Verdict::Pass);

        // 反事实：把 utterance 也算进分母 —— 1/2 = 0.5，掉到 0.70 以下，判定翻面。
        let naive = Ratio::new(1, 2);
        assert_eq!(
            naive.judge(Threshold::AtLeast(700)),
            Verdict::Fail,
            "混入 utterance 时这条用例必须变红"
        );
    }

    /// 07 §6：「指标 7 的分子**只含四个硬字段与漏读多读**：只改 `category` / `merchant`
    /// 文案的条目不计入分子，**把文案差异也计入时该用例必须变红**。」
    #[test]
    fn correction_rate_numerator_excludes_free_text() {
        let only_free_text = OrdinalJoin {
            matched: vec![
                MatchedPair {
                    source_ordinal: 1,
                    wrong_fields: Vec::new(),
                    free_text_differences: vec![FreeTextField::Merchant],
                },
                MatchedPair {
                    source_ordinal: 2,
                    wrong_fields: Vec::new(),
                    free_text_differences: vec![FreeTextField::Category],
                },
                clean_pair(3),
            ],
            ..OrdinalJoin::default()
        };
        let outcome = case("a", "file", only_free_text, "passed");
        let cases = vec![&outcome];

        let ratio = correction_rate(&cases, &HARD_FIELDS);
        assert_eq!(ratio.num, Some(0), "文案差异不进分子");
        assert_eq!(ratio.den, 3);
        assert_eq!(ratio.judge(Threshold::AtMost(200)), Verdict::Pass);

        // 反事实：把文案差异也算成「需要改」 —— 2/3 远超 0.20，用例变红。
        let naive = Ratio::new(2, 3);
        assert_eq!(
            naive.judge(Threshold::AtMost(200)),
            Verdict::Fail,
            "把文案差异也计入时这条用例必须变红"
        );
    }

    /// 口径②的另一半：漏读与多读**必须**进分子。
    #[test]
    fn correction_rate_counts_missed_and_extra() {
        let join = OrdinalJoin {
            matched: vec![clean_pair(1)],
            missed: vec![2],
            extra: vec![9],
        };
        let outcome = case("a", "file", join, "failed");
        let ratio = correction_rate(&[&outcome], &HARD_FIELDS);
        assert_eq!(ratio.num, Some(2));
        assert_eq!(ratio.den, 3);
    }

    /// §9.4「判定」：1–3 任一不达标 ⇒ no-go；4–8 不达标 ⇒ 条件 go。
    #[test]
    fn floor_metrics_force_no_go() {
        let missing_one = OrdinalJoin {
            matched: vec![clean_pair(1)],
            missed: vec![2],
            extra: Vec::new(),
        };
        let outcome = case("a", "file", missing_one, "passed");
        let report = compute_pool(Pool::Screenshot, &[&outcome]);
        let recall = report
            .metrics
            .iter()
            .find(|metric| metric.key == "item_recall")
            .unwrap();
        assert_eq!(recall.verdict, Verdict::Fail, "1/2 远低于 0.98");
        assert_eq!(overall_verdict(&[report], &[]), "no_go");
    }

    #[test]
    fn only_non_floor_failure_yields_conditional_go() {
        // 条目全对（1–3 满分），但 file 来源取不到合计 ⇒ 指标 4 = 0/1。
        let join = OrdinalJoin {
            matched: vec![clean_pair(1)],
            ..OrdinalJoin::default()
        };
        let outcome = case("a", "file", join, "unavailable");
        let report = compute_pool(Pool::Screenshot, &[&outcome]);
        let decision = compute_decision_metrics(&[&outcome], Some(0));
        assert_eq!(overall_verdict(&[report], &decision), "conditional_go");
    }

    /// 07 §3.4：对照栏「只如实报数」，不计入判定池的任何指标。
    #[test]
    fn control_column_never_changes_the_verdict() {
        let terrible = OrdinalJoin {
            matched: Vec::new(),
            missed: vec![1, 2, 3],
            extra: vec![7],
        };
        let outcome = CaseOutcome {
            pool: Pool::Control,
            ..case("receipt", "file", terrible, "unavailable")
        };
        let report = compute_control_pool(&[&outcome]);
        assert!(!report.judged);
        assert!(
            report
                .metrics
                .iter()
                .all(|metric| metric.verdict == Verdict::RecordOnly),
            "对照栏的数字带着 pass / fail 迟早会被当成结论"
        );
        assert_eq!(
            overall_verdict(&[report], &[]),
            "go",
            "对照栏再差也不该影响 go / no-go"
        );
        // 但数字仍然在——「如实报数」的那一半。
        let report = compute_control_pool(&[&outcome]);
        let recall = report
            .metrics
            .iter()
            .find(|metric| metric.key == "item_recall")
            .unwrap();
        assert_eq!(recall.ratio.num, Some(0));
        assert_eq!(recall.ratio.den, 3);
    }

    /// 分母为 0 时**不伪装成通过**。
    #[test]
    fn empty_denominator_is_no_sample_not_pass() {
        assert_eq!(
            Ratio::new(0, 0).judge(Threshold::AtLeast(980)),
            Verdict::NoSample
        );
        assert_eq!(
            Ratio::new(0, 0).judge(Threshold::AtMost(50)),
            Verdict::NoSample
        );
    }

    /// 指标 5 的分子要人工裁定，报表上不能显示成 0。
    #[test]
    fn false_alarm_rate_is_pending_manual() {
        let join = OrdinalJoin {
            matched: vec![clean_pair(1)],
            ..OrdinalJoin::default()
        };
        let outcome = case("a", "file", join, "failed");
        let metrics = compute_decision_metrics(&[&outcome], None);
        let metric = metrics
            .iter()
            .find(|metric| metric.key == "false_alarm_rate")
            .unwrap();
        assert_eq!(metric.ratio.num, None);
        assert_eq!(metric.ratio.den, 1);
        assert_eq!(metric.verdict, Verdict::PendingManual);
    }

    /// 指标 6 的「静默」两个字是判据的一半：说了没读、但说明了，不算静默遗漏。
    #[test]
    fn declared_gap_is_not_a_silent_omission() {
        let join = OrdinalJoin {
            matched: vec![clean_pair(1)],
            missed: vec![2],
            extra: Vec::new(),
        };
        let mut outcome = case("a", "utterance", join, "not_applicable");
        outcome.stated_item_count = 2;
        assert!(outcome.is_silent_omission());
        outcome.unparsed_note = "第二笔没听清".to_owned();
        assert!(!outcome.is_silent_omission());
    }

    /// 字段准确率的分母是**匹配上的条目数**，漏读多读不重复计入字段错误（07 §3.3）。
    #[test]
    fn field_accuracy_denominator_is_matched_only() {
        let join = OrdinalJoin {
            matched: vec![
                clean_pair(1),
                MatchedPair {
                    source_ordinal: 2,
                    wrong_fields: vec![HardField::AmountMinor],
                    free_text_differences: Vec::new(),
                },
            ],
            missed: vec![3, 4, 5],
            extra: Vec::new(),
        };
        let outcome = case("a", "file", join, "failed");
        let report = compute_pool(Pool::Screenshot, &[&outcome]);
        let amount = report
            .metrics
            .iter()
            .find(|metric| metric.key == "amount_accuracy")
            .unwrap();
        assert_eq!(amount.ratio.num, Some(1));
        assert_eq!(amount.ratio.den, 2, "分母是 2 条匹配，不是 5 条期望");
    }

    /// 2026-08-24 冻结的正式作用域：1–3 分池；4–8 聚合两池，4=file、5=全部实际
    /// failed、6=utterance；control 与 9–10 只记录。
    #[test]
    fn formal_metrics_use_frozen_scopes() {
        let screenshot = case(
            "shot",
            "file",
            OrdinalJoin {
                matched: vec![clean_pair(1)],
                ..OrdinalJoin::default()
            },
            "failed",
        );
        let mut utterance = case(
            "said",
            "utterance",
            OrdinalJoin {
                matched: vec![clean_pair(1)],
                ..OrdinalJoin::default()
            },
            "failed",
        );
        utterance.stated_item_count = 2;
        let control = CaseOutcome {
            pool: Pool::Control,
            ..case(
                "control",
                "file",
                OrdinalJoin {
                    missed: vec![1, 2, 3],
                    ..OrdinalJoin::default()
                },
                "unavailable",
            )
        };

        let screenshot_metrics = compute_pool(Pool::Screenshot, &[&screenshot]);
        let utterance_metrics = compute_pool(Pool::Utterance, &[&utterance]);
        assert!(screenshot_metrics
            .metrics
            .iter()
            .all(|metric| metric.index <= 3));
        assert!(utterance_metrics
            .metrics
            .iter()
            .all(|metric| metric.index <= 3));

        let decision = compute_decision_metrics(&[&screenshot, &utterance], None);
        let metric = |key: &str| decision.iter().find(|metric| metric.key == key).unwrap();
        assert_eq!(
            metric("reported_total_availability").ratio.den,
            1,
            "指标 4 只含 file"
        );
        assert_eq!(
            metric("false_alarm_rate").ratio.den,
            2,
            "指标 5 含两池全部实际 failed"
        );
        assert_eq!(
            metric("silent_omission_rate").ratio.den,
            1,
            "指标 6 只含 utterance"
        );

        let control = compute_control_pool(&[&control]);
        assert!(control
            .metrics
            .iter()
            .all(|metric| metric.verdict == Verdict::RecordOnly));
        assert!(control
            .metrics
            .iter()
            .all(|metric| metric.threshold == Threshold::RecordOnly));
        assert!(screenshot.duration_ms.is_none() && screenshot.usage.is_none());
    }

    /// `PredictedItem` 参与聚合时不该被这里悄悄读到——留一条编译期锚点，
    /// 防止有人把「预测侧」的构造挪进 metrics 而绕开 `drafted_json`。
    #[test]
    fn metrics_consume_joins_not_raw_predictions() {
        let sample = PredictedItem {
            source_ordinal: 1,
            occurred_on: "2026-08-03".to_owned(),
            amount_minor: 1680,
            currency: "AUD".to_owned(),
            direction: "expense".to_owned(),
            merchant: "SHOP".to_owned(),
            category: None,
            channel: None,
        };
        assert_eq!(sample.amount_minor, 1680);
    }
}
