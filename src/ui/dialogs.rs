use egui::{Id, Key, Layout, RichText, TextEdit, Ui};
use meskd_keys::keys::{self, KeyInfo, KeyKind};
use meskd_keys::sshconfig::{self, NewHost};
use zeroize::Zeroizing;

use super::{App, theme};

const WIDTH: f32 = 400.0;
const NONE: &str = "None";

pub enum Dialog {
    Generate(Box<GenerateForm>),
    DeleteKey {
        name: String,
        fingerprint: String,
        error: Option<String>,
    },
    AddHost(Box<HostForm>),
    DeleteHost {
        alias: String,
        target: String,
        error: Option<String>,
    },
}

pub struct GenerateForm {
    pub(super) name: String,
    pub(super) comment: String,
    pub(super) kind: KeyKind,
    pub(super) passphrase: Zeroizing<String>,
    pub(super) confirm: Zeroizing<String>,
    pub(super) error: Option<String>,
}

pub struct HostForm {
    pub(super) alias: String,
    pub(super) hostname: String,
    pub(super) user: String,
    pub(super) port: String,
    pub(super) identity: usize,
    pub(super) names: Vec<String>,
    pub(super) error: Option<String>,
}

enum Next {
    Keep,
    Close,
}

impl Dialog {
    pub fn generate() -> Dialog {
        Dialog::Generate(Box::new(GenerateForm {
            name: "id_ed25519".to_owned(),
            comment: String::new(),
            kind: KeyKind::Ed25519,
            passphrase: Zeroizing::new(String::new()),
            confirm: Zeroizing::new(String::new()),
            error: None,
        }))
    }

    pub fn delete_key(_index: usize, key: &KeyInfo) -> Dialog {
        Dialog::DeleteKey {
            name: key.name.clone(),
            fingerprint: key.fingerprint.clone(),
            error: None,
        }
    }

    pub fn add_host(names: Vec<String>) -> Dialog {
        Dialog::AddHost(Box::new(HostForm {
            alias: String::new(),
            hostname: String::new(),
            user: String::new(),
            port: "22".to_owned(),
            identity: 0,
            names,
            error: None,
        }))
    }

    pub fn delete_host(host: &meskd_keys::HostEntry) -> Dialog {
        Dialog::DeleteHost {
            alias: host.alias.clone(),
            target: host.target(),
            error: None,
        }
    }
}

