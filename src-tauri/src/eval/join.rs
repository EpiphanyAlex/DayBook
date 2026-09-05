//! 按 `source_ordinal` 的 full outer join（07 §3.2）。
//!
//! ```sql
//! -- 概念上就这一句
//! expected FULL OUTER JOIN drafted ON expected.source_ordinal = drafted.source_ordinal
//! ```
//!
//! **别把它实现成动态规划。** ordinal 是显式的、两侧都唯一的键，配对就是按键相等去连；
//! 没有 substitution cost、没有回溯、也没有「对齐路径」这回事。**保序是它的性质，不是
//! 它的算法**——下面用 `BTreeMap` 按键升序走一遍，保序是免费的。
//!
//! **为什么不拿被评的字段当匹配键**：初稿用 `(occurred_on, amount_minor, currency)`
//! 精确相等来配对，那让字段准确率变成恒真命题——能配上的行这三个字段按定义全对，
//! 任一字段错的行配不上、变成「一漏读 + 一多读」。于是最该量的三项要么恒为 100%、
//! 要么根本算不出来。
//!
//! **位置报错了会怎样**：join 本身不会失败。期望 5 条而草稿的 ordinal 是 `1,2,7,8,9`，
//! 它会老老实实报出 3 漏读 + 3 多读——那已经是一个正确且有信息量的结果。

use std::collections::BTreeMap;

use serde::Serialize;

use super::expected::ExpectedItem;

/// 预测侧的一条——**来自 `drafted_json`，不是草稿行的当前值**（07 §3.2）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PredictedItem {
    pub source_ordinal: i64,
    pub occurred_on: String,
    pub amount_minor: i64,
    pub currency: String,
    pub direction: String,
    pub merchant: String,
    pub category: Option<String>,
    pub channel: Option<String>,
}

/// 被评的四个硬字段（`docs/PRD.md` §9.4「干净」的口径 + 口径②）。
///
/// **`category` / `channel` / `merchant` 文案不在其中**：前两者由记忆规则演进、不是解析
/// 能力，后者的判据是 07 §5 R1 待决——没定之前不能拿它当门槛的一部分。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HardField {
    AmountMinor,
    Currency,
    OccurredOn,
    Direction,
}

pub const HARD_FIELDS: [HardField; 4] = [
    HardField::AmountMinor,
    HardField::Currency,
    HardField::OccurredOn,
    HardField::Direction,
];

/// 只在 07 §6 的口径②回归用例里用：把自由文本也当成「需要改」的那一套字段。
///
/// **它存在的唯一理由是让那条回归用例真的能变红**——「把文案差异也计入时该用例必须
/// 变红」，而一个永远变不红的回归用例和没有是一回事。生产路径一律用 [`HARD_FIELDS`]。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FreeTextField {
    Merchant,
    Category,
    Channel,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchedPair {
    pub source_ordinal: i64,
    /// 四个硬字段里错了哪些。空 = 这一对全对。
    pub wrong_fields: Vec<HardField>,
    /// 自由文本字段里差了哪些。**不进任何分子**，只是 diff 表上的信息。
    pub free_text_differences: Vec<FreeTextField>,
}

impl MatchedPair {
    pub fn is_clean(&self) -> bool {
        self.wrong_fields.is_empty()
    }
}

/// join 的结果。**漏读与多读分列**——条数相等不代表读对了：漏一条同时多一条，条数一样。
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrdinalJoin {
    pub matched: Vec<MatchedPair>,
    /// 只有期望侧 ⇒ 漏读（false negative）。存的是 ordinal。
    pub missed: Vec<i64>,
    /// 只有草稿侧 ⇒ 多读（false positive）。存的是 ordinal。
    pub extra: Vec<i64>,
}

impl OrdinalJoin {
    pub fn matched_count(&self) -> i64 {
        i64::try_from(self.matched.len()).unwrap_or(i64::MAX)
    }

    pub fn missed_count(&self) -> i64 {
        i64::try_from(self.missed.len()).unwrap_or(i64::MAX)
    }

    pub fn extra_count(&self) -> i64 {
        i64::try_from(self.extra.len()).unwrap_or(i64::MAX)
    }

