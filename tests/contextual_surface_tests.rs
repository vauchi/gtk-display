// SPDX-FileCopyrightText: 2026 Mattia Egloff <mattia.egloff@pm.me>
// SPDX-License-Identifier: GPL-3.0-or-later

mod common;

use vauchi_contract::{
    ActionSpec, ActionTone, Command, ContextBar, Event, InteractionId, MotionPreference,
    NavigationItem, NavigationSpec, OverlayKind, OverlaySpec, PaneLayout, PresentationProfile,
    PresentationTokens, StandardShortcut, SurfaceId, SurfaceLayout, SurfaceSpec, WindowClass,
};
use vauchi_gtk::core_ui::contextual_surface::{
    GtkContextRole, GtkControlArea, GtkOverlayTransition, GtkPresentationState, context_controls,
    context_controls_with, interaction_for_shortcut,
};

fn surface(id: &str) -> SurfaceId {
    SurfaceId::new(id).unwrap()
}

fn action(id: &str, shortcut: Option<StandardShortcut>) -> ActionSpec {
    ActionSpec {
        interaction_id: InteractionId::new(id).unwrap(),
        label: id.to_owned(),
        accessibility_label: id.to_owned(),
        icon_token: None,
        enabled: true,
        tone: ActionTone::Standard,
        shortcut,
    }
}

fn nav_item(id: &str, selected: bool, badge_count: u32) -> NavigationItem {
    NavigationItem {
        interaction_id: InteractionId::new(id).unwrap(),
        label: id.to_owned(),
        accessibility_label: id.to_owned(),
        icon_token: None,
        selected,
        badge_count,
    }
}

fn surface_spec(id: &str, revision: u64) -> SurfaceSpec {
    SurfaceSpec {
        surface_id: surface(id),
        revision,
        title: "Surface".into(),
        subtitle: None,
        accessibility_label: "Surface".into(),
        layout: SurfaceLayout::Scroll,
        tokens: PresentationTokens {
            spacing_small: 8,
            spacing_medium: 16,
            spacing_large: 24,
            corner_radius: 12,
            minimum_target_size: 44,
            ..common::presentation_tokens()
        },
        nodes: Vec::new(),
    }
}

/// Core's revision advances only on user actions, so racing full rebuilds
/// (wakeup re-load, invalidation dispatch) legitimately re-emit the same
/// surface at the same revision. Only a strictly older revision is stale.
///
/// This shell already behaves correctly. The test exists because two other
/// shells did not: Android and macOS both rejected an equal revision
/// (`vauchi/android!610`, `vauchi/macos!346`), and on Android that failed
/// every cold launch. Nothing pinned the behaviour here, so a future
/// tightening of `>=` to `>` would reintroduce it silently.
// @scenario: generic_presentation_protocol.feature :: Every shell renders the same prepared presentation
#[test]
fn re_emitted_same_revision_re_applies_instead_of_being_rejected() {
    let mut state = GtkPresentationState::default();
    state.apply(Command::ReplaceSurface {
        surface: surface_spec("contacts", 2),
    });

    let mut rebuilt = surface_spec("contacts", 2);
    rebuilt.title = "Rebuilt contacts".into();
    let replaced = state.apply(Command::ReplaceSurface {
        surface: rebuilt.clone(),
    });

    assert!(
        replaced,
        "a surface re-emitted at the same revision must re-apply, not be rejected"
    );
    assert_eq!(
        state.surface(),
        Some(&rebuilt),
        "last writer wins, so the rebuilt surface must be the live one"
    );
}

// @scenario: generic_presentation_protocol.feature :: Every shell renders the same prepared presentation
#[test]
fn strictly_older_revision_is_still_rejected() {
    let mut state = GtkPresentationState::default();
    state.apply(Command::ReplaceSurface {
        surface: surface_spec("contacts", 2),
    });

    let stale = surface_spec("contacts", 1);
    let replaced = state.apply(Command::ReplaceSurface { surface: stale });

    assert!(
        !replaced,
        "a strictly older revision must still be rejected"
    );
    assert_eq!(state.surface(), Some(&surface_spec("contacts", 2)));
}

