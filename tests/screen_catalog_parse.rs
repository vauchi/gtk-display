// SPDX-FileCopyrightText: 2026 Mattia Egloff <mattia.egloff@pm.me>
// SPDX-License-Identifier: GPL-3.0-or-later

//! Core's screen catalog and the contract's command batches both decode
//! into render jobs. A code id must never escape the output directory.

use vauchi_contract::Command;
use vauchi_contract::fixtures::presentation_contract_v1_json;
use vauchi_gtk::screen_catalog::{CatalogSource, load_screens};

fn batch_json() -> serde_json::Value {
    let contract: serde_json::Value =
        serde_json::from_str(presentation_contract_v1_json()).unwrap();
    contract["initial_commands"].clone()
}

// @internal
#[test]
fn load_screens_reads_the_catalog_shape() {
    let json = serde_json::json!({
        "schema_version": 1,
        "screens": [
            {"code_id": "contacts", "title": "Contacts", "locale": "en", "commands": batch_json()},
            {"code_id": "my-card.v2", "title": "My Card", "locale": "en", "commands": batch_json()},
        ],
    });

    let loaded = load_screens(&json.to_string()).expect("catalog shape decodes");

    assert_eq!(loaded.source, CatalogSource::ScreenCatalog);
    let stems: Vec<&str> = loaded.screens.iter().map(|s| s.code_id.as_str()).collect();
    assert_eq!(stems, ["contacts", "my-card.v2"]);
    assert!(
        loaded.screens[0]
            .commands
            .iter()
            .any(|c| matches!(c, Command::ReplaceSurface { .. })),
        "commands must decode as vauchi_contract::Command, not stay opaque JSON"
    );
}

// @internal
#[test]
fn load_screens_falls_back_to_the_presentation_contract_batches() {
    let loaded = load_screens(presentation_contract_v1_json()).expect("contract shape decodes");

    assert_eq!(loaded.source, CatalogSource::PresentationContract);
    let stems: Vec<&str> = loaded.screens.iter().map(|s| s.code_id.as_str()).collect();
    assert_eq!(stems, ["initial", "step_1", "step_2"]);
    assert!(loaded.screens.iter().all(|s| !s.commands.is_empty()));
}

// @internal
#[test]
fn load_screens_rejects_code_ids_that_could_leave_the_output_directory() {
    for bad in ["../etc", "a/b", "", ".hidden", "with space"] {
        let json = serde_json::json!({
            "schema_version": 1,
            "screens": [{"code_id": bad, "commands": batch_json()}],
        });
        let err = load_screens(&json.to_string())
            .err()
            .unwrap_or_else(|| panic!("code_id {bad:?} must be rejected"));
        assert!(
            err.contains("code_id"),
            "error should name the field: {err}"
        );
    }
}

// @internal
#[test]
fn load_screens_rejects_duplicate_code_ids() {
    let json = serde_json::json!({
        "schema_version": 1,
        "screens": [
            {"code_id": "contacts", "commands": batch_json()},
            {"code_id": "contacts", "commands": batch_json()},
        ],
    });
    let err = load_screens(&json.to_string()).expect_err("duplicate stems would overwrite a PNG");
    assert!(err.contains("contacts"), "{err}");
}

// @internal
#[test]
fn load_screens_rejects_unknown_shapes() {
    let err = load_screens(r#"{"schema_version": 1}"#).expect_err("neither shape present");
    assert!(err.contains("screens"), "{err}");
}
