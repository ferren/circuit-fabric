//! Secrets presentation and event handlers.
use super::*;

impl ControlPlaneView {
    /// Reports which source currently supplies one variable name: the
    /// unlocked vault wins over the process environment. Rendered under
    /// API-key fields so a mis-typed name is visible before a task fails;
    /// while a vault file exists but is locked, a 🔒 chip next to the
    /// hint opens the quick-unlock dialog.
    pub(super) fn secret_source_hint(
        &self,
        name: &str,
        entity: &Entity<Self>,
    ) -> Option<gpui::Div> {
        let name = name.trim();
        if name.is_empty() {
            return None;
        }
        let language = self.language;
        let (color, text) =
            match secret_source(name, self.vault.as_ref().map(UnlockedVault::values)) {
                Some(SecretSource::Vault) => (
                    0x0016_a34a,
                    language.choose_owned(
                        format!("✔ {name}：由密钥保险库提供（已解锁）"),
                        format!("✔ {name}: supplied by the unlocked secrets vault"),
                    ),
                ),
                Some(SecretSource::Environment) => (
                    0x0016_a34a,
                    language.choose_owned(
                        format!("✔ {name}：由进程环境变量提供"),
                        format!("✔ {name}: supplied by the process environment"),
                    ),
                ),
                None => (
                    0x00dc_2626,
                    language.choose_owned(
                        format!("✘ {name}：未找到该变量，请在密钥保险库收录，或设置同名环境变量"),
                        format!(
                            "✘ {name}: not found — record it in the secrets vault, or set an environment variable with this name"
                        ),
                    ),
                ),
            };
        let quick_unlock = if self.vault_file_exists && self.vault.is_none() {
            let opener = entity.clone();
            Some(
                div()
                    .id(format!("quick-unlock-{name}"))
                    .self_start()
                    .px_1p5()
                    .py_0p5()
                    .flex_none()
                    .rounded_sm()
                    .border_1()
                    .border_color(rgb(ACCENT))
                    .text_xs()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(0x000e_7490))
                    .cursor_pointer()
                    .hover(|this| this.bg(rgb(0x00f0_f9ff)))
                    .on_click(move |_, _, cx| {
                        opener.update(cx, |view, cx| {
                            view.vault_quick_unlock_open = true;
                            view.vault_message = None;
                            cx.notify();
                        });
                    })
                    .child(language.choose("🔒 快速解锁", "🔒 Quick unlock")),
            )
        } else {
            None
        };
        Some(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .whitespace_normal()
                        .text_xs()
                        .text_color(rgb(color))
                        .child(text),
                )
                .when_some(quick_unlock, ParentElement::child),
        )
    }

    /// Unlocks the vault with the typed password. Key derivation runs on a
    /// background thread; a wrong password keeps the prompt open with the
    /// vault's own error message.
    pub(super) fn unlock_vault(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.vault_busy {
            return;
        }
        let password = self.vault_password.read(cx).value().to_string();
        if password.is_empty() {
            self.vault_message = Some("请输入保险库密码。".to_owned());
            cx.notify();
            return;
        }
        self.vault_busy = true;
        self.vault_message = None;
        let path = self.vault_path.clone();
        let work = cx.background_spawn(async move {
            UnlockedVault::unlock(&path, &password).map_err(|error| error.to_string())
        });
        cx.spawn_in(window, async move |view, cx| {
            let result = work.await;
            cx.update(|window, cx| {
                view.update(cx, |view, cx| {
                    view.vault_busy = false;
                    match result {
                        Ok(vault) => {
                            view.vault_index = vault.values().names().cloned().collect::<Vec<_>>();
                            let count = view.vault_index.len();
                            view.vault = Some(vault);
                            view.vault_file_exists = true;
                            view.vault_prompt_open = false;
                            view.vault_quick_unlock_open = false;
                            view.vault_message = None;
                            view.vault_password.update(cx, |state, cx| {
                                state.set_value("", window, cx);
                            });
                            view.status = format!("保险库已解锁，收录 {count} 个变量。");
                        }
                        Err(error) => {
                            view.vault_message = Some(error);
                        }
                    }
                    cx.notify();
                })
                .ok();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    /// Creates a new vault file and unlocks it immediately.
    pub(super) fn create_vault(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.vault_busy {
            return;
        }
        let password = self.vault_password.read(cx).value().to_string();
        let confirm = self.vault_password_confirm.read(cx).value().to_string();
        if password != confirm {
            self.vault_message = Some("两次输入的密码不一致。".to_owned());
            cx.notify();
            return;
        }
        self.vault_busy = true;
        self.vault_message = None;
        let path = self.vault_path.clone();
        let work = cx.background_spawn(async move {
            UnlockedVault::create(&path, &password).map_err(|error| error.to_string())
        });
        cx.spawn_in(window, async move |view, cx| {
            let result = work.await;
            cx.update(|window, cx| {
                view.update(cx, |view, cx| {
                    view.vault_busy = false;
                    match result {
                        Ok(vault) => {
                            view.vault = Some(vault);
                            view.vault_file_exists = true;
                            view.vault_index = Vec::new();
                            view.vault_message = None;
                            view.vault_password.update(cx, |state, cx| {
                                state.set_value("", window, cx);
                            });
                            view.vault_password_confirm.update(cx, |state, cx| {
                                state.set_value("", window, cx);
                            });
                            "保险库已创建并解锁。".clone_into(&mut view.status);
                        }
                        Err(error) => {
                            view.vault_message = Some(error);
                        }
                    }
                    cx.notify();
                })
                .ok();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    /// Locks the vault: dropping it zeroizes the derived key and decrypted
    /// values. The plaintext name index stays readable from disk.
    pub(super) fn relock_vault(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.vault_busy {
            return;
        }
        self.vault_password.update(cx, |state, cx| state.set_value("", window, cx));
        self.vault_password_confirm.update(cx, |state, cx| state.set_value("", window, cx));
        self.vault = None;
        self.vault_prompt_open = false;
        self.vault_index = UnlockedVault::variable_names(&self.vault_path).unwrap_or_default();
        self.selected_secret = None;
        self.vault_message = None;
        "保险库已锁定；正在运行的任务保留其启动时注入的变量。".clone_into(&mut self.status);
        cx.notify();
    }

    /// Adds or replaces one variable. The vault re-encrypts atomically on
    /// each write, and the value field is cleared so it never echoes back.
    pub(super) fn save_secret_entry(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = self.secret_name.read(cx).value().trim().to_owned();
        let value = self.secret_value.read(cx).value().to_string();
        if name.is_empty() {
            self.vault_message = Some("请填写变量名。".to_owned());
            cx.notify();
            return;
        }
        let Some(vault) = self.vault.as_mut() else {
            self.vault_message = Some("请先解锁保险库。".to_owned());
            cx.notify();
            return;
        };
        match vault.set(&name, &value) {
            Ok(()) => {
                self.vault_index = vault.values().names().cloned().collect();
                self.secret_value.update(cx, |state, cx| state.set_value("", window, cx));
                self.selected_secret = Some(name.clone());
                self.vault_message = None;
                self.status = format!("已保存变量 {name}（值已加密写入保险库）。");
            }
            Err(error) => {
                self.vault_message = Some(error.to_string());
            }
        }
        cx.notify();
    }

    pub(super) fn remove_secret_entry(&mut self, name: &str, cx: &mut Context<Self>) {
        let Some(vault) = self.vault.as_mut() else {
            self.vault_message = Some("请先解锁保险库。".to_owned());
            cx.notify();
            return;
        };
        match vault.remove(name) {
            Ok(_) => {
                self.vault_index = vault.values().names().cloned().collect();
                if self.selected_secret.as_deref() == Some(name) {
                    self.selected_secret = None;
                }
                self.vault_message = None;
                self.status = format!("已移除变量 {name}。");
            }
            Err(error) => {
                self.vault_message = Some(error.to_string());
            }
        }
        cx.notify();
    }

    /// Re-encrypts the vault under a new password. Key derivation is slow,
    /// so the vault is moved to a background thread and always put back —
    /// even when the change fails, the session stays unlocked.
    pub(super) fn change_vault_password(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.vault_busy {
            return;
        }
        let password = self.vault_password.read(cx).value().to_string();
        let confirm = self.vault_password_confirm.read(cx).value().to_string();
        if password != confirm {
            self.vault_message = Some("两次输入的密码不一致。".to_owned());
            cx.notify();
            return;
        }
        let Some(vault) = self.vault.take() else {
            self.vault_message = Some("请先解锁保险库。".to_owned());
            cx.notify();
            return;
        };
        self.vault_busy = true;
        self.vault_message = None;
        let work = cx.background_spawn(async move {
            let mut vault = vault;
            let result = vault.change_password(&password).map_err(|error| error.to_string());
            (vault, result)
        });
        cx.spawn_in(window, async move |view, cx| {
            let (vault, result) = work.await;
            cx.update(|window, cx| {
                view.update(cx, |view, cx| {
                    view.vault_busy = false;
                    view.vault = Some(vault);
                    match result {
                        Ok(()) => {
                            view.vault_password.update(cx, |state, cx| {
                                state.set_value("", window, cx);
                            });
                            view.vault_password_confirm.update(cx, |state, cx| {
                                state.set_value("", window, cx);
                            });
                            view.vault_message = None;
                            "保险库密码已更新。".clone_into(&mut view.status);
                        }
                        Err(error) => {
                            view.vault_message = Some(error);
                        }
                    }
                    cx.notify();
                })
                .ok();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    pub(super) fn dismiss_vault_prompt(&mut self, cx: &mut Context<Self>) {
        self.vault_prompt_open = false;
        "已暂缓解锁；缺少密钥的任务会失败并提示来源。可在「密钥保险库」页随时解锁。"
            .clone_into(&mut self.status);
        cx.notify();
    }

    /// The secrets vault page: one encrypted file on disk, held decrypted
    /// only in memory while unlocked. The left column lists the recorded
    /// variable names (names are not secret); the right pane creates,
    /// unlocks, and locks the vault and edits entries without ever
    /// echoing a value back.
    #[allow(clippy::too_many_lines)]
    pub(super) fn render_secrets_page(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let entity = cx.entity().clone();
        let language = self.language;
        let unlocked = self.vault.is_some();
        let file_exists = self.vault_file_exists;
        let busy = self.vault_busy;
        let message = self.vault_message.clone();

        let mut secret_rows = div().v_flex().gap_2();
        if self.vault_index.is_empty() {
            secret_rows = secret_rows.child(
                div()
                    .p_3()
                    .rounded_lg()
                    .border_1()
                    .border_color(rgb(BORDER))
                    .bg(rgb(SURFACE_BG))
                    .text_sm()
                    .text_color(rgb(TEXT_MUTED))
                    .child(language.choose(
                        "暂无收录变量；保存第一条密钥后出现在这里。",
                        "No variables recorded yet; saved keys appear here.",
                    )),
            );
        } else {
            for name in self.vault_index.clone() {
                let selector = entity.clone();
                let selected = unlocked && self.selected_secret.as_deref() == Some(name.as_str());
                let row_name = name.clone();
                let row = div()
                    .id(format!("secret-row-{name}"))
                    .v_flex()
                    .gap_1()
                    .p_3()
                    .rounded_lg()
                    .border_1()
                    .border_color(rgb(if selected { ACCENT } else { BORDER }))
                    .bg(rgb(if selected { 0x00f0_f9ff } else { CARD_BG }))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .when(!unlocked, |this| this.child(status_dot(0x0094_a3b8)))
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(if selected {
                                        FontWeight::SEMIBOLD
                                    } else {
                                        FontWeight::MEDIUM
                                    })
                                    .text_color(rgb(if unlocked {
                                        TEXT_PRIMARY
                                    } else {
                                        TEXT_MUTED
                                    }))
                                    .child(name),
                            ),
                    );
                secret_rows = secret_rows.child(if unlocked {
                    row.cursor_pointer().hover(|this| this.border_color(rgb(ACCENT_SOFT))).on_click(
                        move |_, _, cx| {
                            selector.update(cx, |view, cx| {
                                view.selected_secret = Some(row_name.clone());
                                cx.notify();
                            });
                        },
                    )
                } else {
                    row.opacity(0.7)
                });
            }
        }

        let detail = if !file_exists {
            let creator = entity.clone();
            detail_pane("vault-detail-1")
                .gap_4()
                .p_5()
                .rounded_xl()
                .border_1()
                .border_color(rgb(BORDER))
                .bg(rgb(CARD_BG))
                .child(
                    div()
                        .v_flex()
                        .gap_1()
                        .child(
                            div()
                                .text_lg()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(language
                                    .choose("创建密钥保险库", "Create the secrets vault")),
                        )
                        .child(
                            div()
                                .text_sm()
                                .whitespace_normal()
                                .text_color(rgb(TEXT_SECONDARY))
                                .child(language.choose(
                                    "保险库以你设置的安全密码加密保存各 Provider/MCP 的 API Key；解锁后密钥作为环境变量注入运行时与 MCP 进程，不会写入配置文件或命令行参数。",
                                    "The vault stores Provider/MCP API keys encrypted with your passphrase; once unlocked, keys are injected into runtime and MCP processes as environment variables — never written to config files or command-line arguments.",
                                )),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap_3()
                        .child(labeled_field(
                            language.choose("保险库密码", "Vault password"),
                            "vault-create-password",
                            None,
                            &self.vault_password,
                        ))
                        .child(labeled_field(
                            language.choose("确认密码", "Confirm password"),
                            "vault-create-confirm",
                            None,
                            &self.vault_password_confirm,
                        )),
                )
                .when_some(message, |this, message| {
                    this.child(
                        div()
                            .text_xs()
                            .whitespace_normal()
                            .text_color(rgb(0x00dc_2626))
                            .child(message),
                    )
                })
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_3()
                        .child(
                            action_button("create-vault")
                                .primary()
                                .label(if busy {
                                    language.choose("正在创建…", "Creating…")
                                } else {
                                    language.choose("创建保险库", "Create vault")
                                })
                                .disabled(busy)
                                .on_click(move |_, window, cx| {
                                    creator.update(cx, |view, cx| {
                                        view.create_vault(window, cx);
                                    });
                                }),
                        ),
                )
                .child(
                    div()
                        .text_xs()
                        .whitespace_normal()
                        .text_color(rgb(TEXT_MUTED))
                        .child(language.choose(
                            "密码至少 8 个字符，请牢记：丢失后无法找回已存密钥。",
                            "At least 8 characters; memorize it — a lost password cannot recover stored keys.",
                        )),
                )
                .into_any_element()
        } else if !unlocked {
            let dialog_entity = entity.clone();
            detail_pane("vault-detail-2")
                .gap_4()
                .p_5()
                .rounded_xl()
                .border_1()
                .border_color(rgb(BORDER))
                .bg(rgb(CARD_BG))
                .child(
                    div()
                        .v_flex()
                        .gap_1()
                        .child(
                            div()
                                .text_lg()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(language
                                    .choose("解锁密钥保险库", "Unlock the secrets vault")),
                        )
                        .child(
                            div()
                                .text_sm()
                                .whitespace_normal()
                                .text_color(rgb(TEXT_SECONDARY))
                                .child(language.choose(
                                    "输入密码解锁后，密钥值才会注入运行时与 MCP 进程；锁定状态下任务与工具调用只能从进程环境读取。",
                                    "Keys are injected into runtime and MCP processes only after you unlock with the password; while locked, tasks and tool calls can only read the process environment.",
                                )),
                        ),
                )
                .child(labeled_field(
                    language.choose("保险库密码", "Vault password"),
                    "vault-unlock-password",
                    None,
                    &self.vault_password,
                ))
                .when_some(message, |this, message| {
                    this.child(
                        div()
                            .text_xs()
                            .whitespace_normal()
                            .text_color(rgb(0x00dc_2626))
                            .child(message),
                    )
                })
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_3()
                        .child(
                            action_button("unlock-vault-page")
                                .primary()
                                .label(if busy {
                                    language.choose("正在解锁…", "Unlocking…")
                                } else {
                                    language.choose("解锁", "Unlock")
                                })
                                .disabled(busy)
                                .on_click(move |_, window, cx| {
                                    dialog_entity.update(cx, |view, cx| {
                                        view.unlock_vault(window, cx);
                                    });
                                }),
                        ),
                )
                .child(
                    div()
                        .text_xs()
                        .whitespace_normal()
                        .text_color(rgb(TEXT_MUTED))
                        .child(format!(
                            "{} {}",
                            language.choose("保险库文件：", "Vault file:"),
                            self.vault_path.display()
                        )),
                )
                .into_any_element()
        } else {
            let relocker = entity.clone();
            let saver = entity.clone();
            let password_changer = entity.clone();
            let count = self.vault_index.len();
            let editing_name = self.secret_name.read(cx).value().trim().to_owned();
            let updating = self.vault_index.iter().any(|name| name == &editing_name);

            let mut selected_card = None;
            if let Some(selected) = self.selected_secret.clone()
                && self.vault_index.iter().any(|name| name == &selected)
            {
                let remover = entity.clone();
                let remove_name = selected.clone();
                selected_card = Some(
                    div()
                        .v_flex()
                        .gap_2()
                        .p_4()
                        .rounded_lg()
                        .border_1()
                        .border_color(rgb(BORDER))
                        .bg(rgb(SURFACE_BG))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_between()
                                .gap_3()
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w(px(0.))
                                        .v_flex()
                                        .gap_1()
                                        .child(
                                            div()
                                                .text_sm()
                                                .font_weight(FontWeight::SEMIBOLD)
                                                .child(format!("变量名 {selected}")),
                                        )
                                        .child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(
                                            language.choose(
                                                "值 ••••••••（不回显）",
                                                "Value •••••••• (never echoed)",
                                            ),
                                        )),
                                )
                                .child(
                                    action_button(format!("remove-secret-{selected}"))
                                        .danger()
                                        .label(language.choose("删除", "Remove"))
                                        .on_click(move |_, _, cx| {
                                            remover.update(cx, |view, cx| {
                                                view.remove_secret_entry(&remove_name, cx);
                                            });
                                        }),
                                ),
                        )
                        .into_any_element(),
                );
            }

            detail_pane("vault-detail-3")
                .gap_4()
                .p_5()
                .rounded_xl()
                .border_1()
                .border_color(rgb(BORDER))
                .bg(rgb(CARD_BG))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap_3()
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.))
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(status_dot(0x0022_c55e))
                                .child(
                                    div()
                                        .text_sm()
                                        .whitespace_normal()
                                        .text_color(rgb(TEXT_SECONDARY))
                                        .child(language.choose_owned(
                                            format!(
                                                "已解锁 · {count} 个变量 · 文件 {}",
                                                self.vault_path.display()
                                            ),
                                            format!(
                                                "Unlocked · {count} variables · file {}",
                                                self.vault_path.display()
                                            ),
                                        )),
                                ),
                        )
                        .child(
                            action_button("relock-vault")
                                .ghost()
                                .label(language.choose("锁定保险库", "Lock vault"))
                                .disabled(busy)
                                .on_click(move |_, window, cx| {
                                    relocker.update(cx, |view, cx| {
                                        view.relock_vault(window, cx);
                                    });
                                }),
                        ),
                )
                .when_some(selected_card, ParentElement::child)
                .child(
                    div()
                        .v_flex()
                        .gap_3()
                        .child(div().text_base().font_weight(FontWeight::SEMIBOLD).child(
                            language.choose("收录 / 更新变量", "Record / update a variable"),
                        ))
                        .child(
                            div()
                                .flex()
                                .flex_wrap()
                                .gap_3()
                                .child(labeled_field(
                                    language.choose("变量名", "Variable name"),
                                    "secret-name",
                                    None,
                                    &self.secret_name,
                                ))
                                .child(labeled_field(
                                    language.choose("密钥值", "Secret value"),
                                    "secret-value",
                                    Some(language.choose(
                                        "粘贴密钥值；保存后不再回显。",
                                        "Paste the key value; it is never echoed after saving.",
                                    )),
                                    &self.secret_value,
                                )),
                        )
                        .when_some(message, |this, message| {
                            this.child(
                                div()
                                    .text_xs()
                                    .whitespace_normal()
                                    .text_color(rgb(0x00dc_2626))
                                    .child(message),
                            )
                        })
                        .child(
                            action_button("save-secret")
                                .primary()
                                .label(if updating {
                                    language.choose("更新变量", "Update variable")
                                } else {
                                    language.choose("保存变量", "Save variable")
                                })
                                .on_click(move |_, window, cx| {
                                    saver.update(cx, |view, cx| {
                                        view.save_secret_entry(window, cx);
                                    });
                                }),
                        ),
                )
                .child(
                    div()
                        .v_flex()
                        .gap_3()
                        .child(
                            div()
                                .text_base()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(language.choose("修改保险库密码", "Change vault password")),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_wrap()
                                .gap_3()
                                .child(labeled_field(
                                    language.choose("新密码", "New password"),
                                    "vault-change-password",
                                    None,
                                    &self.vault_password,
                                ))
                                .child(labeled_field(
                                    language.choose("确认新密码", "Confirm new password"),
                                    "vault-change-confirm",
                                    None,
                                    &self.vault_password_confirm,
                                )),
                        )
                        .child(
                            action_button("change-vault-password")
                                .ghost()
                                .label(if busy {
                                    language.choose("正在更新…", "Updating…")
                                } else {
                                    language.choose("更新密码", "Update password")
                                })
                                .disabled(busy)
                                .on_click(move |_, window, cx| {
                                    password_changer.update(cx, |view, cx| {
                                        view.change_vault_password(window, cx);
                                    });
                                }),
                        ),
                )
                .into_any_element()
        };

        workspace_page("secrets-vault-page")
            .min_w(px(720.))
            .relative()
            .v_flex()
            .gap_4()
            .p_6()
            .child(
                div()
                    .v_flex()
                    .gap_1()
                    .child(
                        div()
                            .text_xl()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(language.choose("密钥保险库", "Secrets vault")),
                    )
                    .child(
                        div().text_sm().text_color(rgb(TEXT_SECONDARY)).child(
                            language.choose(
                                "API Key 加密保存在本机保险库文件中，仅解锁期间驻留内存；同名进程环境变量仍是后备来源。",
                                "API keys live in one encrypted local vault file and in memory only while unlocked; same-named process environment variables remain the fallback source.",
                            ),
                        ),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.))
                    .flex()
                    .gap_4()
                    .debug_selector(|| "vault-pane-row".to_owned())
                    .child(
                        div()
                            .id("vault-list-scroll")
                            .min_h(px(0.))
                            .scroll_y()
                            .w(px(320.))
                            .flex_none()
                            .v_flex()
                            .gap_2()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .child(agents_group_label(
                                        language.choose("变量", "Variables"),
                                    ))
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(rgb(TEXT_MUTED))
                                            .child(language.choose_owned(
                                                format!("{} 个", self.vault_index.len()),
                                                format!("{}", self.vault_index.len()),
                                            )),
                                    ),
                            )
                            .child(secret_rows),
                    )
                    .child(detail),
            )
            .child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(self.status.clone()))
    }

    /// Startup prompt shown over the workspace when a vault file exists
    /// but is still locked: unlock now, or explicitly defer. The backdrop
    /// deliberately does not dismiss on click — deferral is a choice.
    pub(super) fn render_vault_prompt(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity().clone();
        let language = self.language;
        let dialog_entity = entity.clone();
        let busy = self.vault_busy;
        let message = self.vault_message.clone();

        div()
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .p_6()
            .child(
                div()
                    .id("vault-prompt-backdrop")
                    .absolute()
                    .inset_0()
                    .bg(rgba(0x00_0f17_2ab3))
                    .occlude(),
            )
            .child(
                div()
                    .relative()
                    .occlude()
                    .w(px(480.))
                    .v_flex()
                    .gap_4()
                    .p_5()
                    .rounded_xl()
                    .border_1()
                    .border_color(rgb(ACCENT_SOFT))
                    .bg(rgb(SURFACE_BG))
                    .shadow_lg()
                    .child(
                        div()
                            .v_flex()
                            .gap_1()
                            .child(
                                div()
                                    .text_lg()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(rgb(TEXT_PRIMARY))
                                    .child(language
                                        .choose("解锁密钥保险库", "Unlock the secrets vault")),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .whitespace_normal()
                                    .text_color(rgb(TEXT_SECONDARY))
                                    .child(language.choose(
                                        "保险库以你设置的安全密码加密保存各 Provider/MCP 的 API Key；解锁后密钥作为环境变量注入运行时与 MCP 进程，不会写入配置文件或命令行参数。",
                                        "The vault stores Provider/MCP API keys encrypted with your passphrase; once unlocked, keys are injected into runtime and MCP processes as environment variables — never written to config files or command-line arguments.",
                                    )),
                            ),
                    )
                    .child(
                        div()
                            .v_flex()
                            .gap_1()
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(language.choose("保险库密码", "Vault password")),
                            )
                            .child(
                                div()
                                    .h(px(36.))
                                    .px_2()
                                    .flex()
                                    .items_center()
                                    .rounded_md()
                                    .border_1()
                                    .border_color(rgb(BORDER))
                                    .bg(rgb(CARD_BG))
                                    .child(
                                        InputBase::new("vault-prompt-password")
                                            .flex_1()
                                            .h_full()
                                            .flex()
                                            .items_center()
                                            .child(self.vault_password.clone()),
                                    ),
                            ),
                    )
                    .when_some(message, |this, message| {
                        this.child(
                            div()
                                .text_xs()
                                .whitespace_normal()
                                .text_color(rgb(0x00dc_2626))
                                .child(message),
                        )
                    })
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_end()
                            .gap_2()
                            .child(
                                action_button("dismiss-vault-prompt")
                                    .ghost()
                                    .label(language.choose("暂不解锁", "Not now"))
                                    .on_click(move |_, _, cx| {
                                        entity
                                            .update(cx, ControlPlaneView::dismiss_vault_prompt);
                                    }),
                            )
                            .child(
                                action_button("unlock-vault-prompt")
                                    .primary()
                                    .label(if busy {
                                        language.choose("正在解锁…", "Unlocking…")
                                    } else {
                                        language.choose("解锁", "Unlock")
                                    })
                                    .disabled(busy)
                                    .on_click(move |_, window, cx| {
                                        dialog_entity.update(cx, |view, cx| {
                                            view.unlock_vault(window, cx);
                                        });
                                    }),
                            ),
                    ),
            )
    }

    /// Quick unlock dialog opened from the 🔒 chips next to secret-source
    /// hints: same card structure as the startup prompt, with Cancel
    /// instead of the deferred choice. The backdrop does not dismiss.
    pub(super) fn render_vault_quick_unlock(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity().clone();
        let language = self.language;
        let dialog_entity = entity.clone();
        let busy = self.vault_busy;
        let message = self.vault_message.clone();

        div()
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .p_6()
            .child(
                div()
                    .id("vault-quick-unlock-backdrop")
                    .absolute()
                    .inset_0()
                    .bg(rgba(0x00_0f17_2ab3))
                    .occlude(),
            )
            .child(
                div()
                    .relative()
                    .occlude()
                    .w(px(480.))
                    .v_flex()
                    .gap_4()
                    .p_5()
                    .rounded_xl()
                    .border_1()
                    .border_color(rgb(ACCENT_SOFT))
                    .bg(rgb(SURFACE_BG))
                    .shadow_lg()
                    .child(
                        div()
                            .v_flex()
                            .gap_1()
                            .child(
                                div()
                                    .text_lg()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(rgb(TEXT_PRIMARY))
                                    .child(language
                                        .choose("快速解锁保险库", "Quick vault unlock")),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .whitespace_normal()
                                    .text_color(rgb(TEXT_SECONDARY))
                                    .child(language.choose(
                                        "输入保险库密码即可解锁；解锁后密钥作为环境变量注入运行时与 MCP 进程，不会写入配置文件或命令行参数。",
                                        "Enter the vault password to unlock; keys are then injected into runtime and MCP processes as environment variables — never written to config files or command-line arguments.",
                                    )),
                            ),
                    )
                    .child(
                        div()
                            .v_flex()
                            .gap_1()
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(language.choose("保险库密码", "Vault password")),
                            )
                            .child(
                                div()
                                    .h(px(36.))
                                    .px_2()
                                    .flex()
                                    .items_center()
                                    .rounded_md()
                                    .border_1()
                                    .border_color(rgb(BORDER))
                                    .bg(rgb(CARD_BG))
                                    .child(
                                        InputBase::new("vault-quick-unlock-password")
                                            .flex_1()
                                            .h_full()
                                            .flex()
                                            .items_center()
                                            .child(self.vault_password.clone()),
                                    ),
                            ),
                    )
                    .when_some(message, |this, message| {
                        this.child(
                            div()
                                .text_xs()
                                .whitespace_normal()
                                .text_color(rgb(0x00dc_2626))
                                .child(message),
                        )
                    })
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_end()
                            .gap_2()
                            .child(
                                action_button("cancel-vault-quick-unlock")
                                    .ghost()
                                    .label(language.choose("取消", "Cancel"))
                                    .on_click(move |_, window, cx| {
                                        entity.update(cx, |view, cx| {
                                            view.vault_quick_unlock_open = false;
                                            view.vault_message = None;
                                            view.vault_password.update(cx, |state, cx| {
                                                state.set_value("", window, cx);
                                            });
                                            cx.notify();
                                        });
                                    }),
                            )
                            .child(
                                action_button("unlock-vault-quick")
                                    .primary()
                                    .label(if busy {
                                        language.choose("正在解锁…", "Unlocking…")
                                    } else {
                                        language.choose("解锁", "Unlock")
                                    })
                                    .disabled(busy)
                                    .on_click(move |_, window, cx| {
                                        dialog_entity.update(cx, |view, cx| {
                                            view.unlock_vault(window, cx);
                                        });
                                    }),
                            ),
                    ),
            )
    }
}
