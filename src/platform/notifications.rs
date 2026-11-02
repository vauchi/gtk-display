// SPDX-FileCopyrightText: 2026 Mattia Egloff <mattia.egloff@pm.me>
// SPDX-License-Identifier: GPL-3.0-or-later

//! Executes prepared notification commands through GIO (ADR-066).

use gtk4::prelude::*;
use gtk4::{gio, glib};
use vauchi_core::{NotificationSpec, NotificationUrgency};

pub fn install_open_action(
    application: &impl IsA<gio::Application>,
    open: impl Fn(String) + 'static,
) {
    let action = gio::SimpleAction::new("open-notification", Some(glib::VariantTy::STRING));
    action.connect_activate(move |_, parameter| {
        if let Some(uri) = parameter.and_then(glib::Variant::str) {
            open(uri.to_owned());
        }
    });
    application.as_ref().add_action(&action);
}

pub fn post_notification(
    application: &impl IsA<gio::Application>,
    notification: &NotificationSpec,
) {
    let native = gio::Notification::new(&notification.title);
    native.set_body(Some(&notification.body));
    native.set_priority(match notification.urgency {
        NotificationUrgency::Default => gio::NotificationPriority::Normal,
        NotificationUrgency::High => gio::NotificationPriority::High,
        NotificationUrgency::Urgent => gio::NotificationPriority::Urgent,
    });
    if let Some(uri) = &notification.deep_link_uri {
        native
            .set_default_action_and_target_value("app.open-notification", Some(&uri.to_variant()));
    }
    application.send_notification(notification.notification_id.as_deref(), &native);
}
