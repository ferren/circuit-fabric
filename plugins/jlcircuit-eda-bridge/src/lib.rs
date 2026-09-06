//! `JLCircuit` EDA bridge scaffold.
//!
//! This crate is intentionally independent from JLCircuit-Agent. It only establishes the
//! `CircuitFabric` plugin contract; no `JLCircuit` SDK or copied bridge code is present here.

pub mod probe;
pub mod supervision;

use circuitfabric_plugin_api::{
    BridgeSessionContext, BridgeSessionRequest, BridgeUiManifest, Capability, EdaBridge,
    PLUGIN_API_VERSION, PluginKind, PluginManifest,
};
use thiserror::Error;

#[derive(Debug, Error, Eq, PartialEq)]
pub enum BridgeError {
    #[error("a bridge session requires a project identifier")]
    MissingProject,
    #[error("a bridge session requires a session identifier")]
    MissingSession,
}

pub struct JlcircuitEdaBridge {
    manifest: PluginManifest,
}

impl Default for JlcircuitEdaBridge {
    fn default() -> Self {
        Self {
            manifest: PluginManifest {
                manifest_version: 1,
                id: "jlcircuit-eda".to_owned(),
                kind: PluginKind::EdaBackend,
                api_version: PLUGIN_API_VERSION.to_owned(),
                transport: "bridge".to_owned(),
                capabilities: vec![
                    Capability::Inspect,
                    Capability::Preview,
                    Capability::Apply,
                    Capability::Readback,
                    Capability::Rollback,
                    Capability::Drc,
                    Capability::VisualCapture,
                    Capability::BridgeChat,
                    Capability::BridgeContext,
                ],
                bridge_ui: Some(BridgeUiManifest {
                    entrypoint: "jlcircuit-eda-bridge".to_owned(),
                    host: "jlcircuit-eda".to_owned(),
                }),
            },
        }
    }
}

impl EdaBridge for JlcircuitEdaBridge {
    type Error = BridgeError;

    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    fn open_session(
        &mut self,
        request: BridgeSessionRequest,
    ) -> Result<BridgeSessionContext, Self::Error> {
        if request.project_id.is_empty() {
            return Err(BridgeError::MissingProject);
        }
        if request.session_id.is_empty() {
            return Err(BridgeError::MissingSession);
        }

        Ok(BridgeSessionContext {
            document_scope: request.project_id.clone(),
            project_id: request.project_id,
            session_id: request.session_id,
            observed_snapshot_hash: request.observed_snapshot_hash,
        })
    }
}

#[cfg(test)]
mod tests {
    use circuitfabric_plugin_api::{Capability, EdaBridge};

    use super::*;

    #[test]
    fn a_session_is_bound_to_its_project_document_scope() {
        let mut bridge = JlcircuitEdaBridge::default();
        let context = bridge
            .open_session(BridgeSessionRequest {
                project_id: "project-1".to_owned(),
                session_id: "session-1".to_owned(),
                observed_snapshot_hash: None,
            })
            .expect("valid session");

        assert_eq!(context.document_scope, "project-1");
        assert!(bridge.manifest().capabilities.contains(&Capability::BridgeChat));
    }
}
