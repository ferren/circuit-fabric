use crate::application::navigation::ControlPlaneScreen;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum UiLanguage {
    SimplifiedChinese,
    English,
}

impl UiLanguage {
    pub(super) const fn from_preference(
        language: circuitfabric_codex_runtime::GlobalLanguage,
    ) -> Self {
        match language {
            circuitfabric_codex_runtime::GlobalLanguage::SimplifiedChinese => {
                Self::SimplifiedChinese
            }
            circuitfabric_codex_runtime::GlobalLanguage::English => Self::English,
        }
    }

    pub(super) const fn preference(self) -> circuitfabric_codex_runtime::GlobalLanguage {
        match self {
            Self::SimplifiedChinese => {
                circuitfabric_codex_runtime::GlobalLanguage::SimplifiedChinese
            }
            Self::English => circuitfabric_codex_runtime::GlobalLanguage::English,
        }
    }

    pub(super) const fn toggled(self) -> Self {
        match self {
            Self::SimplifiedChinese => Self::English,
            Self::English => Self::SimplifiedChinese,
        }
    }

    pub(super) const fn toggle_label(self) -> &'static str {
        match self {
            Self::SimplifiedChinese => "EN",
            Self::English => "中文",
        }
    }

    pub(super) const fn choose(self, chinese: &'static str, english: &'static str) -> &'static str {
        match self {
            Self::SimplifiedChinese => chinese,
            Self::English => english,
        }
    }

    pub(super) fn choose_owned(self, chinese: String, english: String) -> String {
        match self {
            Self::SimplifiedChinese => chinese,
            Self::English => english,
        }
    }

    pub(super) const fn screen_label(self, screen: ControlPlaneScreen) -> &'static str {
        match self {
            Self::English => screen.label(),
            Self::SimplifiedChinese => match screen {
                ControlPlaneScreen::Overview => "总览",
                ControlPlaneScreen::Projects => "项目",
                ControlPlaneScreen::Documents => "文档",
                ControlPlaneScreen::Semantics => "电路语义",
                ControlPlaneScreen::EdaServices => "EDA 服务",
                ControlPlaneScreen::AgentsAndMcp => "智能体与工具",
                ControlPlaneScreen::SessionsAndTasks => "会话与任务",
                ControlPlaneScreen::ChangesAndApprovals => "变更与审批",
                ControlPlaneScreen::BomAndExport => "BOM 与导出",
                ControlPlaneScreen::Plugins => "插件",
                ControlPlaneScreen::Usage => "用量与审计",
                ControlPlaneScreen::SecretsVault => "密钥保险库",
                ControlPlaneScreen::Settings => "设置",
            },
        }
    }

    pub(super) const fn group_label(self, screen: ControlPlaneScreen) -> &'static str {
        match self {
            Self::English => screen.group(),
            Self::SimplifiedChinese => match screen {
                ControlPlaneScreen::Overview
                | ControlPlaneScreen::Projects
                | ControlPlaneScreen::Documents
                | ControlPlaneScreen::Semantics => "设计证据",
                ControlPlaneScreen::EdaServices
                | ControlPlaneScreen::AgentsAndMcp
                | ControlPlaneScreen::SessionsAndTasks => "运行工具",
                ControlPlaneScreen::ChangesAndApprovals | ControlPlaneScreen::BomAndExport => {
                    "物化交付"
                }
                ControlPlaneScreen::Plugins
                | ControlPlaneScreen::Usage
                | ControlPlaneScreen::SecretsVault
                | ControlPlaneScreen::Settings => "治理",
            },
        }
    }
}
