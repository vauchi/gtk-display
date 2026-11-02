// SPDX-FileCopyrightText: 2026 Mattia Egloff <mattia.egloff@pm.me>
// SPDX-License-Identifier: GPL-3.0-or-later

use gtk4::glib;
use gtk4::prelude::*;
use gtk4::{Box as GtkBox, Button, Orientation};
use libadwaita as adw;
use libadwaita::prelude::MessageDialogExt as _;
use std::borrow::Cow;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use vauchi_app::ui::AppEngine;
use vauchi_core::{Event, MotionPreference, OverlayKind, OverlaySpec, SurfaceId};

use crate::core_ui::navigation_icons::resolve_icon_name;

use super::widgets::{dispatch_event, dispatch_interaction};

pub(super) fn present(
    container: &GtkBox,
    app_engine: &Rc<RefCell<AppEngine>>,
    toast_overlay: &adw::ToastOverlay,
    surface_id: SurfaceId,
    overlay: OverlaySpec,
    origin: Option<&Button>,
) {
    match overlay.kind {
        OverlayKind::Navigation => {
            present_navigation(container, app_engine, toast_overlay, surface_id, overlay)
        }
        OverlayKind::ActionMenu => {
            if let Some(origin) = origin {
                present_action_menu(
                    container,
                    app_engine,
                    toast_overlay,
                    surface_id,
                    overlay,
                    origin,
                );
            }
        }
        OverlayKind::Information => {
            present_information(container, app_engine, toast_overlay, surface_id, overlay)
        }
    }
}

/// Text about the surface, read once and closed: the body is a label
/// the screen reader walks, and closing reports a dismissal so Core
/// forgets the overlay.
fn present_information(
    container: &GtkBox,
    app_engine: &Rc<RefCell<AppEngine>>,
    toast_overlay: &adw::ToastOverlay,
    surface_id: SurfaceId,
    overlay: OverlaySpec,
) {
    let parent = container
        .root()
        .and_then(|root| root.downcast::<gtk4::Window>().ok());
    let dialog = adw::MessageDialog::builder()
        .heading(overlay.title.as_deref().unwrap_or_default())
        .body(overlay.body.as_deref().unwrap_or_default())
        .modal(true)
        .build();
    dialog.set_transient_for(parent.as_ref());
    dialog.add_css_class("information-panel");
    dialog.add_response("close", &close_label(&overlay));
    dialog.set_default_response(Some("close"));
    dialog.set_close_response("close");

    let container = container.clone();
    let app_engine = app_engine.clone();
    let toast_overlay = toast_overlay.clone();
    dialog.connect_response(None, move |_, _| {
        dispatch_event(
            &container,
            &app_engine,
            &toast_overlay,
            Event::OverlayDismissed {
                surface_id: surface_id.clone(),
                kind: OverlayKind::Information,
            },
            None,
        );
    });
    dialog.present();
}

/// Core's own wording for the overlay's way out when it sent one
/// (vauchi/private#515); the shell's locale word otherwise, so an older
/// Core that omits `close_label` sees the text it always has.
fn close_label(overlay: &OverlaySpec) -> Cow<'_, str> {
    overlay.close_label.as_deref().map_or_else(
        || {
            Cow::Owned(vauchi_app::i18n::get_string(
                crate::locale::detect_locale(),
                "action.close",
            ))
        },
        Cow::Borrowed,
    )
}

