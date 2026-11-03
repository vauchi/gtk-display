// SPDX-FileCopyrightText: 2026 Mattia Egloff <mattia.egloff@pm.me>
// SPDX-License-Identifier: GPL-3.0-or-later

//! Applies ordered Core command batches to the native GTK projection.

use gtk4::prelude::*;
use gtk4::{Box as GtkBox, Orientation, Separator};
use libadwaita as adw;
use std::rc::Rc;

use vauchi_contract::{Command, InteractionId, SurfaceId};

use super::widgets::{
    COMMAND_STATE, clear_title_row_for, context_bar_button, dispatch_event, render_primary_button,
    render_title_row,
};
use super::{environment, overlays, sidebar};
use crate::core_ui::engine_link::EngineHandle;
use crate::core_ui::generic_surface::{self, OnEvent};

pub(crate) fn handle_commands(
    container: &GtkBox,
    app_engine: &EngineHandle,
    toast_overlay: &adw::ToastOverlay,
    commands: Vec<Command>,
    origin: Option<&InteractionId>,
) {
    let mut platform_commands = Vec::new();
    let mut presentation_changed = false;
    let mut pending_overlay = None;
    for command in commands {
        let dropped = describe_revisioned(&command).map(|(kind, surface_id, revision)| {
            let current = COMMAND_STATE.with(|state| {
                state
                    .borrow()
                    .surface_by_id(surface_id)
                    .map(|surface| surface.revision)
            });
            format!(
                "{kind} for {} rev {revision}, shell has rev {current:?}",
                surface_id.as_str()
            )
        });
        let accepted = COMMAND_STATE.with(|state| state.borrow_mut().apply(command.clone()));
        if !accepted {
            if let Some(detail) = dropped {
                eprintln!("[CoreUI] Failed: dropped {detail}");
            }
            continue;
        }
        match command {
            Command::ReplaceSurface { .. } => presentation_changed = true,
            Command::PresentOverlay {
                surface_id,
                overlay,
                ..
            } => pending_overlay = Some((surface_id, overlay)),
            Command::SetPresentationProfile { profile } => {
                environment::apply_window_class(container, profile.window_class);
                presentation_changed = true;
            }
            Command::SetContextBar { .. } => presentation_changed = true,
            Command::SetNavigation { .. } => presentation_changed = true,
            Command::ShowToast { toast } => {
                toast_overlay.add_toast(adw::Toast::new(&toast.message));
            }
            Command::PresentAlert { alert } => {
                super::super::action_dispatcher::show_alert(
                    container,
                    &alert.title,
                    &alert.message,
                );
            }
            Command::OpenExternalUrl { url } => {
                let _ = gtk4::gio::AppInfo::launch_default_for_uri(
                    &url,
                    None::<&gtk4::gio::AppLaunchContext>,
                );
            }
            Command::PerformNativeBack => {
                if let Some(window) = container
                    .root()
                    .and_then(|root| root.downcast::<gtk4::Window>().ok())
                    && let Some(application) = window.application()
                {
                    application.quit();
                }
            }
            // The 30-second fallback poll (`register_wakeup_poll`) keeps
            // running regardless; this arms the fast path a live QR
            // exchange's sub-second wait needs (#450). Core computes the
            // wait (#548).
            Command::ScheduleWakeup { delay_millis, .. } => {
                crate::shell::arm_wakeup(
                    app_engine,
                    container,
                    toast_overlay,
                    u64::from(delay_millis),
                );
            }
            other => platform_commands.push(other),
        }
    }
    if presentation_changed {
        render_presentation(container, app_engine, toast_overlay);
    }
    if let Some((surface_id, overlay)) = pending_overlay {
        // Resolved after the rebuild above, never before: the button that
        // asked for this overlay is orphaned by `render_presentation`.
        let anchor = origin.and_then(context_bar_button);
        overlays::present(
            container,
            app_engine,
            toast_overlay,
            surface_id,
            overlay,
            anchor.as_ref(),
        );
    }
    if !platform_commands.is_empty() {
        super::super::action_dispatcher::handle_exchange_commands(
            container,
            app_engine,
            toast_overlay,
            &platform_commands,
        );
    }
}

