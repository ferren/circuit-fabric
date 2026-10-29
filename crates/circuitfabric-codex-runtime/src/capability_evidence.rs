//! Versioned probe evidence, deliberately separate from adapter implementation flags.
use serde::{Deserialize, Serialize};

pub const CAPABILITY_EVIDENCE_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceLevel {
    Unverified,
    Documented,
    Simulated,
    HostVerified,
    Failed,
    Unsupported,
}

/// A host claim is valid only for the exact version and test scope recorded here.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CapabilityEvidence {
    pub schema_version: u32,
    pub capability: String,
    pub level: EvidenceLevel,
    pub runtime_version: Option<String>,
    pub sdk_version: Option<String>,
    pub scope: String,
    pub evidence: Vec<String>,
}

impl CapabilityEvidence {
    /// Method presence, mocks, and an unknown host version never establish support.
    #[must_use]
    pub fn is_verified(&self) -> bool {
        let present = |value: &Option<String>| value.as_ref().is_some_and(|v| !v.trim().is_empty());
        self.schema_version == CAPABILITY_EVIDENCE_VERSION
            && self.level == EvidenceLevel::HostVerified
            && present(&self.runtime_version)
            && present(&self.sdk_version)
            && !self.capability.trim().is_empty()
            && !self.scope.trim().is_empty()
            && !self.evidence.is_empty()
            && self.evidence.iter().all(|v| !v.trim().is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn support_requires_versioned_host_evidence() {
        let mut record = CapabilityEvidence {
            schema_version: 1,
            capability: "thread.resume".into(),
            level: EvidenceLevel::HostVerified,
            runtime_version: Some("codex-cli 0.165.0".into()),
            sdk_version: Some("app-server bundled 0.165.0".into()),
            scope: "restart and recall nonce".into(),
            evidence: vec!["trace.jsonl#resume".into()],
        };
        assert!(record.is_verified());
        for level in [
            EvidenceLevel::Unverified,
            EvidenceLevel::Documented,
            EvidenceLevel::Simulated,
            EvidenceLevel::Failed,
            EvidenceLevel::Unsupported,
        ] {
            record.level = level;
            assert!(!record.is_verified());
        }
        record.level = EvidenceLevel::HostVerified;
        record.sdk_version = None;
        assert!(!record.is_verified());
        record.sdk_version = Some("bundled".into());
        record.schema_version = 2;
        assert!(!record.is_verified());
        record.schema_version = 1;
        record.evidence.clear();
        assert!(!record.is_verified());
    }

    #[test]
    fn wire_format_rejects_unsupported_status_and_fields() {
        assert!(serde_json::from_str::<EvidenceLevel>("\"supported\"").is_err());
        let value = serde_json::json!({"schemaVersion":1,"capability":"x","level":"simulated",
            "runtimeVersion":null,"sdkVersion":null,"scope":"unit test","evidence":["mock"]});
        let record: CapabilityEvidence = serde_json::from_value(value.clone()).unwrap();
        assert!(!record.is_verified());
        assert_eq!(serde_json::to_value(record).unwrap(), value);
        let mut invalid = value;
        invalid["supported"] = true.into();
        assert!(serde_json::from_value::<CapabilityEvidence>(invalid).is_err());
    }
}
