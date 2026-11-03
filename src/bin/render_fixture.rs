// SPDX-FileCopyrightText: 2026 Mattia Egloff <mattia.egloff@pm.me>
// SPDX-License-Identifier: GPL-3.0-or-later

//! Offscreen catalog harness: render a generic `SurfaceSpec` JSON fixture
//! through the production renderer and save it as a PNG.
//!
//! Built for the design screenshot catalog
//! (`_private/docs/problems/2026-06-12-device-screenshot-catalog/`); it is a
//! capture tool, not part of the shipping app. Reusing the real renderer (not
//! a reimplementation) is the whole point — the PNG shows exactly what
//! `gvauchi` would paint for that screen.
//!
//! Usage: render-fixture <fixture.json> <out.png> [width] [height]
//!
//! Passing `--keep-open` in place of `<out.png>` presents the window through
//! the same renderer but skips capture and does not quit, so an external
//! accessibility reader (pyatspi) can walk the live AT-SPI tree — used to
//! assert core-driven a11y labels reach the rendered widgets.

use std::cell::Cell;
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{Box as GtkBox, Orientation, glib};
use libadwaita as adw;
use libadwaita::prelude::*;

use vauchi_contract::SurfaceSpec;
use vauchi_gtk::capture::{FRAMES_BEFORE_CAPTURE, MAX_FRAMES_BEFORE_CAPTURE, widget_to_png};
use vauchi_gtk::core_ui::generic_surface::{OnEvent, render};

/// The fixture harness has no engine: strings come from the locale store
/// at the detected locale, as the app's link would answer.
struct FixtureLink;

impl vauchi_gtk::core_ui::engine_link::EngineLink for FixtureLink {
    fn dispatch(&self, _: vauchi_contract::Event) -> Vec<vauchi_contract::Command> {
        Vec::new()
    }
    fn initial_commands(&self) -> Vec<vauchi_contract::Command> {
        Vec::new()
    }
    fn string(&self, key: &str) -> String {
        vauchi_app::i18n::get_string(vauchi_gtk::locale::detect_locale(), key)
    }
    fn string_with_args(&self, key: &str, args: &[(&str, &str)]) -> String {
        vauchi_app::i18n::get_string_with_args(vauchi_gtk::locale::detect_locale(), key, args)
    }

    fn take_invalidation(&self) -> bool {
        false
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let fixture_path = args.next().unwrap_or_else(|| usage());
    let out_path = args.next().unwrap_or_else(|| usage());
    let width: i32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(440);
    let height: i32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(1280);
    let keep_open = out_path == "--keep-open";

    let json = std::fs::read_to_string(&fixture_path)
        .unwrap_or_else(|e| panic!("read fixture {fixture_path}: {e}"));
    let surface: SurfaceSpec =
        serde_json::from_str(&json).unwrap_or_else(|e| panic!("decode {fixture_path}: {e}"));

    // NON_UNIQUE: parallel test runs share one session bus; a unique
    // GApplication would hand `activate` to the first instance and exit 0
    // without rendering anything.
    let app = adw::Application::builder()
        .application_id("app.vauchi.fixture-capture")
        .flags(gtk4::gio::ApplicationFlags::NON_UNIQUE)
        .build();

    app.connect_activate(move |app| {
        let window = adw::ApplicationWindow::builder()
            .application(app)
            .default_width(width)
            .default_height(height)
            .build();

        let container = GtkBox::new(Orientation::Vertical, 0);
        let on_event: OnEvent = Rc::new(|_event| {});
        let link: vauchi_gtk::core_ui::engine_link::EngineHandle = Rc::new(FixtureLink);
        render(&container, &surface, &on_event, &link);
        window.set_content(Some(&container));
        window.present();

        if keep_open {
            eprintln!("[render-fixture] keep-open: window presented for AT-SPI inspection");
            return;
        }

        let frames = Rc::new(Cell::new(0u32));
        let out_path = out_path.clone();
        let app = app.clone();
        container.add_tick_callback(move |widget, _clock| {
            frames.set(frames.get() + 1);
            if frames.get() < FRAMES_BEFORE_CAPTURE {
                return glib::ControlFlow::Continue;
            }

            if !widget_to_png(widget, std::path::Path::new(&out_path)) {
                assert!(
                    frames.get() < MAX_FRAMES_BEFORE_CAPTURE,
                    "{out_path}: no render node after {MAX_FRAMES_BEFORE_CAPTURE} frames"
                );
                return glib::ControlFlow::Continue;
            }
            app.quit();
            glib::ControlFlow::Break
        });
    });

    let no_args: [String; 0] = [];
    app.run_with_args(&no_args);
}

fn usage() -> ! {
    eprintln!("usage: render-fixture <fixture.json> <out.png> [width] [height]");
    std::process::exit(2);
}
