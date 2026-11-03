// SPDX-FileCopyrightText: 2026 Mattia Egloff <mattia.egloff@pm.me>
// SPDX-License-Identifier: GPL-3.0-or-later

use gtk4::glib;
use gtk4::pango;
use gtk4::prelude::*;
use gtk4::{Box as GtkBox, Button, Label, Widget};
use libadwaita as adw;
use std::cell::RefCell;

use vauchi_contract::{
    ActionTone, Command, ContextBar, Event, InteractionId, StandardShortcut, SurfaceId,
};

use crate::core_ui::engine_link::{EngineHandle, EngineLink};

use crate::core_ui::accessibility;
use crate::platform::header_bar::SURFACE_TITLE_BAR_NAME;

use super::environment;
use super::{
    GtkContextRole, GtkControlArea, GtkPresentationState, context_controls_with,
    interaction_for_shortcut,
};

thread_local! {
    pub(super) static COMMAND_STATE: RefCell<GtkPresentationState> =
        RefCell::new(GtkPresentationState::default());

    /// The buttons this shell packed into the window's header bar for the
    /// current surface — tracked so the next render can remove exactly
    /// these before repacking, and so a click that opens the action menu
    /// can still find its anchor once the surface rebuild has orphaned it
    /// (`context_bar_button`).
    static TITLE_ROW_WIDGETS: RefCell<Vec<Widget>> = const { RefCell::new(Vec::new()) };
}

pub(crate) fn render_current_surface(
    container: &GtkBox,
    app_engine: &EngineHandle,
    toast_overlay: &adw::ToastOverlay,
) {
    environment::report_environment(container, app_engine);

    let commands = app_engine.initial_commands();
    if commands.is_empty() {
        return;
    }
    super::commands::handle_commands(container, app_engine, toast_overlay, commands, None);
}

/// Draws the title row's controls on the window's header bar: Core's
/// surface title in the middle, Back (and a sidebar-less Navigation
/// launcher) leading it, Info and Secondary trailing it with Secondary at
/// the true trailing end (`ContactDetail.dc.html`: info beside the title,
/// the ellipsis menu at the outer edge). Primary never reaches here — it
/// draws in the surface content (`render_primary_button`).
pub(super) fn render_title_row(
    container: &GtkBox,
    app_engine: &EngineHandle,
    toast_overlay: &adw::ToastOverlay,
    surface_id: &SurfaceId,
    title: &str,
    bar: &ContextBar,
    navigation_shown: bool,
) {
    let Some(header_bar) = locate_header_bar(container) else {
        return;
    };
    clear_title_row(&header_bar);

    let title_label = Label::builder()
        .label(title)
        .ellipsize(pango::EllipsizeMode::End)
        .single_line_mode(true)
        .build();
    header_bar.set_title_widget(Some(&title_label));

    let mut trailing = Vec::new();
    for control in context_controls_with(bar, navigation_shown) {
        match control.role.area() {
            GtkControlArea::TitleRowLeading => {
                let button = build_button(control.role, control.action, false);
                wire_context_button(
                    &button,
                    surface_id,
                    &control.action.interaction_id,
                    container,
                    app_engine,
                    toast_overlay,
                );
                header_bar.pack_start(&button);
                track_title_row_widget(button.upcast());
            }
            GtkControlArea::TitleRowTrailing => trailing.push(control),
            GtkControlArea::SurfaceContent => {}
        }
    }
    // Secondary (the ellipsis menu) belongs at the true trailing end; Info
    // stays nearer the title. `pack_end` places each new call further out,
    // so Info is packed first regardless of the order the loop above found
    // them in.
    trailing.sort_by_key(|control| control.role == GtkContextRole::Secondary);
    for control in trailing {
        let button = build_button(control.role, control.action, false);
        wire_context_button(
            &button,
            surface_id,
            &control.action.interaction_id,
            container,
            app_engine,
            toast_overlay,
        );
        header_bar.pack_end(&button);
        track_title_row_widget(button.upcast());
    }
}

