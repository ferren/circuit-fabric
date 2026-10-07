//! Navigation metadata and shell selection; never authoritative circuit state.
use circuitfabric_contracts::ProjectId;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ControlPlaneScreen {
    Overview,
    Projects,
    Documents,
    Semantics,
    EdaServices,
    AgentsAndMcp,
    SessionsAndTasks,
    ChangesAndApprovals,
    BomAndExport,
    Plugins,
    Usage,
    SecretsVault,
    Settings,
}

impl ControlPlaneScreen {
    #[cfg(feature = "native-ui")]
    pub(crate) const ALL: [Self; 13] = [
        Self::Overview,
        Self::Projects,
        Self::Documents,
        Self::Semantics,
        Self::EdaServices,
        Self::AgentsAndMcp,
        Self::SessionsAndTasks,
        Self::ChangesAndApprovals,
        Self::BomAndExport,
        Self::Plugins,
        Self::Usage,
        Self::SecretsVault,
        Self::Settings,
    ];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Overview => "Overview",
            Self::Projects => "Projects",
            Self::Documents => "Documents",
            Self::Semantics => "Circuit semantics",
            Self::EdaServices => "EDA services",
            Self::AgentsAndMcp => "Agents & tools",
            Self::SessionsAndTasks => "Sessions & tasks",
            Self::ChangesAndApprovals => "Changes & approvals",
            Self::BomAndExport => "BOM & export",
            Self::Plugins => "Plugins",
            Self::Usage => "Usage & audit",
            Self::SecretsVault => "Secrets vault",
            Self::Settings => "Settings",
        }
    }

    #[must_use]
    pub const fn group(self) -> &'static str {
        match self {
            Self::Overview | Self::Projects | Self::Documents | Self::Semantics => {
                "DESIGN EVIDENCE"
            }
            Self::EdaServices | Self::AgentsAndMcp | Self::SessionsAndTasks => "RUNTIME TOOLS",
            Self::ChangesAndApprovals | Self::BomAndExport => "MATERIALIZE & DELIVER",
            Self::Plugins | Self::Usage | Self::SecretsVault | Self::Settings => "GOVERNANCE",
        }
    }

    #[must_use]
    pub const fn is_todo(self) -> bool {
        !matches!(
            self,
            Self::Overview
                | Self::Projects
                | Self::Documents
                | Self::Semantics
                | Self::EdaServices
                | Self::AgentsAndMcp
                | Self::SessionsAndTasks
                | Self::Plugins
                | Self::Usage
                | Self::SecretsVault
        )
    }

    /// Screens whose page is scoped to one open project: they render a
    /// "select a project first" empty state until one is chosen, so their
    /// navigation entries stay disabled until a project is selected.
    #[cfg_attr(not(feature = "native-ui"), allow(dead_code))]
    #[must_use]
    pub const fn requires_project(self) -> bool {
        matches!(self, Self::Semantics | Self::ChangesAndApprovals | Self::BomAndExport)
    }
}

#[derive(Debug)]
pub struct DesktopShell {
    pub(crate) screen: ControlPlaneScreen,
    pub(crate) selected_project: Option<ProjectId>,
}

impl Default for DesktopShell {
    fn default() -> Self {
        Self { screen: ControlPlaneScreen::Overview, selected_project: None }
    }
}

impl DesktopShell {
    #[must_use]
    pub fn selected_project(&self) -> Option<&str> {
        self.selected_project.as_deref()
    }

    pub fn select_project(&mut self, project_id: ProjectId) {
        self.selected_project = Some(project_id);
    }
}

/// Native window caption: the app name alone until a project is opened,
/// then the app name plus the selected project's name.
#[cfg_attr(not(feature = "native-ui"), allow(dead_code))]
#[must_use]
pub fn app_window_title(selected_project_name: Option<&str>) -> String {
    match selected_project_name {
        Some(name) => format!("CircuitFabric — {name}"),
        None => "CircuitFabric".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selected_project_is_shell_state_not_circuit_state() {
        let mut shell = DesktopShell::default();
        shell.select_project("project-1".to_owned());

        assert_eq!(shell.selected_project(), Some("project-1"));
    }

    #[test]
    fn delivered_record_pages_are_navigation_screens() {
        assert!(!ControlPlaneScreen::Documents.is_todo());
        assert!(!ControlPlaneScreen::SessionsAndTasks.is_todo());
        assert!(!ControlPlaneScreen::Usage.is_todo());
    }

    #[test]
    fn only_project_scoped_screens_require_a_project() {
        let project_independent = [
            ControlPlaneScreen::Overview,
            ControlPlaneScreen::Projects,
            ControlPlaneScreen::Documents,
            ControlPlaneScreen::EdaServices,
            ControlPlaneScreen::AgentsAndMcp,
            ControlPlaneScreen::SessionsAndTasks,
            ControlPlaneScreen::Plugins,
            ControlPlaneScreen::Usage,
            ControlPlaneScreen::SecretsVault,
            ControlPlaneScreen::Settings,
        ];
        for screen in project_independent {
            assert!(
                !screen.requires_project(),
                "{screen:?} should stay usable without a selected project"
            );
        }
        for screen in [
            ControlPlaneScreen::Semantics,
            ControlPlaneScreen::ChangesAndApprovals,
            ControlPlaneScreen::BomAndExport,
        ] {
            assert!(screen.requires_project(), "{screen:?} should require a selected project");
        }
    }

    #[test]
    fn window_title_names_the_selected_project() {
        assert_eq!(app_window_title(None), "CircuitFabric");
        assert_eq!(app_window_title(None), "CircuitFabric");
    }
}
