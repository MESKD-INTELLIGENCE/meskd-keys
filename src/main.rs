#![cfg_attr(not(test), windows_subsystem = "windows")]
#![forbid(unsafe_code)]

mod ui;

use std::path::PathBuf;
use std::process::ExitCode;

use meskd_keys::keys;

const HELP: &str = "Meskd Keys

Usage: meskd-keys [--ssh-dir <folder>]

  --ssh-dir <folder>   look in this folder instead of ~/.ssh
  --version            print the version
  --help               print this help
";

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let mut home: Option<PathBuf> = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--ssh-dir" => match args.next() {
                Some(path) => home = Some(PathBuf::from(path)),
                None => return usage("--ssh-dir needs a folder"),
            },
            "--version" => {
                println!("meskd-keys {}", env!("CARGO_PKG_VERSION"));
                return ExitCode::SUCCESS;
            }
            "--help" | "-h" => {
                print!("{HELP}");
                return ExitCode::SUCCESS;
            }
            other => return usage(&format!("unknown option {other}")),
        }
    }
    let Some(home) = home.or_else(keys::ssh_dir) else {
        return usage("could not find your home folder");
    };

    let mut last_error = String::new();
    for renderer in [eframe::Renderer::Wgpu, eframe::Renderer::Glow] {
        let options = eframe::NativeOptions {
            renderer,
            viewport: egui::ViewportBuilder::default()
                .with_title("Meskd Keys")
                .with_app_id("meskd-keys")
                .with_inner_size([620.0, 700.0])
                .with_min_inner_size([560.0, 480.0])
                .with_icon(ui::theme::icon()),
            centered: true,
            wgpu_options: wgpu_options(),
            ..Default::default()
        };
        let dir = home.clone();
        let result = eframe::run_native(
            "Meskd Keys",
            options,
            Box::new(move |cc| {
                ui::theme::apply(&cc.egui_ctx);
                Ok(Box::new(ui::App::new(dir)))
            }),
        );
        match result {
            Ok(()) => return ExitCode::SUCCESS,
            Err(e) => last_error = e.to_string(),
        }
    }
    eprintln!("meskd-keys: could not open a window: {last_error}");
    ExitCode::FAILURE
}

fn wgpu_options() -> eframe::egui_wgpu::WgpuConfiguration {
    let mut options = eframe::egui_wgpu::WgpuConfiguration::default();
    if cfg!(windows)
        && eframe::wgpu::Backends::from_env().is_none()
        && let eframe::egui_wgpu::WgpuSetup::CreateNew(setup) = &mut options.wgpu_setup
    {
        setup.instance_descriptor.backends = eframe::wgpu::Backends::DX12;
    }
    options
}

fn usage(message: &str) -> ExitCode {
    eprintln!("meskd-keys: {message}\n\n{HELP}");
    ExitCode::from(2)
}
