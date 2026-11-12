// SPDX-FileCopyrightText: 2026 Mattia Egloff <mattia.egloff@pm.me>
// SPDX-License-Identifier: GPL-3.0-or-later

//! Executes display commands from the Core reducer.
//!
//! Handles navigation, alerts, toasts, pickers and the desktop's answers
//! to display-side platform capabilities; Core then decides what the user
//! sees (ADR-066). Hardware commands are app-owned (#1252): they never
//! reach this shell, and no renderer arm handles or emulates them.

mod file_picker;

use gtk4::Box as GtkBox;
use libadwaita as adw;
use libadwaita::prelude::*;
use std::collections::HashSet;

use vauchi_contract::{Command, Event};

use crate::core_ui::engine_link::EngineHandle;

use super::contextual_surface::dispatch_platform_event;

/// Dispatch display commands to platform-specific actions (ADR-031).
///
/// We deduplicate "unavailable" answers per capability to avoid spamming
/// the user when a batch repeats one.
pub(crate) fn handle_exchange_commands(
    container: &GtkBox,
    app_engine: &EngineHandle,
    toast_overlay: &adw::ToastOverlay,
    commands: &[Command],
) {
    // Track which capabilities we've already answered "unavailable" for
    // to avoid spamming when a batch repeats the same one.
    let mut notified_unavailable: HashSet<&str> = HashSet::new();

    for cmd in commands {
        match cmd {
            Command::QrDisplay { .. } => {
                // The paired ReplaceSurface command already carries the
                // updated generic QR node.
            }
            // ── Image picking (avatar editor) ────────────────────────
            Command::ImagePickFromFile => {
                file_picker::open_image_picker(container, app_engine, toast_overlay);
            }
            Command::FilePickFromUser {
                accepted_mime_types,
                ..
            } => {
                file_picker::open_file_picker(
                    container,
                    app_engine,
                    toast_overlay,
                    accepted_mime_types,
                );
            }
            Command::ExportFile { file } => {
                file_picker::open_export(container, app_engine, toast_overlay, file.clone());
            }
            Command::ResetApplication => {
                // ReplaceSurface in the same atomic reducer batch has already
                // reset the platform-owned presentation projection.
            }
            Command::ImagePickFromLibrary => {
                // Linux desktop has no photo library — report unavailable
                dispatch_platform_event(
                    container,
                    app_engine,
                    toast_overlay,
                    Event::HardwareUnavailable {
                        transport: "photo_library".into(),
                    },
                );
            }

            // Phase 2b screen-presentation lifecycle commands. Linux
            // desktop has no programmatic brightness control (the user
            // owns it via system settings) and the OS-level idle timer
            // / screensaver is owned by GNOME / KDE / etc. — answer
            // unavailable so core does not retry. The command/event
            // protocol treats this as "request honoured at platform
            // default."
            Command::SetScreenBrightness { .. } => {
                if notified_unavailable.insert("screen_brightness") {
                    dispatch_unavailable(container, app_engine, toast_overlay, "screen_brightness");
                }
            }
            Command::SetIdleTimerDisabled { .. } => {
                if notified_unavailable.insert("idle_timer") {
                    dispatch_unavailable(container, app_engine, toast_overlay, "idle_timer");
                }
            }
            // ShowShareSheet is the iOS / Android system share affordance;
            // Linux desktop has no equivalent (the user copy/pastes the
            // URL or uses the app's own share dialog). Answer unavailable.
            Command::ShowShareSheet { .. } => {
                if notified_unavailable.insert("share_sheet") {
                    dispatch_unavailable(container, app_engine, toast_overlay, "share_sheet");
                }
            }
            // Phase 2c screen-presentation: orientation lock is a
            // mobile concept — desktop windows are user-resizable and
            // don't rotate with the device. Answer unavailable.
            Command::SetOrientationLock { .. } => {
                if notified_unavailable.insert("orientation_lock") {
                    dispatch_unavailable(container, app_engine, toast_overlay, "orientation_lock");
                }
            }

            // Hardware commands (QrRequestScan, BLE, NFC, audio, camera,
            // location, notifications, wakeups) are app-owned (#1252):
            // linux-app's bridge routes them to linux-hw and answers
            // unavailability itself. Any that still arrive here fall to
            // this arm and do nothing — the renderer never handles them.
            _ => {
                // Display command with no GTK arm, or a hardware command
                // the app should have routed away — no-op either way.
            }
        }
    }
}

fn dispatch_unavailable(
    container: &GtkBox,
    app_engine: &EngineHandle,
    toast_overlay: &adw::ToastOverlay,
    transport: &str,
) {
    dispatch_platform_event(
        container,
        app_engine,
        toast_overlay,
        Event::HardwareUnavailable {
            transport: transport.to_string(),
        },
    );
}

pub(crate) fn show_alert(container: &GtkBox, title: &str, message: &str) {
    if let Some(window) = container
        .root()
        .and_then(|r| r.downcast::<gtk4::Window>().ok())
    {
        let dialog = adw::MessageDialog::new(Some(&window), Some(title), Some(message));
        dialog.add_response("ok", "OK");
        dialog.set_default_response(Some("ok"));
        dialog.set_close_response("ok");
        dialog.present();
    }
}
