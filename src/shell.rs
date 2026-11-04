// SPDX-FileCopyrightText: 2026 Mattia Egloff <mattia.egloff@pm.me>
// SPDX-License-Identifier: GPL-3.0-or-later
//! The GTK application: window, header bar, split view, deep links,
//! shortcuts and the wakeup timers, around the engine and hardware the app
//! hands in. Names no engine type (ADR-078 D-2): the engine is an
//! [`EngineHandle`].
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::{self, Box as GtkBox, Orientation, gio};
use libadwaita as adw;
use std::cell::RefCell;

use vauchi_contract::{Event, StandardShortcut};

use crate::core_ui::contextual_surface::{
    build_split_view, dispatch_platform_event, render_current_surface,
};
use crate::core_ui::engine_link::EngineHandle;
use crate::core_ui::hardware_link::{self, StartHardware};
use crate::platform;

/// Builds the app's engine once the application activates.
pub type StartEngine = Box<dyn FnOnce() -> EngineHandle>;

const APP_ID: &str = "app.vauchi.Vauchi";

/// Flag consumed before GTK's argument parser runs.
static RESET_FOR_TESTING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

type DeepLinkOpener = Box<dyn Fn(String)>;

thread_local! {
    /// Hands an opened `vauchi://` link to the engine of the window
    /// `build_ui` made; `None` until that window exists.
    static OPEN_DEEP_LINK: RefCell<Option<DeepLinkOpener>> = const { RefCell::new(None) };
}

thread_local! {
    static START_ENGINE: RefCell<Option<StartEngine>> = const { RefCell::new(None) };
    static START_HARDWARE: RefCell<Option<StartHardware>> = const { RefCell::new(None) };
}

/// The GTK application around the engine and the hardware the app starts
/// (ADR-078 step 4d): the app (linux-app) owns both, this crate renders and
/// routes commands. `start_engine` runs once, when the application
/// activates.
pub fn run(start_engine: StartEngine, start_hardware: StartHardware) {
    START_ENGINE.with(|slot| *slot.borrow_mut() = Some(start_engine));
    START_HARDWARE.with(|slot| *slot.borrow_mut() = Some(start_hardware));
    crate::core_ui::fonts::register_app_fonts();

    // Consume --reset-for-testing before GTK sees it (GTK rejects unknown flags).
    let args: Vec<String> = std::env::args()
        .filter(|a| {
            if a == "--reset-for-testing" {
                RESET_FOR_TESTING.store(true, std::sync::atomic::Ordering::Relaxed);
                false
            } else {
                true
            }
        })
        .collect();

    platform::screen_capture_protection::enable();

    // HANDLES_OPEN: the desktop entry's `%u` passes a vauchi:// link as an
    // argument, and a running instance receives it through `open`.
    let mut flags = gio::ApplicationFlags::HANDLES_OPEN;
    if std::env::var("VAUCHI_TEST_NON_UNIQUE").as_deref() == Ok("1") {
        flags |= gio::ApplicationFlags::NON_UNIQUE;
    }
    let app = adw::Application::builder()
        .application_id(APP_ID)
        .flags(flags)
        .build();
    app.connect_activate(build_ui);
    app.connect_open(open_deep_links);
    // Drop the hardware shell (its runtime, its BlueZ sessions) before the
    // process exits; a thread-local dropped after Tokio's own aborts.
    app.connect_shutdown(|_| hardware_link::uninstall());
    let weak_app = app.downgrade();
    platform::notifications::install_open_action(&app, move |uri| {
        if let Some(app) = weak_app.upgrade() {
            present_for_deep_link(&app);
            forward_deep_link(uri);
        }
    });
    app.run_with_args(&args.iter().map(String::as_str).collect::<Vec<_>>());
}

/// Whether `--reset-for-testing` was on the command line; `run` strips it
/// before GTK's argument parser sees it.
pub fn reset_for_testing_requested() -> bool {
    RESET_FOR_TESTING.load(std::sync::atomic::Ordering::Relaxed)
}

fn open_deep_links(app: &adw::Application, files: &[gio::File], _hint: &str) {
    let opened: Vec<String> = files.iter().map(|file| file.uri().to_string()).collect();
    present_for_deep_link(app);
    for uri in platform::deep_link::deep_link_uris(&opened) {
        forward_deep_link(uri);
    }
}

