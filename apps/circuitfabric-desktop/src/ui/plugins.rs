//! Plugins presentation and event handlers.
use super::*;

impl ControlPlaneView {
    pub(super) fn persist_plugin_governance(&mut self) -> Result<(), String> {
        self.plugin_governance.save(&self.plugin_governance_path)
    }

    /// Discovery delegates only to the declarative manifest parser.  In
    /// particular, it does not invoke a plugin's entrypoint or health
    /// endpoint, so opening this page cannot start third-party code.
    pub(super) fn discover_plugin_manifests(&mut self, cx: &mut Context<Self>) {
        let root = PathBuf::from(self.plugin_directory.read(cx).value().to_string());
        match self.plugin_governance.discover(&root) {
            Ok(count) => match self.persist_plugin_governance() {
                Ok(()) => {
                    self.status = format!(
                        "Validated {count} plugin manifest(s) in {} without executing plugin code.",
                        root.display()
                    )
                }
                Err(error) => {
                    self.status = format!(
                        "Manifest discovery completed but audit persistence failed: {error}"
                    )
                }
            },
            Err(error) => self.status = format!("Plugin manifest discovery failed: {error}"),
        }
        cx.notify();
    }

    pub(super) fn apply_plugin_operation(
        &mut self,
        id: &str,
        operation: &str,
        cx: &mut Context<Self>,
    ) {
        let result = match operation {
            "install" => self.plugin_governance.install(id),
            "update" => self.plugin_governance.update(id),
            "uninstall" => self.plugin_governance.uninstall(id),
            "revoke" => self.plugin_governance.revoke_permissions(id),
            _ => Err("Unknown plugin operation".to_owned()),
        };
        self.status = match result {
            Ok(()) => match self.persist_plugin_governance() {
                Ok(()) => format!(
                    "Plugin `{id}`: {operation} recorded in the audit trail. No plugin code was started."
                ),
                Err(error) => {
                    format!("Plugin action completed but audit persistence failed: {error}")
                }
            },
            Err(error) => format!("Plugin action failed: {error}"),
        };
        cx.notify();
    }