impl App {
    pub(super) fn show_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut dialog) = self.dialog.take() else {
            return;
        };
        let escape = self.job.is_none() && ctx.input(|i| i.key_pressed(Key::Escape));
        let next = egui::Modal::new(Id::unique("dialog"))
            .frame(theme::dialog())
            .show(ctx, |ui| {
                ui.set_width(WIDTH);
                match &mut dialog {
                    Dialog::Generate(form) => self.generate_dialog(ui, form),
                    Dialog::DeleteKey {
                        name,
                        fingerprint,
                        error,
                    } => self.delete_key_dialog(ui, name, fingerprint, error),
                    Dialog::AddHost(form) => self.add_host_dialog(ui, form),
                    Dialog::DeleteHost {
                        alias,
                        target,
                        error,
                    } => self.delete_host_dialog(ui, alias, target, error),
                }
            })
            .inner;
        match next {
            Next::Keep if !escape => {
                if self.dialog.is_none() {
                    self.dialog = Some(dialog);
                }
            }
            Next::Keep | Next::Close => {}
        }
    }

    fn generate_dialog(&mut self, ui: &mut Ui, f: &mut GenerateForm) -> Next {
        title(ui, "New SSH key");
        field(ui, "File name", &mut f.name, "id_ed25519");
        field(ui, "Comment", &mut f.comment, "you@computer");
        ui.label(RichText::new("Type").small().color(theme::MUTED));
        egui::ComboBox::from_id_salt("key-kind")
            .selected_text(f.kind.label())
            .width(180.0)
            .show_ui(ui, |ui| {
                for kind in KeyKind::ALL {
                    ui.selectable_value(&mut f.kind, kind, kind.label());
                }
            });
        ui.add_space(4.0);
        password_field(ui, &mut f.passphrase, "Passphrase (optional)");
        password_field(ui, &mut f.confirm, "Repeat passphrase");
        muted(
            ui,
            "A passphrase is strongly recommended. Leave both fields empty for a key without one.",
        );
        show_error(ui, &f.error);
        let (cancel, ok) = actions(ui, theme::primary("Create key"));
        if cancel {
            return Next::Close;
        }
        if !ok {
            return Next::Keep;
        }
        let name = f.name.trim().to_owned();
        if !keys::valid_name(&name) {
            f.error = Some("Use letters, numbers, dots, dashes or underscores".to_owned());
            return Next::Keep;
        }
        if *f.passphrase != *f.confirm {
            f.error = Some("The passphrases do not match".to_owned());
            return Next::Keep;
        }
        let home = self.home.clone();
        let comment = f.comment.clone();
        let kind = f.kind;
        let passphrase = Zeroizing::new(std::mem::take(&mut *f.passphrase));
        f.confirm = Zeroizing::new(String::new());
        let ctx = ui.ctx().clone();
        self.spawn("Generating the key", &ctx, move || {
            let created = name.clone();
            let result = keys::generate(&home, &name, kind, &passphrase, &comment);
            Box::new(move |app: &mut App| match result {
                Ok(()) => {
                    app.refresh();
                    app.notify(format!("Created {created}"));
                }
                Err(e) => app.notify(e.to_string()),
            })
        });
        Next::Close
    }

    fn delete_key_dialog(
        &mut self,
        ui: &mut Ui,
        name: &mut String,
        fingerprint: &str,
        error: &mut Option<String>,
    ) -> Next {
        title(ui, "Delete key");
        ui.label(format!("Delete {name}?"));
        muted(
            ui,
            "The private key and its public key are removed from this computer. This cannot be undone.",
        );
        if !fingerprint.is_empty() {
            ui.add(
                egui::Label::new(
                    RichText::new(fingerprint)
                        .monospace()
                        .size(11.5)
                        .color(theme::MUTED),
                )
                .selectable(false),
            );
        }
        show_error(ui, error);
        let (cancel, ok) = actions(ui, theme::danger("Delete"));
        if cancel {
            return Next::Close;
        }
        if ok {
            let Some(key) = self.keys.iter().find(|k| k.name == *name).cloned() else {
                return Next::Close;
            };
            match keys::delete(&key) {
                Ok(()) => {
                    self.refresh();
                    self.notify(format!("Deleted {name}"));
                    return Next::Close;
                }
                Err(e) => *error = Some(e.to_string()),
            }
        }
        Next::Keep
    }

    fn add_host_dialog(&mut self, ui: &mut Ui, f: &mut HostForm) -> Next {
        title(ui, "Add host");
        field(ui, "Alias", &mut f.alias, "prod");
        field(
            ui,
            "Host name or address",
            &mut f.hostname,
            "prod.example.com",
        );
        field(ui, "User", &mut f.user, "deploy");
        field(ui, "Port", &mut f.port, "22");
        ui.label(RichText::new("Identity file").small().color(theme::MUTED));
        let selected = f
            .names
            .get(f.identity.wrapping_sub(1))
            .cloned()
            .unwrap_or_else(|| NONE.to_owned());
        egui::ComboBox::from_id_salt("host-identity")
            .selected_text(selected)
            .width(260.0)
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut f.identity, 0, NONE);
                for (index, name) in f.names.iter().enumerate() {
                    ui.selectable_value(&mut f.identity, index + 1, name.as_str());
                }
            });
        show_error(ui, &f.error);
        let (cancel, ok) = actions(ui, theme::primary("Add"));
        if cancel {
            return Next::Close;
        }
        if ok {
            let identity = if f.identity == 0 {
                String::new()
            } else {
                f.names
                    .get(f.identity - 1)
                    .map(|name| format!("~/.ssh/{name}"))
                    .unwrap_or_default()
            };
            let host = NewHost {
                alias: f.alias.trim().to_owned(),
                hostname: f.hostname.trim().to_owned(),
                user: f.user.trim().to_owned(),
                port: f.port.trim().to_owned(),
                identity,
            };
            match sshconfig::add(&self.home, &host) {
                Ok(()) => {
                    self.refresh();
                    self.notify(format!("Added {}", host.alias));
                    return Next::Close;
                }
                Err(e) => f.error = Some(e.to_string()),
            }
        }
        Next::Keep
    }

    fn delete_host_dialog(
        &mut self,
        ui: &mut Ui,
        alias: &mut String,
        target: &mut String,
        error: &mut Option<String>,
    ) -> Next {
        title(ui, "Delete host");
        let mut question = format!("Delete {alias}?");
        if !target.is_empty() {
            question.push_str(&format!(" ({target})"));
        }
        ui.label(question);
        muted(
            ui,
            "The block is removed from your SSH config. Other hosts and settings stay as they are.",
        );
        show_error(ui, error);
        let (cancel, ok) = actions(ui, theme::danger("Delete"));
        if cancel {
            return Next::Close;
        }
        if ok {
            match sshconfig::remove(&self.home, alias) {
                Ok(()) => {
                    self.refresh();
                    self.notify(format!("Deleted {alias}"));
                    return Next::Close;
                }
                Err(e) => *error = Some(e.to_string()),
            }
        }
        Next::Keep
    }
}

fn title(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).size(18.0).strong());
    ui.add_space(2.0);
}

fn muted(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).color(theme::MUTED));
}

fn show_error(ui: &mut Ui, error: &Option<String>) {
    if let Some(error) = error {
        ui.label(RichText::new(error).color(theme::DANGER));
    }
}

fn field(ui: &mut Ui, label: &str, value: &mut String, hint: &str) -> egui::Response {
    ui.label(RichText::new(label).small().color(theme::MUTED));
    let response = ui.add(
        TextEdit::singleline(value)
            .hint_text(hint.to_owned())
            .char_limit(200)
            .desired_width(f32::INFINITY),
    );
    theme::name(&response, label);
    response
}

fn password_field(ui: &mut Ui, value: &mut Zeroizing<String>, hint: &str) {
    let response = ui.add(
        TextEdit::singleline(&mut **value)
            .password(true)
            .hint_text(hint.to_owned())
            .desired_width(f32::INFINITY),
    );
    theme::name(&response, hint);
}

fn actions(ui: &mut Ui, confirm: egui::Button<'static>) -> (bool, bool) {
    let enter = ui.input(|i| i.key_pressed(Key::Enter));
    ui.add_space(10.0);
    ui.horizontal(|ui| {
        ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
            let ok = ui.add(confirm).clicked() || enter;
            let cancel = ui.button("Cancel").clicked();
            (cancel, ok)
        })
        .inner
    })
    .inner
}
