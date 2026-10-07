//! Exercise actual GPUI layout and wheel dispatch, including shared helper identities.
use super::layout::*;
use super::{ControlPlaneView, RuntimeAdapter, UiLanguage, action_button};
use crate::application::navigation::ControlPlaneScreen;
use circuitfabric_codex_runtime::secrets::UnlockedVault;
use gpui::{
    Context, Div, InteractiveElement, IntoElement, ParentElement, Render, ScrollDelta,
    ScrollWheelEvent, Styled, TestAppContext, VisualTestContext, Window, div, point, px,
};
use gpui_component::StyledExt;
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT_PROFILE: AtomicU64 = AtomicU64::new(0);
struct Profile(PathBuf);
impl Profile {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "cf-ui-layout-{}-{}",
            std::process::id(),
            NEXT_PROFILE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}
impl Drop for Profile {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn mount_profile<'a>(
    cx: &'a mut TestAppContext,
    profile: &Profile,
    screen: ControlPlaneScreen,
) -> (gpui::Entity<ControlPlaneView>, &'a mut VisualTestContext) {
    use gpui::AppContext;
    cx.update(gpui_component::init);
    let mut view = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let entity = cx.new(|cx| {
            let mut view = ControlPlaneView::new_with_paths(
                profile.0.join("runtime.json"),
                profile.0.join("vault.bin"),
                window,
                cx,
            );
            view.navigation.screen = screen;
            view.vault_prompt_open = false;
            view
        });
        view = Some(entity.clone());
        gpui_component::Root::new(entity, window, cx).bordered(false)
    });
    (view.unwrap(), cx)
}

#[gpui::test]
fn real_vault_page_allocates_height_to_its_panels(cx: &mut TestAppContext) {
    let profile = Profile::new();
    let (view, cx) = mount_profile(cx, &profile, ControlPlaneScreen::SecretsVault);
    for exists in [false, true] {
        cx.update(|_, cx| {
            view.update(cx, |view, cx| {
                view.vault_file_exists = exists;
                cx.notify();
            })
        });
        draw(cx);
        let panes = cx.debug_bounds("vault-pane-row").expect("vault panels rendered");
        assert!(panes.size.height > px(200.), "vault panels must have usable height: {panes:?}");
        let field = cx
            .debug_bounds(if exists { "vault-unlock-password" } else { "vault-create-password" })
            .unwrap();
        assert!(
            field.size.height > px(30.)
                && field.top() >= panes.top()
                && field.bottom() <= panes.bottom(),
            "vault form must be visible inside the panels: {field:?}"
        );
    }
    let vault =
        UnlockedVault::create(&profile.0.join("vault.bin"), "layout-test-passphrase").unwrap();
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.vault = Some(vault);
            cx.notify();
        })
    });
    draw(cx);
    let panes = cx.debug_bounds("vault-pane-row").unwrap();
    let field = cx.debug_bounds("secret-name").expect("unlocked vault editor rendered");
    assert!(
        field.size.height > px(30.)
            && field.top() >= panes.top()
            && field.bottom() <= panes.bottom(),
        "unlocked editor must remain visible: {field:?}"
    );
}

#[gpui::test]
fn real_runtime_settings_dialogs_show_their_form_body(cx: &mut TestAppContext) {
    let profile = Profile::new();
    let (view, cx) = mount_profile(cx, &profile, ControlPlaneScreen::AgentsAndMcp);
    for adapter in [RuntimeAdapter::CodexAppServer, RuntimeAdapter::ClaudeCode, RuntimeAdapter::Dsh]
    {
        cx.update(|_, cx| {
            view.update(cx, |view, cx| {
                view.adapter_settings_open = Some(adapter);
                cx.notify();
            })
        });
        draw(cx);
        let card = cx.debug_bounds("adapter-settings-card").expect("runtime dialog rendered");
        assert!(card.size.height > px(250.), "runtime form collapsed for {adapter:?}: {card:?}");
        let command = cx.debug_bounds("codex-command").expect("runtime command field rendered");
        assert!(
            command.size.height > px(30.)
                && command.top() >= card.top()
                && command.bottom() <= card.bottom(),
            "runtime command must be visible inside the dialog: {command:?}"
        );
    }
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.adapter_settings_open = None;
            view.provider_editor_open = true;
            cx.notify();
        })
    });
    draw(cx);
    let card = cx.debug_bounds("provider-settings-card").expect("provider dialog rendered");
    assert!(card.size.height > px(250.), "provider form collapsed: {card:?}");
    let field = cx.debug_bounds("provider-id").expect("provider ID field rendered");
    assert!(
        field.size.height > px(30.) && field.top() >= card.top() && field.bottom() <= card.bottom(),
        "provider field must be visible inside the dialog: {field:?}"
    );
}

