// SPDX-FileCopyrightText: 2026 Mattia Egloff <mattia.egloff@pm.me>
// SPDX-License-Identifier: GPL-3.0-or-later

//! Paints the colours Core sends with each surface as a GTK4 stylesheet.
//!
//! Core resolves the picked theme, "follow system" and contrast; the shell
//! keeps no theme of its own (#814).

use gtk4::CssProvider;
use gtk4::gdk::Display;
use std::cell::RefCell;
use vauchi_contract::{PresentationColors, PresentationFonts, PresentationTokens};

thread_local! {
    /// One provider, reloaded in place: every surface carries colours, so
    /// adding a provider per render would stack them for the app's lifetime.
    static THEME_PROVIDER: RefCell<Option<(CssProvider, String)>> = const { RefCell::new(None) };
    static PAINTED_TOKENS: RefCell<Option<PresentationTokens>> = const { RefCell::new(None) };
}

/// The tokens of the last surface painted. Widgets outside a surface (the
/// window's content margin, dialogs, overlay items) take their spacing and
/// touch-target floor from them; the renderer keeps no tokens of its own.
#[must_use]
pub fn painted_tokens() -> Option<PresentationTokens> {
    PAINTED_TOKENS.with(|slot| slot.borrow().clone())
}

/// Paint the colours and font families Core sends with a surface on the
/// default display. A surface whose tokens match the last ones painted
/// costs nothing.
pub fn apply_tokens(tokens: &PresentationTokens) {
    PAINTED_TOKENS.with(|slot| *slot.borrow_mut() = Some(tokens.clone()));
    let css = generate_css(&tokens.colors, tokens.fonts.as_ref());
    THEME_PROVIDER.with(|slot| {
        let mut slot = slot.borrow_mut();
        if let Some((provider, painted)) = slot.as_mut() {
            if *painted != css {
                provider.load_from_data(&css);
                *painted = css;
            }
            return;
        }
        let Some(display) = Display::default() else {
            return;
        };
        let provider = CssProvider::new();
        provider.load_from_data(&css);
        gtk4::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
        *slot = Some((provider, css));
    });
}

