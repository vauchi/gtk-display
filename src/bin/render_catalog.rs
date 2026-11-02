// SPDX-FileCopyrightText: 2026 Mattia Egloff <mattia.egloff@pm.me>
// SPDX-License-Identifier: GPL-3.0-or-later

//! Offscreen screen-catalog harness: replay every Core command batch in a
//! catalog through the production command path and save one PNG per
//! screen and variant.
//!
//! Shells are humble renderers of Core `Command` batches (ADR-066), so a
//! screen is fully described by its batch; no navigation is driven here.
//! Each batch goes through `replay_command_batch` — the same
//! `handle_commands` path `gvauchi` uses — inside the app's real window
//! chrome (header bar, sidebar split view, toast overlay), so the PNG shows
//! exactly what the desktop app would paint.
//!
//! Usage: render-catalog <catalog.json> <out-dir> [width] [height]
//!
//! Writes `<out-dir>/<code_id>.png` (default theme), `<code_id>.light.png`
//! (first light theme) and `<code_id>.large.png` (150% text scale).
//!
//! The light theme comes from `VAUCHI_THEMES_JSON` (a generated
//! `themes.json`) when set, else from the compiled-in catalog. Core built
//! from a cargo git checkout has no sibling `themes/` repo, so its
//! compiled-in catalog holds only the default dark theme; rendering the
//! light variant with it would silently duplicate the default PNGs, so a
//! missing light theme aborts the run instead.
//! Accepts the screen catalog (`screens[]`) or, as a fallback, the
//! presentation contract fixture (`initial_commands` + `steps[]`).

use vauchi_gtk::core_ui::engine_link::EngineHandle;

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{Box as GtkBox, Orientation, glib};
use libadwaita as adw;

use vauchi_app::theme::{Theme, ThemeMode, bundled_themes, default_theme, load_themes_from_json};
use vauchi_app::ui::AppEngine;
use vauchi_core::api::Vauchi;
use vauchi_core::{Command, PresentationColors};
use vauchi_gtk::capture::{FRAMES_BEFORE_CAPTURE, MAX_FRAMES_BEFORE_CAPTURE, widget_to_png};
use vauchi_gtk::core_ui::contextual_surface::{build_split_view, replay_command_batch};
use vauchi_gtk::screen_catalog::{CatalogScreen, load_screens};

const BASE_DPI: f64 = 96.0;
const LARGE_TEXT_SCALE: f64 = 1.5;
// `gtk-xft-dpi` is Pango-style: dots per inch times 1024.
const XFT_DPI_UNIT: f64 = 1024.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Variant {
    Default,
    Light,
    LargeText,
}

impl Variant {
    const ALL: [Variant; 3] = [Variant::Default, Variant::Light, Variant::LargeText];

    fn file_name(self, code_id: &str) -> String {
        match self {
            Variant::Default => format!("{code_id}.png"),
            Variant::Light => format!("{code_id}.light.png"),
            Variant::LargeText => format!("{code_id}.large.png"),
        }
    }

    fn theme(self, themes: &Themes) -> &Theme {
        match self {
            Variant::Light => &themes.light,
            Variant::Default | Variant::LargeText => &themes.default,
        }
    }

    /// The catalog's surfaces carry the colours Core recorded them with, the
    /// default theme's. The light variant repaints them in the light theme,
    /// mapped as Core maps a theme but without its contrast correction, so
    /// a light PNG is close to, not exactly, what Core would send.
    fn repaint(self, themes: &Themes, commands: Vec<Command>) -> Vec<Command> {
        if self != Variant::Light {
            return commands;
        }
        let colors = presentation_colors(&themes.light);
        commands
            .into_iter()
            .map(|command| match command {
                Command::ReplaceSurface { mut surface } => {
                    surface.tokens.colors = Box::new(colors.clone());
                    Command::ReplaceSurface { surface }
                }
                other => other,
            })
            .collect()
    }

    fn text_scale(self) -> f64 {
        match self {
            Variant::LargeText => LARGE_TEXT_SCALE,
            Variant::Default | Variant::Light => 1.0,
        }
    }

    fn apply(self, themes: &Themes) {
        let theme = self.theme(themes);
        let scheme = match theme.mode {
            ThemeMode::Light => adw::ColorScheme::ForceLight,
            _ => adw::ColorScheme::ForceDark,
        };
        adw::StyleManager::default().set_color_scheme(scheme);
        if let Some(settings) = gtk4::Settings::default() {
            settings.set_gtk_xft_dpi((BASE_DPI * self.text_scale() * XFT_DPI_UNIT) as i32);
        }
    }
}

fn presentation_colors(theme: &Theme) -> PresentationColors {
    let colors = &theme.colors;
    PresentationColors {
        background: colors.bg_primary.clone(),
        surface: colors.bg_secondary.clone(),
        text: colors.text_primary.clone(),
        text_muted: colors.text_secondary.clone(),
        accent: colors.accent.clone(),
        border: colors.border.clone(),
        success: colors.success.clone(),
        warning: colors.warning.clone(),
        error: colors.error.clone(),
        surface_input: colors.surface_input.clone(),
        surface_muted: colors.surface_muted.clone(),
        surface_elevated: colors.surface_elevated.clone(),
        text_on_accent: colors.text_on_accent.clone(),
        focus_ring: colors.focus_ring.clone(),
    }
}

