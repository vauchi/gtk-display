// SPDX-FileCopyrightText: 2026 Mattia Egloff <mattia.egloff@pm.me>
// SPDX-License-Identifier: GPL-3.0-or-later

//! The TCP adapter executes the endpoint Core supplied (private#924).

use std::net::TcpListener;
use std::thread;
use vauchi_contract::DirectEndpoint;
use vauchi_core::exchange::tcp_transport::exchange_payloads;
use vauchi_gtk::platform::tcp_exchange::execute_exchange;

// @internal
#[test]
fn direct_endpoint_uses_cores_address_for_both_exchange_roles() {
    for is_initiator in [true, false] {
        let listener = TcpListener::bind("127.0.0.2:0").unwrap();
        let endpoint = DirectEndpoint {
            fallback_host: "127.0.0.2".into(),
            port: listener.local_addr().unwrap().port(),
            browse_timeout_ms: 0,
            io_timeout_ms: 2_000,
            ..DirectEndpoint::default()
        };
        let peer = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(2)))
                .unwrap();
            stream
                .set_write_timeout(Some(std::time::Duration::from_secs(2)))
                .unwrap();
            exchange_payloads(&mut stream, b"peer", !is_initiator).unwrap()
        });

        let received = execute_exchange(&endpoint, b"ours", is_initiator).unwrap();

        assert_eq!(received, b"peer");
        assert_eq!(peer.join().unwrap(), b"ours");
    }
}

// @internal
#[test]
fn direct_endpoint_rejects_an_invalid_fallback_address() {
    let endpoint = DirectEndpoint {
        fallback_host: "invalid-address".into(),
        browse_timeout_ms: 0,
        ..DirectEndpoint::default()
    };

    let error = execute_exchange(&endpoint, b"ours", true).unwrap_err();

    assert!(error.starts_with("bad address:"), "{error}");
}

// @internal
#[test]
fn a_silent_peer_fails_the_exchange_at_cores_io_deadline() {
    let listener = TcpListener::bind("127.0.0.2:0").unwrap();
    let endpoint = DirectEndpoint {
        fallback_host: "127.0.0.2".into(),
        port: listener.local_addr().unwrap().port(),
        browse_timeout_ms: 0,
        io_timeout_ms: 200,
        ..DirectEndpoint::default()
    };
    // Not joined: an exchange that never reached the listener would leave
    // it in accept() and hang the test instead of failing it.
    let silent_peer = thread::spawn(move || listener.accept().unwrap());

    let started = std::time::Instant::now();
    let received = execute_exchange(&endpoint, b"desktop payload", false);

    assert!(received.is_err(), "a peer that never answers: {received:?}");
    assert!(
        started.elapsed() < std::time::Duration::from_secs(5),
        "the exchange must end at Core's 200 ms deadline, not a shell constant; took {:?}",
        started.elapsed()
    );
    drop(silent_peer);
}
