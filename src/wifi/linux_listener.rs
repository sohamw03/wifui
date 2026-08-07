//! Long-lived Linux D-Bus event listener.

use super::{
    IWD_SERVICE, IWD_STATION_INTERFACE, ListenerSpec, NETWORK_MANAGER_SERVICE, NM_DEVICE_INTERFACE,
    NM_DEVICE_WIFI_INTERFACE, value_string,
};
use crate::error::{WifiError, WifiResult};
use crate::wifi::types::ConnectionEvent;
use futures_lite::StreamExt;
use std::collections::HashMap;
use std::sync::mpsc::{self, SyncSender};
use std::thread::{self, JoinHandle};
use tokio::sync::oneshot;
use zbus::zvariant::{OwnedObjectPath, OwnedValue};
use zbus::{Connection, Proxy};

pub(crate) struct WifiListener {
    stop: Option<oneshot::Sender<()>>,
    worker: Option<JoinHandle<()>>,
}

impl std::fmt::Debug for WifiListener {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("WifiListener")
            .finish_non_exhaustive()
    }
}

impl Drop for WifiListener {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(worker) = self.worker.take()
            && worker.thread().id() != thread::current().id()
        {
            let _ = worker.join();
        }
    }
}

pub(crate) fn start(
    spec: ListenerSpec,
    sender: tokio::sync::mpsc::UnboundedSender<ConnectionEvent>,
) -> WifiResult<WifiListener> {
    let (ready_tx, ready_rx) = mpsc::sync_channel(1);
    let (stop_tx, stop_rx) = oneshot::channel();
    let worker = thread::Builder::new()
        .name("wifui-linux-wifi-listener".to_string())
        .spawn(move || worker_main(spec, sender, ready_tx, stop_rx))
        .map_err(|_| WifiError::Internal("failed to start Linux Wi-Fi listener".to_string()))?;

    match ready_rx.recv() {
        Ok(Ok(())) => Ok(WifiListener {
            stop: Some(stop_tx),
            worker: Some(worker),
        }),
        Ok(Err(error)) => {
            let _ = stop_tx.send(());
            let _ = worker.join();
            Err(error)
        }
        Err(_) => {
            let _ = stop_tx.send(());
            let _ = worker.join();
            Err(WifiError::Internal(
                "Linux Wi-Fi listener exited during startup".to_string(),
            ))
        }
    }
}

fn worker_main(
    spec: ListenerSpec,
    sender: tokio::sync::mpsc::UnboundedSender<ConnectionEvent>,
    ready_tx: SyncSender<WifiResult<()>>,
    stop_rx: oneshot::Receiver<()>,
) {
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(_) => {
            let _ = ready_tx.send(Err(WifiError::Internal(
                "failed to create Linux listener runtime".to_string(),
            )));
            return;
        }
    };

    runtime.block_on(async move {
        let connection = match Connection::system().await {
            Ok(connection) => connection,
            Err(_) => {
                let _ = ready_tx.send(Err(WifiError::Dbus {
                    operation: "connect the Linux Wi-Fi event listener to the system bus"
                        .to_string(),
                }));
                return;
            }
        };
        match spec {
            ListenerSpec::NetworkManager { device_path } => {
                run_network_manager(connection, device_path, sender, ready_tx, stop_rx).await;
            }
            ListenerSpec::Iwd { station_path } => {
                run_iwd(connection, station_path, sender, ready_tx, stop_rx).await;
            }
        }
    });
}

async fn run_network_manager(
    connection: Connection,
    device_path: String,
    sender: tokio::sync::mpsc::UnboundedSender<ConnectionEvent>,
    ready_tx: SyncSender<WifiResult<()>>,
    mut stop_rx: oneshot::Receiver<()>,
) {
    let device = match Proxy::new(
        &connection,
        NETWORK_MANAGER_SERVICE,
        device_path.as_str(),
        NM_DEVICE_INTERFACE,
    )
    .await
    {
        Ok(proxy) => proxy,
        Err(_) => {
            let _ = ready_tx.send(Err(WifiError::Dbus {
                operation: "subscribe to NetworkManager state changes".to_string(),
            }));
            return;
        }
    };
    let mut signals = match device.receive_signal("StateChanged").await {
        Ok(signals) => signals,
        Err(_) => {
            let _ = ready_tx.send(Err(WifiError::Dbus {
                operation: "subscribe to NetworkManager state changes".to_string(),
            }));
            return;
        }
    };
    let _ = ready_tx.send(Ok(()));
    let mut last_connected = None;

    loop {
        tokio::select! {
            _ = &mut stop_rx => break,
            message = signals.next() => {
                let Some(message) = message else { break; };
                let Ok((new_state, _, _)) = message.body().deserialize::<(u32, u32, u32)>() else {
                    continue;
                };
                let connected = if new_state == 100 {
                    nm_connected_ssid(&connection, &device_path).await
                } else {
                    None
                };
                emit_transition(&sender, &mut last_connected, connected);
            }
        }
    }
}