/// Generate a GTK4 CSS string from Core's `PresentationColors` and
/// `PresentationFonts`. Without fonts no family is named and GTK's own
/// font applies.
///
/// Named colours (`@define-color vauchi_*`) let components reference a
/// role; the widget selectors below paint libadwaita's own widgets.
fn generate_css(colors: &PresentationColors, fonts: Option<&PresentationFonts>) -> String {
    let family = |pick: fn(&PresentationFonts) -> &str| {
        fonts
            .map(|fonts| format!("\n    font-family: \"{}\";", css_string(pick(fonts))))
            .unwrap_or_default()
    };
    format!(
        r#"
/* Vauchi theme — generated from the colours Core sends with each surface */
@define-color vauchi_bg_primary {bg_primary};
@define-color vauchi_bg_secondary {bg_secondary};
@define-color vauchi_bg_tertiary {bg_tertiary};
@define-color vauchi_text_primary {text_primary};
@define-color vauchi_text_secondary {text_secondary};
@define-color vauchi_accent {accent};
@define-color vauchi_text_on_accent {text_on_accent};
@define-color vauchi_success {success};
@define-color vauchi_error {error};
@define-color vauchi_warning {warning};
@define-color vauchi_border {border};
@define-color vauchi_focus_ring {focus_ring};

window {{
    background-color: @vauchi_bg_primary;
    color: @vauchi_text_primary;{body_font}
}}

/* libadwaita grounds the split view's `.sidebar-pane` but leaves the
   `.content-pane` transparent, and its buttons, entries and cards fill
   with `alpha(currentColor, .1)` — a tint of the *text* colour. Under a
   dark theme the text is light, so wherever nothing opaque lies beneath
   (the offscreen catalog capture; any pane drawn before the window) the
   controls come out near-white behind light text. Every surface therefore
   carries its own opaque theme ground. `entry` covers `SearchEntry` too:
   both share the `entry` CSS node. */
.content-pane {{
    background-color: @vauchi_bg_primary;
    color: @vauchi_text_primary;
}}

button {{
    background-color: @vauchi_bg_tertiary;
    color: @vauchi_text_primary;
}}

/* The `button` ground above outranks libadwaita's own `:checked` fill, so
   without this the active segment of a toggle group looked idle. */
button:checked {{
    background-color: @vauchi_accent;
    color: @vauchi_text_on_accent;
}}

entry {{
    background-color: @vauchi_bg_tertiary;
    color: @vauchi_text_primary;
    border-color: @vauchi_border;
}}

.suggested-action {{
    background-color: @vauchi_accent;
    color: @vauchi_text_on_accent;
}}

.destructive-action {{
    background-color: @vauchi_error;
    color: @vauchi_bg_primary;
}}

/* `ActionTone::Serious` (verify a fingerprint, schedule a deletion, start
   recovery) is consequential but reversible or protective: an outline, not
   a fill, so it reads as neither the safe default nor "this destroys
   something". */
.serious-action {{
    background-color: transparent;
    color: @vauchi_warning;
    border: 2px solid @vauchi_warning;
}}

/* GTK's own `:focus-visible` pseudo-class carries keyboard focus; nothing
   here answered it before, so a keyboard user had no way to see which
   control had focus. `checkbutton` covers Toggle nodes; `.vauchi-row`
   covers the row containers built in `generic_surface/collections.rs`,
   which are plain boxes with no built-in focus styling of their own. */
button:focus-visible,
checkbutton:focus-visible,
.vauchi-row:focus-visible {{
    outline: 3px solid @vauchi_focus_ring;
    outline-offset: 2px;
}}

.dim-label {{
    color: @vauchi_text_secondary;
}}

.error {{
    color: @vauchi_error;
}}

.card {{
    border-color: @vauchi_border;
    background-color: @vauchi_bg_secondary;
}}

/* Core's PresentationTextStyle::Heading maps to GTK's standard "title-2"
   style class (generic_surface/controls.rs); ::Monospace maps to GTK's
   built-in "monospace" class. Both get a brand typeface here. */
.title-2 {{{display_font}
    font-weight: 700;
}}

.monospace {{{mono_font}
}}

/* Initials stand in for a missing avatar. Without a ground to sit on they
   read as a stray letter rather than as a person, so the class carries the
   whole shape: a fixed square, a fill and a radius. `AdwAvatar` styling
   does not reach a plain Label, which is why this cannot be borrowed. */
.avatar {{
    min-width: 96px;
    min-height: 96px;
    border-radius: 48px;
    background-color: @vauchi_bg_tertiary;
    color: @vauchi_text_primary;
    font-size: 1.6em;
    font-weight: bold;
}}

/* A row's avatar is the same idea at list scale. */
row .avatar,
list .avatar {{
    min-width: 32px;
    min-height: 32px;
    border-radius: 16px;
    font-size: 1em;
}}

/* Core asks for a natural-shaped image to keep its corners. */
.avatar.natural {{
    border-radius: 8px;
}}

/* D4: the persistent navigation sidebar highlights Core's selected
   destination (`NavigationItem::selected`) so it reads as "here" rather
   than as one more row identical to the rest. */
.nav-sidebar-row.selected {{
    background-color: @vauchi_accent;
    color: @vauchi_text_on_accent;
}}

.nav-sidebar-badge {{
    background-color: @vauchi_accent;
    color: @vauchi_text_on_accent;
    border-radius: 999px;
    min-width: 1.4em;
    padding: 0 0.4em;
    font-size: 0.85em;
    font-weight: 600;
}}

/* A row's Status control is the row's state (who sees a card entry,
   vauchi/private#1206): a chip set apart from the value above it by
   shape and ground, in primary text so it is never the quietest line. */
.status-chip {{
    background-color: @vauchi_bg_tertiary;
    color: @vauchi_text_primary;
    border: 1px solid @vauchi_border;
    border-radius: 999px;
    padding: 0.1em 0.7em;
    font-size: 0.85em;
    font-weight: 600;
}}

.status-chip.success {{
    color: @vauchi_success;
}}

.status-chip.warning {{
    color: @vauchi_warning;
}}

.status-chip.error {{
    color: @vauchi_error;
}}

.status-chip.accent {{
    color: @vauchi_accent;
}}
"#,
        bg_primary = colors.background,
        bg_secondary = colors.surface,
        bg_tertiary = colors.surface_input.as_deref().unwrap_or(&colors.surface),
        text_primary = colors.text,
        text_secondary = colors.text_muted,
        accent = colors.accent,
        text_on_accent = colors
            .text_on_accent
            .as_deref()
            .unwrap_or(&colors.background),
        success = colors.success,
        error = colors.error,
        warning = colors.warning,
        border = colors.border,
        focus_ring = colors.focus_ring.as_deref().unwrap_or(&colors.accent),
        body_font = family(|fonts| &fonts.body),
        display_font = family(|fonts| &fonts.display),
        mono_font = family(|fonts| &fonts.mono),
    )
}

