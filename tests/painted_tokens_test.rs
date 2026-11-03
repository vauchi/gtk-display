// SPDX-FileCopyrightText: 2026 Mattia Egloff <mattia.egloff@pm.me>
// SPDX-License-Identifier: GPL-3.0-or-later

//! Widgets drawn outside a surface (the window's content margin, the QR
//! paste dialog, overlay items) take their spacing and touch-target floor
//! from the tokens of the last surface painted, not from a theme of the
//! renderer's own (ADR-078 step 4d.7, private#1029).

use vauchi_contract::PresentationTokens;
use vauchi_contract::fixtures::presentation_contract_v1_json;
use vauchi_gtk::core_ui::theme::{apply_tokens, painted_tokens};

fn contract_tokens() -> PresentationTokens {
    let fixture: serde_json::Value =
        serde_json::from_str(presentation_contract_v1_json()).expect("contract fixture");
    serde_json::from_value(
        fixture["initial_commands"][0]["ReplaceSurface"]["surface"]["tokens"].clone(),
    )
    .expect("surface tokens")
}

// @internal — the renderer keeps no tokens of its own (ADR-038, ADR-078).
#[test]
fn nothing_is_painted_before_the_first_surface() {
    assert_eq!(painted_tokens(), None);
}

// @internal — the renderer keeps no tokens of its own (ADR-038, ADR-078).
#[test]
fn the_last_painted_surface_supplies_the_tokens() {
    let first = contract_tokens();
    let mut second = contract_tokens();
    second.spacing_large = first.spacing_large + 8;
    second.minimum_target_size = first.minimum_target_size + 4;

    apply_tokens(&first);
    assert_eq!(painted_tokens(), Some(first));
    apply_tokens(&second);
    assert_eq!(painted_tokens(), Some(second));
}