/// Primary is the one role the title row never draws: a full-width button
/// pinned under the surface content, in view for a fixed layout and below
/// the scroll region for one that scrolls (both already append after the
/// surface's `ScrolledWindow`, so no layout branch is needed here).
pub(super) fn render_primary_button(
    container: &GtkBox,
    app_engine: &EngineHandle,
    toast_overlay: &adw::ToastOverlay,
    surface_id: &SurfaceId,
    action: &vauchi_contract::ActionSpec,
) {
    let button = build_button(GtkContextRole::Primary, action, true);
    button.set_margin_top(8);
    button.set_margin_bottom(8);
    button.set_margin_start(8);
    button.set_margin_end(8);
    wire_context_button(
        &button,
        surface_id,
        &action.interaction_id,
        container,
        app_engine,
        toast_overlay,
    );
    container.append(&button);
}

fn wire_context_button(
    button: &Button,
    surface_id: &SurfaceId,
    interaction_id: &InteractionId,
    container: &GtkBox,
    app_engine: &EngineHandle,
    toast_overlay: &adw::ToastOverlay,
) {
    let interaction_id = interaction_id.clone();
    let surface_id = surface_id.clone();
    let app_engine = app_engine.clone();
    let container = container.clone();
    let toast_overlay = toast_overlay.clone();
    button.connect_clicked(move |_| {
        dispatch_interaction(
            &container,
            &app_engine,
            &toast_overlay,
            &surface_id,
            &interaction_id,
            Some(&interaction_id),
        );
    });
}

fn locate_header_bar(container: &GtkBox) -> Option<adw::HeaderBar> {
    let window = container.root()?.downcast::<gtk4::Window>().ok()?;
    let widget: Widget = window.upcast();
    super::commands::find_widget_by_name(&widget, SURFACE_TITLE_BAR_NAME)?
        .downcast::<adw::HeaderBar>()
        .ok()
}

/// Blanks the title row when the active surface has no context bar yet, so
/// a previous surface's title and buttons never linger (the header bar is
/// window-level, outside `container`, so it survives `container`'s own
/// child-clearing loop in `render_presentation`).
pub(super) fn clear_title_row_for(container: &GtkBox) {
    if let Some(header_bar) = locate_header_bar(container) {
        clear_title_row(&header_bar);
    }
}

fn clear_title_row(header_bar: &adw::HeaderBar) {
    header_bar.set_title_widget(None::<&Widget>);
    TITLE_ROW_WIDGETS.with(|widgets| {
        for widget in widgets.borrow_mut().drain(..) {
            header_bar.remove(&widget);
        }
    });
}

fn track_title_row_widget(widget: Widget) {
    TITLE_ROW_WIDGETS.with(|widgets| widgets.borrow_mut().push(widget));
}

/// Re-resolve a context-bar button by the interaction it carries, for the
/// action-menu popover's anchor.
///
/// The title row is rebuilt wholesale between the click and the overlay
/// that click asks for, so the emitting widget is orphaned by then.
/// Resolving through the tracked title-row widgets (rather than walking the
/// surface tree) also keeps a same-named surface control from ever being
/// taken as the anchor.
pub(super) fn context_bar_button(interaction_id: &InteractionId) -> Option<Button> {
    TITLE_ROW_WIDGETS.with(|widgets| {
        widgets
            .borrow()
            .iter()
            .find(|widget| widget.widget_name() == interaction_id.as_str())
            .and_then(|widget| widget.clone().downcast::<Button>().ok())
    })
}

fn build_button(
    role: GtkContextRole,
    action: &vauchi_contract::ActionSpec,
    emphasized: bool,
) -> Button {
    let label = match role {
        GtkContextRole::Back => format!("← {}", action.label),
        _ => action.label.clone(),
    };
    let button = Button::builder()
        .label(&label)
        .sensitive(action.enabled)
        .build();
    button.set_widget_name(action.interaction_id.as_str());
    // The Back control paints an arrow before its copy; announcing "← Back"
    // reads the glyph aloud, so the accessible name stays Core's label.
    accessibility::apply_label(&button, &action.accessibility_label);

    match role {
        GtkContextRole::Back => button.add_css_class("context-back"),
        GtkContextRole::Navigation => button.add_css_class("context-navigation"),
        GtkContextRole::Primary => button.add_css_class("context-primary"),
        GtkContextRole::Secondary => button.add_css_class("context-secondary"),
        GtkContextRole::Info => button.add_css_class("context-info"),
    }
    if emphasized {
        button.set_hexpand(true);
        button.add_css_class("suggested-action");
        button.add_css_class("pill");
    }
    if action.tone == ActionTone::Destructive {
        button.add_css_class("destructive-action");
    }
    button
}