/// A font name from a theme file may hold any character; escaping the
/// backslash and the quote keeps it one CSS string, and control characters
/// (a newline ends a CSS string) are dropped.
fn css_string(name: &str) -> String {
    name.chars()
        .filter(|c| !c.is_control())
        .flat_map(|c| match c {
            '\\' | '"' => vec!['\\', c],
            c => vec![c],
        })
        .collect()
}

// INLINE_TEST_REQUIRED: tests exercise private `generate_css` function
#[cfg(test)]
mod tests {
    use super::*;

    /// Every role distinct, so a test can tell which role a rule took.
    fn colors() -> PresentationColors {
        PresentationColors {
            background: "#000001".to_string(),
            surface: "#000002".to_string(),
            text: "#000003".to_string(),
            text_muted: "#000004".to_string(),
            accent: "#000005".to_string(),
            border: "#000006".to_string(),
            success: "#000007".to_string(),
            warning: "#000008".to_string(),
            error: "#000009".to_string(),
            surface_input: Some("#00000a".to_string()),
            surface_muted: Some("#00000b".to_string()),
            surface_elevated: Some("#00000c".to_string()),
            text_on_accent: Some("#00000d".to_string()),
            focus_ring: Some("#00000e".to_string()),
        }
    }

    fn fonts() -> PresentationFonts {
        PresentationFonts {
            display: "Display Face".to_string(),
            body: "Body Face".to_string(),
            mono: "Mono Face".to_string(),
        }
    }

    fn css_for(colors: &PresentationColors) -> String {
        generate_css(colors, Some(&fonts()))
    }

    /// Both the standalone `Image` node and a list row's avatar hand their
    /// initials to a `Label` carrying the `avatar` class, but nothing ever
    /// defined that class — libadwaita's `avatar` styling belongs to
    /// `AdwAvatar`, not to any widget that borrows the name. So the class
    /// was inert and the initials rendered as a bare letter on the window
    /// background, which is the same defect `ios!651` fixed on Apple.
    // @internal
    #[test]
    fn generated_css_gives_the_avatar_class_a_body() {
        let css = css_for(&colors());

        assert!(
            css.contains(".avatar"),
            "no `.avatar` rule: initials render as a bare letter, since a \
             plain Label does not inherit AdwAvatar styling"
        );
        assert!(
            css.contains("border-radius"),
            "`.avatar` must round its ground, or the initials sit in a square"
        );
    }

