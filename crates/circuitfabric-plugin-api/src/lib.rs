//! Plugin contracts. Discovery remains declarative and does not execute plugin code.

use circuitfabric_contracts::{ProjectId, SessionId, SnapshotHash};
use serde::{Deserialize, Serialize};

pub const PLUGIN_API_VERSION: &str = "circuitfabric-plugin/v1";

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PluginKind {
    AgentRuntime,
    EdaBackend,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Capability {
    Inspect,
    Preview,
    Apply,
    Readback,
    Rollback,
    Drc,
    VisualCapture,
    BridgeChat,
    BridgeContext,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BridgeUiManifest {
    pub entrypoint: String,
    pub host: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PluginManifest {
    pub manifest_version: u32,
    pub id: String,
    pub kind: PluginKind,
    pub api_version: String,
    pub transport: String,
    pub capabilities: Vec<Capability>,
    pub bridge_ui: Option<BridgeUiManifest>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BridgeSessionRequest {
    pub project_id: ProjectId,
    pub session_id: SessionId,
    pub observed_snapshot_hash: Option<SnapshotHash>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BridgeSessionContext {
    pub project_id: ProjectId,
    pub session_id: SessionId,
    pub document_scope: ProjectId,
    pub observed_snapshot_hash: Option<SnapshotHash>,
}

pub trait EdaBridge {
    type Error;

    fn manifest(&self) -> &PluginManifest;
    /// Binds an EDA-local session to the project-scoped `CircuitFabric` bridge protocol.
    ///
    /// # Errors
    ///
    /// Returns the bridge implementation's error when the request cannot be accepted.
    fn open_session(
        &mut self,
        request: BridgeSessionRequest,
    ) -> Result<BridgeSessionContext, Self::Error>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_uses_the_versioned_api_identifier() {
        let manifest = PluginManifest {
            manifest_version: 1,
            id: "example".to_owned(),
            kind: PluginKind::EdaBackend,
            api_version: PLUGIN_API_VERSION.to_owned(),
            transport: "bridge".to_owned(),
            capabilities: vec![Capability::Inspect],
            bridge_ui: None,
        };

        assert_eq!(manifest.api_version, "circuitfabric-plugin/v1");
    }
}