    /// 这一对里有几条「需要改」——口径②：四个硬字段错的 + 漏读 + 多读。
    pub fn needing_change(&self, fields: &[HardField]) -> i64 {
        let wrong = self
            .matched
            .iter()
            .filter(|pair| pair.wrong_fields.iter().any(|field| fields.contains(field)))
            .count();
        i64::try_from(wrong).unwrap_or(i64::MAX) + self.missed_count() + self.extra_count()
    }

    /// 这个来源「干净」吗——**条目完整 + 那四个硬字段全对**（`docs/PRD.md` §9.4）。
    pub fn is_clean_source(&self) -> bool {
        self.missed.is_empty()
            && self.extra.is_empty()
            && self.matched.iter().all(MatchedPair::is_clean)
    }
}

/// 两侧按 `source_ordinal` 连。**升序遍历键的并集，因此保序。**
pub fn ordinal_full_outer_join(
    expected: &[ExpectedItem],
    predicted: &[PredictedItem],
) -> OrdinalJoin {
    let expected_by_ordinal: BTreeMap<i64, &ExpectedItem> = expected
        .iter()
        .map(|item| (item.source_ordinal, item))
        .collect();
    let predicted_by_ordinal: BTreeMap<i64, &PredictedItem> = predicted
        .iter()
        .map(|item| (item.source_ordinal, item))
        .collect();

    let mut ordinals: Vec<i64> = expected_by_ordinal.keys().copied().collect();
    ordinals.extend(predicted_by_ordinal.keys().copied());
    ordinals.sort_unstable();
    ordinals.dedup();

    let mut join = OrdinalJoin::default();
    for ordinal in ordinals {
        match (
            expected_by_ordinal.get(&ordinal),
            predicted_by_ordinal.get(&ordinal),
        ) {
            (Some(left), Some(right)) => join.matched.push(compare(left, right)),
            (Some(_), None) => join.missed.push(ordinal),
            (None, Some(_)) => join.extra.push(ordinal),
            (None, None) => unreachable!("ordinal 取自两侧键的并集"),
        }
    }
    join
}

fn compare(left: &ExpectedItem, right: &PredictedItem) -> MatchedPair {
    let mut wrong_fields = Vec::new();
    if left.amount_minor.parse::<i64>().ok() != Some(right.amount_minor) {
        wrong_fields.push(HardField::AmountMinor);
    }
    if left.currency != right.currency {
        wrong_fields.push(HardField::Currency);
    }
    if left.occurred_on != right.occurred_on {
        wrong_fields.push(HardField::OccurredOn);
    }
    if left.direction != right.direction {
        wrong_fields.push(HardField::Direction);
    }

    let mut free_text_differences = Vec::new();
    if left
        .merchant
        .as_ref()
        .is_some_and(|merchant| merchant != &right.merchant)
    {
        free_text_differences.push(FreeTextField::Merchant);
    }

    MatchedPair {
        source_ordinal: left.source_ordinal,
        wrong_fields,
        free_text_differences,
    }
}

/// **降级的集合匹配——只用于诊断，不进正式指标**（07 §3.2，2026-08-10 收窄）。
///
/// 怀疑「其实是位置报错了、内容其实对得上」时，按 `(日期, 金额, 币种)` 跑一次集合匹配。
/// 它回答的是「这批条目的内容到底在不在」。但是——
///
/// - **正式指标一律以 ordinal join 为准**，包括 precision / recall 与全部字段准确率
/// - **降级结果不覆盖、不混入、不替换任何一个正式数**，在 diff 表上单独成栏并标注「诊断用」
/// - **两者不一致本身就是结论**：内容对得上而 ordinal 对不上 ⇒ agent 报位置不可靠（07 §5 R9）
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DegradedMatch {
    /// 恒为 `"诊断用"`——序列化出去带着标签，省得报表那边忘了标（07 §6）。
    pub label: String,
    pub matched: i64,
    pub expected_only: i64,
    pub predicted_only: i64,
}

