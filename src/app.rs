// SPDX-FileCopyrightText: 2026 Mattia Egloff <mattia.egloff@pm.me>
// SPDX-License-Identifier: GPL-3.0-or-later

//! Application entry point and GTK4 setup.

use gtk4::glib;
use gtk4::prelude::*;
use gtk4::{self, Box as GtkBox, Orientation, gio};
use libadwaita as adw;
use std::cell::RefCell;
use std::rc::Rc;

use vauchi_app::i18n;
use vauchi_app::theme::DesignTokens;
use vauchi_app::ui::AppEngine;
use vauchi_core::{Event, StandardShortcut};

use crate::core_ui::contextual_surface::{
    build_split_view, dispatch_platform_event, render_current_surface,
};
use crate::core_ui::engine_link::EngineHandle;
use crate::locale::detect_locale;
use crate::platform;

const APP_ID: &str = "com.vauchi.desktop";

/// Flag consumed before GTK's argument parser runs.
static RESET_FOR_TESTING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

type DeepLinkOpener = Box<dyn Fn(String)>;

thread_local! {
    /// Hands an opened `vauchi://` link to the engine of the window
    /// `build_ui` made; `None` until that window exists.
    static OPEN_DEEP_LINK: RefCell<Option<DeepLinkOpener>> = const { RefCell::new(None) };
}

pub fn run() {
    if let Some(resource_dir) = std::env::var_os("VAUCHI_LOCALES_DIR") {
        let _ = i18n::init(std::path::Path::new(&resource_dir));
    }
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
    #[cfg(all(feature = "ble", target_os = "linux"))]
    // Drop the hardware shell (its runtime, its BlueZ sessions) before the
    // process exits; a thread-local dropped after Tokio's own aborts.
    app.connect_shutdown(|_| crate::core_ui::hardware_link::uninstall());
    let weak_app = app.downgrade();
    platform::notifications::install_open_action(&app, move |uri| {
        if let Some(app) = weak_app.upgrade() {
            present_for_deep_link(&app);
            forward_deep_link(uri);
        }
    });
    app.run_with_args(&args.iter().map(String::as_str).collect::<Vec<_>>());
}

/// Creates a test identity so the app skips onboarding and lands on home.
///
/// Triggered by `--reset-for-testing` CLI flag (consumed before GTK init)
/// or `VAUCHI_TEST_SEED=1` env var (used by CI conftest to avoid GTK
/// argument parser rejecting the unknown flag).
fn maybe_seed_test_identity(vauchi: &mut vauchi_core::api::Vauchi) {
    let cli_flag = RESET_FOR_TESTING.load(std::sync::atomic::Ordering::Relaxed);
    let env_flag = std::env::var("VAUCHI_TEST_SEED").as_deref() == Ok("1");
    if !cli_flag && !env_flag {
        return;
    }
    if vauchi.has_identity() {
        eprintln!("[vauchi] --reset-for-testing: identity already exists, skipping");
        return;
    }
    match vauchi.create_identity("Test User") {
        Ok(()) => eprintln!("[vauchi] --reset-for-testing: test identity created"),
        Err(e) => eprintln!("[vauchi] --reset-for-testing: failed to create identity: {e}"),
    }
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
    let mut vauchi = platform::init::init_vauchi().expect("Failed to initialize Vauchi backend");
    maybe_seed_test_identity(&mut vauchi);
    let mut app_engine = AppEngine::new(vauchi);
    crate::locale::report_system_locale(&mut app_engine, |name| std::env::var(name).ok());
    // CDN languages, themes and social networks: Core runs the update cycle
    // and announces applied content as a Vauchi event, which the handler
    // below re-renders on (vauchi/private#733 D10).
    if let Some(data_dir) = app_engine
        .vauchi()
        .config()
        .storage_path
        .parent()
        .map(std::path::Path::to_path_buf)
    {
        app_engine.start_content_updates(&data_dir, vauchi_app::ui::ContentRunOptions::default());
    }
    app_engine.bootstrap();
    let app_engine = Rc::new(RefCell::new(app_engine));
    let engine_handle: EngineHandle = app_engine.clone();
    platform::network_cost::watch(&engine_handle);

    // Main layout: header + body
    let root = GtkBox::new(Orientation::Vertical, 0);

    let header = platform::header_bar::build(app);
    root.append(&header);

    // Content area wrapped in ToastOverlay for non-blocking toasts
    let tokens = DesignTokens::default();
    let content = GtkBox::new(Orientation::Vertical, 0);
    content.set_hexpand(true);
    content.set_margin_top(tokens.spacing.xl as i32);
    content.set_margin_bottom(tokens.spacing.xl as i32);
    content.set_margin_start(tokens.spacing.xl as i32);
    content.set_margin_end(tokens.spacing.xl as i32);

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
        .title(i18n::get_string(detect_locale(), "app.name"))
        .default_width(700)
        .default_height(600)
        .content(&root)
        .build();

    // After the window exists, never before: the sidebar and the
    // environment report both look up the window from `content`, and a
    // render with no root drops Core's first `SetNavigation` — the sidebar
    // then stays empty while the split view, still expanded, suppresses
    // the navigation overlay too.
    register_event_handler(&app_engine, &content, &toast_overlay);

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
    app_engine: &Rc<RefCell<AppEngine>>,
    content: &GtkBox,
    toast_overlay: &adw::ToastOverlay,
) {
    let rx = app_engine.borrow().subscribe_presentation_invalidations();

    let app_engine: EngineHandle = app_engine.clone();
    let content = content.clone();
    let toast_overlay = toast_overlay.clone();
    let repaint = move || {
        if rx.try_recv().is_ok() {
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

/// Start linux-hw's hardware shell, report its capabilities to Core and
/// route the hardware commands it ports to it (ADR-078 step 3a). Without
/// any hardware feature, or when the shell cannot start, the renderer keeps
/// every command and Core learns of no capability.
#[cfg(any(
    feature = "ble",
    feature = "nfc",
    feature = "audio",
    feature = "camera"
))]
fn install_hardware(engine: &EngineHandle, content: &GtkBox, toast_overlay: &adw::ToastOverlay) {
    let hardware = match vauchi_hardware::LinuxHardware::start() {
        Ok(hardware) => hardware,
        Err(error) => {
            eprintln!("[hardware] linux-hw did not start: {error}");
            return;
        }
    };
    let link = platform::linux_hardware_link::LinuxHardwareLink::new(hardware);
    crate::core_ui::contextual_surface::dispatch_platform_event(
        content,
        engine,
        toast_overlay,
        link.report(),
    );
    crate::core_ui::hardware_link::install(Rc::new(link), content, engine, toast_overlay);
}

#[cfg(not(any(
    feature = "ble",
    feature = "nfc",
    feature = "audio",
    feature = "camera"
)))]
fn install_hardware(_: &EngineHandle, _: &GtkBox, _: &adw::ToastOverlay) {}
