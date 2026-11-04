// SPDX-FileCopyrightText: 2026 Mattia Egloff <mattia.egloff@pm.me>
// SPDX-License-Identifier: GPL-3.0-or-later

//! Executes platform commands from the Core reducer.
//!
//! Handles navigation, alerts, toasts and, when no hardware shell is
//! attached, reports each hardware command's transport as unavailable;
//! Core then decides what the user sees (ADR-066, private#1029).

mod file_picker;

use gtk4::Box as GtkBox;
use libadwaita as adw;
use libadwaita::prelude::*;
use std::collections::HashSet;

use vauchi_contract::{Command, Event};

use crate::core_ui::engine_link::EngineHandle;

use super::contextual_surface::dispatch_platform_event;

/// Dispatch exchange hardware commands to platform-specific actions (ADR-031).
///
/// Commands arrive in batches (e.g., BleStartScanning + BleStartAdvertising together).
/// We deduplicate "unavailable" toasts per transport to avoid spamming the user.
pub(crate) fn handle_exchange_commands(
    container: &GtkBox,
    app_engine: &EngineHandle,
    toast_overlay: &adw::ToastOverlay,
    commands: &[Command],
) {
    // Track which transports we've already shown "unavailable" toasts for
    // to avoid spamming when a batch has multiple commands for the same transport.
    let mut notified_unavailable: HashSet<&str> = HashSet::new();

    for cmd in commands {
        if crate::core_ui::hardware_link::route_installed(cmd)
            == crate::core_ui::hardware_link::Routed::ToHardware
        {
            continue;
        }
        match cmd {
            Command::QrDisplay { .. } => {
                // The paired ReplaceSurface command already carries the
                // updated generic QR node.
            }
            // linux-hw scans when it is attached (Core routes the scan to
            // it); here none is. "Camera" is the name the old in-shell
            // camera reported, which Core answers with a toast and does not
            // treat as a failed exchange.
            Command::QrRequestScan => {
                dispatch_unavailable(container, app_engine, toast_overlay, "Camera");
            }

            // ── Hardware ports (BLE, NFC, audio) ─────────────────────
            // linux-hw runs these through the hardware link (ADR-078 step
            // 3a); they reach this arm only when no link is installed — a
            // build without hardware features, or a shell that did not
            // start — and Core then learns the transport is unavailable.
            Command::AudioEmitChallenge { .. } | Command::AudioListenForResponse { .. } => {
                if notified_unavailable.insert("audio") {
                    dispatch_unavailable(container, app_engine, toast_overlay, "Audio");
                }
            }
            Command::BleStartScanning { .. }
            | Command::BleStartAdvertising { .. }
            | Command::BleConnect { .. }
            | Command::BleWriteCharacteristic { .. }
            | Command::BleReadCharacteristic { .. } => {
                if notified_unavailable.insert("ble") {
                    dispatch_unavailable(container, app_engine, toast_overlay, "BLE");
                }
            }
            Command::NfcActivate { .. } | Command::NfcSendApdu { .. } => {
                if notified_unavailable.insert("nfc") {
                    dispatch_unavailable(container, app_engine, toast_overlay, "NFC");
                }
            }
            Command::AudioStop
            | Command::BleStopScanning
            | Command::BleDisconnect { .. }
            | Command::NfcDeactivate => {}
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
            Command::PostNotification { notification } => {
                post_notification(container, notification);
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
            Command::ImageCaptureFromCamera => {
                // Camera capture not supported on desktop — report unavailable
                dispatch_platform_event(
                    container,
                    app_engine,
                    toast_overlay,
                    Event::HardwareUnavailable {
                        transport: "camera".into(),
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
            // SwitchCamera is multi-stage exchange's front/rear flip —
            // desktop webcams don't have a front/rear distinction.
            Command::SwitchCamera { .. } => {
                if notified_unavailable.insert("camera_switch") {
                    dispatch_unavailable(container, app_engine, toast_overlay, "camera_switch");
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

            // Capture-at-exchange (ADR-051): desktop GTK has no location
            // provider wired (no geoclue dependency), so answer unavailable —
            // consistent with camera/brightness/orientation above. This lets
            // core clear the pending capture immediately instead of waiting
            // out the request timeout. Silent (no toast): location is a
            // background capture, not a user-initiated action.
            Command::LocationRequest { .. } if notified_unavailable.insert("location") => {
                dispatch_unavailable(container, app_engine, toast_overlay, "location");
            }
            _ => {
                // Future exchange command — no-op until implemented.
            }
        }
    }
}

fn post_notification(container: &GtkBox, notification: &vauchi_contract::NotificationSpec) {
    let Some(application) = container
        .root()
        .and_then(|root| root.downcast::<gtk4::Window>().ok())
        .and_then(|window| window.application())
    else {
        return;
    };
    crate::platform::notifications::post_notification(&application, notification);
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