// Core makes the context-bar menu buttons toggle by rewriting a repeat
// PresentOverlay into DismissOverlay. Every shell has to map it: Android and
// iOS dropped it into a generic effect and the menu never closed on a second
// tap (`vauchi/android!621`). GTK already scopes overlays per surface and
// drops them on replacement, so this arm is the only half it was missing.
// @internal
#[test]
fn dismiss_overlay_closes_the_open_overlay() {
    let mut state = GtkPresentationState::default();
    let profile = PresentationProfile {
        window_class: WindowClass::Expanded,
        pane_layout: PaneLayout::Single,
        primary_surface: surface("contacts"),
        detail_surface: None,
        active_surface: surface("contacts"),
        window_title: None,
    };
    let overlay = OverlaySpec {
        kind: OverlayKind::Navigation,
        title: Some("Navigate".into()),
        items: vec![action("opaque.target", None)],
        body: None,
        close_label: None,
    };

    state.apply(Command::ReplaceSurface {
        surface: surface_spec("contacts", 4),
    });
    state.apply(Command::SetPresentationProfile { profile });
    state.apply(Command::PresentOverlay {
        surface_id: surface("contacts"),
        revision: 4,
        overlay: overlay.clone(),
    });
    assert_eq!(state.overlay(), Some((&surface("contacts"), &overlay)));

    let handled = state.apply(Command::DismissOverlay {
        surface_id: surface("contacts"),
        revision: 4,
        kind: OverlayKind::Navigation,
    });

    assert!(handled, "DismissOverlay must be handled by the shell state");
    assert_eq!(
        state.overlay(),
        None,
        "a dismissed overlay must leave nothing to render",
    );
}

// @scenario: generic_presentation_protocol.feature :: Every shell renders the same prepared presentation
#[test]
fn command_state_keeps_core_bar_profile_and_overlay_data_opaque() {
    let mut state = GtkPresentationState::default();
    let bar = ContextBar {
        back: Some(action("back", Some(StandardShortcut::Back))),
        navigation: Some(action("navigate", None)),
        primary: Some(action("save", Some(StandardShortcut::ActivatePrimary))),
        secondary: Some(action("more", None)),
        info: None,
    };
    let profile = PresentationProfile {
        window_class: WindowClass::Expanded,
        pane_layout: PaneLayout::Single,
        primary_surface: surface("contacts"),
        detail_surface: None,
        active_surface: surface("contacts"),
        window_title: None,
    };
    let overlay = OverlaySpec {
        kind: OverlayKind::Navigation,
        title: Some("Navigate".into()),
        items: vec![action("opaque.target", None)],
        body: None,
        close_label: None,
    };

    let mut surface_spec = surface_spec("contacts", 4);
    surface_spec.title = "Contacts".into();
    state.apply(Command::ReplaceSurface {
        surface: surface_spec.clone(),
    });
    state.apply(Command::SetContextBar {
        surface_id: surface("contacts"),
        revision: 4,
        bar: Box::new(bar.clone()),
    });
    state.apply(Command::SetPresentationProfile {
        profile: profile.clone(),
    });
    state.apply(Command::PresentOverlay {
        surface_id: surface("contacts"),
        revision: 4,
        overlay: overlay.clone(),
    });
    let mut stale_surface = surface_spec.clone();
    stale_surface.revision = 3;
    stale_surface.title = "Stale contacts".into();
    state.apply(Command::ReplaceSurface {
        surface: stale_surface,
    });
    assert!(!state.apply(Command::SetContextBar {
        surface_id: surface("contacts"),
        revision: 3,
        bar: Box::new(ContextBar::default()),
    }));

    assert_eq!(state.context_bar(), Some((&surface("contacts"), &bar)));
    assert_eq!(state.profile(), Some(&profile));
    assert_eq!(state.overlay(), Some((&surface("contacts"), &overlay)));
    assert_eq!(state.surface(), Some(&surface_spec));
}