async fn run_iwd(
    connection: Connection,
    station_path: String,
    sender: tokio::sync::mpsc::UnboundedSender<ConnectionEvent>,
    ready_tx: SyncSender<WifiResult<()>>,
    mut stop_rx: oneshot::Receiver<()>,
) {
    let station = match Proxy::new(
        &connection,
        IWD_SERVICE,
        station_path.as_str(),
        IWD_STATION_INTERFACE,
    )
    .await
    {
        Ok(proxy) => proxy,
        Err(_) => {
            let _ = ready_tx.send(Err(WifiError::Dbus {
                operation: "subscribe to iwd state changes".to_string(),
            }));
            return;
        }
    };
    let mut signals = match station.receive_signal("PropertiesChanged").await {
        Ok(signals) => signals,
        Err(_) => {
            let _ = ready_tx.send(Err(WifiError::Dbus {
                operation: "subscribe to iwd state changes".to_string(),
            }));
            return;
        }
    };
    let _ = ready_tx.send(Ok(()));
    let mut last_connected = None;

    loop {
        tokio::select! {
            _ = &mut stop_rx => break,
            message = signals.next() => {
                let Some(message) = message else { break; };
                let Ok((interface, changed, _invalidated)) = message
                    .body()
                    .deserialize::<(String, HashMap<String, OwnedValue>, Vec<String>)>() else {
                    continue;
                };
                if interface != IWD_STATION_INTERFACE {
                    continue;
                }
                let state = changed.get("State").and_then(value_string);
                let connected = match state.as_deref() {
                    Some("disconnected") | Some("disconnecting") => None,
                    _ => iwd_connected_ssid(&connection, &station_path).await,
                };
                emit_transition(&sender, &mut last_connected, connected);
            }
        }
    }
}

fn emit_transition(
    sender: &tokio::sync::mpsc::UnboundedSender<ConnectionEvent>,
    last_connected: &mut Option<String>,
    connected: Option<String>,
) {
    match (&*last_connected, &connected) {
        (None, Some(ssid)) => {
            let _ = sender.send(ConnectionEvent::Connected(ssid.clone()));
        }
        (Some(_), None) => {
            let _ = sender.send(ConnectionEvent::Disconnected);
        }
        (Some(old), Some(new)) if old != new => {
            let _ = sender.send(ConnectionEvent::Connected(new.clone()));
        }
        _ => {}
    }
    *last_connected = connected;
}

async fn nm_connected_ssid(connection: &Connection, device_path: &str) -> Option<String> {
    let device = Proxy::new(
        connection,
        NETWORK_MANAGER_SERVICE,
        device_path,
        NM_DEVICE_INTERFACE,
    )
    .await
    .ok()?;
    let state: u32 = device.get_property("State").await.ok()?;
    if state != 100 {
        return None;
    }
    let wireless = Proxy::new(
        connection,
        NETWORK_MANAGER_SERVICE,
        device_path,
        NM_DEVICE_WIFI_INTERFACE,
    )
    .await
    .ok()?;
    let access_point: OwnedObjectPath = wireless.get_property("ActiveAccessPoint").await.ok()?;
    if access_point.as_str() == "/" {
        return None;
    }
    let access_point = Proxy::new(
        connection,
        NETWORK_MANAGER_SERVICE,
        access_point.as_str(),
        "org.freedesktop.NetworkManager.AccessPoint",
    )
    .await
    .ok()?;
    let ssid: Vec<u8> = access_point.get_property("Ssid").await.ok()?;
    Some(String::from_utf8_lossy(&ssid).into_owned())
}

async fn iwd_connected_ssid(connection: &Connection, station_path: &str) -> Option<String> {
    let station = Proxy::new(connection, IWD_SERVICE, station_path, IWD_STATION_INTERFACE)
        .await
        .ok()?;
    let network_path: OwnedObjectPath = station.get_property("ConnectedNetwork").await.ok()?;
    if network_path.as_str() == "/" {
        return None;
    }
    let network = Proxy::new(
        connection,
        IWD_SERVICE,
        network_path.as_str(),
        "net.connman.iwd.Network",
    )
    .await
    .ok()?;
    network.get_property("Name").await.ok()
}
