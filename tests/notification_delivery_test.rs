// SPDX-FileCopyrightText: 2026 Mattia Egloff <mattia.egloff@pm.me>
// SPDX-License-Identifier: GPL-3.0-or-later

use gtk4::{gio, glib, prelude::*};
use std::sync::mpsc;
use std::time::Duration;
use vauchi_contract::{NotificationSpec, NotificationUrgency};
use vauchi_gtk::platform::notifications::post_notification;

const APP_ID: &str = "app.vauchi.NotificationContract";
const URI: &str = "vauchi://contact/通知?opaque='(target)'&x=%2F";
const DEADLINE: Duration = Duration::from_secs(10);

// A subprocess avoids changing the parent test runner's session bus or GIO caches.
fn isolated_bus(test_name: &str, check: impl FnOnce()) {
    if std::env::var_os("VAUCHI_NOTIFICATION_TEST_CHILD").is_some() {
        check();
        return;
    }
    let output = std::process::Command::new("dbus-run-session")
        .arg("--")
        .arg(std::env::current_exe().unwrap())
        .args(["--exact", test_name, "--nocapture"])
        .env("VAUCHI_NOTIFICATION_TEST_CHILD", "1")
        .env("GNOTIFICATION_BACKEND", "gtk")
        .output()
        .expect("private D-Bus session must start");
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

struct NotificationServer {
    messages: mpsc::Receiver<glib::Variant>,
    main_loop: glib::MainLoop,
    worker: Option<std::thread::JoinHandle<()>>,
}

impl NotificationServer {
    fn start() -> Self {
        let (ready, started) = mpsc::channel();
        let (sender, messages) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            let context = glib::MainContext::new();
            context
                .with_thread_default(|| {
                    let bus = gio::DBusConnection::for_address_sync(
                        &std::env::var("DBUS_SESSION_BUS_ADDRESS").unwrap(),
                        gio::DBusConnectionFlags::AUTHENTICATION_CLIENT
                            | gio::DBusConnectionFlags::MESSAGE_BUS_CONNECTION,
                        None,
                        gio::Cancellable::NONE,
                    )
                    .unwrap();
                    let interface = gio::DBusNodeInfo::for_xml(
                        "<node><interface name='org.gtk.Notifications'>\
                     <method name='AddNotification'>\
                     <arg type='s' direction='in'/><arg type='s' direction='in'/>\
                     <arg type='a{sv}' direction='in'/></method></interface></node>",
                    )
                    .unwrap()
                    .lookup_interface("org.gtk.Notifications")
                    .unwrap();
                    let registration = bus
                        .register_object("/org/gtk/Notifications", &interface)
                        .method_call(move |_, _, _, _, method, parameters, invocation| {
                            assert_eq!(method, "AddNotification");
                            sender.send(parameters).unwrap();
                            invocation.return_value(Some(&().to_variant()));
                        })
                        .build()
                        .unwrap();
                    let claimed = bus
                        .call_sync(
                            Some("org.freedesktop.DBus"),
                            "/org/freedesktop/DBus",
                            "org.freedesktop.DBus",
                            "RequestName",
                            Some(&("org.gtk.Notifications", 4u32).to_variant()),
                            None,
                            gio::DBusCallFlags::NONE,
                            10_000,
                            gio::Cancellable::NONE,
                        )
                        .unwrap();
                    assert_eq!(claimed.child_get::<u32>(0), 1);
                    let main_loop = glib::MainLoop::new(Some(&context), false);
                    ready.send(main_loop.clone()).unwrap();
                    main_loop.run();
                    bus.unregister_object(registration).unwrap();
                    bus.close_sync(gio::Cancellable::NONE).unwrap();
                })
                .unwrap();
        });
        Self {
            messages,
            main_loop: started
                .recv_timeout(DEADLINE)
                .expect("notification server ready"),
            worker: Some(worker),
        }
    }

    fn receive(&self) -> (String, glib::VariantDict) {
        let message = self
            .messages
            .recv_timeout(DEADLINE)
            .expect("native notification handoff");
        assert_eq!(message.child_get::<String>(0), APP_ID);
        (
            message.child_get(1),
            glib::VariantDict::new(Some(&message.child_value(2))),
        )
    }
}

impl Drop for NotificationServer {
    fn drop(&mut self) {
        let main_loop = self.main_loop.clone();
        self.main_loop.context().invoke(move || main_loop.quit());
        let _ = self.worker.take().unwrap().join();
    }
}

fn application() -> gio::Application {
    let app = gio::Application::new(Some(APP_ID), gio::ApplicationFlags::NON_UNIQUE);
    app.register(gio::Cancellable::NONE).unwrap();
    app
}