    /// The shell paints the colours Core sends with each surface (#814);
    /// each of Core's roles fills the named colour the rules below use.
    // @internal
    #[test]
    fn generate_css_defines_each_named_colour_from_cores_role() {
        let css = css_for(&colors());

        for (name, value) in [
            ("vauchi_bg_primary", "#000001"),
            ("vauchi_bg_secondary", "#000002"),
            ("vauchi_text_primary", "#000003"),
            ("vauchi_text_secondary", "#000004"),
            ("vauchi_accent", "#000005"),
            ("vauchi_border", "#000006"),
            ("vauchi_success", "#000007"),
            ("vauchi_warning", "#000008"),
            ("vauchi_error", "#000009"),
            ("vauchi_bg_tertiary", "#00000a"),
            ("vauchi_text_on_accent", "#00000d"),
            ("vauchi_focus_ring", "#00000e"),
        ] {
            assert!(
                css.contains(&format!("@define-color {name} {value};")),
                "{name} should be {value}"
            );
        }
    }

    /// A theme may leave out the input surface; controls then sit on the
    /// surface colour, the nearest required role.
    // @internal
    #[test]
    fn generate_css_grounds_controls_on_the_surface_when_core_sends_no_input_surface() {
        let css = css_for(&PresentationColors {
            surface_input: None,
            ..colors()
        });

        assert!(css.contains("@define-color vauchi_bg_tertiary #000002;"));
    }

    /// A theme may leave out on-accent text; labels on the accent then use
    /// the background colour, as they did before Core sent the role.
    // @internal
    #[test]
    fn generate_css_writes_on_the_accent_in_the_background_colour_when_core_sends_none() {
        let css = css_for(&PresentationColors {
            text_on_accent: None,
            ..colors()
        });

        assert!(css.contains("@define-color vauchi_text_on_accent #000001;"));
    }

    #[test]
    fn generate_css_has_define_color_directives() {
        let css = css_for(&colors());

        assert!(
            css.contains("@define-color vauchi_bg_primary"),
            "CSS should define vauchi_bg_primary"
        );
        assert!(
            css.contains("@define-color vauchi_accent"),
            "CSS should define vauchi_accent"
        );
        assert!(
            css.contains("@define-color vauchi_error"),
            "CSS should define vauchi_error"
        );
    }

    #[test]
    fn generate_css_has_widget_selectors() {
        let css = css_for(&colors());

        assert!(css.contains("window {"), "CSS should style the window");
        assert!(
            css.contains(".suggested-action {"),
            "CSS should style suggested actions"
        );
        assert!(
            css.contains(".destructive-action {"),
            "CSS should style destructive actions"
        );
    }

    /// D4: the persistent navigation sidebar (`contextual_surface::sidebar`)
    /// marks Core's selected `NavigationItem` with a `.selected` class on
    /// its row; nothing painted that class, so every destination looked
    /// identical regardless of which one Core says is current.
    // @internal
    #[test]
    fn generate_css_highlights_the_selected_sidebar_row() {
        let css = css_for(&colors());

        assert!(
            css.contains(".nav-sidebar-row.selected"),
            "no rule for the selected sidebar row: the current destination \
             is indistinguishable from the rest of the list"
        );
    }

    /// `ActionTone::Serious` names a consequential-but-reversible action
    /// (verify a fingerprint, schedule a deletion, start recovery). It must
    /// read as neither the filled `.suggested-action` (the safe default) nor
    /// `.destructive-action` (irreversible), so the rule outlines in the
    /// warning colour and never fills with it.
    // @internal
    #[test]
    fn generate_css_styles_serious_action_as_a_warning_outline_with_no_fill() {
        let css = css_for(&colors());

        assert!(
            css.contains(".serious-action {"),
            "CSS should style serious actions"
        );
        assert!(
            !css.contains("background-color: @vauchi_warning"),
            "`.serious-action` must outline, not fill, or it reads as destructive"
        );
    }

