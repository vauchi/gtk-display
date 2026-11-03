// SPDX-FileCopyrightText: 2026 Mattia Egloff <mattia.egloff@pm.me>
// SPDX-License-Identifier: GPL-3.0-or-later

//! Native GTK renderer for Core's domain-free presentation nodes.

mod collections;
mod controls;
mod targets;

use crate::core_ui::engine_link::EngineHandle;
use gtk4::prelude::*;
use gtk4::{Box as GtkBox, Label, Orientation, Widget};
use std::rc::Rc;

use vauchi_contract::{
    ActionSpec, Event, PresentationNode, PresentationTokens, SurfaceId, SurfaceLayout, SurfaceSpec,
};

use super::accessibility;

/// Reports a rendered control's gesture back to whoever owns the engine.
///
/// The renderer calls this straight from the GTK signal handler, so the
/// callback must not rebuild this widget tree before returning — the shell's
/// implementation queues Core's turn to the next main-loop idle for exactly
/// that reason (`contextual_surface::widgets::dispatch_event`).
pub type OnEvent = Rc<dyn Fn(Event)>;

pub fn render(container: &GtkBox, surface: &SurfaceSpec, on_event: &OnEvent, link: &EngineHandle) {
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }
    crate::core_ui::theme::apply_tokens(&surface.tokens);
    apply_target_radius(&surface.tokens);
    let scrolled = gtk4::ScrolledWindow::builder()
        .vexpand(true)
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .vscrollbar_policy(if surface.layout == SurfaceLayout::Fixed {
            gtk4::PolicyType::Never
        } else {
            gtk4::PolicyType::Automatic
        })
        .build();
    let inner = GtkBox::new(
        Orientation::Vertical,
        i32::from(surface.tokens.spacing_medium),
    );
    let margin = i32::from(surface.tokens.spacing_large);
    inner.set_margin_start(margin);
    inner.set_margin_end(margin);
    inner.set_margin_bottom(margin);
    scrolled.set_child(Some(&inner));
    container.append(&scrolled);

    accessibility::apply_label(&inner, &surface.accessibility_label);

    let title = Label::builder()
        .label(&surface.title)
        .css_classes(["title-1"])
        .halign(gtk4::Align::Start)
        .build();
    title.set_widget_name("screen_title");
    inner.append(&title);
    if let Some(subtitle) = &surface.subtitle {
        inner.append(
            &Label::builder()
                .label(subtitle)
                .css_classes(["dim-label"])
                .halign(gtk4::Align::Start)
                .wrap(true)
                .build(),
        );
    }
    for node in &surface.nodes {
        inner.append(&render_node(
            node,
            &surface.surface_id,
            on_event,
            link,
            &surface.tokens,
        ));
    }
}

/// Applies Core's per-surface corner radius as a CSS rule any widget can
/// opt into via `targets::TARGET_RADIUS_CLASS`. Re-applying on every render
/// works because a later provider at the same priority wins the cascade, so
/// a surface with a different `corner_radius` supersedes the last one rather
/// than needing to be torn down first.
fn apply_target_radius(tokens: &PresentationTokens) {
    let provider = gtk4::CssProvider::new();
    provider.load_from_data(&targets::target_radius_css(tokens));
    if let Some(display) = gtk4::gdk::Display::default() {
        gtk4::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}

pub(super) fn render_node(
    node: &PresentationNode,
    surface_id: &SurfaceId,
    on_event: &OnEvent,
    link: &EngineHandle,
    tokens: &PresentationTokens,
) -> Widget {
    match node {
        PresentationNode::Text { .. }
        | PresentationNode::Input { .. }
        | PresentationNode::Toggle { .. }
        | PresentationNode::Choice { .. }
        | PresentationNode::Confirmation { .. }
        | PresentationNode::Slider { .. }
        | PresentationNode::Progress { .. } => controls::render(node, surface_id, on_event, tokens),
        PresentationNode::Group { .. }
        | PresentationNode::List { .. }
        | PresentationNode::Image { .. }
        | PresentationNode::Status { .. }
        | PresentationNode::Qr { .. }
        | PresentationNode::Divider => {
            collections::render(node, surface_id, on_event, link, tokens)
        }
    }
}

pub(super) fn emit_activation(surface_id: &SurfaceId, action: &ActionSpec, on_event: &OnEvent) {
    on_event(Event::SurfaceActivated {
        surface_id: surface_id.clone(),
    });
    on_event(Event::ActionActivated {
        surface_id: surface_id.clone(),
        interaction_id: action.interaction_id.clone(),
    });
}

/// A button for any Core `ActionSpec`, sized to the surface's touch-target
/// floor and rounded to its corner radius so no call site has to repeat
/// either.
pub(super) fn action_button(
    action: &ActionSpec,
    surface_id: &SurfaceId,
    on_event: &OnEvent,
    tokens: &PresentationTokens,
) -> gtk4::Button {
    let button = gtk4::Button::builder()
        .label(&action.label)
        .sensitive(action.enabled)
        .build();
    button.set_widget_name(action.interaction_id.as_str());
    button.set_size_request(-1, targets::minimum_target_px(tokens));
    button.add_css_class(targets::TARGET_RADIUS_CLASS);
    accessibility::apply_label(&button, &action.accessibility_label);
    // `ActionTone` is `#[non_exhaustive]`, and the linked vauchi-core does
    // not yet carry `Serious` (core!feature/action-tone-serious, not merged
    // here) — the wildcard arm keeps this compiling against both today's
    // pin and a future one that adds it, without a name this build cannot
    // resolve.
    match action.tone {
        vauchi_contract::ActionTone::Destructive => button.add_css_class("destructive-action"),
        vauchi_contract::ActionTone::Standard => {}
        _ => {}
    }
    let action = action.clone();
    let surface_id = surface_id.clone();
    let on_event = on_event.clone();
    button.connect_clicked(move |_| emit_activation(&surface_id, &action, &on_event));
    button
}

/// Report a gesture on a binding that carries no value of its own —
/// submission and focus loss say *that* something happened, not what the
/// field now holds.
pub(super) fn emit_binding_gesture(event: Event, on_event: &OnEvent) {
    on_event(event);
}

pub(super) fn emit_value(
    surface_id: &SurfaceId,
    binding_id: &vauchi_contract::BindingId,
    value: vauchi_contract::InputValue,
    on_event: &OnEvent,
) {
    on_event(Event::SurfaceActivated {
        surface_id: surface_id.clone(),
    });
    on_event(Event::ValueChanged {
        surface_id: surface_id.clone(),
        binding_id: binding_id.clone(),
        value,
    });
}
