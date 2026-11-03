// SPDX-FileCopyrightText: 2026 Mattia Egloff <mattia.egloff@pm.me>
// SPDX-License-Identifier: GPL-3.0-or-later

//! Generic presentation-node wire contracts consumed by the GTK renderer.

use vauchi_contract::{
    AccessibilitySpec, ActionSpec, ActionTone, BindingId, InteractionId, PresentationNode,
    PresentationRow, PresentationTone,
};

fn action(id: &str, label: &str) -> ActionSpec {
    ActionSpec {
        interaction_id: InteractionId::new(id).expect("interaction id"),
        label: label.into(),
        accessibility_label: label.into(),
        icon_token: None,
        enabled: true,
        tone: ActionTone::Standard,
        shortcut: None,
    }
}

#[test]
fn actionable_status_round_trips_without_domain_interpretation() {
    let original = PresentationNode::Status {
        id: Some(BindingId::new("status").expect("binding id")),
        title: "Ready".into(),
        detail: Some("Updated now".into()),
        icon_token: Some("status.ready".into()),
        badge: None,
        tone: PresentationTone::Success,
        activation: Some(action("refresh", "Refresh")),
        accessibility: AccessibilitySpec::label("Ready, updated now"),
    };

    let json = serde_json::to_string(&original).expect("serialize status");
    let decoded: PresentationNode = serde_json::from_str(&json).expect("decode status");
    assert_eq!(decoded, original);
}

#[test]
fn list_rows_keep_primary_and_secondary_actions_opaque() {
    let original = PresentationNode::List {
        id: BindingId::new("items").expect("binding id"),
        label: Some("Items".into()),
        style: vauchi_contract::PresentationListStyle::Rows,
        rows: vec![PresentationRow {
            title: "One".into(),
            subtitle: None,
            detail: None,
            icon_token: None,
            image_data: None,
            fallback_text: None,
            selected: false,
            enabled: true,
            activation: Some(action("open.one", "Open")),
            secondary_actions: vec![action("more.one", "More")],
            controls: Vec::new(),
            accessibility: AccessibilitySpec::label("One"),
            info: None,
        }],
        searchable: false,
        paging: None,
        accessibility: AccessibilitySpec::label("Items"),
    };

    let json = serde_json::to_string(&original).expect("serialize list");
    let decoded: PresentationNode = serde_json::from_str(&json).expect("decode list");
    assert_eq!(decoded, original);
}