#[gpui::test]
fn settings_actions_keep_intrinsic_width_in_both_languages(cx: &mut TestAppContext) {
    let profile = Profile::new();
    let (view, cx) = mount_profile(cx, &profile, ControlPlaneScreen::Settings);
    for language in [UiLanguage::SimplifiedChinese, UiLanguage::English] {
        cx.update(|_, cx| {
            view.update(cx, |view, cx| {
                view.language = language;
                cx.notify();
            })
        });
        draw(cx);
        let content = cx.debug_bounds("global-data-directory").unwrap();
        for selector in [
            "action-button-save-global-preferences",
            "action-button-cycle-log-level",
            "action-button-settings-toggle-language",
        ] {
            let button = cx.debug_bounds(selector).expect("settings action rendered");
            assert!(
                button.size.width > px(20.) && button.size.width < content.size.width / 2.,
                "settings action must not stretch across the content in {language:?}: {button:?}"
            );
        }
    }
}

struct ButtonWidthFixture;
impl Render for ButtonWidthFixture {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .w(px(600.))
            .v_flex()
            .child(action_button("width-column").label("保存"))
            .child(
                div()
                    .flex()
                    .h(px(60.))
                    .items_center()
                    .child(action_button("width-row").label("保存")),
            )
            .child(
                div()
                    .w(px(300.))
                    .v_flex()
                    .child(action_button("width-narrow-column").label("保存")),
            )
    }
}

#[gpui::test]
fn action_width_depends_on_content_across_parent_layouts(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let (_, cx) = cx.add_window_view(|_, _| ButtonWidthFixture);
    draw(cx);
    let column = cx.debug_bounds("action-button-width-column").unwrap();
    assert!(column.size.width > px(20.) && column.size.width < px(150.));
    for selector in ["action-button-width-row", "action-button-width-narrow-column"] {
        assert_eq!(cx.debug_bounds(selector).unwrap().size.width, column.size.width);
    }
}

