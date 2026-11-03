// SPDX-FileCopyrightText: 2026 Mattia Egloff <mattia.egloff@pm.me>
// SPDX-License-Identifier: GPL-3.0-or-later

//! The renderer replays the contract's fixture corpus with no engine behind
//! it (ADR-078 D-11): every command batch the contract ships is accepted by
//! GTK's presentation state and leaves exactly the state the corpus expects.

use vauchi_contract::fixtures::{presentation_contract_v1_json, screen_catalog_v1_json};
use vauchi_contract::{Command, Event};
use vauchi_gtk::core_ui::contextual_surface::GtkPresentationState;

fn corpus(json: &str) -> serde_json::Value {
    let corpus: serde_json::Value = serde_json::from_str(json).expect("corpus is JSON");
    assert_eq!(corpus["schema_version"], 1);
    corpus
}

fn commands(value: &serde_json::Value) -> Vec<Command> {
    serde_json::from_value(value.clone()).expect("corpus commands decode")
}

// @scenario: generic_presentation_protocol.feature :: Every shell renders the same prepared presentation
#[test]
fn the_presentation_contract_reaches_the_expected_state() {
    let fixture = corpus(presentation_contract_v1_json());

    let mut state = GtkPresentationState::default();
    for command in commands(&fixture["initial_commands"]) {
        assert!(state.apply(command), "initial command must be accepted");
    }
    for step in fixture["steps"].as_array().expect("fixture steps") {
        let _: Event = serde_json::from_value(step["event"].clone()).expect("fixture event");
        for command in commands(&step["commands"]) {
            assert!(state.apply(command), "step command must be accepted");
        }
    }

    let surface = state.surface().expect("visible surface");
    let (_, context_bar) = state.context_bar().expect("visible context bar");
    let actual = serde_json::json!({
        "active_surface_id": surface.surface_id,
        "surface": surface,
        "context_bar": context_bar,
    });
    assert_eq!(actual, fixture["expected_state"]);
}

// @scenario: generic_presentation_protocol.feature :: Every shell renders the same prepared presentation
#[test]
fn every_catalog_screen_is_held_as_sent_from_a_cold_state() {
    let catalog = corpus(screen_catalog_v1_json());
    let screens = catalog["screens"].as_array().expect("catalog screens");
    assert!(!screens.is_empty(), "the catalog ships screens");

    for screen in screens {
        let code_id = screen["code_id"].as_str().expect("screen code id");
        let batch = commands(&screen["commands"]);
        let mut state = GtkPresentationState::default();
        for command in batch.clone() {
            assert!(state.apply(command), "{code_id}: command must be accepted");
        }

        let mut replaced = 0;
        for command in &batch {
            let Command::ReplaceSurface { surface } = command else {
                continue;
            };
            replaced += 1;
            assert_eq!(
                state.surface_by_id(&surface.surface_id),
                Some(surface),
                "{code_id}: surface {:?} is held as sent",
                surface.surface_id
            );
        }
        assert!(replaced > 0, "{code_id}: the screen shows a surface");
        assert!(
            state.context_bar().is_some(),
            "{code_id}: the shown surface has its context bar"
        );
    }
}