fn present_navigation(
    container: &GtkBox,
    app_engine: &Rc<RefCell<AppEngine>>,
    toast_overlay: &adw::ToastOverlay,
    surface_id: SurfaceId,
    overlay: OverlaySpec,
) {
    let Some(parent) = container
        .root()
        .and_then(|root| root.downcast::<gtk4::Window>().ok())
    else {
        return;
    };
    let window = gtk4::Window::builder()
        .title(overlay.title.as_deref().unwrap_or("Navigation"))
        .transient_for(&parent)
        .modal(true)
        .default_width(360)
        .default_height(440)
        .build();
    window.add_css_class("navigation-palette");

    let items = GtkBox::new(Orientation::Vertical, 8);
    items.set_margin_top(16);
    items.set_margin_bottom(16);
    items.set_margin_start(16);
    items.set_margin_end(16);
    let activated = Rc::new(Cell::new(false));

    for item in overlay.items {
        let button = overlay_button(&item, OverlayKind::Navigation);
        let interaction_id = item.interaction_id;
        // Weak window capture: button -> closure -> window would cycle and
        // keep the modal alive past close(), leaking it into the AT-SPI tree.
        button.connect_clicked(glib::clone!(
            #[weak]
            window,
            #[strong]
            activated,
            #[strong]
            container,
            #[strong]
            app_engine,
            #[strong]
            toast_overlay,
            #[strong]
            surface_id,
            #[strong]
            interaction_id,
            move |_| {
                activated.set(true);
                window.destroy();
                dispatch_interaction(
                    &container,
                    &app_engine,
                    &toast_overlay,
                    &surface_id,
                    &interaction_id,
                    None,
                );
            }
        ));
        items.append(&button);
    }

    // The destination list is Core's to size, not this shell's, and a
    // GtkBox of buttons put straight into the window has no way to scroll:
    // the window simply grows past its 440 px request, and past a short
    // display, with the overflow unreachable. `propagate_natural_height`
    // keeps a short list drawn at its own height rather than padded out to
    // fill the scroller.
    let scroller = gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .vscrollbar_policy(gtk4::PolicyType::Automatic)
        .propagate_natural_height(true)
        .child(&items)
        .build();
    let revealer = gtk4::Revealer::builder()
        .transition_type(gtk4::RevealerTransitionType::SlideRight)
        .transition_duration(animation_duration(220))
        .child(&scroller)
        .build();
    window.set_child(Some(&revealer));

    let container_for_close = container.clone();
    let engine_for_close = app_engine.clone();
    let toast_for_close = toast_overlay.clone();
    window.connect_close_request(move |_| {
        if !activated.get() {
            dispatch_event(
                &container_for_close,
                &engine_for_close,
                &toast_for_close,
                Event::OverlayDismissed {
                    surface_id: surface_id.clone(),
                    kind: OverlayKind::Navigation,
                },
                None,
            );
        }
        gtk4::glib::Propagation::Proceed
    });
    window.present();
    revealer.set_reveal_child(true);
}

fn present_action_menu(
    container: &GtkBox,
    app_engine: &Rc<RefCell<AppEngine>>,
    toast_overlay: &adw::ToastOverlay,
    surface_id: SurfaceId,
    overlay: OverlaySpec,
    origin: &Button,
) {
    let popover = gtk4::Popover::new();
    popover.add_css_class("secondary-action-popover");
    popover.set_has_arrow(true);
    popover.set_autohide(true);
    popover.set_parent(origin);

    let items = GtkBox::new(Orientation::Vertical, 6);
    items.set_margin_top(8);
    items.set_margin_bottom(8);
    items.set_margin_start(8);
    items.set_margin_end(8);
    let activated = Rc::new(Cell::new(false));

    for item in overlay.items {
        let button = overlay_button(&item, OverlayKind::ActionMenu);
        let interaction_id = item.interaction_id;
        // Weak popover capture: same reference cycle as the navigation window.
        button.connect_clicked(glib::clone!(
            #[weak]
            popover,
            #[strong]
            activated,
            #[strong]
            container,
            #[strong]
            app_engine,
            #[strong]
            toast_overlay,
            #[strong]
            surface_id,
            #[strong]
            interaction_id,
            move |_| {
                activated.set(true);
                popover.popdown();
                dispatch_interaction(
                    &container,
                    &app_engine,
                    &toast_overlay,
                    &surface_id,
                    &interaction_id,
                    None,
                );
            }
        ));
        items.append(&button);
    }

    let revealer = gtk4::Revealer::builder()
        .transition_type(gtk4::RevealerTransitionType::Crossfade)
        .transition_duration(animation_duration(150))
        .child(&items)
        .build();
    popover.set_child(Some(&revealer));

    let container_for_close = container.clone();
    let engine_for_close = app_engine.clone();
    let toast_for_close = toast_overlay.clone();
    popover.connect_closed(move |popover| {
        if !activated.get() {
            dispatch_event(
                &container_for_close,
                &engine_for_close,
                &toast_for_close,
                Event::OverlayDismissed {
                    surface_id: surface_id.clone(),
                    kind: OverlayKind::ActionMenu,
                },
                None,
            );
        }
        popover.unparent();
    });
    popover.popup();
    revealer.set_reveal_child(true);
}

