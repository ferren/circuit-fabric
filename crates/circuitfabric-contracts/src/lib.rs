//! Stable data contracts shared across `CircuitFabric` services and plugins.
//!
//! The types in this crate deliberately contain no EDA SDK, network client, or secret value.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub mod datasheet;

pub use datasheet::{
    DATASHEET_EXTRACTION_SCHEMA_VERSION, DatasheetExtraction, DatasheetOverview,
    DatasheetParameter, DatasheetPin, DatasheetPinKind, MAX_DATASHEET_FEATURES,
    MAX_DATASHEET_PARAMETERS, MAX_DATASHEET_PINS,
};

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
    SetComponentValue { component_id: ComponentId, value: String },
    RenameNet { net_id: NetId, name: String },
    /// 新增元件只描述语义；逻辑 ID（`component:<reference>`）与证据由系统派生，
    /// 模型不得伪造快照内部字段。`clone_from` 为可选的观测位号：放置复用其库件
    /// 身份（libraryUuid/componentUuid），缺失或不可解析时该步骤不可物化。
    AddComponent {
        reference: String,
        value: Option<String>,
        pins: Vec<Pin>,
        clone_from: Option<String>,
    },
    RemoveComponent { component_id: ComponentId },
    /// 新增网络：同名既有网络视为显式挂接成员，不是隐式合并网络。
    AddNet { name: String, pins: Vec<PinRef> },
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

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DocumentKind {
    Pdf,
    Word,
    Markdown,
    Html,
    Excel,
    Presentation,
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

    #[test]
    fn patch_operations_use_author_friendly_flat_payloads() {
        // 模型提交格式：新增元件/网络不携带系统派生的 id/evidence 字段。
        let patch: IrPatch = serde_json::from_str(
            r#"{"base_snapshot_hash":"sha256:b","operations":[
                {"remove_component":{"component_id":"component:U1"}},
                {"rename_net":{"net_id":"net:","name":"GND"}},
                {"add_component":{"reference":"J1","value":"HOST_SPI_8P","clone_from":"P1",
                    "pins":[{"id":"1","name":"VCC"},{"id":"2","name":"GND"}]}},
                {"add_component":{"reference":"R9","pins":[{"id":"1","name":"A"}]}},
                {"add_net":{"name":"BS","pins":[{"component_id":"component:R9","pin_id":"1"}]}}
            ],"rationale":"无源转接板"}"#,
        )
        .expect("扁平载荷必须可解析");
        assert_eq!(
            patch.operations[2],
            PatchOperation::AddComponent {
                reference: "J1".to_owned(),
                value: Some("HOST_SPI_8P".to_owned()),
                pins: vec![
                    Pin { id: "1".to_owned(), name: "VCC".to_owned() },
                    Pin { id: "2".to_owned(), name: "GND".to_owned() },
                ],
                clone_from: Some("P1".to_owned()),
            },
            "clone_from 可选携带"
        );

        assert_eq!(
            patch.operations[3],
            PatchOperation::AddComponent {
                reference: "R9".to_owned(),
                value: None,
                pins: vec![Pin { id: "1".to_owned(), name: "A".to_owned() }],
                clone_from: None,
            },
            "value 可省略"
        );
        assert_eq!(
            patch.operations[4],
            PatchOperation::AddNet {
                name: "BS".to_owned(),
                pins: vec![PinRef { component_id: "component:R9".to_owned(), pin_id: "1".to_owned() }],
            }
        );

        let serialized = serde_json::to_string(&patch).expect("序列化不应失败");
        let round_trip: IrPatch = serde_json::from_str(&serialized).expect("往返解析");
        assert_eq!(round_trip, patch);
    }
}
