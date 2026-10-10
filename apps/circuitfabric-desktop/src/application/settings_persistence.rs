//! Explicit save boundaries for the desktop's independently edited settings.

use circuitfabric_codex_runtime::{
    LlmProviderSettings, RuntimeError, RuntimeSettings, ToolAuthorizationSettings,
    tools::ToolCatalog,
};
use std::path::Path;

/// One independently saved section of the runtime settings.
///
/// The `DefaultProvider`, `ProviderEnabled`, and `RemoveProvider` variants carry
/// the provider quick actions from the Agents & tools detail pane: each switches
/// exactly its own field on the saved copy, so unsaved provider form drafts on
/// the page are never pulled into the transaction.
#[derive(Clone, Debug)]
pub enum SettingsUpdate {
    Providers { providers: Vec<LlmProviderSettings>, default_provider_id: String },
    DefaultProvider { id: String },
    ProviderEnabled { id: String, enabled: bool },
    RemoveProvider { id: String },
    Codex { command: String, provider_id: String },
    Claude { command: String, provider_id: String },
    Dsh { command: String, provider_id: String },
    Bridge { listen_address: String },
    Catalog(ToolCatalog),
    Authorizations(ToolAuthorizationSettings),
    RemoveResource { kind: circuitfabric_codex_runtime::ToolAuthorizationKind, id: String },
}

impl SettingsUpdate {
    fn apply(self, saved: &mut RuntimeSettings) -> Result<(), RuntimeError> {
        match self {
            Self::Providers { providers, default_provider_id } => {
                saved.providers = providers;
                saved.default_provider_id = default_provider_id;
            }
            // An unknown or disabled target is rejected by the whole-file validation
            // in `RuntimeSettings::save`, with the same message a dialog save yields.
            Self::DefaultProvider { id } => saved.default_provider_id = id,
            Self::ProviderEnabled { id, enabled } => {
                let provider = saved.providers.iter_mut().find(|provider| provider.id == id);
                if let Some(provider) = provider {
                    provider.enabled = enabled;
                } else {
                    return Err(RuntimeError::InvalidSettings(format!(
                        "provider `{id}` is not configured"
                    )));
                }
            }
            Self::RemoveProvider { id } => {
                if !saved.providers.iter().any(|provider| provider.id == id) {
                    return Err(RuntimeError::InvalidSettings(format!(
                        "provider `{id}` is not configured"
                    )));
                }
                if saved.providers.len() == 1 {
                    return Err(RuntimeError::InvalidSettings(
                        "at least one LLM provider must be configured".to_owned(),
                    ));
                }
                saved.providers.retain(|provider| provider.id != id);
                if saved.default_provider_id == id {
                    saved.default_provider_id = saved.providers[0].id.clone();
                }
            }
            Self::Codex { command, provider_id } => {
                saved.codex.command = command;
                saved.adapters.codex_provider_id = provider_id;
            }
            Self::Claude { command, provider_id } => {
                saved.adapters.claude_command = command;
                saved.adapters.claude_provider_id = provider_id;
            }
            Self::Dsh { command, provider_id } => {
                saved.adapters.dsh_command = command;
                saved.adapters.dsh_provider_id = provider_id;
            }
            Self::Bridge { listen_address } => saved.bridge.listen_address = listen_address,
            Self::Catalog(catalog) => saved.catalog = catalog,
            Self::Authorizations(tools) => saved.tools = tools,
            Self::RemoveResource { kind, id } => match kind {
                circuitfabric_codex_runtime::ToolAuthorizationKind::Skill => {
                    saved.catalog.skills.retain(|s| s.id != id);
                    saved.tools.authorized_skill_ids.retain(|s| s != &id);
                }
                circuitfabric_codex_runtime::ToolAuthorizationKind::McpServer => {
                    saved.catalog.mcp_servers.retain(|s| s.id != id);
                    saved.tools.authorized_mcp_server_ids.retain(|s| s != &id);
                    if id == circuitfabric_codex_runtime::tools::BUNDLED_JEV_SERVER_ID
                        && !saved.catalog.removed_bundled_servers.contains(&id)
                    {
                        saved.catalog.removed_bundled_servers.push(id);
                    }
                }
            },
        }
        Ok(())
    }
}

