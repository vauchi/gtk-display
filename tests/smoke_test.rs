// SPDX-FileCopyrightText: 2026 Mattia Egloff <mattia.egloff@pm.me>
// SPDX-License-Identifier: GPL-3.0-or-later

//! Headless smoke tests for GTK's generic Core presentation boundary.

use vauchi_app::i18n::get_string;
use vauchi_app::ui::AppEngine;
use vauchi_core::{
    AlertSpec, Command, Event, InputMode, MotionPreference, SurfaceId, WindowClass, api::Vauchi,
};
use vauchi_gtk::core_ui::contextual_surface::commands_for_event;
use vauchi_gtk::engine::CoreEngine;

// @internal — native timers enter the same command/event boundary as UI events (#764).
#[test]
fn wakeup_observation_returns_prepared_notifications_once() {
    let mut core = Vauchi::in_memory().expect("in-memory Core");
    core.create_identity("Alice").unwrap();
    let app_engine = AppEngine::new(core);
    app_engine
        .vauchi()
        .dispatch_event(vauchi_core::VauchiEvent::EmergencyAlertReceived {
            contact_id: "notification-target".into(),
            message: "HELP".into(),
            timestamp: 1_700_000_000,
            location: None,
            alert_nonce: [7; 32],
        });
    let engine = CoreEngine::new(app_engine);
    let commands = commands_for_event(
        &engine,
        vauchi_core::event_from_json(r#""WakeupFired""#).unwrap(),
    );
    let notifications: Vec<_> = commands
        .iter()
        .filter_map(|command| match command {
            Command::PostNotification { notification } => Some(notification),
            _ => None,
        })
        .collect();
    assert_eq!(notifications.len(), 1);
    assert_eq!(
        notifications[0].deep_link_uri.as_deref(),
        Some("vauchi://contact/notification-target")
    );
    assert_eq!(
        notifications[0].notification_id,
        Some(format!("alert:notification-target:{}", "07".repeat(32)))
    );
    let again = commands_for_event(
        &engine,
        vauchi_core::event_from_json(r#""WakeupFired""#).unwrap(),
    );
    assert_eq!(
        again
            .iter()
            .filter(|command| matches!(command, Command::PostNotification { .. }))
            .count(),
        0
    );
}

// @scenario: generic_presentation_protocol.feature :: Every shell renders the same prepared presentation
#[test]
fn fresh_install_boots_with_a_generic_surface_and_context_bar() {
    let mut engine = AppEngine::new(Vauchi::in_memory().expect("in-memory Core"));
    let commands = engine.initial_commands().expect("initial commands");

    assert!(matches!(
        commands.first(),
        Some(Command::ReplaceSurface { surface }) if !surface.title.is_empty()
    ));
    assert!(
        commands
            .iter()
            .any(|command| matches!(command, Command::SetContextBar { .. }))
    );
}

#[test]
fn seeded_install_boots_through_the_same_generic_boundary() {
    let mut core = Vauchi::in_memory().expect("in-memory Core");
    core.create_identity("Test User").expect("create identity");
    let mut engine = AppEngine::new(core);
    let commands = engine.initial_commands().expect("initial commands");

    assert!(matches!(
        commands.first(),
        Some(Command::ReplaceSurface { surface }) if surface.revision == 1
    ));
}

#[test]
fn desktop_window_facts_produce_a_core_owned_profile() {
    let mut engine = AppEngine::new(Vauchi::in_memory().expect("in-memory Core"));
    let commands = engine
        .dispatch(Event::PresentationEnvironmentChanged {
            available_width: 960,
            available_height: 720,
            input_modes: vec![InputMode::Pointer, InputMode::Keyboard],
            motion: MotionPreference::Full,
            color_scheme: None,
        })
        .expect("environment event");

    assert!(matches!(
        commands.as_slice(),
        [Command::SetPresentationProfile { profile }]
            if profile.window_class == WindowClass::Expanded
    ));
}

// ADR-045 Am1: Core owns what a rejected event turns into; the shell applies it.
// @internal
#[test]
fn rejected_event_yields_the_alert_core_prepared() {
    let app_engine = AppEngine::new(Vauchi::in_memory().expect("in-memory Core"));
    let locale = app_engine.render_context().resolved_locale();
    let engine = CoreEngine::new(app_engine);
    let activation_before_environment_reported = Event::SurfaceActivated {
        surface_id: SurfaceId::new("never-presented").unwrap(),
    };

    let commands = commands_for_event(&engine, activation_before_environment_reported);

    assert_eq!(
        commands,
        vec![Command::PresentAlert {
            alert: AlertSpec {
                title: get_string(locale, "error.title"),
                message: get_string(locale, "error.generic"),
            },
        }]
    );
}
