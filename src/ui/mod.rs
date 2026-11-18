mod clip;
mod dialogs;
#[cfg(test)]
mod tests;
pub mod theme;

use std::fs;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::time::{Duration, Instant};

use egui::{Align, CursorIcon, Key, Layout, RichText, Sense, Stroke, TextEdit, Ui};
use meskd_keys::keys::{self, KeyInfo};
use meskd_keys::sshconfig::{self, HostEntry};

use clip::Clip;
use dialogs::Dialog;

type Apply = Box<dyn FnOnce(&mut App) + Send>;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Keys,
    Hosts,
}

#[derive(Clone, Copy)]
enum Action {
    NewKey,
    AddHost,
    Import,
    OpenFolder,
    Refresh,
    CopyKey(usize),
    DeleteKey(usize),
    FixPerms(usize),
    CopyHost(usize),
    DeleteHost(usize),
}

pub struct App {
    home: PathBuf,
    tab: Tab,
    keys: Vec<KeyInfo>,
    hosts: Vec<HostEntry>,
    search: String,
    focus_search: bool,
    dialog: Option<Dialog>,
    job: Option<(String, Receiver<Apply>)>,
    toast: Option<(String, Instant)>,
    clip: Clip,
}

impl App {
    pub fn new(home: PathBuf) -> App {
        let mut app = App {
            home,
            tab: Tab::Keys,
            keys: Vec::new(),
            hosts: Vec::new(),
            search: String::new(),
            focus_search: false,
            dialog: None,
            job: None,
            toast: None,
            clip: Clip::default(),
        };
        app.refresh();
        app
    }

    fn refresh(&mut self) {
        self.keys = keys::scan(&self.home);
        let text = fs::read_to_string(sshconfig::config_path(&self.home)).unwrap_or_default();
        self.hosts = sshconfig::parse(&text);
    }

    fn notify(&mut self, message: impl Into<String>) {
        self.toast = Some((message.into(), Instant::now()));
    }