/// Merge only the requested section into the latest file. Return the new snapshot
/// only after validation and the atomic write succeed; never replace a read error
/// with defaults, or pull drafts from any other page into this transaction.
pub fn save_update(path: &Path, update: SettingsUpdate) -> Result<RuntimeSettings, RuntimeError> {
    let mut saved = RuntimeSettings::load_or_default(path)?;
    update.apply(&mut saved)?;
    saved.save(path)?;
    Ok(saved)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
    };

    static NEXT: AtomicU64 = AtomicU64::new(0);

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let dir = std::env::temp_dir().join(format!(
                "circuitfabric-save-scope-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
        fn path(&self) -> PathBuf {
            self.0.join("runtime.json")
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn independent_saves_preserve_other_sections_and_reload_latest_file() {
        let fixture = Fixture::new();
        let path = fixture.path();
        let initial = RuntimeSettings::default();
        initial.save(&path).unwrap();
        let initial = RuntimeSettings::load_or_default(&path).unwrap();
        let mut providers = initial.providers.clone();
        providers[0].model = "saved-model".into();
        save_update(
            &path,
            SettingsUpdate::Providers {
                providers: providers.clone(),
                default_provider_id: initial.default_provider_id.clone(),
            },
        )
        .unwrap();
        // Another page saves after the Provider change. It must merge into the
        // latest file, retaining the changed provider and all unrelated fields.
        let saved =
            save_update(&path, SettingsUpdate::Bridge { listen_address: "127.0.0.1:49631".into() })
                .unwrap();
        let mut expected = initial;
        expected.providers = providers;
        expected.bridge.listen_address = "127.0.0.1:49631".into();
        assert_eq!(saved, expected);
        assert_eq!(RuntimeSettings::load_or_default(&path).unwrap(), expected);
    }

    #[test]
    fn runtime_catalog_and_grant_saves_do_not_touch_provider_or_bridge_settings() {
        let fixture = Fixture::new();
        let path = fixture.path();
        let mut expected = RuntimeSettings::default();
        expected.providers[0].model = "saved-model".into();
        expected.save(&path).unwrap();
        let mut expected = RuntimeSettings::load_or_default(&path).unwrap();
        let mut catalog = expected.catalog.clone();
        catalog.skills.push(circuitfabric_codex_runtime::tools::SkillDefinition {
            id: "review".into(),
            path: fixture.0.join("SKILL.md"),
            enabled: true,
        });
        expected.codex.command = "saved-codex".into();
        expected.adapters.codex_provider_id = expected.default_provider_id.clone();
        assert_eq!(
            save_update(
                &path,
                SettingsUpdate::Codex {
                    command: "saved-codex".into(),
                    provider_id: expected.default_provider_id.clone(),
                }
            )
            .unwrap(),
            expected
        );
        expected.adapters.claude_command = "saved-claude".into();
        assert_eq!(
            save_update(
                &path,
                SettingsUpdate::Claude {
                    command: "saved-claude".into(),
                    provider_id: String::new(),
                }
            )
            .unwrap(),
            expected
        );
        expected.adapters.dsh_command = "saved-dsh".into();
        assert_eq!(
            save_update(
                &path,
                SettingsUpdate::Dsh { command: "saved-dsh".into(), provider_id: String::new() }
            )
            .unwrap(),
            expected
        );
        expected.catalog = catalog.clone();
        assert_eq!(save_update(&path, SettingsUpdate::Catalog(catalog)).unwrap(), expected);
        expected.tools.authorized_skill_ids = vec!["review".into()];
        assert_eq!(
            save_update(&path, SettingsUpdate::Authorizations(expected.tools.clone())).unwrap(),
            expected
        );
        assert_eq!(RuntimeSettings::load_or_default(&path).unwrap(), expected);
    }

    #[test]
    fn saving_catalog_and_grants_ignores_invalid_drafts_on_other_pages() {
        let fixture = Fixture::new();
        let path = fixture.path();
        let initial = RuntimeSettings::load_or_default(&path).unwrap();
        initial.save(&path).unwrap();
        let mut drafts = initial.clone();
        drafts.providers[0].base_url.clear();
        drafts.codex.command.clear();
        drafts.bridge.listen_address = "invalid address".into();
        drafts.global_preferences.data_directory = PathBuf::new();
        drafts.tools.authorized_skill_ids = vec!["review".into()];
        assert!(drafts.validate().is_err());
        assert_eq!(save_update(&path, SettingsUpdate::Catalog(drafts.catalog)).unwrap(), initial);
        let saved =
            save_update(&path, SettingsUpdate::Authorizations(drafts.tools.clone())).unwrap();
        let mut expected = initial;
        expected.tools = drafts.tools;
        assert_eq!(saved, expected);
        // A task or restart reads the saved model/address, never the invalid drafts.
        assert_eq!(RuntimeSettings::load_or_default(&path).unwrap(), expected);
    }

    #[test]
    fn invalid_binding_or_provider_does_not_replace_saved_configuration() {
        let fixture = Fixture::new();
        let path = fixture.path();
        RuntimeSettings::default().save(&path).unwrap();
        let original = fs::read(&path).unwrap();
        assert!(
            save_update(
                &path,
                SettingsUpdate::Claude {
                    command: "claude".into(),
                    provider_id: "unsaved-provider".into()
                }
            )
            .is_err()
        );
        assert_eq!(fs::read(&path).unwrap(), original);
        assert!(
            save_update(
                &path,
                SettingsUpdate::Providers { providers: vec![], default_provider_id: String::new() }
            )
            .is_err()
        );
        assert_eq!(fs::read(&path).unwrap(), original);
    }

    #[test]
    fn read_and_write_failures_return_errors_without_claiming_a_saved_snapshot() {
        let fixture = Fixture::new();
        let path = fixture.path();
        fs::write(&path, b"invalid json").unwrap();
        assert!(save_update(&path, SettingsUpdate::Catalog(ToolCatalog::default())).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"invalid json");
        fs::remove_file(&path).unwrap();
        fs::create_dir(&path).unwrap();
        let saved = RuntimeSettings::default();
        assert!(saved.save(&path).is_err());
        assert!(path.is_dir());
        // A file used as a parent makes a first-save write fail too.
        let parent = fixture.0.join("file-as-parent");
        fs::write(&parent, b"preserve").unwrap();
        assert!(
            save_update(
                &parent.join("runtime.json"),
                SettingsUpdate::Catalog(ToolCatalog::default())
            )
            .is_err()
        );
        assert_eq!(fs::read(parent).unwrap(), b"preserve");
    }

    #[test]
    fn deleting_a_definition_removes_global_grants_and_bundled_restore_without_other_saves() {
        use circuitfabric_codex_runtime::{ToolAuthorizationKind, tools::BUNDLED_JEV_SERVER_ID};
        let fixture = Fixture::new();
        let path = fixture.path();
        let mut initial = RuntimeSettings::default();
        initial.tools.authorized_mcp_server_ids =
            vec![BUNDLED_JEV_SERVER_ID.into(), "other".into()];
        initial.providers[0].model = "saved-model".into();
        initial.save(&path).unwrap();
        let saved = save_update(
            &path,
            SettingsUpdate::RemoveResource {
                kind: ToolAuthorizationKind::McpServer,
                id: BUNDLED_JEV_SERVER_ID.into(),
            },
        )
        .unwrap();
        assert_eq!(saved.tools.authorized_mcp_server_ids, ["other"]);
        assert_eq!(saved.providers, initial.providers);
        assert!(saved.catalog.removed_bundled_servers.contains(&BUNDLED_JEV_SERVER_ID.into()));
        let restored = RuntimeSettings::load_or_default(&path).unwrap();
        assert!(!restored.catalog.mcp_servers.iter().any(|s| s.id == BUNDLED_JEV_SERVER_ID));
    }

    fn two_provider_settings(default: &str) -> RuntimeSettings {
        let mut settings = RuntimeSettings::default();
        let mut added = LlmProviderSettings::default();
        added.id = "added".into();
        settings.providers.push(added);
        settings.default_provider_id = default.into();
        settings
    }

    #[test]
    fn provider_quick_actions_persist_only_their_section_and_reload_latest_file() {
        let fixture = Fixture::new();
        let path = fixture.path();
        // Mirrors the reported flow: a newly added provider is the default, and the
        // user switches the default back to the previous provider. Loading back the
        // seed keeps the bundled-catalog merge out of the comparison.
        two_provider_settings("added").save(&path).unwrap();
        let initial = RuntimeSettings::load_or_default(&path).unwrap();

        let saved =
            save_update(&path, SettingsUpdate::DefaultProvider { id: "zai".into() }).unwrap();
        let mut expected = initial.clone();
        expected.default_provider_id = "zai".into();
        assert_eq!(saved, expected);
        assert_eq!(RuntimeSettings::load_or_default(&path).unwrap(), expected);

        // The saved copy's enabled flag flips for exactly the named provider; any
        // unsaved form edits elsewhere are not pulled into this transaction.
        let saved = save_update(
            &path,
            SettingsUpdate::ProviderEnabled { id: "added".into(), enabled: false },
        )
        .unwrap();
        expected.providers[1].enabled = false;
        assert_eq!(saved, expected);
        assert_eq!(RuntimeSettings::load_or_default(&path).unwrap(), expected);

        // Removing a non-default provider leaves the default untouched.
        let saved =
            save_update(&path, SettingsUpdate::RemoveProvider { id: "added".into() }).unwrap();
        expected.providers.remove(1);
        assert_eq!(saved, expected);
        assert_eq!(RuntimeSettings::load_or_default(&path).unwrap(), expected);
    }

    #[test]
    fn removing_the_default_provider_reassigns_the_default_to_the_first_remaining_one() {
        let fixture = Fixture::new();
        let path = fixture.path();
        two_provider_settings("zai").save(&path).unwrap();
        let initial = RuntimeSettings::load_or_default(&path).unwrap();
        let saved =
            save_update(&path, SettingsUpdate::RemoveProvider { id: "zai".into() }).unwrap();
        let mut expected = initial;
        expected.providers.remove(0);
        expected.default_provider_id = "added".into();
        assert_eq!(saved, expected);
        assert_eq!(RuntimeSettings::load_or_default(&path).unwrap(), expected);
    }

    #[test]
    fn provider_quick_actions_reject_unknown_ids_invalid_targets_and_keep_the_file() {
        let fixture = Fixture::new();
        let path = fixture.path();
        two_provider_settings("zai").save(&path).unwrap();
        let original = fs::read(&path).unwrap();

        assert!(
            save_update(&path, SettingsUpdate::DefaultProvider { id: "missing".into() }).is_err()
        );
        assert!(
            save_update(
                &path,
                SettingsUpdate::ProviderEnabled { id: "missing".into(), enabled: false }
            )
            .is_err()
        );
        // Disabling the default provider would leave the settings without an
        // enabled default; validation must reject the whole transaction.
        assert!(
            save_update(
                &path,
                SettingsUpdate::ProviderEnabled { id: "zai".into(), enabled: false }
            )
            .is_err()
        );
        assert!(
            save_update(&path, SettingsUpdate::RemoveProvider { id: "missing".into() }).is_err()
        );
        assert_eq!(fs::read(&path).unwrap(), original);

        // Removing the last remaining provider is rejected, keeping one configured.
        save_update(&path, SettingsUpdate::RemoveProvider { id: "added".into() }).unwrap();
        assert!(save_update(&path, SettingsUpdate::RemoveProvider { id: "zai".into() }).is_err());
        let remaining = RuntimeSettings::load_or_default(&path).unwrap();
        assert_eq!(
            remaining.providers.iter().map(|provider| provider.id.as_str()).collect::<Vec<_>>(),
            ["zai"]
        );
        assert_eq!(remaining.default_provider_id, "zai");
    }
}
