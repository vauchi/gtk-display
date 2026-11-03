// SPDX-FileCopyrightText: 2026 Mattia Egloff <mattia.egloff@pm.me>
// SPDX-License-Identifier: GPL-3.0-or-later

//! Maps Core's prepared accessibility copy onto GTK widgets (ADR-066).

use gtk4::accessible::{Property, Relation, State};
use gtk4::prelude::*;
use gtk4::{Accessible, AccessibleInvalidState, AccessibleRelation, AccessibleRole};

use vauchi_contract::AccessibilitySpec;

pub(crate) fn apply(widget: &impl IsA<Accessible>, accessibility: &AccessibilitySpec) {
    make_nameable(widget);
    let mut properties = vec![Property::Label(&accessibility.label)];
    if let Some(description) = accessibility.description.as_deref() {
        properties.push(Property::Description(description));
    }
    widget.update_property(&properties);
    detach_painted_label(widget);
}

pub(crate) fn apply_label(widget: &impl IsA<Accessible>, label: &str) {
    make_nameable(widget);
    widget.update_property(&[Property::Label(label)]);
    detach_painted_label(widget);
}

/// Announce a rejected field as invalid and point at the message describing it.
///
/// WCAG 2.1 SC 3.3.1 needs the error programmatically associated with the
/// field; a sibling label carries no association at all.
pub(crate) fn mark_invalid(field: &impl IsA<Accessible>, message: &impl IsA<Accessible>) {
    field.update_state(&[State::Invalid(AccessibleInvalidState::True)]);
    field.update_relation(&[Relation::ErrorMessage(&[message.upcast_ref()])]);
}

/// Give a plain container a role that is allowed to carry a name.
///
/// A bare `GtkBox` defaults to the `generic` role, and ARIA — which GTK4
/// follows — prohibits naming `generic`, so the label is accepted and then
/// silently dropped on the bus. `group` is the nameable equivalent.
fn make_nameable(widget: &impl IsA<Accessible>) {
    if widget.accessible_role() == AccessibleRole::Generic {
        widget.set_accessible_role(AccessibleRole::Group);
    }
}

/// Stop a widget's own visible label from supplying its accessible name.
///
/// GTK4 resolves the `labelled-by` relation ahead of the explicit label
/// property, so a button or check button built with a label child announces
/// its painted copy and Core's `accessibility_label` never reaches the bus —
/// the call looks correct in the source and is inert in practice
/// (`problems/2026-08-21-linux-shells-drop-core-a11y`).
fn detach_painted_label(widget: &impl IsA<Accessible>) {
    widget.reset_relation(AccessibleRelation::LabelledBy);
}
