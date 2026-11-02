// SPDX-FileCopyrightText: 2026 Mattia Egloff <mattia.egloff@pm.me>
// SPDX-License-Identifier: GPL-3.0-or-later

//! BLE exchange handler via BlueZ.
//!
//! Scanning runs on Core's shared discovery executor (ADR-075). Advertising,
//! connecting and GATT still use `bluer` on background threads, reporting
//! to the GTK main loop via mpsc channels, until their executors land
//! (vauchi/private#770).

#[cfg(all(feature = "ble", target_os = "linux"))]
mod inner {
    use std::cell::Cell;
    use std::collections::HashMap;
    use std::sync::Mutex as StdMutex;
    use std::sync::mpsc;

    use gtk4::glib;
    use libadwaita as adw;

    use vauchi_app::i18n::{self, Locale};
    use vauchi_core::{BleLinkDirection, Event};

    use crate::core_ui::engine_link::EngineHandle;

    use crate::core_ui::contextual_surface::dispatch_platform_event;
    use crate::platform::ble_discovery::DiscoveryHost;

    /// Persistent BLE connection state shared across connect/write/read/disconnect calls.
    struct BleConnection {
        runtime: tokio::runtime::Runtime,
        _session: bluer::Session,
        device: bluer::Device,
        characteristics: HashMap<String, bluer::gatt::remote::Characteristic>,
    }

    static BLE_CONNECTION: StdMutex<Option<BleConnection>> = StdMutex::new(None);

