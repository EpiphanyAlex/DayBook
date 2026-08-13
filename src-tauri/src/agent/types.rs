use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ListPendingSourcesArgs {}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReadSourceArgs {
    pub source_id: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceSpan {
    pub start: i64,
    pub end: i64,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DraftTransactionArgs {
    pub source_id: String,
    pub evidence_text: String,
    pub source_ordinal: i64,
    pub evidence_span: Option<EvidenceSpan>,
    pub occurred_on: String,
    pub amount_minor: String,
    pub currency: String,
    pub base_amount_minor: Option<String>,
    pub base_currency: Option<String>,
    pub rate_ppm: Option<String>,
    pub direction: String,
    pub merchant: String,
    pub category: Option<String>,
    pub channel: Option<String>,
    pub confidence: Option<i64>,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReportSourceTotalArgs {
    pub source_id: String,
    pub amount_minor: String,
    pub currency: String,
    pub kind: String,
    pub evidence_text: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CompleteSourceArgs {
    pub source_id: String,
    pub item_count: i64,
    pub unparsed_note: String,
}
