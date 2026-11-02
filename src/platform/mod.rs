// SPDX-FileCopyrightText: 2026 Mattia Egloff <mattia.egloff@pm.me>
// SPDX-License-Identifier: GPL-3.0-or-later

//! Platform-specific GTK/GNOME chrome (header bar, notifications).

pub mod camera;
pub mod deep_link;
#[cfg(all(feature = "camera", target_os = "linux"))]
pub mod executors;
pub mod hardware;
pub mod header_bar;
pub mod init;
#[cfg(any(
    feature = "ble",
    feature = "nfc",
    feature = "audio",
    feature = "camera"
))]
pub mod linux_hardware_link;
pub mod network_cost;
pub mod notifications;
pub mod screen_capture_protection;
pub mod tcp_exchange;
