// SPDX-FileCopyrightText: 2026 Mattia Egloff <mattia.egloff@pm.me>
// SPDX-License-Identifier: GPL-3.0-or-later

//! Executes Core's direct TCP endpoint policy using the VXCH transport.

use std::net::{IpAddr, SocketAddr, TcpStream};
use std::time::Duration;

use vauchi_contract::DirectEndpoint;
use vauchi_core::exchange::tcp_transport::TcpDirectTransport;

#[cfg(feature = "mdns")]
use mdns_sd::{ServiceDaemon, ServiceEvent};

#[cfg(feature = "mdns")]
fn discover_phone(endpoint: &DirectEndpoint) -> Option<SocketAddr> {
    if endpoint.browse_timeout_ms == 0 {
        return None;
    }
    let mdns = ServiceDaemon::new().ok()?;
    let receiver = mdns.browse(&endpoint.service_type).ok()?;
    let deadline =
        std::time::Instant::now() + Duration::from_millis(u64::from(endpoint.browse_timeout_ms));

    while std::time::Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        match receiver.recv_timeout(remaining) {
            Ok(ServiceEvent::ServiceResolved(info)) => {
                if let Some(addr) = info.get_addresses().iter().next() {
                    let address = SocketAddr::new(*addr, info.get_port());
                    mdns.shutdown().ok();
                    return Some(address);
                }
            }
            Ok(_) => continue,
            Err(_) => break,
        }
    }

    mdns.shutdown().ok();
    None
}

#[cfg(not(feature = "mdns"))]
fn discover_phone(_endpoint: &DirectEndpoint) -> Option<SocketAddr> {
    None
}

/// Browse once, then exchange with the discovered peer or Core's fallback.
pub fn execute_exchange(
    endpoint: &DirectEndpoint,
    our_payload: &[u8],
    is_initiator: bool,
) -> Result<Vec<u8>, String> {
    let sock_addr = match discover_phone(endpoint) {
        Some(address) => address,
        None => {
            let host: IpAddr = endpoint
                .fallback_host
                .parse()
                .map_err(|e| format!("bad address: {e}"))?;
            SocketAddr::new(host, endpoint.port)
        }
    };
    let timeout = Duration::from_millis(u64::from(endpoint.io_timeout_ms));
    let stream = TcpStream::connect_timeout(&sock_addr, timeout)
        .map_err(|e| format!("TCP connect failed: {e}"))?;

    stream
        .set_read_timeout(Some(timeout))
        .map_err(|e| format!("set timeout: {e}"))?;
    stream
        .set_write_timeout(Some(timeout))
        .map_err(|e| format!("set timeout: {e}"))?;

    let mut transport = TcpDirectTransport::physical(stream);
    transport
        .exchange(our_payload, is_initiator)
        .map_err(|e| format!("exchange failed: {e}"))
}