#[gpui::test]
fn usage_page_renders_empty_zero_charts_and_resets_pagination_after_filtering(
    cx: &mut TestAppContext,
) {
    use crate::application::usage_audit::{
        AuditKind, AuditRecord, AuditSourceContext, UsageAuditModel, UsageRecord,
    };
    let profile = Profile::new();
    let (view, cx) = mount_profile(cx, &profile, ControlPlaneScreen::Usage);
    // Painting culls primitives outside the viewport; keep all four chart bodies visible.
    cx.simulate_resize(gpui::size(px(1400.), px(2200.)));
    draw(cx);
    for selector in
        ["usage-daily-chart", "usage-project-chart", "usage-provider-chart", "usage-runtime-chart"]
    {
        assert!(
            cx.debug_bounds(selector).unwrap().size.height > px(30.),
            "empty chart must remain readable"
        );
    }
    for selector in [
        "usage-daily-plot",
        "usage-project-chart-donut",
        "usage-provider-chart-donut",
        "usage-runtime-chart-donut",
    ] {
        assert!(
            cx.debug_bounds(selector).unwrap().size.height > px(150.),
            "empty data must show a complete chart scaffold"
        );
    }
    assert!(cx.debug_bounds("usage-daily-empty").is_some());
    assert!(
        cx.debug_bounds("usage-daily-chart").unwrap().top()
            < cx.debug_bounds("usage-summary").unwrap().top(),
        "charts should precede text summaries"
    );
    assert_usage_series_painted(cx);
    let audit = AuditRecord {
        timestamp_unix_seconds: 0,
        timestamp_known: true,
        kind: AuditKind::Approval,
        summary: "approved".into(),
        source: AuditSourceContext {
            project_id: "p".into(),
            provider_id: "provider".into(),
            runtime_id: "codex".into(),
            session_id: "s".into(),
            source_type: "session".into(),
            source_id: "s".into(),
            locator: "sessions/s.md#body-line:1".into(),
        },
    };
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.usage_audit_cached = Some((
                std::time::Instant::now(),
                UsageAuditModel {
                    usage: vec![UsageRecord {
                        project_id: "p".into(),
                        provider_id: "provider".into(),
                        runtime_id: "codex".into(),
                        session_id: "s".into(),
                        timestamp_unix_seconds: 0,
                        input_tokens: 0,
                        output_tokens: 0,
                        usage_reported: true,
                        source_locator: "sessions/s.md#front-matter:usage".into(),
                    }],
                    audit: vec![audit; 201],
                    diagnostics: Vec::new(),
                },
            ));
            view.audit_page = 4;
            cx.notify();
        });
    });
    draw(cx);
    assert_eq!(view.read_with(cx, |view, _| view.audit_page), 4);
    for selector in
        ["usage-daily-chart", "usage-project-chart", "usage-provider-chart", "usage-runtime-chart"]
    {
        assert!(
            cx.debug_bounds(selector).unwrap().size.height > px(30.),
            "zero chart must remain readable"
        );
    }
    assert!(
        cx.debug_bounds("usage-daily-empty").is_none(),
        "recorded zero usage must use the real dataset"
    );
    assert!(cx.debug_bounds("usage-daily-zero").is_some());
    assert_usage_series_painted(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let model = &mut view.usage_audit_cached.as_mut().unwrap().1;
            model.usage[0].input_tokens = 100;
            model.usage[0].output_tokens = 50;
            cx.notify();
        });
    });
    draw(cx);
    assert!(cx.debug_bounds("usage-daily-zero").is_none());
    assert_usage_series_painted(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.audit_filter.update(cx, |input, cx| input.set_value("missing-source", window, cx));
            view.usage_filter.update(cx, |input, cx| input.set_value("missing-source", window, cx));
            cx.notify();
        });
    });
    draw(cx);
    assert_eq!(view.read_with(cx, |view, _| view.audit_page), 0);
    assert!(cx.debug_bounds("usage-daily-empty").is_some());
    assert!(
        cx.debug_bounds("usage-daily-plot").unwrap().size.height > px(150.),
        "an empty filter result uses the same default scaffold"
    );
    assert_usage_series_painted(cx);
    assert_eq!(
        view.read_with(cx, |view, _| view.usage_audit_cached.as_ref().unwrap().1.usage.len()),
        1,
        "default scaffold must not insert sample records"
    );
}

fn assert_usage_series_painted(cx: &mut VisualTestContext) {
    let plot = cx.debug_bounds("usage-daily-plot").unwrap();
    cx.update(|window, _| {
        let scale = window.scale_factor();
        for color in [super::ACCENT, 0x008b_5cf6] {
            let color: gpui::Background = super::rgb(color).into();
            let dots = window
                .painted_quads()
                .into_iter()
                .filter(|quad| {
                    quad.background == color
                        && quad.bounds.size.width.as_f32() <= 8. * scale
                        && quad.bounds.origin.x.as_f32() >= plot.left().as_f32() * scale
                        && quad.bounds.origin.y.as_f32() >= plot.top().as_f32() * scale
                        && quad.bounds.origin.y.as_f32() < plot.bottom().as_f32() * scale
                })
                .count();
            assert!(dots >= 2, "both series must paint visible data points, not only text or container bounds: plot={plot:?}, dots={dots}, color={color:?}");
        }
    });
}