    fn spawn(
        &mut self,
        label: &str,
        ctx: &egui::Context,
        work: impl FnOnce() -> Apply + Send + 'static,
    ) {
        let (tx, rx) = channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(work());
            ctx.request_repaint();
        });
        self.job = Some((label.to_owned(), rx));
    }

    fn poll_job(&mut self) {
        let apply: Option<Apply> = match &self.job {
            Some((_, rx)) => match rx.try_recv() {
                Ok(apply) => Some(apply),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => Some(Box::new(|app: &mut App| {
                    app.notify("Something went wrong, please try again");
                })),
            },
            None => None,
        };
        if let Some(apply) = apply {
            self.job = None;
            apply(self);
        }
    }

    fn run(&mut self, action: Action) {
        match action {
            Action::NewKey => self.dialog = Some(Dialog::generate()),
            Action::AddHost => {
                let names = self.keys.iter().map(|k| k.name.clone()).collect();
                self.dialog = Some(Dialog::add_host(names));
            }
            Action::Import => {
                let Some(paths) = rfd::FileDialog::new()
                    .set_title("Import SSH keys")
                    .pick_files()
                else {
                    return;
                };
                let mut messages = Vec::new();
                for path in paths {
                    match keys::import(&self.home, &path) {
                        Ok(message) => messages.push(message),
                        Err(e) => messages.push(format!(
                            "{}: {e}",
                            path.file_name().unwrap_or_default().to_string_lossy()
                        )),
                    }
                }
                self.refresh();
                self.notify(messages.join(", "));
            }
            Action::OpenFolder => {
                let _ = fs::create_dir_all(&self.home);
                open_folder(&self.home);
            }
            Action::Refresh => {
                self.refresh();
                self.notify("Refreshed");
            }
            Action::CopyKey(index) => {
                let Some(key) = self.keys.get(index) else {
                    return;
                };
                let Some(text) = key.public_text.clone() else {
                    self.notify("There is no public key to copy");
                    return;
                };
                if self.clip.copy(&text) {
                    self.notify("Public key copied");
                } else {
                    self.notify("Could not reach the clipboard");
                }
            }
            Action::DeleteKey(index) => {
                if let Some(key) = self.keys.get(index) {
                    self.dialog = Some(Dialog::delete_key(index, key));
                }
            }
            Action::FixPerms(index) => {
                let Some(key) = self.keys.get(index) else {
                    return;
                };
                match keys::tighten(key) {
                    Ok(()) => {
                        self.refresh();
                        self.notify("Permissions set to 0600");
                    }
                    Err(e) => self.notify(e.to_string()),
                }
            }
            Action::CopyHost(index) => {
                let Some(host) = self.hosts.get(index) else {
                    return;
                };
                let command = host.ssh_command();
                if self.clip.copy(&command) {
                    self.notify(format!("Copied {command}"));
                } else {
                    self.notify("Could not reach the clipboard");
                }
            }
            Action::DeleteHost(index) => {
                if let Some(host) = self.hosts.get(index) {
                    self.dialog = Some(Dialog::delete_host(host));
                }
            }
        }
    }

    fn shortcuts(&mut self, ctx: &egui::Context) -> Option<Action> {
        if self.job.is_some() {
            return None;
        }
        let typing = ctx.text_edit_focused();
        ctx.input_mut(|i| {
            if i.consume_key(egui::Modifiers::COMMAND, Key::R) {
                return Some(Action::Refresh);
            }
            if !typing {
                if i.consume_key(egui::Modifiers::COMMAND, Key::F) {
                    self.focus_search = true;
                }
                if i.consume_key(egui::Modifiers::COMMAND, Key::N) {
                    return Some(Action::NewKey);
                }
            }
            None
        })
    }

    fn header(&mut self, ui: &mut Ui) -> Option<Action> {
        let mut action = None;
        ui.horizontal(|ui| {
            if ui
                .selectable_label(self.tab == Tab::Keys, RichText::new("Keys").size(15.0))
                .clicked()
            {
                self.tab = Tab::Keys;
                self.search.clear();
            }
            if ui
                .selectable_label(self.tab == Tab::Hosts, RichText::new("Hosts").size(15.0))
                .clicked()
            {
                self.tab = Tab::Hosts;
                self.search.clear();
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.menu_button("Menu", |ui| {
                    ui.set_min_width(200.0);
                    if ui.button("Import key").clicked() {
                        action = Some(Action::Import);
                    }
                    if ui.button("Open .ssh folder").clicked() {
                        action = Some(Action::OpenFolder);
                    }
                    ui.separator();
                    if ui.button("Refresh").shortcut_text("Ctrl+R").clicked() {
                        action = Some(Action::Refresh);
                    }
                    if action.is_some() {
                        ui.close();
                    }
                });
                let label = match self.tab {
                    Tab::Keys => "New key",
                    Tab::Hosts => "Add host",
                };
                if ui.add(theme::primary(label)).clicked() {
                    action = Some(match self.tab {
                        Tab::Keys => Action::NewKey,
                        Tab::Hosts => Action::AddHost,
                    });
                }
            });
        });
        ui.add_space(6.0);
        let search = ui.add(
            TextEdit::singleline(&mut self.search)
                .hint_text("Search")
                .desired_width(f32::INFINITY),
        );
        theme::name(&search, "Search");
        if self.focus_search {
            search.request_focus();
            self.focus_search = false;
        }
        ui.add_space(6.0);
        action
    }

    fn list_keys(&mut self, ui: &mut Ui) -> Option<Action> {
        let mut action = None;
        let query = self.search.trim().to_lowercase();
        if self.keys.is_empty() {
            column(ui, 360.0, |ui| {
                theme::logo(ui, 44.0);
                ui.add_space(4.0);
                ui.label(RichText::new("No SSH keys yet").size(18.0).strong());
                ui.label(
                    RichText::new(
                        "Create a key for this computer, or import one you already use. Keys live in .ssh in your home folder.",
                    )
                    .color(theme::MUTED),
                );
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.add(theme::primary("Create key")).clicked() {
                        action = Some(Action::NewKey);
                    }
                    if ui.button("Import").clicked() {
                        action = Some(Action::Import);
                    }
                });
            });
            return action;
        }
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                let mut shown = 0;
                for (index, key) in self.keys.iter().enumerate() {
                    if !query.is_empty()
                        && !key.name.to_lowercase().contains(&query)
                        && !key.comment.to_lowercase().contains(&query)
                        && !key.fingerprint.to_lowercase().contains(&query)
                    {
                        continue;
                    }
                    shown += 1;
                    if let Some(a) = key_row(ui, index, key) {
                        action = Some(a);
                    }
                }
                if shown == 0 {
                    ui.add_space(20.0);
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new("No matches").color(theme::MUTED));
                    });
                }
            });
        action
    }

    fn list_hosts(&mut self, ui: &mut Ui) -> Option<Action> {
        let mut action = None;
        let query = self.search.trim().to_lowercase();
        if self.hosts.is_empty() {
            column(ui, 360.0, |ui| {
                ui.label(RichText::new("No hosts yet").size(18.0).strong());
                ui.label(
                    RichText::new(
                        "Hosts you add here are written to your SSH config, so ssh can reach them by name.",
                    )
                    .color(theme::MUTED),
                );
                ui.add_space(8.0);
                if ui.add(theme::primary("Add host")).clicked() {
                    action = Some(Action::AddHost);
                }
            });
            return action;
        }
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                let mut shown = 0;
                for (index, host) in self.hosts.iter().enumerate() {
                    if !query.is_empty()
                        && !host.alias.to_lowercase().contains(&query)
                        && !host.target().to_lowercase().contains(&query)
                    {
                        continue;
                    }
                    shown += 1;
                    if let Some(a) = host_row(ui, index, host) {
                        action = Some(a);
                    }
                }
                if shown == 0 {
                    ui.add_space(20.0);
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new("No matches").color(theme::MUTED));
                    });
                }
            });
        action
    }

    fn show_toast(&mut self, ctx: &egui::Context) {
        let Some((message, at)) = &self.toast else {
            return;
        };
        if at.elapsed() > Duration::from_millis(3200) {
            self.toast = None;
            return;
        }
        egui::Area::new(egui::Id::unique("toast"))
            .anchor(egui::Align2::CENTER_BOTTOM, [0.0, -18.0])
            .order(egui::Order::Tooltip)
            .interactable(false)
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(theme::CARD_HOVER)
                    .stroke(Stroke::new(1.0, theme::LINE))
                    .corner_radius(8)
                    .inner_margin(egui::Margin::symmetric(14, 8))
                    .show(ui, |ui| ui.label(message.as_str()));
            });
    }

    fn schedule(&self, ctx: &egui::Context) {
        let mut wait = Duration::from_secs(3600);
        if self.job.is_some() {
            wait = wait.min(Duration::from_millis(100));
        }
        if let Some((_, at)) = &self.toast {
            wait = wait.min(Duration::from_millis(3200).saturating_sub(at.elapsed()));
        }
        ctx.request_repaint_after(wait);
    }

    fn frame(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        self.poll_job();
        let mut action = self.shortcuts(&ctx);

        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(theme::BG).inner_margin(16))
            .show(ui, |ui| {
                if let Some(a) = self.header(ui) {
                    action = Some(a);
                }
                let tab_action = match self.tab {
                    Tab::Keys => self.list_keys(ui),
                    Tab::Hosts => self.list_hosts(ui),
                };
                if let Some(a) = tab_action {
                    action = Some(a);
                }
            });

        if let Some(action) = action {
            self.run(action);
        }
        self.show_dialog(&ctx);
        if self.job.is_some() {
            busy_overlay(
                &ctx,
                self.job
                    .as_ref()
                    .map(|(l, _)| l.as_str())
                    .unwrap_or_default(),
            );
        }
        self.show_toast(&ctx);
        self.schedule(&ctx);
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        self.frame(ui);
    }
}

