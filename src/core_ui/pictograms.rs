// SPDX-FileCopyrightText: 2026 Mattia Egloff <mattia.egloff@pm.me>
// SPDX-License-Identifier: GPL-3.0-or-later

//! Vauchi's bundled pictograms, compiled into the binary as a GResource and
//! exposed to the icon theme as `-symbolic` icons so GTK draws them in the
//! surrounding foreground colour.

use std::sync::Once;

use gtk4::gio;

/// Resource directory added to the icon theme; icons live under
/// `scalable/actions/` below it, as `data/icons/` lays them out.
pub const ICON_RESOURCE_PATH: &str = "/app/vauchi/Vauchi/icons";

/// Makes the bundled pictograms visible to `gio::resources_*`. Idempotent.
pub fn register_resources() {
    static REGISTER: Once = Once::new();
    REGISTER.call_once(|| {
        gio::resources_register_include!("vauchi.gresource")
            .expect("bundled GResource is well-formed")
    });
}

/// The display's icon theme with the bundled pictograms on its search path.
pub fn icon_theme() -> Option<gtk4::IconTheme> {
    register_resources();
    let display = gtk4::gdk::Display::default()?;
    let theme = gtk4::IconTheme::for_display(&display);
    if !theme
        .resource_path()
        .iter()
        .any(|path| path.as_str() == ICON_RESOURCE_PATH)
    {
        theme.add_resource_path(ICON_RESOURCE_PATH);
    }
    Some(theme)
}
