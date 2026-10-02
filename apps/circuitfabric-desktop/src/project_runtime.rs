//! Project-owned working directories are launch state, never global settings.

use circuitfabric_codex_runtime::{CodexAppServerSettings, RuntimeSettings};
use std::path::Path;

pub fn codex_for_project(
    saved: &RuntimeSettings,
    project_root: Option<&Path>,
) -> Result<CodexAppServerSettings, String> {
    let root = project_root.ok_or_else(|| {
        "未启动：请先在「项目」页打开或选择项目，Codex 将使用项目根目录。".to_owned()
    })?;
    if !root.is_absolute() || !root.is_dir() {
        return Err("未启动：项目根目录不存在或不是有效的绝对目录，请重新打开项目。".into());
    }
    let mut launch = saved.codex.clone();
    launch.working_directory = root.to_path_buf();
    Ok(launch)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_root_overrides_legacy_directory_without_changing_saved_settings() {
        let mut saved = RuntimeSettings::default();
        saved.codex.working_directory = "C:/obsolete-global-directory".into();
        let before = saved.clone();
        let first = std::env::temp_dir().canonicalize().unwrap();
        let second = std::env::current_dir().unwrap();
        assert_ne!(first, second);
        let launch = codex_for_project(&saved, Some(&first)).unwrap();
        assert_eq!(launch.working_directory, first);
        assert_eq!(launch.command, saved.codex.command);
        let next = codex_for_project(&saved, Some(&second)).unwrap();
        assert_eq!(next.working_directory, second);
        assert_eq!(saved, before);
    }

    #[test]
    fn missing_or_invalid_project_never_falls_back_to_global_directory() {
        let saved = RuntimeSettings::default();
        assert!(codex_for_project(&saved, None).unwrap_err().contains("选择项目"));
        assert!(codex_for_project(&saved, Some(Path::new("."))).is_err());
        assert!(codex_for_project(&saved, Some(&std::env::current_exe().unwrap())).is_err());
        let missing =
            std::env::temp_dir().join(format!("cf-missing-project-{}", std::process::id()));
        assert!(codex_for_project(&saved, Some(&missing)).is_err());
    }
}