pub fn degraded_set_match(expected: &[ExpectedItem], predicted: &[PredictedItem]) -> DegradedMatch {
    let mut pool: Vec<(String, i64, String)> = expected
        .iter()
        .map(|item| {
            (
                item.occurred_on.clone(),
                item.amount_minor.parse::<i64>().unwrap_or(i64::MIN),
                item.currency.clone(),
            )
        })
        .collect();

    let mut matched = 0_i64;
    let mut predicted_only = 0_i64;
    for item in predicted {
        let key = (
            item.occurred_on.clone(),
            item.amount_minor,
            item.currency.clone(),
        );
        if let Some(position) = pool.iter().position(|candidate| candidate == &key) {
            pool.swap_remove(position);
            matched += 1;
        } else {
            predicted_only += 1;
        }
    }

    DegradedMatch {
        label: "诊断用".to_owned(),
        matched,
        expected_only: i64::try_from(pool.len()).unwrap_or(i64::MAX),
        predicted_only,
    }
}

#[cfg(test)]
mod eval {
    use super::*;

    fn expected(ordinal: i64, day: &str, amount: &str) -> ExpectedItem {
        ExpectedItem {
            source_ordinal: ordinal,
            occurred_on: day.to_owned(),
            amount_minor: amount.to_owned(),
            currency: "AUD".to_owned(),
            direction: "expense".to_owned(),
            evidence_span_start: None,
            evidence_span_end: None,
            merchant: Some("SHOP".to_owned()),
        }
    }

    fn predicted(ordinal: i64, day: &str, amount: i64) -> PredictedItem {
        PredictedItem {
            source_ordinal: ordinal,
            occurred_on: day.to_owned(),
            amount_minor: amount,
            currency: "AUD".to_owned(),
            direction: "expense".to_owned(),
            merchant: "SHOP".to_owned(),
            category: None,
            channel: None,
        }
    }

    /// 07 §6：两侧都按 `source_ordinal` 做 full outer join。
    #[test]
    fn alignment_uses_reported_ordinal() {
        // 内容完全相同，只有 ordinal 不同 —— 按内容会配上，按 ordinal 配不上。
        let left = vec![expected(1, "2026-08-03", "1680")];
        let right = vec![predicted(7, "2026-08-03", 1680)];
        let join = ordinal_full_outer_join(&left, &right);
        assert!(join.matched.is_empty(), "ordinal 不同就不该配上");
        assert_eq!(join.missed, vec![1]);
        assert_eq!(join.extra, vec![7]);
    }

    /// 07 §6：「构造一条『位置对得上但金额读错』的用例，断言它记为 1 条匹配 + 1 个金额
    /// 错误，而不是 1 漏读 + 1 多读。**把匹配键改回 `(日期, 金额, 币种)` 时该用例必须变红**。」
    #[test]
    fn field_accuracy_is_not_vacuous() {
        let left = vec![expected(1, "2026-08-03", "1680")];
        let right = vec![predicted(1, "2026-08-03", 16800)]; // 168 读成 1680
        let join = ordinal_full_outer_join(&left, &right);

        assert_eq!(join.matched_count(), 1, "位置对得上就该配对");
        assert_eq!(join.missed_count(), 0);
        assert_eq!(join.extra_count(), 0);
        assert_eq!(join.matched[0].wrong_fields, vec![HardField::AmountMinor]);

        // 「改回 `(日期, 金额, 币种)` 时必须变红」——这就是那个反事实：换成按内容配对，
        // 同一份输入会变成 1 漏读 + 1 多读，上面四条断言里有三条会挂。
        let by_content = degraded_set_match(&left, &right);
        assert_eq!(by_content.matched, 0);
        assert_eq!(by_content.expected_only, 1);
        assert_eq!(by_content.predicted_only, 1);
    }

