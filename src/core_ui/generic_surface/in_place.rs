// SPDX-FileCopyrightText: 2026 Mattia Egloff <mattia.egloff@pm.me>
// SPDX-License-Identifier: GPL-3.0-or-later

//! Same-revision replacements update the shown surface in place.
//!
//! A replacement with the surface id and revision already shown keeps the
//! surface's structure (contract `SurfaceSpec::revision`), so only nodes
//! whose content changed are rebuilt, in their slot, and an input the user
//! is typing in keeps its widget when only its value changed: the shell owns
//! the focused input's text (vauchi/private#1062). A rebuild would lose the
//! caret, selection, input-method composition, scroll position and the
//! screen reader's place.

use std::cell::RefCell;

use gtk4::prelude::*;
use gtk4::{Box as GtkBox, Label, Widget, glib};
use vauchi_contract::{PresentationNode, SurfaceSpec};

use super::{OnEvent, render_node};
use crate::core_ui::accessibility;
use crate::core_ui::engine_link::EngineHandle;

/// What one container shows, so the next replacement can be compared.
pub(super) struct Shown {
    container: glib::WeakRef<GtkBox>,
    surface: SurfaceSpec,
    inner: GtkBox,
    title: Label,
    subtitle: Option<Label>,
    nodes: Vec<Widget>,
}

thread_local! {
    static SHOWN: RefCell<Vec<Shown>> = const { RefCell::new(Vec::new()) };
}

pub(super) fn remember(
    container: &GtkBox,
    surface: &SurfaceSpec,
    inner: GtkBox,
    title: Label,
    subtitle: Option<Label>,
    nodes: Vec<Widget>,
) {
    SHOWN.with(|shown| {
        let mut shown = shown.borrow_mut();
        shown.retain(|entry| {
            entry
                .container
                .upgrade()
                .is_some_and(|other| &other != container)
        });
        shown.push(Shown {
            container: container.downgrade(),
            surface: surface.clone(),
            inner,
            title,
            subtitle,
            nodes,
        });
    });
}

/// Applies `surface` to what `container` already shows when the contract
/// lets it; `false` when the caller has to rebuild.
pub(super) fn update(
    container: &GtkBox,
    surface: &SurfaceSpec,
    on_event: &OnEvent,
    link: &EngineHandle,
) -> bool {
    SHOWN.with(|shown| {
        let mut shown = shown.borrow_mut();
        let Some(entry) = shown
            .iter_mut()
            .find(|entry| entry.container.upgrade().as_ref() == Some(container))
        else {
            return false;
        };
        if !same_structure(&entry.surface, surface) || !entry.inner.is_ancestor(container) {
            return false;
        }
        entry.title.set_label(&surface.title);
        if let (Some(label), Some(subtitle)) = (&entry.subtitle, &surface.subtitle) {
            label.set_label(subtitle);
        }
        accessibility::apply_label(&entry.inner, &surface.accessibility_label);
        for (index, node) in surface.nodes.iter().enumerate() {
            let previous = &entry.surface.nodes[index];
            if previous == node || only_the_typed_value_differs(previous, node, &entry.nodes[index])
            {
                continue;
            }
            let replacement =
                render_node(node, &surface.surface_id, on_event, link, &surface.tokens);
            let old = &entry.nodes[index];
            entry.inner.insert_child_after(&replacement, Some(old));
            entry.inner.remove(old);
            entry.nodes[index] = replacement;
        }
        entry.surface = surface.clone();
        drop_trailing_widgets(container, &entry.inner);
        true
    })
}

/// The caller appends what follows the surface (the primary button) after
/// every render; the kept surface must not end up with two.
fn drop_trailing_widgets(container: &GtkBox, inner: &GtkBox) {
    let mut child = container.first_child();
    while let Some(widget) = child {
        child = widget.next_sibling();
        if !inner.is_ancestor(&widget) {
            container.remove(&widget);
        }
    }
}

/// The contract's promise covers the nodes; the frame around them (layout,
/// tokens, whether there is a subtitle) is compared too, since the widgets
/// built from it are kept.
fn same_structure(shown: &SurfaceSpec, next: &SurfaceSpec) -> bool {
    shown.surface_id == next.surface_id
        && shown.revision == next.revision
        && shown.layout == next.layout
        && shown.tokens == next.tokens
        && shown.subtitle.is_some() == next.subtitle.is_some()
        && shown.nodes.len() == next.nodes.len()
}

/// An input whose only change is its value, while the user is typing in it,
/// and whose widget already shows that value: it is the one the shell
/// reported. A value Core changed itself (normalised, cleared) still renders.
fn only_the_typed_value_differs(
    previous: &PresentationNode,
    next: &PresentationNode,
    widget: &Widget,
) -> bool {
    let (
        PresentationNode::Input { value: before, .. },
        PresentationNode::Input { value: after, .. },
    ) = (previous, next)
    else {
        return false;
    };
    if before == after
        || !widget
            .state_flags()
            .contains(gtk4::StateFlags::FOCUS_WITHIN)
    {
        return false;
    }
    if shown_text(widget).as_deref() != Some(after.as_str()) {
        return false;
    }
    let mut with_previous_value = next.clone();
    if let PresentationNode::Input { value, .. } = &mut with_previous_value {
        value.clone_from(before);
    }
    &with_previous_value == previous
}

fn shown_text(widget: &Widget) -> Option<String> {
    if let Some(editable) = widget.downcast_ref::<gtk4::Editable>() {
        return Some(editable.text().to_string());
    }
    let mut child = widget.first_child();
    while let Some(candidate) = child {
        if let Some(text) = shown_text(&candidate) {
            return Some(text);
        }
        child = candidate.next_sibling();
    }
    None
}