fn key_row(ui: &mut Ui, index: usize, key: &KeyInfo) -> Option<Action> {
    let mut action = None;
    let id = egui::Id::unique(("key", &key.name));
    let hovered = ui.ctx().read_response(id).is_some_and(|r| r.hovered());
    let response = ui
        .scope_builder(
            egui::UiBuilder::new().scope_id(id).sense(Sense::click()),
            |ui| {
                theme::card()
                    .fill(if hovered {
                        theme::CARD_HOVER
                    } else {
                        theme::CARD
                    })
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                ui.add(
                                    egui::Label::new(RichText::new(&key.name).size(15.5).strong())
                                        .truncate()
                                        .selectable(false),
                                );
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(format!("{}  {}", key.kind, key.comment))
                                            .size(12.5)
                                            .color(theme::MUTED),
                                    )
                                    .truncate()
                                    .selectable(false),
                                );
                                if !key.fingerprint.is_empty() {
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(&key.fingerprint)
                                                .monospace()
                                                .size(11.5)
                                                .color(theme::MUTED),
                                        )
                                        .truncate()
                                        .selectable(false),
                                    );
                                }
                                let line = match (&key.problem, &key.note) {
                                    (Some(problem), _) => Some((problem.as_str(), theme::DANGER)),
                                    (None, Some(note)) if key.permissions_too_open() => {
                                        Some((note.as_str(), theme::DANGER))
                                    }
                                    (None, Some(note)) => Some((note.as_str(), theme::WARN)),
                                    _ => None,
                                };
                                if let Some((text, color)) = line {
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(text).size(11.5).color(color),
                                        )
                                        .truncate()
                                        .selectable(false),
                                    );
                                }
                            });
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                ui.menu_button(RichText::new("...").strong(), |ui| {
                                    ui.set_min_width(170.0);
                                    if let Some(a) = key_menu(ui, index, key) {
                                        action = Some(a);
                                    }
                                });
                                if key.encrypted {
                                    ui.label(
                                        RichText::new("passphrase").small().color(theme::ACCENT),
                                    );
                                }
                            });
                        });
                    });
            },
        )
        .response;
    let response = response.on_hover_cursor(CursorIcon::PointingHand);
    if response.clicked() && action.is_none() {
        action = Some(Action::CopyKey(index));
    }
    response.context_menu(|ui| {
        ui.set_min_width(170.0);
        if let Some(a) = key_menu(ui, index, key) {
            action = Some(a);
        }
    });
    action
}

