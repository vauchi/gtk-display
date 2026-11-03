// SPDX-FileCopyrightText: 2026 Mattia Egloff <mattia.egloff@pm.me>
// SPDX-License-Identifier: GPL-3.0-or-later

//! Core's per-surface `PresentationTokens` carry the touch-target floor and
//! corner radius across every shell, including the C ABI ones — but this
//! shell read neither before this module: buttons, rows, toggles and the
//! standalone avatar node sized and rounded themselves from nothing at all.

use vauchi_contract::PresentationTokens;

/// The touch-target floor as a GTK size-request value.
pub(crate) fn minimum_target_px(tokens: &PresentationTokens) -> i32 {
    i32::from(tokens.minimum_target_size)
}

/// Shared class name for the rule `target_radius_css` generates, so every
/// sized widget opts in with one `add_css_class` call.
pub(crate) const TARGET_RADIUS_CLASS: &str = "vauchi-target-radius";

/// CSS rounding the touch targets a `SurfaceSpec` asked for. A dedicated
/// class rather than baking the radius into each widget's own style: Core
/// sets `corner_radius` per surface, and the same button/row chrome is
/// reused across surfaces that may ask for different values.
pub(crate) fn target_radius_css(tokens: &PresentationTokens) -> String {
    format!(
        ".{TARGET_RADIUS_CLASS} {{ border-radius: {radius}px; }}",
        radius = tokens.corner_radius,
    )
}

// INLINE_TEST_REQUIRED: tests exercise private-to-the-crate helpers with no
// public accessor, and cannot be reached through a widget without a display.
#[cfg(test)]
mod tests {
    use super::*;

    fn tokens(minimum_target_size: u16, corner_radius: u16) -> PresentationTokens {
        let fixture: serde_json::Value =
            serde_json::from_str(vauchi_app::ui::presentation_contract_fixture_json()).unwrap();
        let mut tokens: PresentationTokens = serde_json::from_value(
            fixture["initial_commands"][0]["ReplaceSurface"]["surface"]["tokens"].clone(),
        )
        .unwrap();
        tokens.minimum_target_size = minimum_target_size;
        tokens.corner_radius = corner_radius;
        tokens
    }

    // @internal
    #[test]
    fn minimum_target_px_carries_cores_value_in_gtk_units() {
        assert_eq!(minimum_target_px(&tokens(44, 8)), 44);
        assert_eq!(minimum_target_px(&tokens(48, 8)), 48);
    }

    // @internal
    #[test]
    fn target_radius_css_names_the_shared_class_and_cores_radius() {
        let css = target_radius_css(&tokens(44, 12));

        assert_eq!(css, ".vauchi-target-radius { border-radius: 12px; }");
    }
}