// @scenario: generic_presentation_protocol.feature :: Interaction activates its visible pane first
#[test]
fn activation_orders_surface_activation_before_opaque_interaction() {
    let mut state = GtkPresentationState::default();
    state.apply(Command::ReplaceSurface {
        surface: surface_spec("detail", 1),
    });
    state.apply(Command::SetContextBar {
        surface_id: surface("detail"),
        revision: 1,
        bar: Box::new(ContextBar {
            primary: Some(action("save", Some(StandardShortcut::ActivatePrimary))),
            ..ContextBar::default()
        }),
    });

    assert_eq!(
        state.activation_events(&InteractionId::new("save").unwrap()),
        vec![
            Event::SurfaceActivated {
                surface_id: surface("detail"),
            },
            Event::ActionActivated {
                surface_id: surface("detail"),
                interaction_id: InteractionId::new("save").unwrap(),
            },
        ]
    );
}

// @scenario: generic_presentation_protocol.feature :: Interaction activates its visible pane first
#[test]
fn command_state_retains_both_panes_and_selects_context_from_active_surface() {
    let mut state = GtkPresentationState::default();
    let contacts_bar = ContextBar {
        primary: Some(action(
            "add_contact",
            Some(StandardShortcut::ActivatePrimary),
        )),
        ..ContextBar::default()
    };
    let detail_bar = ContextBar {
        primary: Some(action("edit", Some(StandardShortcut::ActivatePrimary))),
        ..ContextBar::default()
    };
    for id in ["contacts", "contact_detail"] {
        assert!(state.apply(Command::ReplaceSurface {
            surface: surface_spec(id, 7),
        }));
    }
    assert!(state.apply(Command::SetContextBar {
        surface_id: surface("contacts"),
        revision: 7,
        bar: Box::new(contacts_bar.clone()),
    }));
    assert!(state.apply(Command::SetContextBar {
        surface_id: surface("contact_detail"),
        revision: 7,
        bar: Box::new(detail_bar.clone()),
    }));
    assert!(state.apply(Command::SetPresentationProfile {
        profile: PresentationProfile {
            window_class: WindowClass::Expanded,
            pane_layout: PaneLayout::Split,
            primary_surface: surface("contacts"),
            detail_surface: Some(surface("contact_detail")),
            active_surface: surface("contact_detail"),
            window_title: None,
        },
    }));

    assert_eq!(
        state
            .surface_by_id(&surface("contacts"))
            .map(|surface| surface.surface_id.as_str()),
        Some("contacts")
    );
    assert_eq!(
        state
            .surface_by_id(&surface("contact_detail"))
            .map(|surface| surface.surface_id.as_str()),
        Some("contact_detail")
    );
    assert_eq!(
        state.context_bar(),
        Some((&surface("contact_detail"), &detail_bar))
    );

    assert!(state.apply(Command::SetPresentationProfile {
        profile: PresentationProfile {
            window_class: WindowClass::Expanded,
            pane_layout: PaneLayout::Split,
            primary_surface: surface("contacts"),
            detail_surface: Some(surface("contact_detail")),
            active_surface: surface("contacts"),
            window_title: None,
        },
    }));
    assert_eq!(
        state.context_bar(),
        Some((&surface("contacts"), &contacts_bar))
    );
}

// @scenario: generic_presentation_protocol.feature :: Responsive transitions preserve interaction state
#[test]
fn collapsing_to_one_pane_preserves_the_hidden_surface_for_reexpansion() {
    let mut state = GtkPresentationState::default();
    for id in ["contacts", "contact_detail"] {
        state.apply(Command::ReplaceSurface {
            surface: surface_spec(id, 3),
        });
    }
    state.apply(Command::SetPresentationProfile {
        profile: PresentationProfile {
            window_class: WindowClass::Compact,
            pane_layout: PaneLayout::Single,
            primary_surface: surface("contacts"),
            detail_surface: Some(surface("contact_detail")),
            active_surface: surface("contact_detail"),
            window_title: None,
        },
    });

    assert_eq!(state.visible_surface_ids(), vec![surface("contact_detail")]);
    assert!(state.surface_by_id(&surface("contacts")).is_some());

    state.apply(Command::SetPresentationProfile {
        profile: PresentationProfile {
            window_class: WindowClass::Expanded,
            pane_layout: PaneLayout::Split,
            primary_surface: surface("contacts"),
            detail_surface: Some(surface("contact_detail")),
            active_surface: surface("contact_detail"),
            window_title: None,
        },
    });
    assert_eq!(
        state.visible_surface_ids(),
        vec![surface("contacts"), surface("contact_detail")]
    );
}