fn key_menu(ui: &mut Ui, index: usize, key: &KeyInfo) -> Option<Action> {
    let mut action = None;
    if ui
        .add_enabled(
            key.public_text.is_some(),
            egui::Button::new("Copy public key"),
        )
        .clicked()
    {
        action = Some(Action::CopyKey(index));
    }
    if key.permissions_too_open() && ui.button("Fix permissions").clicked() {
        action = Some(Action::FixPerms(index));
    }
    ui.separator();
    if ui
        .button(RichText::new("Delete").color(theme::DANGER))
        .clicked()
    {
        action = Some(Action::DeleteKey(index));
    }
    if action.is_some() {
        ui.close();
    }
    action
}

fn host_row(ui: &mut Ui, index: usize, host: &HostEntry) -> Option<Action> {
    let mut action = None;
    let id = egui::Id::unique(("host", &host.alias));
    let hovered = ui.ctx().read_response(id).is_some_and(|r| r.hovered());
    let response = ui
        .scope_builder(
            egui::UiBuilder::new().scope_id(id).sense(Sense::click()),
            |ui| {
                theme::card()
                    .fill(if hovered {
                        theme::CARD_HOVER
                    } else {
                        theme::CARD
                    })
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(&host.alias).size(15.5).strong(),
                                    )
                                    .truncate()
                                    .selectable(false),
                                );
                                let mut line = host.target();
                                if !host.port.is_empty() {
                                    line.push_str(&format!(" : {}", host.port));
                                }
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(line).size(12.5).color(theme::MUTED),
                                    )
                                    .truncate()
                                    .selectable(false),
                                );
                                if !host.identity.is_empty() {
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(&host.identity)
                                                .monospace()
                                                .size(11.5)
                                                .color(theme::MUTED),
                                        )
                                        .truncate()
                                        .selectable(false),
                                    );
                                }
                            });
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                ui.menu_button(RichText::new("...").strong(), |ui| {
                                    ui.set_min_width(170.0);
                                    if let Some(a) = host_menu(ui, index) {
                                        action = Some(a);
                                    }
                                });
                            });
                        });
                    });
            },
        )
        .response;
    let response = response.on_hover_cursor(CursorIcon::PointingHand);
    if response.clicked() && action.is_none() {
        action = Some(Action::CopyHost(index));
    }
    response.context_menu(|ui| {
        ui.set_min_width(170.0);
        if let Some(a) = host_menu(ui, index) {
            action = Some(a);
        }
    });
    action
}

fn host_menu(ui: &mut Ui, index: usize) -> Option<Action> {
    let mut action = None;
    if ui.button("Copy ssh command").clicked() {
        action = Some(Action::CopyHost(index));
    }
    ui.separator();
    if ui
        .button(RichText::new("Delete host").color(theme::DANGER))
        .clicked()
    {
        action = Some(Action::DeleteHost(index));
    }
    if action.is_some() {
        ui.close();
    }
    action
}

fn busy_overlay(ctx: &egui::Context, label: &str) {
    egui::Modal::new(egui::Id::unique("busy"))
        .frame(theme::dialog())
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(label);
            });
        });
}

pub fn column(ui: &mut Ui, width: f32, add: impl FnOnce(&mut Ui)) {
    let width = width.min(ui.available_width());
    let pad = ((ui.available_width() - width) / 2.0).max(0.0);
    let top = (ui.available_height() * 0.16).clamp(12.0, 120.0);
    ui.add_space(top);
    ui.horizontal(|ui| {
        ui.add_space(pad);
        ui.vertical(|ui| {
            ui.set_width(width);
            add(ui);
        });
    });
}

fn open_folder(path: &std::path::Path) {
    #[cfg(windows)]
    let _ = std::process::Command::new("explorer").arg(path).spawn();
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(path).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let _ = std::process::Command::new("xdg-open").arg(path).spawn();
}