fn present_for_deep_link(app: &adw::Application) {
    match app.active_window() {
        Some(window) => window.present(),
        None => app.activate(),
    }
}

fn forward_deep_link(uri: String) {
    OPEN_DEEP_LINK.with(|open| {
        if let Some(open) = open.borrow().as_ref() {
            open(uri);
        }
    });
}

fn build_ui(app: &adw::Application) {
    let Some(start_engine) = START_ENGINE.with(|slot| slot.borrow_mut().take()) else {
        return;
    };
    let engine_handle = start_engine();

    // Main layout: header + body
    let root = GtkBox::new(Orientation::Vertical, 0);

    let header = platform::header_bar::build(app);
    root.append(&header);

    // Content area wrapped in ToastOverlay for non-blocking toasts
    // Its margin comes with each surface (`handle_commands`), from Core's
    // tokens; the renderer keeps none of its own (private#1029).
    let content = GtkBox::new(Orientation::Vertical, 0);
    content.set_hexpand(true);

    let toast_overlay = adw::ToastOverlay::new();
    toast_overlay.set_child(Some(&content));
    toast_overlay.set_hexpand(true);

    // F6/D4: a persistent sidebar for the destinations Core publishes via
    // `SetNavigation`, alongside the toast-wrapped content pane. Core's
    // navigation overlay still opens on request, sidebar shown or not.
    let split_view = build_split_view(&content, &engine_handle, &toast_overlay);

    root.append(&split_view);

    register_wakeup_poll(&engine_handle, &content, &toast_overlay);
    install_hardware(&engine_handle, &content, &toast_overlay);

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .default_width(700)
        .default_height(600)
        .content(&root)
        .build();

    // After the window exists, never before: the sidebar and the
    // environment report both look up the window from `content`, and a
    // render with no root drops Core's first `SetNavigation` — the sidebar
    // then stays empty while the split view, still expanded, suppresses
    // the navigation overlay too.
    register_event_handler(&engine_handle, &content, &toast_overlay);

    {
        let engine_handle = engine_handle.clone();
        let content = content.clone();
        let toast_overlay = toast_overlay.clone();
        OPEN_DEEP_LINK.with(|open| {
            *open.borrow_mut() = Some(Box::new(move |uri| {
                dispatch_platform_event(
                    &content,
                    &engine_handle,
                    &toast_overlay,
                    Event::DeepLinkOpened { uri },
                );
            }));
        });
    }

    // Auto-lock: navigate to lock screen when window loses focus (C1 App Security)
    {
        let engine_handle = engine_handle.clone();
        let content = content.clone();
        let toast_overlay = toast_overlay.clone();
        window.connect_notify_local(Some("is-active"), move |w, _| {
            if !w.is_active() {
                dispatch_platform_event(
                    &content,
                    &engine_handle,
                    &toast_overlay,
                    Event::AppBackgrounded,
                );
            }
        });
    }

    crate::core_ui::contextual_surface::install_environment_reporting(
        &window,
        &content,
        &engine_handle,
        &toast_overlay,
    );

    // Native keys activate only shortcuts declared by Core for this surface.
    let key_ctrl = gtk4::EventControllerKey::new();
    {
        let engine_handle = engine_handle.clone();
        let content = content.clone();
        let toast_overlay = toast_overlay.clone();
        key_ctrl.connect_key_pressed(move |_, key, _, modifier| {
            if key == gtk4::gdk::Key::Escape
                && !modifier.contains(gtk4::gdk::ModifierType::ALT_MASK)
                && crate::core_ui::contextual_surface::request_back(
                    &content,
                    &engine_handle,
                    &toast_overlay,
                )
            {
                return gtk4::glib::Propagation::Stop;
            }

            let control = modifier.contains(gtk4::gdk::ModifierType::CONTROL_MASK);
            let shortcut = if control && key == gtk4::gdk::Key::z {
                Some(StandardShortcut::Undo)
            } else if control && matches!(key, gtk4::gdk::Key::Return | gtk4::gdk::Key::KP_Enter) {
                Some(StandardShortcut::ActivatePrimary)
            } else {
                None
            };
            if let Some(shortcut) = shortcut
                && crate::core_ui::contextual_surface::dispatch_shortcut(
                    &content,
                    &engine_handle,
                    &toast_overlay,
                    shortcut,
                )
            {
                return gtk4::glib::Propagation::Stop;
            }
            gtk4::glib::Propagation::Proceed
        });
    }
    window.add_controller(key_ctrl);

    window.present();
}

