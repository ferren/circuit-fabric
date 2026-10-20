//! Native presentation composition root. Business services live in `application`.
use crate::application::navigation::{ControlPlaneScreen, DesktopShell, app_window_title};
use assistant::{ASSISTANT_SHORTCUT_LABEL, AssistantState};
#[cfg(windows)]
use crate::{pdf_cursors, windows_icon};
use circuitfabric_contracts::ProjectId;
use language::UiLanguage;
use state::*;
use theme::*;
use tools::{CatalogEditor, ToolReport};
mod assistant;
mod assistant_selector;
mod authorization;
mod bootstrap;
mod changes;
mod commands;
mod datasheet_stream;
mod documents;
mod evidence;
mod export;
mod jev;
mod language;
mod overview;
#[cfg(feature = "ui-test-support")]
mod overview_capture;
mod overview_charts;
mod plugins;
mod preview;
mod projects;
mod runtime;
mod secrets;
mod selectable_text;
mod semantics;
mod session_markdown;
mod sessions;
mod settings;
mod shell;
mod state;
mod theme;
mod tools;
mod usage;
mod usage_charts;
mod widgets;

use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{Arc, atomic::Ordering},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use crate::application::plugin_governance::PluginGovernanceStore;
use crate::application::usage_audit::{AuditKindFilter, UsageAuditModel, UsagePeriod};
use circuitfabric_codex_runtime::secrets::{SecretSource, UnlockedVault, secret_source};
use circuitfabric_codex_runtime::tools::BUNDLED_JEV_SERVER_ID;
use circuitfabric_codex_runtime::{
    CodexAppServerHandle, CodexAppServerSettings, GlobalPreferences, GlobalTheme,
    LlmProviderSettings, LogLevel, ProviderConnection, ProviderWireApi, RuntimeSettings,
    SecretStorageProvider, ToolAuthorizationKind, ToolAuthorizationSettings,
};
use circuitfabric_contracts::{
    DatasheetExtraction, DocumentKind, FactStatus, Project, SnapshotAuthority,
};
use circuitfabric_document_opener::{
    DocumentOpenerRegistry, extract_datasheet_by_category_with_pages,
};
use circuitfabric_plugin_api::{
    DocumentBlockKind, DocumentOpenDenial, DocumentSpanStyle, DocumentView, DocumentViewBody,
};
use circuitfabric_project::{
    ChangeSetAuditEntry, ChangeSetStageStatus, DocumentCategory, EvidenceCorpus, EvidenceHit,
    EvidenceScope, EvidenceSearch, FragmentAnchor, ProjectDocument, ProjectRegistry,
    ProjectStorage, ProjectWorkspace, SessionReplay, is_evidence_indexable, rfc3339,
};
use gpui::{
    AnyElement, AppContext, ClickEvent, Context, Div, DragMoveEvent, Entity, FontStyle,
    FontWeight, HighlightStyle, Image, ImageFormat, InteractiveElement, IntoElement,
    KeystrokeEvent, MouseButton, ParentElement, Render, Size, StatefulInteractiveElement,
    StrikethroughStyle, Styled, StyledText, Window, WindowOptions, deferred, div, img,
    prelude::FluentBuilder as _, px, rgba,
};
use gpui_base::{InputBase, Selectable, input::InputEditorStyle};
use gpui_component::{
    Disableable, Icon, IconName, Root, Sizable, StyledExt,
    breadcrumb::{Breadcrumb, BreadcrumbItem},
    button::{Button, ButtonVariants},
    input::{Input, InputEvent, InputState},
    scroll::ScrollableElement as _,
    tab::{Tab, TabBar},
};
use jlcircuit_eda_bridge::{
    probe::{self, BridgeStatusReport},
    supervision::BridgeProcessHandle,
};
const SIDEBAR_MARK: &[u8] =
    include_bytes!("../../../../assets/branding/circuitfabric-sidebar-mark.png");

