use std::collections::BTreeSet;

use schemars::{schema_for, JsonSchema};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

use super::types::{
    CompleteSourceArgs, DraftTransactionArgs, ListPendingSourcesArgs, ReadSourceArgs,
    ReportSourceTotalArgs,
};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolSpec {
    pub name: &'static str,
    pub input_schema: Value,
    pub write_targets: &'static [&'static str],
    pub read_scope: &'static [&'static str],
}

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct CapabilityEntry {
    pub kind: String,
    pub provider: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capability: Option<String>,
}

pub fn m0_tool_registry() -> Vec<ToolSpec> {
    vec![
        spec::<ListPendingSourcesArgs>("list_pending_sources", &[], &["assigned_sources.metadata"]),
        spec::<ReadSourceArgs>("read_source", &[], &["assigned_sources.evidence"]),
        spec::<DraftTransactionArgs>("draft_transaction", &["draft_transactions"], &[]),
        spec::<ReportSourceTotalArgs>(
            "report_source_total",
            &["parse_attempts.reported_total_*"],
            &[],
        ),
        spec::<CompleteSourceArgs>(
            "complete_source",
            &[
                "parse_attempts.reported_item_count",
                "parse_attempts.unparsed_note",
            ],
            &[],
        ),
    ]
}

pub fn tool_surface_version() -> String {
    let mut registry = m0_tool_registry();
    registry.sort_by_key(|spec| spec.name);
    hash_json(&registry)
}

pub fn expected_capabilities() -> BTreeSet<CapabilityEntry> {
    m0_tool_registry()
        .into_iter()
        .map(|tool| CapabilityEntry {
            kind: "tool".to_owned(),
            provider: "daybook".to_owned(),
            name: Some(format!("mcp__daybook__{}", tool.name)),
            capability: None,
        })
        .collect()
}

pub fn effective_capability_hash(entries: &BTreeSet<CapabilityEntry>) -> String {
    hash_json(entries)
}

fn spec<T: JsonSchema>(
    name: &'static str,
    write_targets: &'static [&'static str],
    read_scope: &'static [&'static str],
) -> ToolSpec {
    ToolSpec {
        name,
        input_schema: serde_json::to_value(schema_for!(T)).expect("工具 schema 必须可序列化"),
        write_targets,
        read_scope,
    }
}

fn hash_json(value: &impl Serialize) -> String {
    let bytes = serde_json::to_vec(value).expect("canonical registry 必须可序列化");
    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(test)]
mod agent {
    use super::*;

    #[test]
    fn m0_tool_surface_is_exactly_five() {
        let names: Vec<_> = m0_tool_registry()
            .into_iter()
            .map(|tool| tool.name)
            .collect();
        assert_eq!(
            names,
            vec![
                "list_pending_sources",
                "read_source",
                "draft_transaction",
                "report_source_total",
                "complete_source"
            ]
        );
    }

    #[test]
    fn tool_surface_has_no_sql_tool() {
        for tool in m0_tool_registry() {
            assert!(!tool.name.contains("sql"));
            assert!(!tool.name.contains("command"));
            assert!(!tool.name.contains("file_write"));
        }
    }

    #[test]
    fn tools_cannot_write_fact_tables() {
        for target in m0_tool_registry()
            .iter()
            .flat_map(|tool| tool.write_targets)
        {
            assert_ne!(*target, "transactions");
            assert_ne!(*target, "items");
        }
    }

    #[test]
    fn tools_declare_read_scope() {
        for tool in m0_tool_registry() {
            assert!(!tool.read_scope.contains(&"all"));
            assert!(!tool.read_scope.contains(&"database"));
        }
    }

    #[test]
    fn tool_surface_version_covers_registry_schema() {
        let current = tool_surface_version();
        let mut registry = m0_tool_registry();
        registry[0].input_schema = serde_json::json!({ "type": "string" });
        registry.sort_by_key(|spec| spec.name);
        assert_ne!(current, hash_json(&registry));
    }

    #[test]
    fn capability_hash_covers_non_tool_entries() {
        let baseline = expected_capabilities();
        let mut with_hook = baseline.clone();
        with_hook.insert(CapabilityEntry {
            kind: "hook".to_owned(),
            provider: "claude-code".to_owned(),
            name: None,
            capability: Some("PreToolUse:*".to_owned()),
        });
        assert_ne!(
            effective_capability_hash(&baseline),
            effective_capability_hash(&with_hook)
        );
    }
}
