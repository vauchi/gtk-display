// SPDX-FileCopyrightText: 2026 Mattia Egloff <mattia.egloff@pm.me>
// SPDX-License-Identifier: GPL-3.0-or-later

//! Platform-specific GTK/GNOME chrome (header bar, notifications).

pub mod deep_link;
pub mod header_bar;
#[cfg(any(
    feature = "ble",
    feature = "nfc",
    feature = "audio",
    feature = "camera"
))]
pub mod linux_hardware_link;
pub mod notifications;
pub mod screen_capture_protection;
