//! Explicit save boundaries for the desktop's independently edited settings.

use circuitfabric_codex_runtime::{
    LlmProviderSettings, RuntimeError, RuntimeSettings, ToolAuthorizationSettings,
    tools::ToolCatalog,
};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub enum SettingsUpdate {
    Providers { providers: Vec<LlmProviderSettings>, default_provider_id: String },
    Codex { command: String, working_directory: PathBuf, provider_id: String },
    Claude { command: String, provider_id: String },
    Dsh { command: String, provider_id: String },
    Bridge { listen_address: String },
    Catalog(ToolCatalog),
    Authorizations(ToolAuthorizationSettings),
}

impl SettingsUpdate {
    fn apply(self, saved: &mut RuntimeSettings) {
        match self {
            Self::Providers { providers, default_provider_id } => {
                saved.providers = providers;
                saved.default_provider_id = default_provider_id;
            }
            Self::Codex { command, working_directory, provider_id } => {
                saved.codex.command = command;
                saved.codex.working_directory = working_directory;
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
        }
    }
}

/// Merge only the requested section into the latest file. Return the new snapshot
/// only after validation and the atomic write succeed; never replace a read error
/// with defaults, or pull drafts from any other page into this transaction.
pub fn save_update(path: &Path, update: SettingsUpdate) -> Result<RuntimeSettings, RuntimeError> {
    let mut saved = RuntimeSettings::load_or_default(path)?;
    update.apply(&mut saved);
    saved.save(path)?;
    Ok(saved)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
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
        expected.codex.working_directory = "C:/project".into();
        expected.adapters.codex_provider_id = expected.default_provider_id.clone();
        assert_eq!(
            save_update(
                &path,
                SettingsUpdate::Codex {
                    command: "saved-codex".into(),
                    working_directory: "C:/project".into(),
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
}