    /// 07 §6：「期望 5 条实得 4 条且其中一条是多读时，报 1 漏读 + 1 多读，**不是「条数差 1」**。」
    #[test]
    fn missed_and_extra_items_are_counted() {
        let left = vec![
            expected(1, "2026-08-01", "100"),
            expected(2, "2026-08-02", "200"),
            expected(3, "2026-08-03", "300"),
            expected(4, "2026-08-04", "400"),
            expected(5, "2026-08-05", "500"),
        ];
        // 读到 1/2/3/4 里的四条中，把第 5 条读成了不存在的第 9 条 —— 条数 5 对 5。
        let right = vec![
            predicted(1, "2026-08-01", 100),
            predicted(2, "2026-08-02", 200),
            predicted(3, "2026-08-03", 300),
            predicted(4, "2026-08-04", 400),
            predicted(9, "2026-08-09", 900),
        ];
        let join = ordinal_full_outer_join(&left, &right);
        assert_eq!(join.matched_count(), 4);
        assert_eq!(join.missed, vec![5], "漏读单独计数");
        assert_eq!(join.extra, vec![9], "多读单独计数");
        assert_eq!(
            left.len(),
            right.len(),
            "条数相等——正是这一点让「条数对不对」这个口径没用"
        );
    }

    /// 07 §6：「中间漏掉一条时，其后各条仍与正确的期望条目对齐，不整体错位。」
    #[test]
    fn alignment_is_order_preserving() {
        let left = vec![
            expected(1, "2026-08-01", "100"),
            expected(2, "2026-08-02", "200"),
            expected(3, "2026-08-03", "300"),
        ];
        // 中间那条没读到，第 3 条照旧报 ordinal = 3。
        let right = vec![
            predicted(1, "2026-08-01", 100),
            predicted(3, "2026-08-03", 300),
        ];
        let join = ordinal_full_outer_join(&left, &right);
        assert_eq!(join.missed, vec![2]);
        assert!(join.extra.is_empty());
        assert_eq!(join.matched_count(), 2);
        for pair in &join.matched {
            assert!(
                pair.is_clean(),
                "第 3 条不该被错位对到第 2 条上：ordinal {} 出现了 {:?}",
                pair.source_ordinal,
                pair.wrong_fields
            );
        }
    }

    /// 07 §6：「构造一个『ordinal 全错但内容全对』的用例，**正式 precision / recall 仍按
    /// ordinal join 报（即 0）**，集合匹配的结果只出现在诊断栏。」
    #[test]
    fn degraded_match_never_enters_official_metrics() {
        let left = vec![
            expected(1, "2026-08-01", "100"),
            expected(2, "2026-08-02", "200"),
        ];
        let right = vec![
            predicted(11, "2026-08-01", 100),
            predicted(12, "2026-08-02", 200),
        ];
        let join = ordinal_full_outer_join(&left, &right);
        assert_eq!(join.matched_count(), 0, "正式口径按 ordinal，全不配对");
        assert_eq!(join.missed_count(), 2);
        assert_eq!(join.extra_count(), 2);

        let degraded = degraded_set_match(&left, &right);
        assert_eq!(degraded.matched, 2, "内容其实全对——这本身就是结论（R9）");
        // 诊断结果与正式结果分属两个字段，结构上就混不进去。
        assert_ne!(degraded.matched, join.matched_count());
    }

    /// 07 §6：「诊断用的集合匹配结果在 diff 表上单独成栏并标注『诊断用』。」
    #[test]
    fn degraded_match_is_labelled() {
        let degraded = degraded_set_match(&[], &[]);
        assert_eq!(degraded.label, "诊断用");
        let json = serde_json::to_value(&degraded).unwrap();
        assert_eq!(json["label"], "诊断用", "标签必须随结果一起序列化出去");
    }

    /// 口径②：自由文本的差异不进「需要改」的分子。
    #[test]
    fn free_text_difference_is_not_a_wrong_field() {
        let left = vec![expected(1, "2026-08-01", "100")];
        let right = vec![PredictedItem {
            merchant: "SHOP 1234 CENTRAL".to_owned(),
            category: Some("餐饮".to_owned()),
            ..predicted(1, "2026-08-01", 100)
        }];
        let join = ordinal_full_outer_join(&left, &right);
        assert!(join.matched[0].wrong_fields.is_empty());
        assert_eq!(
            join.matched[0].free_text_differences,
            vec![FreeTextField::Merchant]
        );
        assert!(join.is_clean_source(), "商户文案不同不算脏");
    }
}