/// Bridge Core's coalesced invalidation wakeups to the GTK main loop.
fn register_event_handler(
    app_engine: &EngineHandle,
    content: &GtkBox,
    toast_overlay: &adw::ToastOverlay,
) {
    let app_engine = app_engine.clone();
    let content = content.clone();
    let toast_overlay = toast_overlay.clone();
    let repaint = move || {
        if app_engine.take_invalidation() {
            render_current_surface(&content, &app_engine, &toast_overlay);
            fire_wakeup(&app_engine, &content, &toast_overlay);
        }
    };
    // Consume the initial wakeup before showing controls, otherwise the first
    // timer tick replaces widgets the user may already be activating (#764).
    repaint();
    glib::timeout_add_local(std::time::Duration::from_millis(200), move || {
        repaint();
        glib::ControlFlow::Continue
    });
}

/// The fallback heartbeat covers native timer loss (ADR-044 Am2a).
fn register_wakeup_poll(
    app_engine: &EngineHandle,
    content: &GtkBox,
    toast_overlay: &adw::ToastOverlay,
) {
    let app_engine = app_engine.clone();
    let content = content.clone();
    let toast_overlay = toast_overlay.clone();

    glib::timeout_add_local(std::time::Duration::from_secs(30), move || {
        fire_wakeup(&app_engine, &content, &toast_overlay);
        glib::ControlFlow::Continue
    });
}

fn fire_wakeup(app_engine: &EngineHandle, content: &GtkBox, toast_overlay: &adw::ToastOverlay) {
    dispatch_platform_event(content, app_engine, toast_overlay, Event::WakeupFired);
}

thread_local! {
    /// The fast-path wakeup `arm_wakeup` is currently waiting on, if any.
    /// Single-threaded (the GTK main loop owns every call), so a
    /// thread-local is enough to let a new `ScheduleWakeup` replace a
    /// stale one instead of both firing.
    static PENDING_FAST_WAKEUP: RefCell<Option<glib::SourceId>> = const { RefCell::new(None) };
}

/// Arms a one-shot wakeup timer at `delay_millis`, replacing any pending
/// one (ADR-044 Am2a). The 30-second poll `register_wakeup_poll` installs
/// keeps running underneath as the fallback heartbeat; this is the fast
/// path a live QR exchange's sub-second `ScheduleWakeup` needs to advance
/// on time (#450).
pub(crate) fn arm_wakeup(
    app_engine: &EngineHandle,
    content: &GtkBox,
    toast_overlay: &adw::ToastOverlay,
    delay_millis: u64,
) {
    PENDING_FAST_WAKEUP.with(|pending| {
        if let Some(previous) = pending.borrow_mut().take() {
            previous.remove();
        }
    });
    let app_engine = app_engine.clone();
    let content = content.clone();
    let toast_overlay = toast_overlay.clone();
    let source_id =
        glib::timeout_add_local_once(std::time::Duration::from_millis(delay_millis), move || {
            PENDING_FAST_WAKEUP.with(|pending| {
                pending.borrow_mut().take();
            });
            fire_wakeup(&app_engine, &content, &toast_overlay);
        });
    PENDING_FAST_WAKEUP.with(|pending| {
        *pending.borrow_mut() = Some(source_id);
    });
}

/// Start the hardware the app chose (`run`), report its capabilities
/// to Core once and route the hardware commands it ports to it.
fn install_hardware(engine: &EngineHandle, content: &GtkBox, toast_overlay: &adw::ToastOverlay) {
    let Some(start) = START_HARDWARE.with(|slot| slot.borrow_mut().take()) else {
        return;
    };
    let Some(started) = start() else {
        return;
    };
    if let Some(capabilities) = started.capabilities {
        dispatch_platform_event(content, engine, toast_overlay, capabilities);
    }
    hardware_link::install(started.link, content, engine, toast_overlay);
}