#[gpui::test]
fn overview_native_charts_draw_zero_and_real_data_and_keep_narrow_scrolling(
    cx: &mut TestAppContext,
) {
    use circuitfabric_contracts::{
        ConstraintResult, FactStatus, LogicalCircuitSnapshot, Project, Severity, SnapshotAuthority,
    };
    use circuitfabric_project::{DocumentCategory, SessionSeed};
    let profile = Profile::new();
    let (view, cx) = mount_profile(cx, &profile, ControlPlaneScreen::Overview);
    cx.simulate_resize(gpui::size(px(1450.), px(1500.)));
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.bridge_probed_at = Some(std::time::Instant::now());
            cx.notify();
        })
    });
    draw(cx);
    for selector in ["overview-fact-ring", "overview-session-plot", "overview-resource-chart"] {
        assert!(cx.debug_bounds(selector).unwrap().size.height > px(100.));
    }
    assert_overview_line_painted(cx, true);
    let root = profile.0.join("project");
    std::fs::create_dir_all(&root).unwrap();
    let store = circuitfabric_project::ProjectStorage::create(
        &root,
        Project { id: "p".into(), name: "真实项目".into(), description: None },
    )
    .unwrap();
    let source = profile.0.join("evidence.md");
    std::fs::write(&source, "evidence").unwrap();
    store.import_document(source, DocumentCategory::Datasheet, "ui fixture").unwrap();
    store
        .start_session(SessionSeed {
            session_id: "s".into(),
            runtime_profile_id: "local".into(),
            backend_id: None,
        })
        .unwrap();
    let mut snapshot = LogicalCircuitSnapshot::empty(SnapshotAuthority::Observed);
    snapshot.snapshot_hash = "tip".into();
    for (index, status) in
        [FactStatus::Passed, FactStatus::Failed, FactStatus::Inconclusive, FactStatus::NotRun]
            .into_iter()
            .enumerate()
    {
        snapshot.constraints.push(ConstraintResult {
            constraint_id: index.to_string(),
            layer: "logic".into(),
            subject_refs: vec![],
            status,
            severity: Severity::Warning,
            evidence_refs: vec![],
            snapshot_hash: "tip".into(),
            explanation: "check".into(),
            recommendation: None,
        });
    }
    std::fs::write(root.join("logic/snapshots/tip.json"), serde_json::to_vec(&snapshot).unwrap())
        .unwrap();
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            crate::application::project_data::attach_project_storage(
                &mut view.workspace,
                &mut view.project_storages,
                &mut view.project_data,
                store,
            )
            .unwrap();
            cx.notify();
        })
    });
    draw(cx);
    assert_overview_line_painted(cx, true);
    let bars = cx.debug_bounds("overview-bars-p").unwrap();
    cx.update(|window, _| {
        let scale = window.scale_factor();
        for color in super::overview_charts::RESOURCE_COLORS {
            let color: gpui::Background = super::rgb(color).into();
            assert!(
                window.painted_quads().iter().any(|q| q.background == color
                    && q.bounds.size.width.as_f32() > 10. * scale
                    && q.bounds.size.height.as_f32() == 16. * scale
                    && q.bounds.origin.y.as_f32() >= bars.top().as_f32() * scale),
                "each real resource must draw a colored bar"
            );
        }
    });
    assert_eq!(view.update(cx, |view, _| view.overview_model().facts), [1; 4]);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.project_read_errors.insert("p".into(), "corrupt".into());
            cx.notify();
        })
    });
    draw(cx);
    assert_overview_line_painted(cx, false);
    assert_eq!(view.update(cx, |view, _| view.overview_model().resources[0].counts), None);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.project_read_errors.clear();
            cx.notify();
        })
    });
    cx.simulate_resize(gpui::size(px(850.), px(700.)));
    draw(cx);
    let ring = cx.debug_bounds("overview-fact-ring").unwrap();
    let plot = cx.debug_bounds("overview-session-plot").unwrap();
    assert_eq!(ring.size.height, px(160.));
    assert_eq!(plot.size.height, px(160.));
    assert!(plot.top() > ring.bottom(), "narrow window stacks chart panels");
    assert!(plot.right() <= px(850.), "plot remains inside narrow window");
    let heading = cx.debug_bounds("overview-project-kpi").unwrap();
    scroll(cx, 650., 600.);
    assert!(
        cx.debug_bounds("overview-project-kpi").unwrap().top() < heading.top(),
        "flow page scrolls rather than compressing chart canvases"
    );
}