struct Job {
    screen: Rc<CatalogScreen>,
    variant: Variant,
    out_path: PathBuf,
}

struct Themes {
    default: Theme,
    light: Theme,
}

const THEMES_JSON_ENV: &str = "VAUCHI_THEMES_JSON";

fn resolve_themes() -> Themes {
    let (catalog, origin) = match std::env::var_os(THEMES_JSON_ENV) {
        Some(path) => {
            let path = PathBuf::from(path);
            let bytes = std::fs::read(&path)
                .unwrap_or_else(|e| panic!("{THEMES_JSON_ENV}={}: {e}", path.display()));
            let catalog = load_themes_from_json(&bytes)
                .unwrap_or_else(|e| panic!("{THEMES_JSON_ENV}={}: {e}", path.display()));
            (catalog, format!("{THEMES_JSON_ENV}={}", path.display()))
        }
        None => (bundled_themes(), "compiled-in catalog".to_string()),
    };
    let ids: Vec<&str> = catalog.iter().map(|theme| theme.id.as_str()).collect();
    let light = catalog
        .iter()
        .find(|theme| theme.mode == ThemeMode::Light)
        .cloned()
        .unwrap_or_else(|| {
            panic!(
                "no light theme in {origin} (themes: {ids:?}); set {THEMES_JSON_ENV} to a generated themes.json"
            )
        });
    eprintln!(
        "[render-catalog] themes from {origin}: default={} light={}",
        default_theme().id,
        light.id
    );
    Themes {
        default: default_theme(),
        light,
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let catalog_path = args.next().unwrap_or_else(|| usage());
    let out_dir = PathBuf::from(args.next().unwrap_or_else(|| usage()));
    let width: i32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(900);
    let height: i32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(1400);

    let json = std::fs::read_to_string(&catalog_path)
        .unwrap_or_else(|e| panic!("read catalog {catalog_path}: {e}"));
    let loaded = load_screens(&json).unwrap_or_else(|e| panic!("{catalog_path}: {e}"));
    std::fs::create_dir_all(&out_dir)
        .unwrap_or_else(|e| panic!("create {}: {e}", out_dir.display()));
    eprintln!(
        "[render-catalog] source: {} — {} screen(s) x {} variant(s)",
        loaded.source.describe(),
        loaded.screens.len(),
        Variant::ALL.len()
    );
    let jobs = plan_jobs(loaded.screens, &out_dir);
    let themes = Rc::new(resolve_themes());

    // NON_UNIQUE: parallel test runs share one session bus; a unique
    // GApplication would hand `activate` to the first instance and exit 0
    // without rendering anything.
    let app = adw::Application::builder()
        .application_id("app.vauchi.catalog-capture")
        .flags(gtk4::gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.connect_activate(move |app| {
        // Every surface reloads Core's colours, so a capture a few frames
        // after a colour change would catch Adwaita's CSS transitions halfway
        // and differ with whatever the previous screen painted.
        if let Some(settings) = gtk4::Settings::default() {
            settings.set_gtk_enable_animations(false);
        }
        let catalog_window = Rc::new(build_app_window(app, width, height));
        catalog_window.window.present();
        drive_jobs(app, catalog_window, jobs.clone(), themes.clone());
    });

    let no_args: [String; 0] = [];
    app.run_with_args(&no_args);
}

fn plan_jobs(screens: Vec<CatalogScreen>, out_dir: &Path) -> VecDeque<Rc<Job>> {
    let screens: Vec<Rc<CatalogScreen>> = screens.into_iter().map(Rc::new).collect();
    let mut jobs = VecDeque::new();
    // Variants outermost: theme and text-scale switches are process-global,
    // so switching once per variant keeps every screen in a run consistent.
    for variant in Variant::ALL {
        for screen in &screens {
            jobs.push_back(Rc::new(Job {
                screen: screen.clone(),
                variant,
                out_path: out_dir.join(variant.file_name(&screen.code_id)),
            }));
        }
    }
    jobs
}

struct CatalogWindow {
    window: adw::ApplicationWindow,
    /// Everything inside the window frame — the capture target.
    root: GtkBox,
    /// The box Core's surfaces and context bar are written into.
    replay_target: GtkBox,
    split_view: adw::OverlaySplitView,
    /// Replaced before every job (`fresh_toast_overlay`), never reused.
    toast_overlay: RefCell<adw::ToastOverlay>,
    app_engine: EngineHandle,
}

impl CatalogWindow {
    /// Swap in an empty toast overlay as the split view's content pane.
    ///
    /// A toast one screen raises (BLE unavailable on the runner, say) must
    /// not linger into the next screen's capture. libadwaita 1.4 offers no
    /// way to drop an overlay's pending toasts (`dismiss_all` is 1.7), so
    /// the overlay itself is replaced. The sidebar built by
    /// `build_split_view` keeps the first overlay for its row-activation
    /// dispatch, which the harness never triggers.
    fn fresh_toast_overlay(&self) -> adw::ToastOverlay {
        let fresh = adw::ToastOverlay::new();
        fresh.set_hexpand(true);
        let stale = self.toast_overlay.replace(fresh.clone());
        stale.set_child(gtk4::Widget::NONE);
        fresh.set_child(Some(&self.replay_target));
        self.split_view.set_content(Some(&fresh));
        fresh
    }
}

/// The same chrome `app::build_ui` assembles: header bar over a sidebar
/// split view whose content pane is the toast-wrapped command target.
fn build_app_window(app: &adw::Application, width: i32, height: i32) -> CatalogWindow {
    vauchi_gtk::core_ui::fonts::register_app_fonts();
    let app_engine: EngineHandle = Rc::new(RefCell::new(AppEngine::new(
        Vauchi::in_memory().expect("in-memory Core for the catalog harness"),
    )));

    let root = GtkBox::new(Orientation::Vertical, 0);
    root.append(&vauchi_gtk::platform::header_bar::build(app));

    let tokens = vauchi_app::theme::DesignTokens::default();
    let content = GtkBox::new(Orientation::Vertical, 0);
    content.set_hexpand(true);
    content.set_margin_top(tokens.spacing.xl as i32);
    content.set_margin_bottom(tokens.spacing.xl as i32);
    content.set_margin_start(tokens.spacing.xl as i32);
    content.set_margin_end(tokens.spacing.xl as i32);

    let toast_overlay = adw::ToastOverlay::new();
    toast_overlay.set_child(Some(&content));
    toast_overlay.set_hexpand(true);
    let split_view = build_split_view(&content, &app_engine, &toast_overlay);
    root.append(&split_view);

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .default_width(width)
        .default_height(height)
        .content(&root)
        .build();
    CatalogWindow {
        window,
        root,
        replay_target: content,
        split_view,
        toast_overlay: RefCell::new(toast_overlay),
        app_engine,
    }
}

/// Apply one job per `FRAMES_BEFORE_CAPTURE` frames: switch variant, replay
/// the batch, let the window paint, capture, advance. Quits when drained.
fn drive_jobs(
    app: &adw::Application,
    window: Rc<CatalogWindow>,
    mut jobs: VecDeque<Rc<Job>>,
    themes: Rc<Themes>,
) {
    let Some(first) = jobs.pop_front() else {
        eprintln!("[render-catalog] nothing to render");
        app.quit();
        return;
    };
    let settled = Rc::new(Cell::new(false));
    start_job(&first, &themes, &window);
    settle_after_replay(&settled);

    let current = Rc::new(RefCell::new(first));
    let queue = Rc::new(RefCell::new(jobs));
    let frames = Rc::new(Cell::new(0u32));
    let app = app.clone();
    let root = window.root.clone();
    root.add_tick_callback(move |widget, _clock| {
        if !settled.get() {
            return glib::ControlFlow::Continue;
        }
        frames.set(frames.get() + 1);
        if frames.get() < FRAMES_BEFORE_CAPTURE {
            return glib::ControlFlow::Continue;
        }
        if !widget_to_png(widget, &current.borrow().out_path) {
            assert!(
                frames.get() < MAX_FRAMES_BEFORE_CAPTURE,
                "{}: no render node after {MAX_FRAMES_BEFORE_CAPTURE} frames",
                current.borrow().out_path.display()
            );
            return glib::ControlFlow::Continue;
        }

        let Some(next) = queue.borrow_mut().pop_front() else {
            app.quit();
            return glib::ControlFlow::Break;
        };
        start_job(&next, &themes, &window);
        settle_after_replay(&settled);
        *current.borrow_mut() = next;
        frames.set(0);
        glib::ControlFlow::Continue
    });
}

/// Hold the capture countdown until the events a replay queued have reached
/// Core. A replayed platform command (BLE scan, say) answers with an event on
/// a default-priority idle, and Core's reply re-renders the window; counted
/// from the replay, the capture could beat that reply and the next job
/// would show it instead. GLib runs no low-priority idle while a
/// default-priority one is pending, so this one fires after them.
fn settle_after_replay(settled: &Rc<Cell<bool>>) {
    settled.set(false);
    let settled = settled.clone();
    glib::idle_add_local_full(glib::Priority::LOW, move || {
        settled.set(true);
        glib::ControlFlow::Break
    });
}

fn start_job(job: &Job, themes: &Themes, window: &CatalogWindow) {
    eprintln!(
        "[render-catalog] {} ({:?}) -> {}",
        job.screen.code_id,
        job.variant,
        job.out_path.display()
    );
    job.variant.apply(themes);
    let toast_overlay = window.fresh_toast_overlay();
    replay_command_batch(
        &window.replay_target,
        &window.app_engine,
        &toast_overlay,
        job.variant.repaint(themes, job.screen.commands.clone()),
    );
}

fn usage() -> ! {
    eprintln!("usage: render-catalog <catalog.json> <out-dir> [width] [height]");
    std::process::exit(2);
}