// @scenario: generic_presentation_protocol.feature :: Overlay kinds remain distinct with reduced motion
#[test]
fn overlay_transitions_remain_structurally_distinct_under_reduced_motion() {
    assert_ne!(
        GtkOverlayTransition::for_overlay(OverlayKind::Navigation, MotionPreference::Full),
        GtkOverlayTransition::for_overlay(OverlayKind::ActionMenu, MotionPreference::Full),
    );
    assert_ne!(
        GtkOverlayTransition::for_overlay(OverlayKind::Navigation, MotionPreference::Reduced),
        GtkOverlayTransition::for_overlay(OverlayKind::ActionMenu, MotionPreference::Reduced),
    );
    assert_eq!(
        GtkOverlayTransition::for_overlay(OverlayKind::Navigation, MotionPreference::Reduced),
        GtkOverlayTransition::NavigationReveal,
    );
    assert_eq!(
        GtkOverlayTransition::for_overlay(OverlayKind::ActionMenu, MotionPreference::Reduced),
        GtkOverlayTransition::ActionCrossfade,
    );
}

// @scenario: generic_presentation_protocol.feature :: Contextual controls expose four stable roles
#[test]
fn context_controls_preserve_core_semantics_in_four_role_order() {
    let mut destructive_more = action("more", None);
    destructive_more.tone = ActionTone::Destructive;
    destructive_more.accessibility_label = "More contact actions".into();
    let bar = ContextBar {
        back: Some(action("back", Some(StandardShortcut::Back))),
        navigation: Some(action("navigate", None)),
        primary: Some(action("save", Some(StandardShortcut::ActivatePrimary))),
        secondary: Some(destructive_more),
        info: None,
    };

    let controls = context_controls(&bar);

    assert_eq!(
        controls
            .iter()
            .map(|control| control.role)
            .collect::<Vec<_>>(),
        vec![
            GtkContextRole::Back,
            GtkContextRole::Navigation,
            GtkContextRole::Primary,
            GtkContextRole::Secondary,
        ]
    );
    assert!(!controls[0].emphasized);
    assert!(!controls[1].emphasized);
    assert!(controls[2].emphasized);
    assert!(!controls[3].emphasized);
    assert_eq!(
        controls[3].action.accessibility_label,
        "More contact actions"
    );
    assert_eq!(controls[3].action.tone, ActionTone::Destructive);
}

// @scenario: generic_presentation_protocol.feature :: User interaction returns as an opaque event
#[test]
fn keyboard_activation_uses_only_core_declared_shortcuts() {
    let mut undo = action("undo.archive", Some(StandardShortcut::Undo));
    undo.label = "Undo archive".into();
    let bar = ContextBar {
        back: Some(action("back", Some(StandardShortcut::Back))),
        navigation: Some(action("navigate", None)),
        primary: Some(undo),
        secondary: None,
        info: None,
    };

    assert_eq!(
        interaction_for_shortcut(&bar, StandardShortcut::Back).map(InteractionId::as_str),
        Some("back")
    );
    assert_eq!(
        interaction_for_shortcut(&bar, StandardShortcut::Undo).map(InteractionId::as_str),
        Some("undo.archive")
    );
    assert_eq!(
        interaction_for_shortcut(&bar, StandardShortcut::ActivatePrimary),
        None
    );
}

// @scenario: generic_presentation_protocol.feature :: Release contains only the generic action system
#[test]
fn live_application_enters_only_through_the_generic_reducer_protocol() {
    let source = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/shell.rs"),
    )
    .expect("read the GTK application loop");

    for legacy_boundary in [
        "screen_renderer",
        "ActionResult",
        "handle_action(",
        "on_wakeup(",
        "drain_pending_notifications(",
        "poll_notifications(",
    ] {
        assert!(
            !source.contains(legacy_boundary),
            "live GTK entry point still uses legacy boundary {legacy_boundary}"
        );
    }
    assert!(source.contains("render_current_surface"));
    assert!(source.contains("dispatch_platform_event"));
    assert!(source.contains("Event::WakeupFired"));
}

