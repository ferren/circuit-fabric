//! Declarative plugin inventory and governance records.
//!
//! Discovery deliberately reads only `plugin-manifest.json` files.  It never
//! loads a dynamic library, starts a process, imports a script, or evaluates an
//! entrypoint.  Lifecycle buttons in the desktop app modify this inventory and
//! append an audit record; materializing a plugin is a separate, explicit
//! runtime responsibility.

use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

const STORE_VERSION: u32 = 1;
const MANIFEST_FILE: &str = "plugin-manifest.json";
const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;
const MAX_DISCOVERY_DEPTH: usize = 16;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PluginState {
    Discovered,
    Installed,
    UpdateAvailable,
    Uninstalled,
}

impl PluginState {
    #[must_use]
    pub const fn label(&self) -> &'static str {
        match self {
            Self::Discovered => "Discovered",
            Self::Installed => "Installed",
            Self::UpdateAvailable => "Update available",
            Self::Uninstalled => "Uninstalled",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum VerificationStatus {
    /// A manifest self-declaration; this is not cryptographic verification.
    DeclaredVerified,
    Unsigned,
    Invalid,
    Unknown,
}

impl VerificationStatus {
    #[must_use]
    pub const fn label(&self) -> &'static str {
        match self {
            Self::DeclaredVerified => "Declared verified (not checked)",
            Self::Unsigned => "Unsigned",
            Self::Invalid => "Invalid",
            Self::Unknown => "Not checked",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HealthStatus {
    Unknown,
    Healthy,
    Degraded,
    Unavailable,
}

impl HealthStatus {
    #[must_use]
    pub const fn label(&self) -> &'static str {
        match self {
            Self::Unknown => "Not probed",
            Self::Healthy => "Reported healthy",
            Self::Degraded => "Reported degraded",
            Self::Unavailable => "Reported unavailable",
        }
    }
}

/// The only schema accepted during discovery.  The entrypoint is retained for
/// later supervised execution but is never opened by this module.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Manifest {
    id: String,
    version: String,
    kind: String,
    api_version: String,
    #[serde(default)]
    capabilities: Vec<String>,
    #[serde(default)]
    requested_permissions: Vec<String>,
    #[serde(default)]
    version_lock: Option<String>,
    #[serde(default)]
    signature: Option<ManifestSignature>,
    #[serde(default)]
    isolation: Option<String>,
    #[serde(default)]
    health: Option<String>,
    #[serde(default)]
    entrypoint: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ManifestSignature {
    status: String,
    #[serde(default)]
    signer: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginRecord {
    pub id: String,
    pub version: String,
    pub kind: String,
    pub api_version: String,
    pub capabilities: Vec<String>,
    pub requested_permissions: Vec<String>,
    pub granted_permissions: Vec<String>,
    pub version_lock: Option<String>,
    pub signature: VerificationStatus,
    pub signer: Option<String>,
    pub health: HealthStatus,
    pub isolation: String,
    pub state: PluginState,
    pub manifest_path: PathBuf,
    /// Retained for audit display only; discovery never attempts to execute it.
    pub entrypoint: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginAuditEvent {
    pub occurred_at_unix_seconds: u64,
    pub plugin_id: String,
    pub action: String,
    pub detail: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginGovernanceStore {
    #[serde(default = "store_version")]
    version: u32,
    #[serde(default)]
    records: Vec<PluginRecord>,
    #[serde(default)]
    audit: Vec<PluginAuditEvent>,
}

fn store_version() -> u32 {
    STORE_VERSION
}

impl PluginGovernanceStore {
    /// Reads persisted governance state. A malformed store is rejected rather
    /// than silently discarded, preserving the audit trail for investigation.
    pub fn load_or_default(path: &Path) -> Result<Self, String> {
        if !path.exists() {
            return Ok(Self { version: STORE_VERSION, ..Self::default() });
        }
        let raw = fs::read(path).map_err(|error| format!("read governance store: {error}"))?;
        let store = serde_json::from_slice::<Self>(&raw)
            .map_err(|error| format!("parse governance store: {error}"))?;
        if store.version != STORE_VERSION {
            return Err(format!("unsupported governance store version {}", store.version));
        }
        Ok(store)
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        let data = serde_json::to_vec_pretty(self)
            .map_err(|error| format!("serialize governance store: {error}"))?;
        let parent =
            path.parent().ok_or_else(|| "governance store has no parent directory".to_owned())?;
        fs::create_dir_all(parent)
            .map_err(|error| format!("create governance directory: {error}"))?;
        let temporary = path.with_extension(format!("{}.tmp", std::process::id()));
        fs::write(&temporary, data).map_err(|error| format!("write governance store: {error}"))?;
        fs::rename(&temporary, path).map_err(|error| format!("replace governance store: {error}"))
    }

    #[must_use]
    pub fn records(&self) -> &[PluginRecord] {
        &self.records
    }

    #[must_use]
    pub fn audit(&self) -> &[PluginAuditEvent] {
        &self.audit
    }

    /// Finds manifests below `root`, validates their JSON schema, and records
    /// the result. No plugin code, entrypoint, or package archive is executed.
    pub fn discover(&mut self, root: &Path) -> Result<usize, String> {
        let mut paths = Vec::new();
        collect_manifest_paths(root, &mut paths, 0)?;
        let mut discovered = 0;
        for path in paths {
            let raw =
                fs::read(&path).map_err(|error| format!("read {}: {error}", path.display()))?;
            if raw.len() as u64 > MAX_MANIFEST_BYTES {
                self.record_audit(
                    "<unknown>",
                    "manifest-rejected",
                    format!("{} exceeds 1 MiB", path.display()),
                );
                continue;
            }
            match serde_json::from_slice::<Manifest>(&raw) {
                Ok(manifest) => {
                    if manifest.id.trim().is_empty() || manifest.version.trim().is_empty() {
                        self.record_audit(
                            "<unknown>",
                            "manifest-rejected",
                            format!("{} has an empty id or version", path.display()),
                        );
                        continue;
                    }
                    let signature =
                        match manifest.signature.as_ref().map(|value| value.status.as_str()) {
                            Some("verified") => VerificationStatus::DeclaredVerified,
                            Some("unsigned") | None => VerificationStatus::Unsigned,
                            Some("invalid") => VerificationStatus::Invalid,
                            _ => VerificationStatus::Unknown,
                        };
                    let record = PluginRecord {
                        id: manifest.id.clone(),
                        version: manifest.version,
                        kind: manifest.kind,
                        api_version: manifest.api_version,
                        capabilities: manifest.capabilities,
                        requested_permissions: manifest.requested_permissions,
                        granted_permissions: Vec::new(),
                        version_lock: manifest.version_lock,
                        signature,
                        signer: manifest.signature.and_then(|value| value.signer),
                        health: match manifest.health.as_deref() {
                            Some("healthy") => HealthStatus::Healthy,
                            Some("degraded") => HealthStatus::Degraded,
                            Some("unavailable") => HealthStatus::Unavailable,
                            _ => HealthStatus::Unknown,
                        },
                        isolation: manifest.isolation.unwrap_or_else(|| "unspecified".to_owned()),
                        state: PluginState::Discovered,
                        manifest_path: path.clone(),
                        entrypoint: manifest.entrypoint,
                    };
                    self.records.retain(|existing| existing.id != record.id);
                    self.records.push(record);
                    self.record_audit(
                        &manifest.id,
                        "manifest-validated",
                        format!("{} read without executing plugin code", path.display()),
                    );
                    discovered += 1;
                }
                Err(error) => self.record_audit(
                    "<unknown>",
                    "manifest-rejected",
                    format!("{}: {error}", path.display()),
                ),
            }
        }
        self.records.sort_by(|left, right| left.id.cmp(&right.id));
        Ok(discovered)
    }

    pub fn install(&mut self, id: &str) -> Result<(), String> {
        self.transition(id, PluginState::Installed, "installed")
    }
    /// Records an approved update in the inventory. Package bytes are not
    /// fetched or executed here; a materializer must perform that separately.
    pub fn update(&mut self, id: &str) -> Result<(), String> {
        self.transition(id, PluginState::Installed, "updated")
    }
    pub fn uninstall(&mut self, id: &str) -> Result<(), String> {
        self.transition(id, PluginState::Uninstalled, "uninstalled")
    }

    pub fn revoke_permissions(&mut self, id: &str) -> Result<(), String> {
        let record = self
            .records
            .iter_mut()
            .find(|record| record.id == id)
            .ok_or_else(|| format!("plugin `{id}` is not in the inventory"))?;
        let count = record.granted_permissions.len();
        record.granted_permissions.clear();
        self.record_audit(
            id,
            "permissions-revoked",
            format!("revoked {count} granted permission(s)"),
        );
        Ok(())
    }

    fn transition(&mut self, id: &str, state: PluginState, action: &str) -> Result<(), String> {
        let record = self
            .records
            .iter_mut()
            .find(|record| record.id == id)
            .ok_or_else(|| format!("plugin `{id}` is not in the inventory"))?;
        record.state = state;
        self.record_audit(
            id,
            action,
            "inventory state changed; no plugin code was started".to_owned(),
        );
        Ok(())
    }

    fn record_audit(&mut self, plugin_id: &str, action: &str, detail: String) {
        let occurred_at_unix_seconds =
            SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |value| value.as_secs());
        self.audit.push(PluginAuditEvent {
            occurred_at_unix_seconds,
            plugin_id: plugin_id.to_owned(),
            action: action.to_owned(),
            detail,
        });
    }
}

fn collect_manifest_paths(
    root: &Path,
    result: &mut Vec<PathBuf>,
    depth: usize,
) -> Result<(), String> {
    if !root.exists() {
        return Ok(());
    }
    if depth > MAX_DISCOVERY_DEPTH {
        return Err(format!(
            "plugin manifest directory nesting exceeds {MAX_DISCOVERY_DEPTH} at {}",
            root.display()
        ));
    }
    for entry in fs::read_dir(root)
        .map_err(|error| format!("read plugin directory {}: {error}", root.display()))?
    {
        let entry = entry.map_err(|error| format!("read plugin directory entry: {error}"))?;
        let path = entry.path();
        let file_type =
            entry.file_type().map_err(|error| format!("inspect {}: {error}", path.display()))?;
        // Never follow user-controlled links while recursively discovering
        // manifests: they can escape the selected boundary or form a loop.
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            collect_manifest_paths(&path, result, depth + 1)?;
        } else if file_type.is_file() && path.file_name().is_some_and(|name| name == MANIFEST_FILE)
        {
            result.push(path);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovery_validates_a_manifest_without_touching_its_entrypoint() {
        let root =
            std::env::temp_dir().join(format!("circuitfabric-plugin-test-{}", std::process::id()));
        let plugin = root.join("example");
        fs::create_dir_all(&plugin).expect("test directory");
        fs::write(plugin.join(MANIFEST_FILE), r#"{"id":"example","version":"1.2.3","kind":"eda-backend","apiVersion":"circuitfabric-plugin/v1","capabilities":["inspect"],"requestedPermissions":["project.read"],"signature":{"status":"verified","signer":"CircuitFabric"},"isolation":"process","entrypoint":"must-not-run"}"#).expect("manifest");
        let mut store = PluginGovernanceStore::default();
        assert_eq!(store.discover(&root).expect("discover"), 1);
        let record = &store.records()[0];
        assert_eq!(record.id, "example");
        assert!(matches!(record.signature, VerificationStatus::DeclaredVerified));
        assert!(matches!(record.state, PluginState::Discovered));
        assert!(store.audit().iter().any(|event| event.action == "manifest-validated"));
        fs::remove_dir_all(root).expect("remove test directory");
    }

    #[test]
    fn revocation_is_recorded_in_the_audit_trail() {
        let mut store = PluginGovernanceStore {
            records: vec![PluginRecord {
                id: "example".to_owned(),
                version: "1".to_owned(),
                kind: "backend".to_owned(),
                api_version: "v1".to_owned(),
                capabilities: vec![],
                requested_permissions: vec!["project.read".to_owned()],
                granted_permissions: vec!["project.read".to_owned()],
                version_lock: None,
                signature: VerificationStatus::Unknown,
                signer: None,
                health: HealthStatus::Unknown,
                isolation: "process".to_owned(),
                state: PluginState::Installed,
                manifest_path: PathBuf::new(),
                entrypoint: None,
            }],
            ..PluginGovernanceStore::default()
        };
        store.revoke_permissions("example").expect("revoke");
        assert!(store.records()[0].granted_permissions.is_empty());
        assert_eq!(store.audit().last().expect("audit").action, "permissions-revoked");
    }
}