fn prepared() -> NotificationSpec {
    NotificationSpec {
        notification_id: Some("opaque/通知/17".into()),
        title: "Prepared title — 通知".into(),
        body: "Prepared body <unchanged>".into(),
        deep_link_uri: Some(URI.into()),
        category_id: "opaque-category".into(),
        channel_id: "opaque-channel".into(),
        urgency: NotificationUrgency::Urgent,
        category_options: vec![],
    }
}

// @internal — verify the native handoff, not notification fields in a local wrapper (#764).
#[test]
fn notification_sender_preserves_prepared_native_payload() {
    isolated_bus(
        "notification_sender_preserves_prepared_native_payload",
        || {
            let server = NotificationServer::start();
            let app = application();
            let mut notification = prepared();
            for (urgency, priority) in [
                (NotificationUrgency::Default, "normal"),
                (NotificationUrgency::High, "high"),
                (NotificationUrgency::Urgent, "urgent"),
            ] {
                notification.urgency = urgency;
                post_notification(&app, &notification);
                let (_, wire) = server.receive();
                assert_eq!(
                    wire.lookup::<String>("title").unwrap().as_deref(),
                    Some("Prepared title — 通知")
                );
                assert_eq!(
                    wire.lookup::<String>("body").unwrap().as_deref(),
                    Some("Prepared body <unchanged>")
                );
                assert_eq!(
                    wire.lookup::<String>("priority").unwrap().as_deref(),
                    Some(priority)
                );
                assert_eq!(
                    wire.lookup::<String>("default-action").unwrap().as_deref(),
                    Some("app.open-notification")
                );
                assert_eq!(
                    wire.lookup::<String>("default-action-target")
                        .unwrap()
                        .as_deref(),
                    Some(URI)
                );
            }
        },
    );
}

// @internal — replacement identity must survive GIO's native transport unchanged (#764).
#[test]
fn notification_sender_preserves_opaque_identity() {
    isolated_bus("notification_sender_preserves_opaque_identity", || {
        let server = NotificationServer::start();
        let app = application();
        post_notification(&app, &prepared());
        let (id, _) = server.receive();
        assert_eq!(id, "opaque/通知/17");
        let mut replacement = prepared();
        replacement.title = "Updated prepared title".into();
        replacement.body = "Updated prepared body".into();
        post_notification(&app, &replacement);
        let (replacement_id, wire) = server.receive();
        assert_eq!(replacement_id, id);
        assert_eq!(
            wire.lookup::<String>("title").unwrap().as_deref(),
            Some("Updated prepared title")
        );
        assert_eq!(
            wire.lookup::<String>("body").unwrap().as_deref(),
            Some("Updated prepared body")
        );
    });
}

// @internal — older anonymous commands must remain separate, with no invented click target (#764).
#[test]
fn anonymous_notifications_remain_distinct_without_a_click_target() {
    isolated_bus(
        "anonymous_notifications_remain_distinct_without_a_click_target",
        || {
            let server = NotificationServer::start();
            let app = application();
            let mut notification = prepared();
            notification.notification_id = None;
            notification.deep_link_uri = None;
            post_notification(&app, &notification);
            let (first, wire) = server.receive();
            assert_eq!(wire.lookup::<String>("default-action").unwrap(), None);
            assert_eq!(
                wire.lookup::<String>("default-action-target").unwrap(),
                None
            );
            post_notification(&app, &notification);
            let (second, _) = server.receive();
            assert_ne!(first, second);
            assert!(gio::dbus_is_guid(&first));
            assert!(gio::dbus_is_guid(&second));
        },
    );
}

// @internal — activating the native notification forwards the opaque target without parsing (#764).
#[test]
fn notification_click_forwards_the_captured_target() {
    isolated_bus("notification_click_forwards_the_captured_target", || {
        let server = NotificationServer::start();
        let app = application();
        let (sent, received) = mpsc::channel();
        vauchi_gtk::platform::notifications::install_open_action(&app, move |uri| {
            sent.send(uri).unwrap();
        });
        post_notification(&app, &prepared());
        let (_, wire) = server.receive();
        let action = wire.lookup::<String>("default-action").unwrap().unwrap();
        let target = wire
            .lookup::<String>("default-action-target")
            .unwrap()
            .unwrap();
        app.activate_action(
            action.strip_prefix("app.").unwrap(),
            Some(&target.to_variant()),
        );
        assert_eq!(received.recv_timeout(DEADLINE).unwrap(), URI);
        assert_eq!(received.try_recv(), Err(mpsc::TryRecvError::Empty));
    });
}