    /// GTK draws keyboard focus through the `:focus-visible` pseudo-class;
    /// nothing in this stylesheet answered it before, so a keyboard user saw
    /// no indication of which button or row currently had focus.
    // @internal
    #[test]
    fn generate_css_draws_a_focus_ring_from_the_theme() {
        let css = css_for(&colors());

        assert!(
            css.contains(":focus-visible"),
            "CSS should draw a ring on keyboard focus"
        );
        assert!(
            css.contains("outline: 3px solid"),
            "focus ring should be a 3px outline"
        );
        assert!(
            css.contains("outline-offset: 2px"),
            "focus ring should sit clear of the widget edge"
        );
    }

    /// The theme's dedicated `focus-ring` role (ADR-038 Amendment 3) wins
    /// when a theme supplies one — falling back to `accent` is only for
    /// themes predating that amendment.
    // @internal
    #[test]
    fn generate_css_focus_ring_prefers_the_dedicated_theme_role_over_accent() {
        let css = css_for(&PresentationColors {
            accent: "#0000ff".to_string(),
            focus_ring: Some("#ff00ff".to_string()),
            ..colors()
        });

        assert!(
            css.contains("@define-color vauchi_focus_ring #ff00ff;"),
            "focus ring should use the theme's dedicated focus-ring colour"
        );
    }

    /// Themes from before ADR-038 Amendment 3 have no `focus-ring` role at
    /// all; the ring must still draw, using `accent` so it stays visible.
    // @internal
    #[test]
    fn generate_css_focus_ring_falls_back_to_accent_when_theme_has_none() {
        let css = css_for(&PresentationColors {
            accent: "#0000ff".to_string(),
            focus_ring: None,
            ..colors()
        });

        assert!(
            css.contains("@define-color vauchi_focus_ring #0000ff;"),
            "focus ring should fall back to accent when the theme has no focus-ring role"
        );
    }

