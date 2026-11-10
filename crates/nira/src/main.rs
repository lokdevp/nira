//! Nira, a comic book and manga viewer. Entry point: command line, logging
//! and the GTK application.

mod chrome;
mod texture;
mod theme;
mod window;
mod workers;

use std::path::PathBuf;

use clap::{ArgAction, Parser};
use gtk::prelude::*;
use gtk::{gio, glib};
use nira_core::settings::Settings;
use nira_core::zoom::FitMode;

use crate::window::NiraWindow;

/// View images and comic book archives.
#[derive(Parser, Clone, Debug)]
#[command(
    name = "nira",
    version,
    about = "View images and comic book archives.",
    disable_help_flag = true
)]
struct Args {
    /// Show this help and exit.
    #[arg(long = "help", action = ArgAction::Help)]
    help: Option<bool>,

    /// Start in fullscreen.
    #[arg(short = 'f', long)]
    fullscreen: bool,

    /// Start in manga mode (right to left).
    #[arg(short = 'm', long)]
    manga: bool,

    /// Start in double-page mode.
    #[arg(short = 'd', long)]
    double_page: bool,

    /// Fit mode to start with.
    #[arg(long, value_enum)]
    fit: Option<FitArg>,

    /// Open the book at this page (1-based) instead of where it was left.
    #[arg(short = 'p', long)]
    page: Option<usize>,

    /// Log level: error, warn, info, debug or trace.
    #[arg(short = 'W', long = "log", default_value = "info")]
    log: log::LevelFilter,

    /// A comic archive, PDF or folder of images.
    #[arg(value_name = "PATH")]
    path: Option<PathBuf>,
}

#[derive(clap::ValueEnum, Clone, Copy, Debug)]
enum FitArg {
    Best,
    Width,
    Height,
    Size,
    Manual,
}

impl From<FitArg> for FitMode {
    fn from(f: FitArg) -> FitMode {
        match f {
            FitArg::Best => FitMode::Best,
            FitArg::Width => FitMode::Width,
            FitArg::Height => FitMode::Height,
            FitArg::Size => FitMode::Size,
            FitArg::Manual => FitMode::Manual,
        }
    }
}

fn main() -> glib::ExitCode {
    let args = Args::parse();
    env_logger::Builder::new()
        .filter_level(args.log)
        .format_timestamp(None)
        .init();
    log::info!("Nira {} starting", env!("CARGO_PKG_VERSION"));

    // The book's path reaches the app through clap (file managers launch
    // `nira %f`), so GTK gets no arguments and only `activate` fires.
    let app = gtk::Application::builder()
        .application_id("io.github.lokdevp.Nira")
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.connect_activate(move |app| activate(app, &args));
    app.run_with_args(&[] as &[&str])
}

fn activate(app: &gtk::Application, args: &Args) {
    let settings = Settings::load();
    // Unpacked archives left behind by crashed runs.
    std::thread::spawn(nira_core::paths::remove_stale_unpack_dirs);
    // The least used thumbnails over the cap.
    std::thread::spawn(nira_core::thumb_cache::prune);
    nira_core::i18n::init(&settings.appearance.language);
    let fullscreen = args.fullscreen || settings.view.fullscreen;

    // The icon installed as packaging/linux/icons/*/apps/nira.png, or the
    // embedded copy when it isn't installed.
    theme::ensure_app_icon();
    gtk::Window::set_default_icon_name("nira");
    theme::load_css();
    theme::apply_theme(&settings.appearance.theme);
    let reopen = settings.files.reopen_last;
    let win = NiraWindow::new(app, settings);
    // Command-line options apply to this run only; they are not saved.
    win.apply_overrides(args.manga, args.double_page, args.fit.map(Into::into));
    if fullscreen {
        win.fullscreen();
    }
    if let Some(path) = &args.path {
        win.open(path.clone(), args.page.map(|p| p.saturating_sub(1)));
    } else if reopen {
        win.reopen_last();
    }
    win.present();
}