fn render_presentation(
    container: &GtkBox,
    app_engine: &EngineHandle,
    toast_overlay: &adw::ToastOverlay,
) {
    let focus_name = current_focus_name(container);
    let (surfaces, context_bar, navigation) = COMMAND_STATE.with(|state| {
        let state = state.borrow();
        let surfaces = state
            .visible_surfaces()
            .into_iter()
            .cloned()
            .collect::<Vec<_>>();
        let context_bar = state
            .context_bar()
            .map(|(surface_id, bar)| (surface_id.clone(), bar.clone()));
        let navigation = state.navigation().map(|(_, navigation)| navigation.clone());
        (surfaces, context_bar, navigation)
    });
    sidebar::update(container, navigation.as_ref());
    let container_for_event = container.clone();
    let app_engine_for_event = app_engine.clone();
    let toast_for_event = toast_overlay.clone();
    let on_event: OnEvent = Rc::new(move |event| {
        dispatch_event(
            &container_for_event,
            &app_engine_for_event,
            &toast_for_event,
            event,
            None,
        );
    });

    while let Some(child) = container.first_child() {
        container.remove(&child);
    }
    if surfaces.len() > 1 {
        let panes = GtkBox::new(Orientation::Horizontal, 0);
        panes.set_hexpand(true);
        panes.set_vexpand(true);
        for (index, surface) in surfaces.iter().enumerate() {
            if index > 0 {
                panes.append(&Separator::new(Orientation::Vertical));
            }
            let pane = GtkBox::new(Orientation::Vertical, 0);
            pane.set_hexpand(true);
            pane.set_vexpand(true);
            generic_surface::render(&pane, surface, &on_event, app_engine);
            panes.append(&pane);
        }
        container.append(&panes);
    } else if let Some(surface) = surfaces.first() {
        generic_surface::render(container, surface, &on_event, app_engine);
    }
    if let Some((surface_id, bar)) = &context_bar {
        let navigation_shown = navigation
            .as_ref()
            .is_some_and(|navigation| !navigation.items.is_empty());
        let title = surfaces
            .iter()
            .find(|surface| &surface.surface_id == surface_id)
            .map(|surface| surface.title.as_str())
            .unwrap_or_default();
        render_title_row(
            container,
            app_engine,
            toast_overlay,
            surface_id,
            title,
            bar,
            navigation_shown,
        );
        if let Some(primary) = bar.primary.as_ref() {
            render_primary_button(container, app_engine, toast_overlay, surface_id, primary);
        }
    } else {
        clear_title_row_for(container);
    }
    restore_focus(container, focus_name.as_deref());
}

fn current_focus_name(container: &GtkBox) -> Option<String> {
    container
        .root()
        .and_then(|root| root.downcast::<gtk4::Window>().ok())
        .and_then(|window| gtk4::prelude::GtkWindowExt::focus(&window))
        .map(|widget| widget.widget_name().to_string())
        .filter(|name| !name.is_empty())
}

fn restore_focus(container: &GtkBox, widget_name: Option<&str>) {
    let Some(widget_name) = widget_name else {
        return;
    };
    let root: gtk4::Widget = container.clone().upcast();
    if let Some(widget) = find_widget_by_name(&root, widget_name) {
        widget.grab_focus();
    }
}

pub(super) fn find_widget_by_name(root: &gtk4::Widget, widget_name: &str) -> Option<gtk4::Widget> {
    if root.widget_name() == widget_name {
        return Some(root.clone());
    }
    let mut child = root.first_child();
    while let Some(widget) = child {
        if let Some(found) = find_widget_by_name(&widget, widget_name) {
            return Some(found);
        }
        child = widget.next_sibling();
    }
    None
}

fn describe_revisioned(command: &Command) -> Option<(&'static str, &SurfaceId, u64)> {
    match command {
        Command::ReplaceSurface { surface } => {
            Some(("ReplaceSurface", &surface.surface_id, surface.revision))
        }
        Command::SetContextBar {
            surface_id,
            revision,
            ..
        } => Some(("SetContextBar", surface_id, *revision)),
        Command::SetNavigation {
            surface_id,
            revision,
            ..
        } => Some(("SetNavigation", surface_id, *revision)),
        Command::PresentOverlay {
            surface_id,
            revision,
            ..
        } => Some(("PresentOverlay", surface_id, *revision)),
        Command::DismissOverlay {
            surface_id,
            revision,
            ..
        } => Some(("DismissOverlay", surface_id, *revision)),
        _ => None,
    }
}
