use crate::client::ClientManager;
use lan_mouse_ipc::{ClientHandle, DEFAULT_PORT};
use lan_mouse_proto::{MAX_EVENT_SIZE, ProtoEvent};
use local_channel::mpsc::{Receiver, Sender, channel};
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    io,
    net::SocketAddr,
    rc::Rc,
    sync::Arc,
    time::Duration,
};
use thiserror::Error;
use tokio::{
    net::UdpSocket,
    sync::Mutex,
    task::{JoinSet, spawn_local},
};
use webrtc_dtls::{
    config::{Config, ExtendedMasterSecretType},
    conn::DTLSConn,
    crypto::Certificate,
};
use webrtc_util::Conn;

#[derive(Debug, Error)]
pub(crate) enum LanMouseConnectionError {
    #[error(transparent)]
    Bind(#[from] io::Error),
    #[error(transparent)]
    Dtls(#[from] webrtc_dtls::Error),
    #[error(transparent)]
    Webrtc(#[from] webrtc_util::Error),
    #[error("not connected")]
    NotConnected,
    #[error("emulation is disabled on the target device")]
    TargetEmulationDisabled,
    #[error("Connection timed out")]
    Timeout,
}

const DEFAULT_CONNECTION_TIMEOUT: Duration = Duration::from_secs(5);

async fn connect(
    addr: SocketAddr,
    cert: Certificate,
) -> Result<(Arc<dyn Conn + Sync + Send>, SocketAddr), (SocketAddr, LanMouseConnectionError)> {
    log::info!("connecting to {addr} ...");
    log::debug!("Creating UDP socket for connection to {addr}");
    let conn = Arc::new(
        UdpSocket::bind("0.0.0.0:0")
            .await
            .map_err(|e| {
                log::error!("Failed to bind UDP socket: {e}");
                (addr, e.into())
            })?,
    );
    log::debug!("Connecting UDP socket to {addr}");
    conn.connect(addr).await.map_err(|e| {
        log::error!("Failed to connect UDP socket to {addr}: {e}");
        (addr, e.into())
    })?;
    let config = Config {
        certificates: vec![cert],
        server_name: "ignored".to_owned(),
        insecure_skip_verify: true,
        // Use Request instead of Require for better compatibility with different DTLS implementations
        extended_master_secret: ExtendedMasterSecretType::Request,
        // Request client certificate from server (server may or may not require it)
        client_auth: webrtc_dtls::config::ClientAuthType::RequestClientCert,
        ..Default::default()
    };
    log::debug!("Starting DTLS handshake with {addr} (timeout: {:?})", DEFAULT_CONNECTION_TIMEOUT);
    let timeout = tokio::time::sleep(DEFAULT_CONNECTION_TIMEOUT);
    tokio::select! {
        _ = timeout => {
            log::error!("Connection to {addr} timed out after {:?}", DEFAULT_CONNECTION_TIMEOUT);
            log::error!("This typically means:");
            log::error!("  1. The remote lan-mouse daemon is not running");
            log::error!("  2. The remote daemon is not listening on port {}", addr.port());
            log::error!("  3. A firewall is blocking the connection");
            log::error!("  4. The IP address {} is incorrect or unreachable", addr.ip());
            log::error!("");
            log::error!("Troubleshooting steps:");
            log::error!("  1. Run the diagnostic script: ./scripts/diagnose-connection.sh {}", addr.ip());
            log::error!("  2. Check if lan-mouse is running on the remote machine: ssh {} 'pgrep -f lan-mouse'", addr.ip());
            log::error!("  3. Test network connectivity: ping -c 3 {}", addr.ip());
            log::error!("  4. Check if the port is open: nc -zv {} {}", addr.ip(), addr.port());
            log::error!("  5. Verify firewall settings on both machines");
            Err((addr, LanMouseConnectionError::Timeout))
        }
        result = DTLSConn::new(conn, config, true, None) => match result {
            Ok(dtls_conn) => {
                log::info!("Successfully established DTLS connection with {addr}");
                Ok((Arc::new(dtls_conn), addr))
            }
            Err(e) => {
                log::error!("DTLS handshake failed with {addr}: {e}");
                log::error!("This may be due to:");
                log::error!("  - Network connectivity issues (firewall, NAT)");
                log::error!("  - DTLS version or cipher suite mismatch");
                log::error!("  - Certificate validation issues");
                log::error!("  - The remote device may not be running or may have a different version");
                log::error!("Try:");
                log::error!("  - Check if the remote device is running lan-mouse");
                log::error!("  - Verify network connectivity (ping {addr})");
                log::error!("  - Check firewall settings");
                log::error!("  - Ensure both devices are running compatible versions");
                Err((addr, e.into()))
            }
        }
    }
}

async fn connect_any(
    addrs: &[SocketAddr],
    cert: Certificate,
) -> Result<(Arc<dyn Conn + Send + Sync>, SocketAddr), LanMouseConnectionError> {
    log::debug!("Attempting to connect to {} address(es): {:?}", addrs.len(), addrs);
    let mut joinset = JoinSet::new();
    for &addr in addrs {
        log::debug!("Spawning connection task for {addr}");
        joinset.spawn_local(connect(addr, cert.clone()));
    }
    loop {
        match joinset.join_next().await {
            None => {
                log::error!("All connection attempts failed for addresses: {:?}", addrs);
                return Err(LanMouseConnectionError::NotConnected);
            }
            Some(r) => match r.expect("join error") {
                Ok(conn) => {
                    log::info!("Successfully connected to one of the addresses");
                    return Ok(conn);
                }
                Err((a, e)) => {
                    log::warn!("failed to connect to {a}: `{e}`");
                    log::debug!("Remaining connection attempts: {}", joinset.len());
                }
            },
        };
    }
}

pub(crate) struct LanMouseConnection {
    cert: Certificate,
    client_manager: ClientManager,
    conns: Rc<Mutex<HashMap<SocketAddr, Arc<dyn Conn + Send + Sync>>>>,
    connecting: Rc<Mutex<HashSet<ClientHandle>>>,
    recv_rx: Receiver<(ClientHandle, ProtoEvent)>,
    recv_tx: Sender<(ClientHandle, ProtoEvent)>,
    ping_response: Rc<RefCell<HashSet<SocketAddr>>>,
}

impl LanMouseConnection {
    pub(crate) fn new(cert: Certificate, client_manager: ClientManager) -> Self {
        let (recv_tx, recv_rx) = channel();
        Self {
            cert,
            client_manager,
            conns: Default::default(),
            connecting: Default::default(),
            recv_rx,
            recv_tx,
            ping_response: Default::default(),
        }
    }

    pub(crate) async fn recv(&mut self) -> (ClientHandle, ProtoEvent) {
        self.recv_rx.recv().await.expect("channel closed")
    }

    pub(crate) async fn send(
        &self,
        event: ProtoEvent,
        handle: ClientHandle,
    ) -> Result<(), LanMouseConnectionError> {
        let (buf, len): ([u8; MAX_EVENT_SIZE], usize) = event.into();
        let buf = &buf[..len];
        if let Some(addr) = self.client_manager.active_addr(handle) {
            let conn = {
                let conns = self.conns.lock().await;
                conns.get(&addr).cloned()
            };
            if let Some(conn) = conn {
                if !self.client_manager.alive(handle) {
                    return Err(LanMouseConnectionError::TargetEmulationDisabled);
                }
                match conn.send(buf).await {
                    Ok(_) => {}
                    Err(e) => {
                        log::warn!("client {handle} failed to send: {e}");
                        disconnect(&self.client_manager, handle, addr, &self.conns, &self.recv_tx).await;
                    }
                }
                log::trace!("{event} >->->->->- {addr}");
                return Ok(());
            }
        }

        // check if we are already trying to connect
        let mut connecting = self.connecting.lock().await;
        if !connecting.contains(&handle) {
            connecting.insert(handle);
            // connect in the background
            spawn_local(connect_to_handle(
                self.client_manager.clone(),
                self.cert.clone(),
                handle,
                self.conns.clone(),
                self.connecting.clone(),
                self.recv_tx.clone(),
                self.ping_response.clone(),
            ));
        }
        Err(LanMouseConnectionError::NotConnected)
    }
}

async fn connect_to_handle(
    client_manager: ClientManager,
    cert: Certificate,
    handle: ClientHandle,
    conns: Rc<Mutex<HashMap<SocketAddr, Arc<dyn Conn + Send + Sync>>>>,
    connecting: Rc<Mutex<HashSet<ClientHandle>>>,
    tx: Sender<(ClientHandle, ProtoEvent)>,
    ping_response: Rc<RefCell<HashSet<SocketAddr>>>,
) -> Result<(), LanMouseConnectionError> {
    log::info!("client {handle} connecting ...");
    // sending did not work, figure out active conn.
    if let Some(addrs) = client_manager.get_ips(handle) {
        let port = client_manager.get_port(handle).unwrap_or(DEFAULT_PORT);
        let addrs = addrs
            .into_iter()
            .map(|a| SocketAddr::new(a, port))
            .collect::<Vec<_>>();
        log::info!("client ({handle}) connecting ... (ips: {addrs:?})");
        log::debug!("Attempting to connect to {} address(es)", addrs.len());
        let res = connect_any(&addrs, cert).await;
        let (conn, addr) = match res {
            Ok(c) => c,
            Err(e) => {
                log::warn!("Failed to connect client {handle} to any of the addresses: {e}");
                log::warn!("Client {handle} will remain disconnected until the remote daemon becomes available");
                connecting.lock().await.remove(&handle);
                return Err(e);
            }
        };
        log::info!("client ({handle}) connected @ {addr}");
        client_manager.set_active_addr(handle, Some(addr));
        conns.lock().await.insert(addr, conn.clone());
        connecting.lock().await.remove(&handle);

        // poll connection for active
        spawn_local(ping_pong(addr, conn.clone(), ping_response.clone()));

        // receiver
        spawn_local(receive_loop(
            client_manager,
            handle,
            addr,
            conn,
            conns,
            tx,
            ping_response.clone(),
        ));
        return Ok(());
    }
    log::warn!("No IP addresses configured for client {handle}");
    connecting.lock().await.remove(&handle);
    Err(LanMouseConnectionError::NotConnected)
}

async fn ping_pong(
    addr: SocketAddr,
    conn: Arc<dyn Conn + Send + Sync>,
    ping_response: Rc<RefCell<HashSet<SocketAddr>>>,
) {
    loop {
        let (buf, len) = ProtoEvent::Ping.into();

        // send 4 pings, at least one must be answered
        for _ in 0..4 {
            if let Err(e) = conn.send(&buf[..len]).await {
                log::warn!("{addr}: send error `{e}`, closing connection");
                let _ = conn.close().await;
                break;
            }
            log::trace!("PING >->->->->- {addr}");

            tokio::time::sleep(Duration::from_millis(500)).await;
        }

        if !ping_response.borrow_mut().remove(&addr) {
            log::warn!("{addr} did not respond, closing connection");
            let _ = conn.close().await;
            return;
        }
    }
}

async fn receive_loop(
    client_manager: ClientManager,
    handle: ClientHandle,
    addr: SocketAddr,
    conn: Arc<dyn Conn + Send + Sync>,
    conns: Rc<Mutex<HashMap<SocketAddr, Arc<dyn Conn + Send + Sync>>>>,
    tx: Sender<(ClientHandle, ProtoEvent)>,
    ping_response: Rc<RefCell<HashSet<SocketAddr>>>,
) {
    let mut buf = [0u8; MAX_EVENT_SIZE];
    while conn.recv(&mut buf).await.is_ok() {
        if let Ok(event) = buf.try_into() {
            log::trace!("{addr} <==<==<== {event}");
            match event {
                ProtoEvent::Pong(b) => {
                    client_manager.set_active_addr(handle, Some(addr));
                    client_manager.set_alive(handle, b);
                    ping_response.borrow_mut().insert(addr);
                }
                event => tx.send((handle, event)).expect("channel closed"),
            }
        }
    }
    log::warn!("recv error");
    disconnect(&client_manager, handle, addr, &conns, &tx).await;
}

async fn disconnect(
    client_manager: &ClientManager,
    handle: ClientHandle,
    addr: SocketAddr,
    conns: &Mutex<HashMap<SocketAddr, Arc<dyn Conn + Send + Sync>>>,
    tx: &Sender<(ClientHandle, ProtoEvent)>,
) {
    log::warn!("client ({handle}) @ {addr} connection closed");
    conns.lock().await.remove(&addr);
    client_manager.set_active_addr(handle, None);
    
    // Notify capture task about disconnection by sending Leave event
    // This ensures the capture state is reset properly
    log::info!("sending Leave event to capture task for handle {handle} due to disconnect");
    if let Err(e) = tx.send((handle, ProtoEvent::Leave(0))) {
        log::warn!("failed to send disconnect notification to capture task: {e}");
    }
    
    let active: Vec<SocketAddr> = conns.lock().await.keys().copied().collect();
    log::info!("active connections: {active:?}");
}