// @scenario: generic_presentation_protocol.feature :: Responsive transitions preserve interaction state
#[test]
fn native_renderer_restores_focus_by_core_binding_identity() {
    let source = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/core_ui/contextual_surface/commands.rs"),
    )
    .expect("read GTK presentation renderer");

    assert!(source.contains("fn current_focus("));
    assert!(source.contains("restore_focus"));
    assert!(source.contains("widget_name"));
}

// @scenario: generic_presentation_protocol.feature :: User interaction returns as an opaque event
#[test]
fn live_platform_callbacks_report_raw_events_to_the_generic_reducer() {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    for relative in [
        "src/core_ui/action_dispatcher.rs",
        "src/core_ui/hardware_link.rs",
    ] {
        let source = std::fs::read_to_string(manifest.join(relative)).expect("read GTK source");
        for legacy_boundary in [
            "handle_hardware_event",
            "handle_app_engine_result",
            "ActionResult",
        ] {
            assert!(
                !source.contains(legacy_boundary),
                "{relative} still uses legacy callback boundary {legacy_boundary}"
            );
        }
    }
}

// @internal
#[test]
fn generic_file_and_notification_effects_have_native_adapters() {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let dispatcher = std::fs::read_to_string(manifest.join("src/core_ui/action_dispatcher.rs"))
        .expect("read command dispatcher");
    let picker =
        std::fs::read_to_string(manifest.join("src/core_ui/action_dispatcher/file_picker.rs"))
            .expect("read file picker");
    let header =
        std::fs::read_to_string(manifest.join("src/platform/header_bar.rs")).expect("read header");

    for command in [
        "Command::FilePickFromUser",
        "Command::ExportFile",
        "Command::PostNotification",
    ] {
        assert!(
            dispatcher.contains(command),
            "missing native adapter for {command}"
        );
    }
    assert!(picker.contains("Event::FilePickedFromUser"));
    assert!(!header.contains("app.import-contacts"));
}

// @scenario: generic_presentation_protocol.feature :: Every shell renders the same prepared presentation
// @internal
#[test]
fn applied_set_navigation_command_is_stored() {
    let mut state = GtkPresentationState::default();
    state.apply(Command::ReplaceSurface {
        surface: surface_spec("contacts", 3),
    });
    let navigation = NavigationSpec {
        items: vec![nav_item("home", true, 0), nav_item("contacts", false, 2)],
    };

    let accepted = state.apply(Command::SetNavigation {
        surface_id: surface("contacts"),
        revision: 3,
        navigation: navigation.clone(),
    });

    assert!(accepted, "a current-revision SetNavigation must be handled");
    assert_eq!(
        state.navigation(),
        Some((&surface("contacts"), &navigation))
    );
}

// @internal
#[test]
fn set_navigation_with_empty_items_is_stored_as_empty() {
    let mut state = GtkPresentationState::default();
    state.apply(Command::ReplaceSurface {
        surface: surface_spec("locked", 1),
    });

    state.apply(Command::SetNavigation {
        surface_id: surface("locked"),
        revision: 1,
        navigation: NavigationSpec { items: Vec::new() },
    });

    let (surface_id, stored) = state
        .navigation()
        .expect("an applied SetNavigation must leave a stored spec, even an empty one");
    assert_eq!(surface_id, &surface("locked"));
    assert!(
        stored.items.is_empty(),
        "empty navigation.items must round-trip as empty, not be dropped"
    );
}

/// The sidebar lists the same destinations the navigation launcher opens,
/// so beside a sidebar the bar leaves that control out (vauchi/private#479).
// @scenario: generic_presentation_protocol.feature :: Contextual controls expose four stable roles
#[test]
fn context_controls_leave_the_navigation_launcher_out_beside_a_sidebar() {
    let bar = ContextBar {
        back: Some(action("back", Some(StandardShortcut::Back))),
        navigation: Some(action("navigate", None)),
        primary: Some(action("save", Some(StandardShortcut::ActivatePrimary))),
        secondary: Some(action("more", None)),
        info: None,
    };

    let beside_sidebar = context_controls_with(&bar, true);
    assert_eq!(
        beside_sidebar
            .iter()
            .map(|control| control.role)
            .collect::<Vec<_>>(),
        vec![
            GtkContextRole::Back,
            GtkContextRole::Primary,
            GtkContextRole::Secondary
        ]
    );

    let alone = context_controls_with(&bar, false);
    assert_eq!(alone.len(), 4, "without a sidebar the launcher stays");
}

