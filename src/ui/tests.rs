use std::time::{Duration, Instant};

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use meskd_keys::keys::{self, KeyKind};
use zeroize::Zeroizing;

use super::App;
use super::dialogs::Dialog;

fn harness(home: std::path::PathBuf) -> Harness<'static, App> {
    let app = App::new(home);
    let mut h = Harness::builder()
        .with_size(egui::vec2(620.0, 700.0))
        .build_ui_state(|ui, app: &mut App| app.frame(ui), app);
    h.run_steps(2);
    h
}

fn settle(h: &mut Harness<'static, App>) {
    let start = Instant::now();
    while h.state().job.is_some() && start.elapsed() < Duration::from_secs(30) {
        h.step();
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(h.state().job.is_none(), "background work did not finish");
    h.run_steps(3);
}

#[test]
fn lists_the_keys_in_the_folder() {
    let dir = tempfile::tempdir().unwrap();
    keys::generate(dir.path(), "id_laptop", KeyKind::Ed25519, "", "me@laptop").unwrap();
    let h = harness(dir.path().to_path_buf());
    h.get_by_label("id_laptop");
    h.get_by_label_contains("Ed25519");
    h.get_by_label_contains("SHA256:");
    h.get_by_label_contains("me@laptop");
}

#[test]
fn creates_a_key_through_the_dialog() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = harness(dir.path().to_path_buf());
    h.get_by_label("New key").click();
    h.run_steps(2);
    let Some(Dialog::Generate(form)) = &mut h.state_mut().dialog else {
        panic!("the new key dialog did not open");
    };
    form.name = "id_work".to_owned();
    form.passphrase = Zeroizing::new("a long passphrase".to_owned());
    form.confirm = Zeroizing::new("a long passphrase".to_owned());
    h.run_steps(1);
    h.key_press(egui::Key::Enter);
    h.run_steps(1);
    settle(&mut h);
    assert!(h.state().dialog.is_none());
    assert!(dir.path().join("id_work").exists());
    assert!(dir.path().join("id_work.pub").exists());
    h.get_by_label("id_work");
    let text = std::fs::read_to_string(dir.path().join("id_work")).unwrap();
    assert!(text.contains("ENCRYPTED"));
}

#[test]
fn a_mismatched_passphrase_keeps_the_dialog_open() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = harness(dir.path().to_path_buf());
    h.get_by_label("New key").click();
    h.run_steps(2);
    let Some(Dialog::Generate(form)) = &mut h.state_mut().dialog else {
        unreachable!()
    };
    form.passphrase = Zeroizing::new("one".to_owned());
    form.confirm = Zeroizing::new("two".to_owned());
    h.run_steps(1);
    h.key_press(egui::Key::Enter);
    h.run_steps(2);
    h.get_by_label("The passphrases do not match");
    assert!(matches!(h.state().dialog, Some(Dialog::Generate(_))));
    assert!(h.state().keys.is_empty());
}

#[test]
fn deleting_a_key_asks_first() {
    let dir = tempfile::tempdir().unwrap();
    keys::generate(dir.path(), "id_old", KeyKind::Ed25519, "", "").unwrap();
    let mut h = harness(dir.path().to_path_buf());
    h.get_by_label("id_old");
    let key = h.state().keys[0].clone();
    h.state_mut().dialog = Some(Dialog::delete_key(0, &key));
    h.run_steps(2);
    h.get_by_label("Delete id_old?");
    h.get_by_label("Cancel").click();
    h.run_steps(2);
    assert!(dir.path().join("id_old").exists());
    h.state_mut().dialog = Some(Dialog::delete_key(0, &key));
    h.run_steps(2);
    h.get_by_label("Delete").click();
    h.run_steps(2);
    assert!(!dir.path().join("id_old").exists());
    assert!(!dir.path().join("id_old.pub").exists());
    h.get_by_label("No SSH keys yet");
}

#[test]
fn adds_and_removes_a_host() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = harness(dir.path().to_path_buf());
    h.get_by_label("Hosts").click();
    h.run_steps(2);
    h.get_by_label("No hosts yet");
    h.get_all_by_label("Add host").last().unwrap().click();
    h.run_steps(2);
    let Some(Dialog::AddHost(form)) = &mut h.state_mut().dialog else {
        panic!("the add host dialog did not open");
    };
    form.alias = "prod".to_owned();
    form.hostname = "prod.example.com".to_owned();
    form.user = "deploy".to_owned();
    form.port = "2222".to_owned();
    h.run_steps(1);
    h.key_press(egui::Key::Enter);
    h.run_steps(2);
    let config = std::fs::read_to_string(dir.path().join("config")).unwrap();
    assert!(config.contains("Host prod"));
    assert!(config.contains("HostName prod.example.com"));
    assert!(config.contains("User deploy"));
    assert!(config.contains("Port 2222"));
    h.get_by_label("prod");

    let host = h.state().hosts[0].clone();
    h.state_mut().dialog = Some(Dialog::delete_host(&host));
    h.run_steps(2);
    h.get_by_label("Delete host");
    h.get_by_label("Delete").click();
    h.run_steps(2);
    let config = std::fs::read_to_string(dir.path().join("config")).unwrap();
    assert!(config.is_empty());
    assert!(h.state().hosts.is_empty());
}

#[test]
fn search_filters_both_lists() {
    let dir = tempfile::tempdir().unwrap();
    keys::generate(dir.path(), "id_one", KeyKind::Ed25519, "", "").unwrap();
    keys::generate(dir.path(), "id_two", KeyKind::Ed25519, "", "").unwrap();
    let mut h = harness(dir.path().to_path_buf());
    h.state_mut().search = "two".to_owned();
    h.run_steps(2);
    h.get_by_label("id_two");
    assert!(h.query_by_label("id_one").is_none());
    h.state_mut().search = "nothing".to_owned();
    h.run_steps(2);
    h.get_by_label("No matches");
}

#[test]
fn unsupported_private_keys_are_shown_with_a_reason() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("id_rsa_old"),
        "-----BEGIN RSA PRIVATE KEY-----\nAAAA\n-----END RSA PRIVATE KEY-----\n",
    )
    .unwrap();
    let h = harness(dir.path().to_path_buf());
    h.get_by_label("id_rsa_old");
    h.get_by_label("not in OpenSSH format");
}

#[test]
fn refresh_finds_keys_added_outside_the_app() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = harness(dir.path().to_path_buf());
    h.get_by_label("No SSH keys yet");
    keys::generate(dir.path(), "id_later", KeyKind::Ed25519, "", "").unwrap();
    h.state_mut().refresh();
    h.run_steps(2);
    h.get_by_label("id_later");
}
