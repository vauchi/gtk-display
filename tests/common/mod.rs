// SPDX-FileCopyrightText: 2026 Mattia Egloff <mattia.egloff@pm.me>
// SPDX-License-Identifier: GPL-3.0-or-later

pub fn presentation_tokens() -> vauchi_contract::PresentationTokens {
    let fixture: serde_json::Value =
        serde_json::from_str(vauchi_contract::fixtures::presentation_contract_v1_json()).unwrap();
    serde_json::from_value(
        fixture["initial_commands"][0]["ReplaceSurface"]["surface"]["tokens"].clone(),
    )
    .unwrap()
}
