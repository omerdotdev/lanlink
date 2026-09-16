use crate::state::{AppState, Peer, SERVICE_TYPE};
use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use std::collections::HashMap;
use std::sync::OnceLock;
use tracing::{info, warn};

static DAEMON: OnceLock<ServiceDaemon> = OnceLock::new();

pub fn start(state: AppState) -> Result<(), String> {
    let mdns = ServiceDaemon::new().map_err(|e| e.to_string())?;
    register(&mdns, &state).map_err(|e| e.to_string())?;
    let browse = mdns.browse(SERVICE_TYPE).map_err(|e| e.to_string())?;
    let _ = DAEMON.set(mdns);

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<ServiceEvent>();
    std::thread::Builder::new()
        .name("lanlink-mdns".into())
        .spawn(move || {
            while let Ok(event) = browse.recv() {
                if tx.send(event).is_err() {
                    break;
                }
            }
        })
        .map_err(|e| e.to_string())?;

    tokio::spawn(async move {
        while let Some(event) = rx.recv().await {
            match event {
                ServiceEvent::ServiceResolved(info) => {
                    if let Some(peer) = peer_from_info(&info) {
                        if peer.id == state.inner.id {
                            continue;
                        }
                        state
                            .inner
                            .peers
                            .write()
                            .await
                            .insert(peer.id.clone(), peer);
                        state.broadcast_peers().await;
                    }
                }
                ServiceEvent::ServiceRemoved(_, fullname) => {
                    let mut peers = state.inner.peers.write().await;
                    peers.retain(|_, p| {
                        format!("{}.{SERVICE_TYPE}", instance_name(&p.id)) != fullname
                    });
                    drop(peers);
                    state.broadcast_peers().await;
                }
                _ => {}
            }
        }
    });

    info!("mDNS advertised as {SERVICE_TYPE}");
    Ok(())
}

pub fn reregister(state: &AppState) {
    let Some(mdns) = DAEMON.get() else {
        return;
    };
    let instance = instance_name(&state.inner.id);
    let fullname = format!("{instance}.{SERVICE_TYPE}");
    if let Err(err) = mdns.unregister(&fullname) {
        warn!("mDNS unregister: {err}");
    }
    if let Err(err) = register(mdns, state) {
        warn!("mDNS register: {err}");
    }
}

fn register(mdns: &ServiceDaemon, state: &AppState) -> Result<(), mdns_sd::Error> {
    let name = state
        .inner
        .name
        .try_read()
        .map(|n| n.clone())
        .unwrap_or_else(|_| "Lanlink".into());
    let ip = crate::state::lan_ip();
    let host = format!("{}.local.", sanitize_host(&ip));
    let instance = instance_name(&state.inner.id);
    let mut properties = HashMap::new();
    properties.insert("id".to_string(), state.inner.id.clone());
    properties.insert("name".to_string(), name);

    let info = ServiceInfo::new(
        SERVICE_TYPE,
        &instance,
        &host,
        ip.as_str(),
        state.inner.port,
        Some(properties),
    )?;
    mdns.register(info)?;
    Ok(())
}

fn instance_name(id: &str) -> String {
    let short: String = id
        .chars()
        .filter(|c| c.is_ascii_hexdigit())
        .take(8)
        .collect();
    format!("lanlink-{short}")
}

fn sanitize_host(ip: &str) -> String {
    ip.replace(['.', ':'], "-")
}

fn peer_from_info(info: &ServiceInfo) -> Option<Peer> {
    let id = info
        .get_property_val_str("id")
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())?;
    let name = info
        .get_property_val_str("name")
        .filter(|s| !s.is_empty())
        .unwrap_or("Lanlink device")
        .to_string();
    let host = info
        .get_addresses()
        .iter()
        .find(|a| a.is_ipv4())
        .or_else(|| info.get_addresses().iter().next())
        .map(|a| a.to_string())?;
    Some(Peer {
        id,
        name,
        host,
        port: info.get_port(),
        kind: "lan".into(),
    })
}