    /// Lock BLE_CONNECTION, recovering from poisoning by clearing stale state.
    ///
    /// If a previous holder panicked, the connection is likely stale.
    /// Clearing it lets connect() establish a fresh session instead of
    /// crashing the app on every subsequent BLE operation.
    fn lock_ble_connection() -> std::sync::MutexGuard<'static, Option<BleConnection>> {
        BLE_CONNECTION.lock().unwrap_or_else(|poisoned| {
            eprintln!("[BLE] Recovered from poisoned mutex");
            let mut guard = poisoned.into_inner();
            *guard = None;
            guard
        })
    }

    // A static, not a thread-local: a thread-local host would be disposed
    // after Tokio's own thread-local is gone and abort the process at exit.
    static DISCOVERY: StdMutex<Option<DiscoveryHost>> = StdMutex::new(None);

    thread_local! {
        static SCAN_GENERATION: Cell<u64> = const { Cell::new(0) };
    }

    fn lock_discovery() -> std::sync::MutexGuard<'static, Option<DiscoveryHost>> {
        DISCOVERY
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Scan through Core's shared discovery executor (ADR-075). Core decides
    /// how long the scan runs; observations reach it on the main loop until
    /// it stops the scan or starts another.
    pub fn start_scanning(
        container: &gtk4::Box,
        app_engine: &EngineHandle,
        toast_overlay: &adw::ToastOverlay,
        service_uuid: String,
    ) {
        let msg = i18n::get_string(Locale::default(), "platform.ble_scanning");
        let scan_toast = adw::Toast::new(&msg);
        scan_toast.set_timeout(3);
        toast_overlay.add_toast(scan_toast);

        let generation = next_scan_generation();
        if let Err(error) = start_discovery(service_uuid) {
            dispatch_platform_event(
                container,
                app_engine,
                toast_overlay,
                Event::HardwareError {
                    transport: "BLE".into(),
                    error,
                },
            );
            return;
        }

        let container = container.clone();
        let app_engine = app_engine.clone();
        let toast_overlay = toast_overlay.clone();
        glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
            // The lock is released before dispatch, which may stop the scan.
            while SCAN_GENERATION.get() == generation {
                let next = lock_discovery().as_mut().and_then(DiscoveryHost::pop);
                let Some(event) = next else {
                    break;
                };
                dispatch_platform_event(&container, &app_engine, &toast_overlay, event);
            }
            if SCAN_GENERATION.get() == generation {
                glib::ControlFlow::Continue
            } else {
                glib::ControlFlow::Break
            }
        });
    }

    pub fn stop_scanning() {
        next_scan_generation();
        if let Some(host) = lock_discovery().as_mut() {
            host.stop();
        }
    }

    /// Release the executor and the OS scan before the application exits.
    pub fn dispose_scanning() {
        next_scan_generation();
        let host = lock_discovery().take();
        drop(host);
    }

    fn next_scan_generation() -> u64 {
        let generation = SCAN_GENERATION.get().wrapping_add(1);
        SCAN_GENERATION.set(generation);
        generation
    }

    fn start_discovery(service_uuid: String) -> Result<(), String> {
        let mut host = lock_discovery();
        if host.is_none() {
            *host = Some(
                DiscoveryHost::new(vauchi_hardware::host_discovery())
                    .map_err(|error| error.to_string())?,
            );
        }
        match host.as_mut() {
            Some(host) => host.start(service_uuid).map_err(|error| error.to_string()),
            None => Ok(()),
        }
    }

    /// Start advertising our vauchi BLE service.
    pub fn start_advertising(toast_overlay: &adw::ToastOverlay, service_uuid: String) {
        let toast_overlay = toast_overlay.clone();
        let (tx, rx) = mpsc::channel::<Result<(), String>>();

        std::thread::spawn(move || {
            let rt = match tokio::runtime::Runtime::new() {
                Ok(rt) => rt,
                Err(e) => {
                    tx.send(Err(format!("Tokio runtime: {}", e))).ok();
                    return;
                }
            };
            let result = rt.block_on(advertise_service(&service_uuid));
            tx.send(result).ok();
        });

        glib::timeout_add_local(std::time::Duration::from_millis(200), move || {
            match rx.try_recv() {
                Ok(Ok(())) => {
                    let msg =
                        i18n::get_string(Locale::default(), "platform.ble_advertising_started");
                    let toast = adw::Toast::new(&msg);
                    toast_overlay.add_toast(toast);
                    glib::ControlFlow::Break
                }
                Ok(Err(e)) => {
                    let msg = i18n::get_string_with_args(
                        Locale::default(),
                        "platform.ble_advertise_failed",
                        &[("error", &e)],
                    );
                    let toast = adw::Toast::new(&msg);
                    toast_overlay.add_toast(toast);
                    glib::ControlFlow::Break
                }
                Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
                Err(mpsc::TryRecvError::Disconnected) => glib::ControlFlow::Break,
            }
        });
    }

    /// Connect to a BLE device and report the result.
    pub fn connect(
        container: &gtk4::Box,
        app_engine: &EngineHandle,
        toast_overlay: &adw::ToastOverlay,
        device_id: String,
    ) {
        let container = container.clone();
        let app_engine = app_engine.clone();
        let toast_overlay = toast_overlay.clone();
        let (tx, rx) = mpsc::channel::<Result<(), String>>();

        let device_id_for_thread = device_id.clone();
        std::thread::spawn(move || {
            let rt = match tokio::runtime::Runtime::new() {
                Ok(rt) => rt,
                Err(e) => {
                    tx.send(Err(format!("Tokio runtime: {}", e))).ok();
                    return;
                }
            };

            let result = rt.block_on(async {
                let addr: bluer::Address = device_id_for_thread
                    .parse()
                    .map_err(|e| format!("Invalid address: {}", e))?;

                let session = bluer::Session::new()
                    .await
                    .map_err(|e| format!("BlueZ session: {}", e))?;
                let adapter = session
                    .default_adapter()
                    .await
                    .map_err(|e| format!("No BLE adapter: {}", e))?;

                let device = adapter.device(addr).map_err(|e| format!("Device: {}", e))?;
                device
                    .connect()
                    .await
                    .map_err(|e| format!("Connect failed: {}", e))?;

                // Discover GATT services and cache characteristics by UUID
                let mut characteristics = HashMap::new();
                let services = device
                    .services()
                    .await
                    .map_err(|e| format!("GATT discovery: {}", e))?;

                for service in services {
                    if let Ok(chars) = service.characteristics().await {
                        for c in chars {
                            if let Ok(uuid) = c.uuid().await {
                                characteristics.insert(uuid.to_string().to_lowercase(), c);
                            }
                        }
                    }
                }

                Ok::<_, String>((session, device, characteristics))
            });

            match result {
                Ok((session, device, characteristics)) => {
                    *lock_ble_connection() = Some(BleConnection {
                        runtime: rt,
                        _session: session,
                        device,
                        characteristics,
                    });
                    tx.send(Ok(())).ok();
                }
                Err(e) => {
                    tx.send(Err(e)).ok();
                }
            }
        });

        let device_id_for_event = device_id;
        glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
            match rx.try_recv() {
                Ok(Ok(())) => {
                    dispatch_platform_event(
                        &container,
                        &app_engine,
                        &toast_overlay,
                        Event::BleConnected {
                            device_id: device_id_for_event.clone(),
                            direction: BleLinkDirection::Outbound,
                        },
                    );
                    glib::ControlFlow::Break
                }
                Ok(Err(e)) => {
                    dispatch_platform_event(
                        &container,
                        &app_engine,
                        &toast_overlay,
                        Event::HardwareError {
                            transport: "BLE".into(),
                            error: e,
                        },
                    );
                    glib::ControlFlow::Break
                }
                Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
                Err(mpsc::TryRecvError::Disconnected) => glib::ControlFlow::Break,
            }
        });
    }

    /// Write data to a GATT characteristic.
    pub fn write_characteristic(
        container: &gtk4::Box,
        app_engine: &EngineHandle,
        toast_overlay: &adw::ToastOverlay,
        uuid: String,
        data: Vec<u8>,
    ) {
        let container = container.clone();
        let app_engine = app_engine.clone();
        let toast_overlay = toast_overlay.clone();
        let (tx, rx) = mpsc::channel::<Result<(), String>>();

        std::thread::spawn(move || {
            let guard = lock_ble_connection();
            let Some(conn) = guard.as_ref() else {
                tx.send(Err("No active BLE connection".into())).ok();
                return;
            };
            let Some(char) = conn.characteristics.get(&uuid.to_lowercase()).cloned() else {
                tx.send(Err(format!("Characteristic {} not found", uuid)))
                    .ok();
                return;
            };
            let handle = conn.runtime.handle().clone();
            drop(guard); // Release lock before blocking

            let result = handle.block_on(async move {
                char.write(&data)
                    .await
                    .map_err(|e| format!("GATT write: {}", e))
            });
            tx.send(result).ok();
        });

        glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
            match rx.try_recv() {
                Ok(Ok(())) => glib::ControlFlow::Break,
                Ok(Err(e)) => {
                    dispatch_platform_event(
                        &container,
                        &app_engine,
                        &toast_overlay,
                        Event::HardwareError {
                            transport: "BLE".into(),
                            error: e,
                        },
                    );
                    glib::ControlFlow::Break
                }
                Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
                Err(mpsc::TryRecvError::Disconnected) => glib::ControlFlow::Break,
            }
        });
    }

    /// Read data from a GATT characteristic.
    pub fn read_characteristic(
        container: &gtk4::Box,
        app_engine: &EngineHandle,
        toast_overlay: &adw::ToastOverlay,
        device_id: String,
        direction: BleLinkDirection,
        uuid: String,
    ) {
        let container = container.clone();
        let app_engine = app_engine.clone();
        let toast_overlay = toast_overlay.clone();
        let uuid_for_event = uuid.clone();
        let (tx, rx) = mpsc::channel::<Result<Vec<u8>, String>>();

        std::thread::spawn(move || {
            let guard = lock_ble_connection();
            let Some(conn) = guard.as_ref() else {
                tx.send(Err("No active BLE connection".into())).ok();
                return;
            };
            let Some(char) = conn.characteristics.get(&uuid.to_lowercase()).cloned() else {
                tx.send(Err(format!("Characteristic {} not found", uuid)))
                    .ok();
                return;
            };
            let handle = conn.runtime.handle().clone();
            drop(guard);

            let result = handle.block_on(async move {
                char.read().await.map_err(|e| format!("GATT read: {}", e))
            });
            tx.send(result).ok();
        });

        glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
            match rx.try_recv() {
                Ok(Ok(data)) => {
                    dispatch_platform_event(
                        &container,
                        &app_engine,
                        &toast_overlay,
                        Event::BleCharacteristicRead {
                            device_id: device_id.clone(),
                            direction,
                            uuid: uuid_for_event.clone(),
                            data,
                        },
                    );
                    glib::ControlFlow::Break
                }
                Ok(Err(e)) => {
                    dispatch_platform_event(
                        &container,
                        &app_engine,
                        &toast_overlay,
                        Event::HardwareError {
                            transport: "BLE".into(),
                            error: e,
                        },
                    );
                    glib::ControlFlow::Break
                }
                Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
                Err(mpsc::TryRecvError::Disconnected) => glib::ControlFlow::Break,
            }
        });
    }

    /// Disconnect from the current BLE device.
    pub fn disconnect(toast_overlay: &adw::ToastOverlay) {
        let prev = lock_ble_connection().take();
        if let Some(conn) = prev {
            // Disconnect the device on the connection's runtime before dropping
            let _ = conn.runtime.block_on(conn.device.disconnect());
            // Runtime + session + device + chars dropped here
        }
        let msg = i18n::get_string(Locale::default(), "platform.ble_disconnected");
        let toast = adw::Toast::new(&msg);
        toast_overlay.add_toast(toast);
    }

    // ── Async BlueZ operations ──────────────────────────────────────

    async fn advertise_service(service_uuid: &str) -> Result<(), String> {
        use bluer::adv::Advertisement;

        let session = bluer::Session::new()
            .await
            .map_err(|e| format!("BlueZ session: {}", e))?;
        let adapter = session
            .default_adapter()
            .await
            .map_err(|e| format!("No BLE adapter: {}", e))?;
        adapter
            .set_powered(true)
            .await
            .map_err(|e| format!("Power on adapter: {}", e))?;

        let target_uuid: bluer::Uuid = service_uuid
            .parse()
            .map_err(|e| format!("Invalid UUID: {}", e))?;

        let adv = Advertisement {
            advertisement_type: bluer::adv::Type::Peripheral,
            service_uuids: vec![target_uuid].into_iter().collect(),
            local_name: Some("Vauchi".into()),
            ..Default::default()
        };

        let _handle = adapter
            .advertise(adv)
            .await
            .map_err(|e| format!("Start advertising: {}", e))?;

        // Keep advertising for 30 seconds
        tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;

        Ok(())
    }
}

#[cfg(all(feature = "ble", target_os = "linux"))]
pub use inner::*;