fn assert_overview_line_painted(cx: &mut VisualTestContext, expected: bool) {
    let plot = cx.debug_bounds("overview-session-plot").unwrap();
    assert!(
        plot.size.width > px(200.),
        "line canvas must have actual horizontal space, not seven dots on one vertical line"
    );
    cx.update(|window, _| {
        let scale = window.scale_factor();
        let color: gpui::Background = super::rgb(super::ACCENT).into();
        let dots: Vec<_> = window
            .painted_quads()
            .into_iter()
            .filter(|q| {
                q.background == color
                    && q.bounds.size.width.as_f32() == 6. * scale
                    && q.bounds.size.height.as_f32() == 6. * scale
                    && q.bounds.origin.x.as_f32() >= plot.left().as_f32() * scale
                    && q.bounds.origin.y.as_f32() >= plot.top().as_f32() * scale
                    && q.bounds.origin.y.as_f32() < plot.bottom().as_f32() * scale
            })
            .collect();
        assert_eq!(
            dots.len() >= 7,
            expected,
            "known data draws seven real daily points; unknown source draws no zero series"
        );
        if expected {
            let left =
                dots.iter().map(|q| q.bounds.origin.x.as_f32()).fold(f32::INFINITY, f32::min);
            let right =
                dots.iter().map(|q| q.bounds.origin.x.as_f32()).fold(f32::NEG_INFINITY, f32::max);
            assert!(right - left > 200. * scale, "painted daily points span the date axis");
        }
    });
}

#[gpui::test]
fn overview_refresh_recovers_failed_sources_registry_and_removes_projects(cx: &mut TestAppContext) {
    use circuitfabric_contracts::Project;
    use circuitfabric_project::{ProjectRegistry, ProjectStorage};
    let profile = Profile::new();
    let root = profile.0.join("project");
    std::fs::create_dir_all(&root).unwrap();
    let store = ProjectStorage::create(
        &root,
        Project { id: "p".into(), name: "恢复验证".into(), description: None },
    )
    .unwrap();
    let registry_path = profile.0.join("projects.json");
    let mut registry = ProjectRegistry::default();
    registry.register(&store).unwrap();
    registry.save(&registry_path).unwrap();
    let saved_registry = std::fs::read(&registry_path).unwrap();
    let (view, cx) = mount_profile(cx, &profile, ControlPlaneScreen::Overview);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.bridge_probed_at = Some(std::time::Instant::now());
            view.navigation.selected_project = Some("p".into());
            cx.notify();
        })
    });
    assert!(view.update(cx, |view, _| view.overview_model().complete()));
    let old_stamp = view.read_with(cx, |view, _| view.project_data["p"].loaded_at_unix_nanos);
    let broken = root.join("logic/changesets/broken.json");
    std::fs::write(&broken, "broken").unwrap();
    view.update(cx, |view, cx| view.refresh_overview(cx));
    assert_eq!(view.update(cx, |view, _| view.overview_model().resources[0].counts), None);
    assert_eq!(
        view.read_with(cx, |view, _| view.project_data["p"].loaded_at_unix_nanos),
        old_stamp
    );
    assert!(view.read_with(cx, |view, _| view.project_read_errors["p"].contains("broken.json")));
    std::fs::remove_file(broken).unwrap();
    view.update(cx, |view, cx| view.refresh_overview(cx));
    assert!(view.update(cx, |view, _| view.overview_model().complete()));
    assert!(view.read_with(cx, |view, _| view.project_read_errors.is_empty()));
    std::fs::write(&registry_path, "broken registry").unwrap();
    view.update(cx, |view, cx| view.refresh_overview(cx));
    assert!(view.update(cx, |view, _| view.overview_model().registry_unknown));
    assert!(!view.update(cx, |view, _| view.overview_model().complete()));
    std::fs::write(&registry_path, saved_registry).unwrap();
    view.update(cx, |view, cx| view.refresh_overview(cx));
    assert!(view.update(cx, |view, _| view.overview_model().complete()));
    view.update(cx, |view, cx| {
        view.language = super::UiLanguage::English;
        view.navigation.selected_project = Some("p".into());
        cx.notify();
    });
    cx.simulate_resize(gpui::size(px(850.), px(700.)));
    draw(cx);
    assert!(cx.debug_bounds("overview-session-plot").unwrap().size.width > px(200.));
    ProjectRegistry::default().save(&registry_path).unwrap();
    view.update(cx, |view, cx| view.refresh_overview(cx));
    assert!(view.update(cx, |view, _| view.overview_model().resources.is_empty()));
    assert!(view.read_with(cx, |view, _| view.navigation.selected_project.is_none()));
    assert!(root.exists(), "removal from registry does not delete domain files");
}