    /// The CSS block that follows `selector` (up to its closing brace), or
    /// an empty string when no rule opens with exactly that selector line.
    fn rule_body<'a>(css: &'a str, selector: &str) -> &'a str {
        let open = format!("\n{selector} {{");
        css.find(&open)
            .map(|start| {
                let body = &css[start + open.len()..];
                &body[..body.find('}').unwrap_or(body.len())]
            })
            .unwrap_or("")
    }

    /// libadwaita paints the split view's `.sidebar-pane` but leaves the
    /// `.content-pane` transparent; its buttons, entries and cards fill with
    /// `alpha(currentColor, .1)`. Under a dark theme the window ground is
    /// dark, so the transparent pane and its near-transparent controls read
    /// as white wherever nothing sits beneath them (the offscreen catalog
    /// capture) while the text stays light-on-dark — every row invisible.
    /// Each surface therefore takes its own opaque theme ground.
    // @internal
    #[test]
    fn generate_css_grounds_the_content_pane_and_its_controls_in_theme_tokens() {
        let css = css_for(&colors());

        for (selector, token) in [
            (".content-pane", "@vauchi_bg_primary"),
            (".card", "@vauchi_bg_secondary"),
            ("button", "@vauchi_bg_tertiary"),
            ("entry", "@vauchi_bg_tertiary"),
        ] {
            let body = rule_body(&css, selector);
            assert!(
                body.contains(&format!("background-color: {token}")),
                "`{selector}` must take its ground from {token}, or under a dark \
                 theme it stays libadwaita-transparent and reads as white: {body:?}"
            );
        }
        for selector in [".content-pane", "button", "entry"] {
            assert!(
                rule_body(&css, selector).contains("color: @vauchi_text_primary"),
                "`{selector}` must pair its ground with @vauchi_text_primary so \
                 the text stays readable on it"
            );
        }
    }

    /// The `button` ground above is applied at application priority, which
    /// also beats libadwaita's own `button:checked` fill — so a toggle group
    /// (the segmented Choice control) painted its active segment exactly
    /// like the idle ones and the reader could not tell which was chosen.
    // @internal
    #[test]
    fn generate_css_fills_a_checked_button_with_the_accent() {
        let css = css_for(&colors());

        let body = rule_body(&css, "button:checked");
        assert!(
            body.contains("background-color: @vauchi_accent"),
            "`button:checked` must take the accent ground so the active \
             segment of a toggle group stands out from the idle ones: {body:?}"
        );
        assert!(
            body.contains("color: @vauchi_text_on_accent"),
            "`button:checked` must pair the accent ground with Core's \
             on-accent text so the label stays readable on it: {body:?}"
        );
    }

    /// A row's `Status` control (who sees a card entry, vauchi/private#1206)
    /// is set apart from the value above it by shape and ground, not by
    /// dimming — and its text stays the primary colour so it is never the
    /// quietest thing in the row.
    // @internal
    #[test]
    fn generate_css_shapes_a_status_chip_on_the_tertiary_ground() {
        let css = css_for(&colors());

        let body = rule_body(&css, ".status-chip");
        assert!(
            body.contains("background-color: @vauchi_bg_tertiary"),
            "`.status-chip` takes the tertiary ground: {body:?}"
        );
        assert!(
            body.contains("color: @vauchi_text_primary"),
            "`.status-chip` keeps primary text, it is a state, not a footnote: {body:?}"
        );
        assert!(
            body.contains("border-radius: 999px"),
            "`.status-chip` is a pill: {body:?}"
        );
        for (tone, colour) in [
            ("success", "@vauchi_success"),
            ("warning", "@vauchi_warning"),
            ("error", "@vauchi_error"),
            ("accent", "@vauchi_accent"),
        ] {
            let toned = rule_body(&css, &format!(".status-chip.{tone}"));
            assert!(
                toned.contains(&format!("color: {colour}")),
                "`.status-chip.{tone}` colours its text with {colour}: {toned:?}"
            );
        }
    }

    /// `PresentationTextStyle::Heading` maps to the `title-2` GTK style
    /// class (`generic_surface/controls.rs`) and `::Monospace` maps to
    /// GTK's built-in `monospace` class — the CSS must give both a brand
    /// typeface, and the base `window` rule carries the body family for
    /// everything that inherits from it.
    // @internal
    #[test]
    fn generate_css_sets_the_font_families_core_sends_with_the_surface() {
        let css = css_for(&colors());

        assert!(
            rule_body(&css, "window").contains(r#"font-family: "Body Face""#),
            "window rule should set the body family for base widgets"
        );
        let heading = rule_body(&css, ".title-2");
        assert!(
            heading.contains(r#"font-family: "Display Face""#),
            "`.title-2` rule should set the display family for headings"
        );
        assert!(
            heading.contains("font-weight: 700"),
            "`.title-2` rule should use the display weight for headings"
        );
        assert!(
            rule_body(&css, ".monospace").contains(r#"font-family: "Mono Face""#),
            "`.monospace` rule should set the mono family"
        );
    }

    // @internal
    #[test]
    fn generate_css_leaves_the_fonts_to_gtk_when_core_sends_none() {
        let css = generate_css(&colors(), None);

        assert!(
            !css.contains("font-family"),
            "a batch without a font block must not name a family"
        );
        assert!(
            rule_body(&css, ".title-2").contains("font-weight: 700"),
            "headings keep their weight without a family"
        );
    }

    // @internal
    #[test]
    fn generate_css_keeps_a_quote_in_a_font_name_inside_its_css_string() {
        let fonts = PresentationFonts {
            display: "Display".to_string(),
            body: "Evil\"; color: red; x: \"".to_string(),
            mono: "Mono".to_string(),
        };

        let css = generate_css(&colors(), Some(&fonts));

        assert!(
            rule_body(&css, "window").contains(r#"font-family: "Evil\"; color: red; x: \"";"#),
            "the quote must be escaped so the name stays one CSS string"
        );
    }
}