    #[allow(clippy::too_many_lines)]
    pub(super) fn render_plugins_page(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity().clone();
        let records = self.plugin_governance.records().to_vec();
        let audit =
            self.plugin_governance.audit().iter().rev().take(20).cloned().collect::<Vec<_>>();
        let discoverer = entity.clone();

        let manifest_source = div()
            .w_full()
            .v_flex()
            .gap_3()
            .p_4()
            .rounded_xl()
            .border_1()
            .border_color(rgb(BORDER))
            .bg(rgb(CARD_BG))
            .child(div().text_base().font_weight(FontWeight::SEMIBOLD).child("Manifest discovery"))
            .child(div().text_sm().text_color(rgb(TEXT_SECONDARY)).child(
                "Discovery reads and schema-validates plugin-manifest.json only. It does not load libraries, run scripts, launch processes, or probe plugin endpoints.",
            ))
            .child(labeled_field(
                "Manifest directory",
                "plugin-manifest-directory",
                Some("Discovery walks subdirectories and accepts only a bounded JSON manifest."),
                &self.plugin_directory,
            ))
            .child(
                action_button("discover-plugin-manifests")
                    .primary()
                    .label("Discover & validate manifests")
                    .on_click(move |_, _, cx| {
                        discoverer.update(cx, |view, cx| view.discover_plugin_manifests(cx));
                    }),
            );

        let mut inventory = div().v_flex().gap_3();
        if records.is_empty() {
            inventory = inventory.child(
                div()
                    .p_4()
                    .rounded_lg()
                    .border_1()
                    .border_color(rgb(BORDER))
                    .bg(rgb(SURFACE_BG))
                    .text_sm()
                    .text_color(rgb(TEXT_MUTED))
                    .child("No validated plugin manifests yet. Select a directory and run declarative discovery."),
            );
        }
        for record in records {
            let id = record.id.clone();
            let installer = entity.clone();
            let updater = entity.clone();
            let uninstaller = entity.clone();
            let revoker = entity.clone();
            let install_id = id.clone();
            let update_id = id.clone();
            let uninstall_id = id.clone();
            let revoke_id = id.clone();
            let requested = if record.requested_permissions.is_empty() {
                "none".to_owned()
            } else {
                record.requested_permissions.join(", ")
            };
            let granted = if record.granted_permissions.is_empty() {
                "none".to_owned()
            } else {
                record.granted_permissions.join(", ")
            };
            let lock = record.version_lock.clone().unwrap_or_else(|| "not locked".to_owned());
            let capabilities = if record.capabilities.is_empty() {
                "none declared".to_owned()
            } else {
                record.capabilities.join(", ")
            };
            inventory = inventory.child(
                div()
                    .id(format!("plugin-inventory-{}", id))
                    .w_full()
                    .v_flex()
                    .gap_3()
                    .p_4()
                    .rounded_xl()
                    .border_1()
                    .border_color(rgb(BORDER))
                    .bg(rgb(CARD_BG))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.))
                                    .truncate()
                                    .text_base()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(record.id.clone()),
                            )
                            .child(
                                div()
                                    .px_2()
                                    .py_0p5()
                                    .rounded_sm()
                                    .text_xs()
                                    .bg(rgb(0x00e0_f2fe))
                                    .text_color(rgb(0x000e_7490))
                                    .child(record.state.label()),
                            )
                            .child(
                                div()
                                    .px_2()
                                    .py_0p5()
                                    .rounded_sm()
                                    .text_xs()
                                    .bg(rgb(SURFACE_BG))
                                    .text_color(rgb(TEXT_SECONDARY))
                                    .child(format!("v{}", record.version)),
                            ),
                    )
                    .child(div().text_sm().text_color(rgb(TEXT_SECONDARY)).child(format!(
                        "{} | API {} | isolation: {} | health: {}",
                        record.kind,
                        record.api_version,
                        record.isolation,
                        record.health.label()
                    )))
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(TEXT_MUTED))
                            .child(format!("Capabilities: {capabilities}")),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(TEXT_MUTED))
                            .child(format!("Version lock: {lock}")),
                    )
                    .child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(format!(
                            "Signature: {}{}",
                            record.signature.label(),
                            record
                                .signer
                                .as_ref()
                                .map(|signer| format!(" ({signer})"))
                                .unwrap_or_default()
                        )))
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(TEXT_MUTED))
                            .child(format!("Requested permissions: {requested}")),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(TEXT_MUTED))
                            .child(format!("Granted permissions: {granted}")),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(TEXT_MUTED))
                            .child(format!("Manifest: {}", record.manifest_path.display())),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap_2()
                            .child(
                                action_button(format!("plugin-install-{id}"))
                                    .label("Install")
                                    .on_click(move |_, _, cx| {
                                        installer.update(cx, |view, cx| {
                                            view.apply_plugin_operation(&install_id, "install", cx)
                                        });
                                    }),
                            )
                            .child(
                                action_button(format!("plugin-update-{id}"))
                                    .label("Update")
                                    .on_click(move |_, _, cx| {
                                        updater.update(cx, |view, cx| {
                                            view.apply_plugin_operation(&update_id, "update", cx)
                                        });
                                    }),
                            )
                            .child(
                                action_button(format!("plugin-uninstall-{id}"))
                                    .label("Uninstall")
                                    .on_click(move |_, _, cx| {
                                        uninstaller.update(cx, |view, cx| {
                                            view.apply_plugin_operation(
                                                &uninstall_id,
                                                "uninstall",
                                                cx,
                                            )
                                        });
                                    }),
                            )
                            .child(
                                action_button(format!("plugin-revoke-{id}"))
                                    .ghost()
                                    .label("Revoke authorization")
                                    .on_click(move |_, _, cx| {
                                        revoker.update(cx, |view, cx| {
                                            view.apply_plugin_operation(&revoke_id, "revoke", cx)
                                        });
                                    }),
                            ),
                    ),
            );
        }

        let mut audit_rows = div().v_flex().gap_2();
        if audit.is_empty() {
            audit_rows = audit_rows.child(
                div()
                    .text_sm()
                    .text_color(rgb(TEXT_MUTED))
                    .child("No plugin governance events recorded."),
            );
        }
        for event in audit {
            audit_rows = audit_rows.child(
                div()
                    .p_3()
                    .rounded_lg()
                    .border_1()
                    .border_color(rgb(BORDER))
                    .bg(rgb(SURFACE_BG))
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .child(format!("{}: {}", event.plugin_id, event.action)),
                    )
                    .child(div().text_xs().text_color(rgb(TEXT_SECONDARY)).child(event.detail))
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(TEXT_MUTED))
                            .child(rfc3339(event.occurred_at_unix_seconds)),
                    ),
            );
        }

        page("plugins-governance-page")
            .child(div().text_xl().font_weight(FontWeight::SEMIBOLD).child("Plugin governance"))
            .child(div().text_sm().text_color(rgb(TEXT_SECONDARY)).child("Manifest inventory, trust, permissions, health, isolation, lifecycle controls, and an append-only local audit view."))
            .child(manifest_source)
            .child(div().text_base().font_weight(FontWeight::SEMIBOLD).child("Validated manifest inventory"))
            .child(inventory)
            .child(div().text_base().font_weight(FontWeight::SEMIBOLD).child("Audit trail"))
            .child(audit_rows)
    }
}