struct AutoHeightDialog;
impl Render for AutoHeightDialog {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .w(px(400.))
            .v_flex()
            .child(row("auto-dialog-header", 30.))
            .child(
                capped_scroll_body("auto-dialog-body", px(200.))
                    .v_flex()
                    .child(row("auto-dialog-first", 120.))
                    .child(row("auto-dialog-last", 480.)),
            )
            .child(row("auto-dialog-footer", 40.))
    }
}

#[gpui::test]
fn auto_height_dialog_body_grows_to_its_cap_and_still_scrolls(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let (_, cx) = cx.add_window_view(|_, _| AutoHeightDialog);
    draw(cx);
    let footer = cx.debug_bounds("auto-dialog-footer").unwrap();
    assert_eq!(footer.top(), px(230.));
    let first = cx.debug_bounds("auto-dialog-first").unwrap();
    scroll(cx, 100., 100.);
    assert!(cx.debug_bounds("auto-dialog-first").unwrap().top() < first.top());
    assert_eq!(cx.debug_bounds("auto-dialog-footer").unwrap(), footer);
}

fn draw(cx: &mut VisualTestContext) {
    cx.run_until_parked();
    cx.update(|window, cx| {
        _ = window.draw(cx);
    });
}

fn scroll(cx: &mut VisualTestContext, x: f32, y: f32) {
    cx.simulate_event(ScrollWheelEvent {
        position: point(px(x), px(y)),
        delta: ScrollDelta::Pixels(point(px(0.), px(-90.))),
        ..Default::default()
    });
    draw(cx);
}

fn row(selector: &'static str, height: f32) -> Div {
    div().h(px(height)).flex_none().debug_selector(move || selector.to_owned())
}

struct LayoutFixture {
    screen: ControlPlaneScreen,
}
impl Render for LayoutFixture {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let content = match PageLayout::for_screen(self.screen) {
            PageLayout::Flow => page("flow-content")
                .child(row("flow-first", 400.))
                .child(row("flow-last", 400.))
                .into_any_element(),
            PageLayout::Workspace => workspace_page("workspace-content")
                .child(row("workspace-heading", 30.))
                .child(div().flex().flex_1().min_h(px(0.)).children(
                    [("left-pane", "left-first"), ("right-pane", "right-first")].into_iter().map(
                        |(id, selector)| {
                            div().id(id).flex_1().v_flex().scroll_y().child(row(selector, 600.))
                        },
                    ),
                ))
                .into_any_element(),
        };
        div()
            .w(px(500.))
            .h(px(300.))
            .v_flex()
            .child(row("fixed-header", 30.))
            .child(div().flex_1().min_h(px(0.)).child(page_host(self.screen, content)))
            .child(row("fixed-footer", 20.))
    }
}

#[gpui::test]
fn flow_page_scrolls_without_moving_shell_chrome(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let (_, cx) =
        cx.add_window_view(|_, _| LayoutFixture { screen: ControlPlaneScreen::Documents });
    draw(cx);
    let first = cx.debug_bounds("flow-first").unwrap();
    let header = cx.debug_bounds("fixed-header").unwrap();
    let footer = cx.debug_bounds("fixed-footer").unwrap();
    assert_eq!(footer.top(), px(280.));
    scroll(cx, 100., 120.);
    assert!(cx.debug_bounds("flow-first").unwrap().top() < first.top());
    assert_eq!(cx.debug_bounds("fixed-header").unwrap(), header);
    assert_eq!(cx.debug_bounds("fixed-footer").unwrap(), footer);
}

#[gpui::test]
fn workspace_panes_scroll_independently_and_keep_heading_visible(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let (_, cx) =
        cx.add_window_view(|_, _| LayoutFixture { screen: ControlPlaneScreen::AgentsAndMcp });
    draw(cx);
    let left = cx.debug_bounds("left-first").unwrap();
    let right = cx.debug_bounds("right-first").unwrap();
    let heading = cx.debug_bounds("workspace-heading").unwrap();
    scroll(cx, 100., 140.);
    assert!(cx.debug_bounds("left-first").unwrap().top() < left.top());
    assert_eq!(cx.debug_bounds("right-first").unwrap(), right);
    assert_eq!(cx.debug_bounds("workspace-heading").unwrap(), heading);
    assert_eq!(cx.debug_bounds("fixed-footer").unwrap().top(), px(280.));
}