/// Core's fifth slot explains the surface (vauchi/private#479). It is
/// drawn last, and a bar from an older Core without it changes nothing.
// @scenario: generic_presentation_protocol.feature :: Contextual controls expose four stable roles
#[test]
fn context_controls_draw_the_info_slot_last_and_only_when_core_sends_it() {
    let with_info = ContextBar {
        back: Some(action("back", Some(StandardShortcut::Back))),
        navigation: None,
        primary: Some(action("save", Some(StandardShortcut::ActivatePrimary))),
        secondary: Some(action("more", None)),
        info: Some(action("presentation.info", None)),
    };
    let roles = |bar: &ContextBar| {
        context_controls_with(bar, true)
            .iter()
            .map(|control| control.role)
            .collect::<Vec<_>>()
    };

    assert_eq!(
        roles(&with_info),
        vec![
            GtkContextRole::Back,
            GtkContextRole::Primary,
            GtkContextRole::Secondary,
            GtkContextRole::Info
        ]
    );
    let info = context_controls_with(&with_info, true)
        .into_iter()
        .find(|control| control.role == GtkContextRole::Info)
        .expect("the info control");
    assert!(!info.emphasized, "only the primary action fills the row");

    let without = ContextBar {
        info: None,
        ..with_info
    };
    assert_eq!(
        roles(&without),
        vec![
            GtkContextRole::Back,
            GtkContextRole::Primary,
            GtkContextRole::Secondary
        ]
    );
}

/// The bottom row above the tab bar is retired (vauchi/private#534): every
/// role but the full-width action now draws in the surface's header bar,
/// Back and a sidebar-less Navigation leading it, Info and Secondary
/// trailing it; Primary alone moves into the surface content.
// @scenario: generic_presentation_protocol.feature :: Contextual controls expose four stable roles
#[test]
fn only_primary_draws_in_the_surface_content_the_rest_join_the_title_row() {
    assert_eq!(GtkContextRole::Back.area(), GtkControlArea::TitleRowLeading);
    assert_eq!(
        GtkContextRole::Navigation.area(),
        GtkControlArea::TitleRowLeading
    );
    assert_eq!(
        GtkContextRole::Secondary.area(),
        GtkControlArea::TitleRowTrailing
    );
    assert_eq!(
        GtkContextRole::Info.area(),
        GtkControlArea::TitleRowTrailing
    );
    assert_eq!(
        GtkContextRole::Primary.area(),
        GtkControlArea::SurfaceContent
    );
}

/// A text panel has nothing to slide in from; it fades like the action
/// menu under either motion preference.
// @scenario: generic_presentation_protocol.feature :: Overlay kinds remain distinct with reduced motion
#[test]
fn an_information_overlay_crossfades_under_either_motion_preference() {
    assert_eq!(
        GtkOverlayTransition::for_overlay(OverlayKind::Information, MotionPreference::Full),
        GtkOverlayTransition::ActionCrossfade,
    );
    assert_eq!(
        GtkOverlayTransition::for_overlay(OverlayKind::Information, MotionPreference::Reduced),
        GtkOverlayTransition::ActionCrossfade,
    );
}

// A keystroke's SurfaceActivated answers with the profile already applied;
// re-rendering for it rebuilt the focused entry from the previous value,
// before Core's echo of the new one arrived (vauchi/private#1062).
// @scenario: generic_presentation_protocol.feature :: Responsive transitions preserve interaction state
#[test]
fn an_unchanged_presentation_profile_is_not_applied_again() {
    let mut state = GtkPresentationState::default();
    let profile = PresentationProfile {
        window_class: WindowClass::Compact,
        pane_layout: PaneLayout::Single,
        primary_surface: surface("onboarding"),
        detail_surface: None,
        active_surface: surface("onboarding"),
        window_title: None,
    };

    assert!(state.apply(Command::SetPresentationProfile {
        profile: profile.clone(),
    }));
    assert!(!state.apply(Command::SetPresentationProfile {
        profile: profile.clone(),
    }));
    assert!(state.apply(Command::SetPresentationProfile {
        profile: PresentationProfile {
            window_class: WindowClass::Expanded,
            ..profile
        },
    }));
}
