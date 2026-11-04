// SPDX-FileCopyrightText: 2026 Mattia Egloff <mattia.egloff@pm.me>
// SPDX-License-Identifier: GPL-3.0-or-later

//! Vauchi's GTK4 renderer (ADR-078 gtk-display): it maps Core's generic
//! presentation commands to widgets and reports events. The app that wires
//! it to Core, `gvauchi`, lives in linux-app.

pub mod capture;
pub mod core_ui;
pub mod platform;
pub mod screen_catalog;
pub mod shell;