/// Lucide icons (ISC, see `assets/icons/LICENSE-LUCIDE`) the navigation needs
/// beyond the default component bundle, embedded individually instead of the
/// complete catalog.
const APP_ICONS: [(&str, &[u8]); 8] = [
    ("icons/folder-kanban.svg", include_bytes!("../../../../assets/icons/folder-kanban.svg")),
    ("icons/circuit-board.svg", include_bytes!("../../../../assets/icons/circuit-board.svg")),
    ("icons/messages-square.svg", include_bytes!("../../../../assets/icons/messages-square.svg")),
    (
        "icons/git-pull-request.svg",
        include_bytes!("../../../../assets/icons/git-pull-request.svg"),
    ),
    ("icons/package.svg", include_bytes!("../../../../assets/icons/package.svg")),
    ("icons/puzzle.svg", include_bytes!("../../../../assets/icons/puzzle.svg")),
    (
        "icons/chart-no-axes-column.svg",
        include_bytes!("../../../../assets/icons/chart-no-axes-column.svg"),
    ),
    ("icons/vault.svg", include_bytes!("../../../../assets/icons/vault.svg")),
];

/// The component icon bundle plus [`APP_ICONS`].
struct AppAssets;

impl gpui::AssetSource for AppAssets {
    fn load(&self, path: &str) -> gpui::Result<Option<std::borrow::Cow<'static, [u8]>>> {
        if let Some((_, bytes)) = APP_ICONS.iter().find(|(icon, _)| *icon == path) {
            return Ok(Some(std::borrow::Cow::Borrowed(bytes)));
        }
        gpui_component_assets::Assets.load(path)
    }

    fn list(&self, path: &str) -> gpui::Result<Vec<gpui::SharedString>> {
        let mut names = gpui_component_assets::Assets.list(path)?;
        names.extend(
            APP_ICONS.iter().filter(|(icon, _)| icon.starts_with(path)).map(|(i, _)| (*i).into()),
        );
        Ok(names)
    }
}

fn status_dot(color: u32) -> impl IntoElement {
    div().size(px(8.)).rounded_full().bg(rgb(color)).flex_none()
}

pub(crate) fn run() {
    // gpui-component's icons (buttons, spinners, …) are SVG assets; without a registered
    // asset source every `Icon` renders as empty space — the control still occupies its
    // layout box and handles input, but nothing is visible.
    gpui_platform::application().with_assets(AppAssets).run(move |cx| {
        gpui_component::init(cx);
        cx.set_app_identity("io.circuitfabric.desktop", "CircuitFabric");
        cx.spawn(async move |cx| {
            cx.open_window(
                WindowOptions {
                    app_id: Some("io.circuitfabric.desktop".to_owned()),
                    #[cfg(feature = "ui-test-support")]
                    window_bounds: overview_capture::requested_bounds(),
                    ..Default::default()
                },
                |window, cx| {
                    // `WindowOptions::default` leaves the native title unset on
                    // Windows, which produces an otherwise normal caption bar with
                    // an empty text region.
                    window.set_window_title("CircuitFabric");

                    #[cfg(windows)]
                    windows_icon::apply(window)
                        .expect("failed to apply the CircuitFabric Windows window icon");

                    #[cfg(windows)]
                    pdf_cursors::install(window);

                    let view = cx.new(|cx| ControlPlaneView::new(window, cx));
                    view.update(cx, |view, cx| view.prefetch_direct_model_catalogs(cx));
                    #[cfg(feature = "ui-test-support")]
                    view.update(cx, |view, cx| view.schedule_overview_capture(window, cx));
                    cx.observe_keystrokes({
                        let view = view.clone();
                        move |event, window, cx| {
                            view.update(cx, |view, cx| {
                                view.handle_command_keystroke(event, window, cx);
                            });
                        }
                    })
                    .detach();
                    cx.new(|cx| Root::new(view, window, cx))
                },
            )
            .expect("failed to open CircuitFabric desktop window");
        })
        .detach();
    });
}

use crate::application::project_data::{self, ProjectWorkspaceData};

use crate::application::semantic_query::{SemanticQueryScope, semantic_query_hits, short_hash};

mod layout;
#[cfg(all(test, feature = "ui-test-support"))]
mod layout_tests;
use layout::{ScrollRegionExt as _, capped_scroll_body, page, workspace_page};

use widgets::{
    action_button, agents_group_label, detail_pane, info_note, keycap, labeled_field,
    project_empty_state, settings_summary_row,
};
