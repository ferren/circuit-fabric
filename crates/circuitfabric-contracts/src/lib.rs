//! Stable data contracts shared across `CircuitFabric` services and plugins.
//!
//! The types in this crate deliberately contain no EDA SDK, network client, or secret value.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub const LOGICAL_CIRCUIT_SCHEMA_VERSION: u32 = 1;

pub type ComponentId = String;
pub type DocumentId = String;
pub type NetId = String;
pub type ProjectId = String;
pub type SessionId = String;
pub type SnapshotHash = String;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Project {
    pub id: ProjectId,
    pub name: String,
    pub description: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SnapshotAuthority {
    Observed,
    Planned,
    Verified,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FactStatus {
    Passed,
    Failed,
    Inconclusive,
    NotRun,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Info,
    Warning,
    Error,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EvidenceRef {
    pub document_id: DocumentId,
    pub content_hash: String,
    pub locator: String,
    pub excerpt: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Pin {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Component {
    pub id: ComponentId,
    pub reference: String,
    pub value: Option<String>,
    pub pins: Vec<Pin>,
    pub evidence: Vec<EvidenceRef>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PinRef {
    pub component_id: ComponentId,
    pub pin_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Net {
    pub id: NetId,
    pub name: Option<String>,
    pub pins: Vec<PinRef>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ConstraintResult {
    pub constraint_id: String,
    pub layer: String,
    pub subject_refs: Vec<String>,
    pub status: FactStatus,
    pub severity: Severity,
    pub evidence_refs: Vec<EvidenceRef>,
    pub snapshot_hash: SnapshotHash,
    pub explanation: String,
    pub recommendation: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct LogicalCircuitSnapshot {
    pub schema_version: u32,
    pub snapshot_hash: SnapshotHash,
    pub logical_hash: String,
    pub physical_hash: Option<String>,
    pub parent_snapshot_hash: Option<SnapshotHash>,
    pub authority: SnapshotAuthority,
    pub components: Vec<Component>,
    pub nets: Vec<Net>,
    pub application_semantics: BTreeMap<String, String>,
    pub constraints: Vec<ConstraintResult>,
    pub evidence: Vec<EvidenceRef>,
}

impl LogicalCircuitSnapshot {
    #[must_use]
    pub fn empty(authority: SnapshotAuthority) -> Self {
        Self {
            schema_version: LOGICAL_CIRCUIT_SCHEMA_VERSION,
            snapshot_hash: String::new(),
            logical_hash: String::new(),
            physical_hash: None,
            parent_snapshot_hash: None,
            authority,
            components: Vec::new(),
            nets: Vec::new(),
            application_semantics: BTreeMap::new(),
            constraints: Vec::new(),
            evidence: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct IrPatch {
    pub base_snapshot_hash: SnapshotHash,
    pub operations: Vec<PatchOperation>,
    pub rationale: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PatchOperation {
    AddComponent { component: Component },
    RemoveComponent { component_id: ComponentId },
    AddNet { net: Net },
    AnnotateSemantic { key: String, value: String },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ChangeSet {
    pub id: String,
    pub base_snapshot_hash: SnapshotHash,
    pub target_snapshot_hash: SnapshotHash,
    pub plan_hash: String,
    pub approved: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DocumentKind {
    Pdf,
    Word,
    Markdown,
    Bom,
    Netlist,
    Text,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DocumentRecord {
    pub id: DocumentId,
    pub project_id: ProjectId,
    pub kind: DocumentKind,
    pub title: String,
    pub source_locator: String,
    pub content_hash: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DocumentFragment {
    pub document_id: DocumentId,
    pub content_hash: String,
    pub locator: String,
    pub text: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EvidencePackage {
    pub project_id: ProjectId,
    pub query: String,
    pub fragments: Vec<DocumentFragment>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SecretReference {
    pub id: String,
    pub provider: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_snapshot_uses_the_current_schema() {
        let snapshot = LogicalCircuitSnapshot::empty(SnapshotAuthority::Observed);

        assert_eq!(snapshot.schema_version, LOGICAL_CIRCUIT_SCHEMA_VERSION);
        assert!(snapshot.snapshot_hash.is_empty());
    }
}
