// SPDX-FileCopyrightText: 2026 Mattia Egloff <mattia.egloff@pm.me>
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::core_ui::engine_link::EngineHandle;

use gtk4::Box as GtkBox;
use gtk4::prelude::*;
use libadwaita as adw;
use std::cell::Cell;
use std::rc::Rc;

use vauchi_core::{ColorScheme, Command, Event, InputMode, MotionPreference, WindowClass};

pub(crate) fn install_environment_reporting(
    window: &adw::ApplicationWindow,
    container: &GtkBox,
    app_engine: &EngineHandle,
    toast_overlay: &adw::ToastOverlay,
) {
    // Core answers a light/dark switch under "follow system" with the
    // surface in the other theme, so the switch re-presents the surface.
    {
        let container = container.clone();
        let app_engine = app_engine.clone();
        let toast_overlay = toast_overlay.clone();
        adw::StyleManager::default().connect_dark_notify(move |_| {
            super::widgets::render_current_surface(&container, &app_engine, &toast_overlay);
        });
    }
    let last_size = Rc::new(Cell::new((0, 0)));
    let container = container.clone();
    let app_engine = app_engine.clone();
    window.add_tick_callback(move |window, _| {
        let size = (window.width(), window.height());
        if size != last_size.get() && size.0 > 0 && size.1 > 0 {
            last_size.set(size);
            report_environment_with_size(&container, &app_engine, size.0, size.1);
        }
        gtk4::glib::ControlFlow::Continue
    });
}

/// Reports the window's size, never `container`'s: the container sits inside
/// the sidebar split view, so its width shrinks when Core's Medium class
/// shows the sidebar — reporting it would flip Core back to Compact, which
/// collapses the sidebar and widens the container again, on every
/// interaction.
pub(super) fn report_environment(container: &GtkBox, app_engine: &EngineHandle) {
    let (width, height) = container.root().map_or_else(
        || (container.width(), container.height()),
        |root| (root.width(), root.height()),
    );
    report_environment_with_size(container, app_engine, width.max(1), height.max(1));
}

fn report_environment_with_size(
    container: &GtkBox,
    app_engine: &EngineHandle,
    width: i32,
    height: i32,
) {
    let motion = gtk4::Settings::default().map_or(MotionPreference::Full, |settings| {
        if settings.is_gtk_enable_animations() {
            MotionPreference::Full
        } else {
            MotionPreference::Reduced
        }
    });
    let event = environment_event(
        width,
        height,
        motion,
        adw::StyleManager::default().is_dark(),
    );
    for command in super::commands_for_event(app_engine.as_ref(), event) {
        match command {
            Command::SetPresentationProfile { profile } => {
                apply_window_class(container, profile.window_class);
            }
            Command::PresentAlert { alert } => {
                crate::core_ui::action_dispatcher::show_alert(
                    container,
                    &alert.title,
                    &alert.message,
                );
            }
            _ => {}
        }
    }
}

fn environment_event(width: i32, height: i32, motion: MotionPreference, dark: bool) -> Event {
    Event::PresentationEnvironmentChanged {
        available_width: width as u32,
        available_height: height as u32,
        input_modes: vec![InputMode::Keyboard, InputMode::Pointer],
        motion,
        color_scheme: Some(if dark {
            ColorScheme::Dark
        } else {
            ColorScheme::Light
        }),
    }
}

pub(super) fn apply_window_class(container: &GtkBox, class: WindowClass) {
    for css_class in [
        "presentation-compact",
        "presentation-medium",
        "presentation-expanded",
    ] {
        container.remove_css_class(css_class);
    }
    match class {
        WindowClass::Compact => container.add_css_class("presentation-compact"),
        WindowClass::Medium => container.add_css_class("presentation-medium"),
        WindowClass::Expanded => container.add_css_class("presentation-expanded"),
    }
    // D4: the persistent sidebar collapses under Core's own Compact
    // decision rather than a shell-computed width threshold (VRS02 — the
    // shell never branches on form factor itself).
    super::sidebar::set_collapsed(container, matches!(class, WindowClass::Compact));
}

// INLINE_TEST_REQUIRED: tests exercise private `environment_event`
#[cfg(test)]
mod tests {
    use super::*;

    /// Core resolves "follow system" from this report (#814), so it must
    /// say whether the desktop is light or dark, never leave it out.
    // @internal
    #[test]
    fn the_environment_report_says_whether_the_desktop_is_dark() {
        for (dark, expected) in [(true, ColorScheme::Dark), (false, ColorScheme::Light)] {
            let event = environment_event(800, 600, MotionPreference::Full, dark);

            let Event::PresentationEnvironmentChanged { color_scheme, .. } = event else {
                panic!("expected PresentationEnvironmentChanged, got {event:?}");
            };
            assert_eq!(color_scheme, Some(expected), "dark = {dark}");
        }
    }
}