pub(crate) fn dispatch_shortcut(
    container: &GtkBox,
    app_engine: &EngineHandle,
    toast_overlay: &adw::ToastOverlay,
    shortcut: StandardShortcut,
) -> bool {
    let target = COMMAND_STATE.with(|state| {
        let state = state.borrow();
        let (surface_id, bar) = state.context_bar()?;
        let interaction_id = interaction_for_shortcut(bar, shortcut)?;
        Some((surface_id.clone(), interaction_id.clone()))
    });
    let Some((surface_id, interaction_id)) = target else {
        return false;
    };
    dispatch_interaction(
        container,
        app_engine,
        toast_overlay,
        &surface_id,
        &interaction_id,
        None,
    );
    true
}

pub(crate) fn request_back(
    container: &GtkBox,
    app_engine: &EngineHandle,
    toast_overlay: &adw::ToastOverlay,
) -> bool {
    let surface_id = COMMAND_STATE.with(|state| {
        state
            .borrow()
            .context_bar()
            .map(|(surface_id, _)| surface_id.clone())
    });
    let Some(surface_id) = surface_id else {
        return false;
    };
    environment::report_environment(container, app_engine);
    dispatch_event(
        container,
        app_engine,
        toast_overlay,
        Event::SurfaceActivated {
            surface_id: surface_id.clone(),
        },
        None,
    );
    dispatch_event(
        container,
        app_engine,
        toast_overlay,
        Event::BackRequested { surface_id },
        None,
    );
    true
}

pub(super) fn dispatch_interaction(
    container: &GtkBox,
    app_engine: &EngineHandle,
    toast_overlay: &adw::ToastOverlay,
    surface_id: &SurfaceId,
    interaction_id: &InteractionId,
    origin: Option<&InteractionId>,
) {
    environment::report_environment(container, app_engine);
    for event in [
        Event::SurfaceActivated {
            surface_id: surface_id.clone(),
        },
        Event::ActionActivated {
            surface_id: surface_id.clone(),
            interaction_id: interaction_id.clone(),
        },
    ] {
        dispatch_event(container, app_engine, toast_overlay, event, origin);
    }
}

pub(crate) fn dispatch_platform_event(
    container: &GtkBox,
    app_engine: &EngineHandle,
    toast_overlay: &adw::ToastOverlay,
    event: Event,
) {
    dispatch_event(container, app_engine, toast_overlay, event, None);
}

/// Hand an event to Core on the next main-loop idle, never inside the GTK
/// signal emission that produced it.
///
/// Core's answer rebuilds the widget tree. Rebuilding from inside an emission
/// leaves GTK resuming on widgets it has already dropped: the Actions popover
/// anchored itself to the button the rebuild had just orphaned
/// (`gtk_widget_realize` on a widget outside any toplevel, then SIGSEGV), and
/// control handlers destroyed their own emitter. Deferring is the whole guard
/// for that class, so it lives here — the one place the shell re-enters Core
/// — instead of at each call site.
pub(super) fn dispatch_event(
    container: &GtkBox,
    app_engine: &EngineHandle,
    toast_overlay: &adw::ToastOverlay,
    event: Event,
    origin: Option<&InteractionId>,
) {
    let container = container.clone();
    let app_engine = app_engine.clone();
    let toast_overlay = toast_overlay.clone();
    let origin = origin.cloned();
    glib::idle_add_local_once(move || {
        let commands = commands_for_event(app_engine.as_ref(), event);
        super::commands::handle_commands(
            &container,
            &app_engine,
            &toast_overlay,
            commands,
            origin.as_ref(),
        );
    });
}

/// The batch the shell applies for one event, rejected or not.
pub fn commands_for_event<L: EngineLink + ?Sized>(link: &L, event: Event) -> Vec<Command> {
    link.dispatch(event)
}