/// Core names an icon on navigation items only, so an action menu stays plain
/// rather than growing a column of neutral markers.
fn overlay_icon_name(
    action: &vauchi_core::ActionSpec,
    kind: OverlayKind,
) -> Option<std::borrow::Cow<'static, str>> {
    if kind != OverlayKind::Navigation && action.icon_token.is_none() {
        return None;
    }
    let theme = crate::core_ui::pictograms::icon_theme();
    Some(resolve_icon_name(action.icon_token.as_deref(), |name| {
        theme.as_ref().is_some_and(|theme| theme.has_icon(name))
    }))
}

fn overlay_button(action: &vauchi_core::ActionSpec, kind: OverlayKind) -> Button {
    let button = Button::builder()
        .sensitive(action.enabled)
        .halign(gtk4::Align::Fill)
        .build();
    // Icon *and* label, never icon alone: the icon is a recognition aid for a
    // reader who skims, and dropping the word would trade one barrier for
    // another.
    match overlay_icon_name(action, kind) {
        Some(icon_name) => button.set_child(Some(
            &adw::ButtonContent::builder()
                .icon_name(icon_name.as_ref())
                .label(&action.label)
                .build(),
        )),
        None => button.set_label(&action.label),
    }
    button.set_widget_name(action.interaction_id.as_str());
    // `OverlaySpec` carries no `PresentationTokens` of its own (unlike
    // `SurfaceSpec`), so a nav item has no per-surface touch-target floor to
    // read. The bundled design tokens' touch-target minimum is the same
    // value Core derives `PresentationTokens::minimum_target_size` from, and
    // is the best available source until overlays carry tokens too.
    let target_px = i32::from(
        vauchi_app::theme::default_theme()
            .tokens
            .touch_target
            .minimum,
    );
    button.set_size_request(-1, target_px);
    crate::core_ui::accessibility::apply_label(&button, &action.accessibility_label);
    // See `generic_surface::action_button`: `Serious` is not yet visible to
    // this build's linked vauchi-core, and `ActionTone` is
    // `#[non_exhaustive]`, so the wildcard arm is required either way.
    match action.tone {
        vauchi_core::ActionTone::Destructive => button.add_css_class("destructive-action"),
        vauchi_core::ActionTone::Standard => {}
        _ => {}
    }
    button
}

fn animation_duration(full_duration_ms: u32) -> u32 {
    let motion = gtk4::Settings::default().map_or(MotionPreference::Full, |settings| {
        if settings.is_gtk_enable_animations() {
            MotionPreference::Full
        } else {
            MotionPreference::Reduced
        }
    });
    match motion {
        MotionPreference::Full => full_duration_ms,
        MotionPreference::Reduced => 0,
    }
}

// INLINE_TEST_REQUIRED: the dialog `close_label` feeds has no public
// accessor and cannot be reached through a widget without a display.
#[cfg(test)]
mod close_label_tests {
    use super::close_label;
    use vauchi_core::{OverlayKind, OverlaySpec};

    fn overlay(close_label: Option<&str>) -> OverlaySpec {
        OverlaySpec {
            kind: OverlayKind::Information,
            title: None,
            items: Vec::new(),
            body: None,
            close_label: close_label.map(str::to_owned),
        }
    }

    // @internal
    #[test]
    fn cores_close_label_wins_when_it_sends_one() {
        assert_eq!(close_label(&overlay(Some("Schließen"))), "Schließen");
    }

    // @internal
    #[test]
    fn an_older_core_with_no_close_label_keeps_the_locale_word() {
        let expected = vauchi_app::i18n::get_string(crate::locale::detect_locale(), "action.close");
        assert_eq!(close_label(&overlay(None)), expected);
    }
}
