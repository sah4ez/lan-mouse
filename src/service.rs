use crate::{
    capture::{Capture, CaptureType, ICaptureEvent},
    client::ClientManager,
    config::{Config, ConfigClient},
    connect::LanMouseConnection,
    crypto,
    dns::{DnsEvent, DnsResolver},
    emulation::{Emulation, EmulationEvent},
    listen::{LanMouseListener, ListenerCreationError},
};
use futures::StreamExt;
use hickory_resolver::ResolveError;
use lan_mouse_ipc::{
    AsyncFrontendListener, ClientConfig, ClientHandle, ClientState, FrontendEvent, FrontendRequest,
    IpcError, IpcListenerCreationError, Position, Status,
};
use log;
use std::{
    collections::{HashMap, HashSet, VecDeque},
    io,
    net::{IpAddr, SocketAddr},
    sync::{Arc, RwLock},
    time::Duration,
    thread,
};
use thiserror::Error;
use tokio::{process::Command, signal, sync::Notify};

#[cfg(target_os = "linux")]
use x11::{
    xlib::{self, XCloseDisplay, XDisplayHeight, XDisplayWidth, XQueryPointer},
};

#[derive(Debug, Error)]
pub enum ServiceError {
    #[error(transparent)]
    Dns(#[from] ResolveError),
    #[error(transparent)]
    IpcListen(#[from] IpcListenerCreationError),
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    ListenError(#[from] ListenerCreationError),
    #[error("failed to load certificate: `{0}`")]
    Certificate(#[from] crypto::Error),
}

pub struct Service {
    /// configuration
    config: Config,
    /// input capture
    capture: Capture,
    /// input emulation
    emulation: Emulation,
    /// dns resolver
    resolver: DnsResolver,
    /// frontend listener
    frontend_listener: AsyncFrontendListener,
    /// authorized public key sha256 fingerprints
    authorized_keys: Arc<RwLock<HashMap<String, String>>>,
    /// (outgoing) client information
    client_manager: ClientManager,
    /// current port
    port: u16,
    /// the public key fingerprint for (D)TLS
    public_key_fingerprint: String,
    /// notify for pending frontend events
    frontend_event_pending: Notify,
    /// frontend events queued for sending
    pending_frontend_events: VecDeque<FrontendEvent>,
    /// status of input capture (enabled / disabled)
    capture_status: Status,
    /// status of input emulation (enabled / disabled)
    emulation_status: Status,
    /// keep track of registered connections to avoid duplicate barriers
    incoming_conns: HashSet<SocketAddr>,
    /// map from capture handle to connection info
    incoming_conn_info: HashMap<ClientHandle, Incoming>,
    next_trigger_handle: u64,
    /// cursor tracking thread handle for X11 edge crossing detection
    #[cfg(target_os = "linux")]
    cursor_tracking_thread: Option<thread::JoinHandle<()>>,
    /// notify cursor tracking thread to stop
    #[cfg(target_os = "linux")]
    cursor_tracking_stop: Arc<std::sync::atomic::AtomicBool>,
    /// channel for edge crossing events from cursor tracking thread
    #[cfg(target_os = "linux")]
    edge_crossing_rx: local_channel::mpsc::Receiver<EdgeCrossingEvent>,
    /// channel for edge crossing events from cursor tracking thread
    #[cfg(target_os = "linux")]
    edge_crossing_tx: local_channel::mpsc::Sender<EdgeCrossingEvent>,
}

#[derive(Debug)]
struct Incoming {
    fingerprint: String,
    addr: SocketAddr,
    pos: Position,
}

/// Edge crossing event from cursor tracking thread
#[derive(Debug, Clone)]
struct EdgeCrossingEvent {
    addr: SocketAddr,
    pos: Position,
}

impl Service {
    pub async fn new(config: Config) -> Result<Self, ServiceError> {
        let client_manager = ClientManager::default();
        for client in config.clients() {
            let config = ClientConfig {
                hostname: client.hostname,
                fix_ips: client.ips.into_iter().collect(),
                port: client.port,
                pos: client.pos,
                cmd: client.enter_hook,
            };
            let state = ClientState {
                active: client.active,
                ips: HashSet::from_iter(config.fix_ips.iter().cloned()),
                ..Default::default()
            };
            let handle = client_manager.add_client();
            client_manager.set_config(handle, config);
            client_manager.set_state(handle, state);
        }

        // load certificate
        let cert = crypto::load_or_generate_key_and_cert(config.cert_path())?;
        let public_key_fingerprint = crypto::certificate_fingerprint(&cert);

        // create frontend communication adapter, exit if already running
        let frontend_listener = AsyncFrontendListener::new().await?;

        let authorized_keys = Arc::new(RwLock::new(config.authorized_fingerprints()));
        // listener + connection
        let listener =
            LanMouseListener::new(config.port(), cert.clone(), authorized_keys.clone()).await?;
        let conn = LanMouseConnection::new(cert.clone(), client_manager.clone());

        // input capture + emulation
        let capture_backend = config.capture_backend().map(|b| b.into());
        let capture = Capture::new(capture_backend, conn, config.release_bind());
        let emulation_backend = config.emulation_backend().map(|b| b.into());
        let emulation = Emulation::new(emulation_backend, listener);

        // create dns resolver
        let resolver = DnsResolver::new()?;

        let port = config.port();
        
        #[cfg(target_os = "linux")]
        let (edge_crossing_tx, edge_crossing_rx) = local_channel::mpsc::channel();
        #[cfg(target_os = "linux")]
        let (cursor_tracking_thread, cursor_tracking_stop) = Self::start_cursor_tracking(edge_crossing_tx);
        
        let service = Self {
            config,
            capture,
            emulation,
            frontend_listener,
            resolver,
            authorized_keys,
            public_key_fingerprint,
            client_manager,
            frontend_event_pending: Default::default(),
            port,
            pending_frontend_events: Default::default(),
            capture_status: Default::default(),
            emulation_status: Default::default(),
            incoming_conn_info: Default::default(),
            incoming_conns: Default::default(),
            next_trigger_handle: 0,
            #[cfg(target_os = "linux")]
            cursor_tracking_thread,
            #[cfg(target_os = "linux")]
            cursor_tracking_stop,
            #[cfg(target_os = "linux")]
            edge_crossing_rx,
            #[cfg(target_os = "linux")]
            edge_crossing_tx,
        };
        Ok(service)
    }

    #[cfg(target_os = "linux")]
    fn start_cursor_tracking(
        edge_crossing_tx: local_channel::mpsc::Sender<EdgeCrossingEvent>,
    ) -> (Option<thread::JoinHandle<()>>, Arc<std::sync::atomic::AtomicBool>) {
        use std::ptr;
        
        // Check if X11 is available
        let display_env = std::env::var("DISPLAY").unwrap_or_else(|_| ":0".to_string());
        log::info!("Starting cursor tracking for X11 display: {}", display_env);
        
        let display = unsafe {
            match xlib::XOpenDisplay(ptr::null()) {
                d if ptr::eq(d, ptr::null_mut::<xlib::Display>()) => {
                    log::warn!("Failed to open X11 display for cursor tracking. Edge crossing detection will not be available.");
                    return (None, Arc::new(std::sync::atomic::AtomicBool::new(true)));
                }
                display => display,
            }
        };
        
        let stop_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stop_flag_clone = stop_flag.clone();
        
        let handle = thread::spawn(move || {
            log::info!("Cursor tracking thread started");
            
            // Get screen dimensions
            let screen_width = unsafe { XDisplayWidth(display, 0) };
            let screen_height = unsafe { XDisplayHeight(display, 0) };
            log::info!("Screen dimensions: {}x{}", screen_width, screen_height);
            
            // Poll cursor position periodically
            let poll_interval = Duration::from_millis(50); // 20 Hz
            let mut last_edge_crossing_time = std::time::Instant::now();
            let edge_crossing_cooldown = Duration::from_millis(500); // Prevent rapid edge crossing events
            
            while !stop_flag_clone.load(std::sync::atomic::Ordering::Relaxed) {
                // Query cursor position
                let mut root_x: i32 = 0;
                let mut root_y: i32 = 0;
                let mut win_x: i32 = 0;
                let mut win_y: i32 = 0;
                let mut mask: u32 = 0;
                let mut root_return: xlib::Window = 0;
                let mut child_return: xlib::Window = 0;
                
                unsafe {
                    XQueryPointer(
                        display,
                        xlib::XDefaultRootWindow(display),
                        &mut root_return,
                        &mut child_return,
                        &mut root_x,
                        &mut root_y,
                        &mut win_x,
                        &mut win_y,
                        &mut mask,
                    );
                }
                
                // Check for edge crossings
                let edge_crossed = if root_x <= 0 {
                    Some(Position::Left)
                } else if root_x >= screen_width - 1 {
                    Some(Position::Right)
                } else if root_y <= 0 {
                    Some(Position::Top)
                } else if root_y >= screen_height - 1 {
                    Some(Position::Bottom)
                } else {
                    None
                };
                
                // If edge crossed and cooldown has elapsed, send event
                if let Some(pos) = edge_crossed {
                    let now = std::time::Instant::now();
                    if now.duration_since(last_edge_crossing_time) >= edge_crossing_cooldown {
                        log::info!("Cursor crossed edge at position {:?}", pos);
                        
                        // Send edge crossing event to service
                        // Note: We don't have the address here, so we'll send None
                        // The service will need to determine which connection to notify
                        let event = EdgeCrossingEvent {
                            addr: "0.0.0.0:0".parse().unwrap(), // Placeholder, will be determined by service
                            pos,
                        };
                        
                        if edge_crossing_tx.send(event).is_err() {
                            log::warn!("Failed to send edge crossing event: channel closed");
                            break;
                        }
                        
                        last_edge_crossing_time = now;
                    }
                }
                
                // Sleep for poll interval
                std::thread::sleep(poll_interval);
            }
            
            log::info!("Cursor tracking thread stopped");
            
            // Close display
            unsafe {
                XCloseDisplay(display);
            }
        });
        
        (Some(handle), stop_flag)
    }

    pub async fn run(&mut self) -> Result<(), ServiceError> {
        let active = self.client_manager.active_clients();
        for handle in active.iter() {
            // small hack: `activate_client()` checks, if the client
            // is already active in client_manager and does not create a
            // capture barrier in that case so we have to deactivate it first
            self.client_manager.deactivate_client(*handle);
        }

        for handle in active {
            self.activate_client(handle);
        }

        loop {
            #[cfg(target_os = "linux")]
            {
                tokio::select! {
                    request = self.frontend_listener.next() => self.handle_frontend_request(request),
                    _ = self.frontend_event_pending.notified() => self.handle_frontend_pending().await,
                    event = self.emulation.event() => self.handle_emulation_event(event),
                    event = self.capture.event() => self.handle_capture_event(event),
                    event = self.resolver.event() => self.handle_resolver_event(event),
                    edge_crossing = self.edge_crossing_rx.recv() => {
                        self.handle_edge_crossing_event(edge_crossing.expect("channel closed"));
                    }
                    r = signal::ctrl_c() => break r.expect("failed to wait for CTRL+C"),
                }
            }
            #[cfg(not(target_os = "linux"))]
            {
                tokio::select! {
                    request = self.frontend_listener.next() => self.handle_frontend_request(request),
                    _ = self.frontend_event_pending.notified() => self.handle_frontend_pending().await,
                    event = self.emulation.event() => self.handle_emulation_event(event),
                    event = self.capture.event() => self.handle_capture_event(event),
                    event = self.resolver.event() => self.handle_resolver_event(event),
                    r = signal::ctrl_c() => break r.expect("failed to wait for CTRL+C"),
                }
            }
        }

        log::info!("terminating service ...");
        
        #[cfg(target_os = "linux")]
        {
            log::debug!("stopping cursor tracking thread ...");
            if let Some(stop_flag) = &self.cursor_tracking_stop {
                stop_flag.store(true, std::sync::atomic::Ordering::Relaxed);
            }
            if let Some(handle) = self.cursor_tracking_thread.take() {
                let _ = handle.join();
            }
        }
        
        log::debug!("terminating capture ...");
        self.capture.terminate().await;
        log::debug!("terminating emulation ...");
        self.emulation.terminate().await;
        log::debug!("terminating dns resolver ...");
        self.resolver.terminate().await;

        Ok(())
    }

    #[cfg(target_os = "linux")]
    fn handle_edge_crossing_event(&mut self, event: EdgeCrossingEvent) {
        log::info!("Edge crossing event received: {:?}", event);
        
        // Find the incoming connection at the crossed position
        // We need to find which incoming connection corresponds to the crossed edge
        let incoming_addr = self.incoming_conn_info.values().find(|incoming| {
            // Check if the incoming connection's position matches the crossed edge
            // For example, if cursor crosses Left edge, we look for an incoming connection
            // that entered from the Right (opposite position)
            match event.pos {
                Position::Left => incoming.pos == Position::Right,
                Position::Right => incoming.pos == Position::Left,
                Position::Top => incoming.pos == Position::Bottom,
                Position::Bottom => incoming.pos == Position::Top,
            }
        }).map(|incoming| incoming.addr);
        
        if let Some(addr) = incoming_addr {
            log::info!("Found incoming connection at position {:?}, sending EdgeCrossed event", event.pos);
            self.emulation.edge_crossed(addr, event.pos);
        } else {
            log::debug!("No incoming connection found for edge position {:?}", event.pos);
        }
    }

    fn handle_frontend_request(&mut self, request: Option<Result<FrontendRequest, IpcError>>) {
        let request = match request.expect("frontend listener closed") {
            Ok(r) => r,
            Err(e) => return log::error!("error receiving request: {e}"),
        };
        match request {
            FrontendRequest::Activate(handle, active) => {
                self.set_client_active(handle, active);
                self.save_config();
            }
            FrontendRequest::AuthorizeKey(desc, fp) => {
                self.add_authorized_key(desc, fp);
                self.save_config();
            }
            FrontendRequest::ChangePort(port) => self.change_port(port),
            FrontendRequest::Create => {
                self.add_client();
                self.save_config();
            }
            FrontendRequest::Delete(handle) => {
                self.remove_client(handle);
                self.save_config();
            }
            FrontendRequest::EnableCapture => self.capture.reenable(),
            FrontendRequest::EnableEmulation => self.emulation.reenable(),
            FrontendRequest::Enumerate() => self.enumerate(),
            FrontendRequest::UpdateFixIps(handle, fix_ips) => {
                self.update_fix_ips(handle, fix_ips);
                self.save_config();
            }
            FrontendRequest::UpdateHostname(handle, host) => {
                self.update_hostname(handle, host);
                self.save_config();
            }
            FrontendRequest::UpdatePort(handle, port) => {
                self.update_port(handle, port);
                self.save_config();
            }
            FrontendRequest::UpdatePosition(handle, pos) => {
                self.update_pos(handle, pos);
                self.save_config();
            }
            FrontendRequest::ResolveDns(handle) => self.resolve(handle),
            FrontendRequest::Sync => self.sync_frontend(),
            FrontendRequest::RemoveAuthorizedKey(key) => {
                self.remove_authorized_key(key);
                self.save_config();
            }
            FrontendRequest::UpdateEnterHook(handle, enter_hook) => {
                self.update_enter_hook(handle, enter_hook)
            }
            FrontendRequest::SaveConfiguration => self.save_config(),
        }
    }

    fn save_config(&mut self) {
        let clients = self.client_manager.clients();
        let clients = clients
            .into_iter()
            .map(|(c, s)| ConfigClient {
                ips: HashSet::from_iter(c.fix_ips),
                hostname: c.hostname,
                port: c.port,
                pos: c.pos,
                active: s.active,
                enter_hook: c.cmd,
            })
            .collect();
        self.config.set_clients(clients);
        let authorized_keys = self.authorized_keys.read().expect("lock").clone();
        self.config.set_authorized_keys(authorized_keys);
        if let Err(e) = self.config.write_back() {
            log::warn!("failed to write config: {e}");
        }
    }

    async fn handle_frontend_pending(&mut self) {
        while let Some(event) = self.pending_frontend_events.pop_front() {
            self.frontend_listener.broadcast(event).await;
        }
    }

    fn handle_emulation_event(&mut self, event: EmulationEvent) {
        match event {
            EmulationEvent::ConnectionAttempt { fingerprint } => {
                self.notify_frontend(FrontendEvent::ConnectionAttempt { fingerprint });
            }
            EmulationEvent::Entered {
                addr,
                pos,
                fingerprint,
            } => {
                // check if already registered
                if !self.incoming_conns.contains(&addr) {
                    self.add_incoming(addr, pos, fingerprint.clone());
                    self.notify_frontend(FrontendEvent::DeviceEntered {
                        fingerprint,
                        addr,
                        pos,
                    });
                } else {
                    self.update_incoming(addr, pos, fingerprint);
                }
            }
            EmulationEvent::Disconnected { addr } => {
                if let Some(addr) = self.remove_incoming(addr) {
                    self.notify_frontend(FrontendEvent::IncomingDisconnected(addr));
                }
            }
            EmulationEvent::PortChanged(port) => match port {
                Ok(port) => {
                    self.port = port;
                    self.notify_frontend(FrontendEvent::PortChanged(port, None));
                }
                Err(e) => self
                    .notify_frontend(FrontendEvent::PortChanged(self.port, Some(format!("{e}")))),
            },
            EmulationEvent::EmulationDisabled => {
                self.emulation_status = Status::Disabled;
                self.notify_frontend(FrontendEvent::EmulationStatus(self.emulation_status));
            }
            EmulationEvent::EmulationEnabled => {
                self.emulation_status = Status::Enabled;
                self.notify_frontend(FrontendEvent::EmulationStatus(self.emulation_status));
            }
            EmulationEvent::ReleaseNotify => self.capture.release(),
            EmulationEvent::Connected { addr, fingerprint } => {
                self.notify_frontend(FrontendEvent::DeviceConnected { addr, fingerprint });
            }
            EmulationEvent::EdgeCrossed { addr, pos } => {
                log::info!("cursor crossed edge at position {:?} for connection {}", pos, addr);
                // Send Enter event to the remote machine to notify that cursor is returning
                if let Some(incoming) = self.incoming_conn_info.values().find(|i| i.addr == addr) {
                    log::info!("sending Enter event to remote machine at position {:?}", pos);
                    self.emulation.send_enter_event(addr, pos);
                }
            }
        }
    }

    fn handle_capture_event(&mut self, event: ICaptureEvent) {
        match event {
            ICaptureEvent::CaptureBegin(handle) => {
                // we entered the capture zone for an incoming connection
                // => notify it that its capture should be released
                if let Some(incoming) = self.incoming_conn_info.get(&handle) {
                    self.emulation.send_leave_event(incoming.addr);
                }
            }
            ICaptureEvent::CaptureDisabled => {
                self.capture_status = Status::Disabled;
                self.notify_frontend(FrontendEvent::CaptureStatus(self.capture_status));
            }
            ICaptureEvent::CaptureEnabled => {
                self.capture_status = Status::Enabled;
                self.notify_frontend(FrontendEvent::CaptureStatus(self.capture_status));
            }
            ICaptureEvent::ClientEntered(handle) => {
                log::info!("entering client {handle} ...");
                self.spawn_hook_command(handle);
            }
        }
    }

    fn handle_resolver_event(&mut self, event: DnsEvent) {
        let handle = match event {
            DnsEvent::Resolving(handle) => {
                self.client_manager.set_resolving(handle, true);
                handle
            }
            DnsEvent::Resolved(handle, hostname, ips) => {
                self.client_manager.set_resolving(handle, false);
                if let Err(e) = &ips {
                    log::warn!("could not resolve {hostname}: {e}");
                }
                let ips = ips.unwrap_or_default();
                self.client_manager.set_dns_ips(handle, ips);
                handle
            }
        };
        self.broadcast_client(handle);
    }

    fn resolve(&self, handle: ClientHandle) {
        if let Some(hostname) = self.client_manager.get_hostname(handle) {
            self.resolver.resolve(handle, hostname);
        }
    }

    fn sync_frontend(&mut self) {
        self.enumerate();
        self.notify_frontend(FrontendEvent::EmulationStatus(self.emulation_status));
        self.notify_frontend(FrontendEvent::CaptureStatus(self.capture_status));
        self.notify_frontend(FrontendEvent::PortChanged(self.port, None));
        self.notify_frontend(FrontendEvent::PublicKeyFingerprint(
            self.public_key_fingerprint.clone(),
        ));
        let keys = self.authorized_keys.read().expect("lock").clone();
        self.notify_frontend(FrontendEvent::AuthorizedUpdated(keys));
    }

    const ENTER_HANDLE_BEGIN: u64 = u64::MAX / 2 + 1;

    fn add_incoming(&mut self, addr: SocketAddr, pos: Position, fingerprint: String) {
        let handle = Self::ENTER_HANDLE_BEGIN + self.next_trigger_handle;
        self.next_trigger_handle += 1;
        self.capture.create(handle, pos, CaptureType::EnterOnly);
        self.incoming_conns.insert(addr);
        self.incoming_conn_info.insert(
            handle,
            Incoming {
                fingerprint,
                addr,
                pos,
            },
        );
    }

    fn update_incoming(&mut self, addr: SocketAddr, pos: Position, fingerprint: String) {
        let incoming = self
            .incoming_conn_info
            .iter_mut()
            .find(|(_, i)| i.addr == addr)
            .map(|(_, i)| i)
            .expect("no such client");
        let mut changed = false;
        if incoming.fingerprint != fingerprint {
            incoming.fingerprint = fingerprint.clone();
            changed = true;
        }
        if incoming.pos != pos {
            incoming.pos = pos;
            changed = true;
        }
        if changed {
            self.remove_incoming(addr);
            self.add_incoming(addr, pos, fingerprint.clone());
            self.notify_frontend(FrontendEvent::IncomingDisconnected(addr));
            self.notify_frontend(FrontendEvent::DeviceEntered {
                fingerprint,
                addr,
                pos,
            });
        }
    }

    fn remove_incoming(&mut self, addr: SocketAddr) -> Option<SocketAddr> {
        let handle = self
            .incoming_conn_info
            .iter()
            .find(|(_, incoming)| incoming.addr == addr)
            .map(|(k, _)| *k)?;
        self.capture.destroy(handle);
        self.incoming_conns.remove(&addr);
        self.incoming_conn_info
            .remove(&handle)
            .map(|incoming| incoming.addr)
    }

    fn notify_frontend(&mut self, event: FrontendEvent) {
        self.pending_frontend_events.push_back(event);
        self.frontend_event_pending.notify_one();
    }

    fn add_authorized_key(&mut self, desc: String, fp: String) {
        self.authorized_keys.write().expect("lock").insert(fp, desc);
        let keys = self.authorized_keys.read().expect("lock").clone();
        self.notify_frontend(FrontendEvent::AuthorizedUpdated(keys));
    }

    fn remove_authorized_key(&mut self, fp: String) {
        self.authorized_keys.write().expect("lock").remove(&fp);
        let keys = self.authorized_keys.read().expect("lock").clone();
        self.notify_frontend(FrontendEvent::AuthorizedUpdated(keys));
    }

    fn enumerate(&mut self) {
        let clients = self.client_manager.get_client_states();
        self.notify_frontend(FrontendEvent::Enumerate(clients));
    }

    fn add_client(&mut self) {
        let handle = self.client_manager.add_client();
        log::info!("added client {handle}");
        let (c, s) = self.client_manager.get_state(handle).unwrap();
        self.notify_frontend(FrontendEvent::Created(handle, c, s));
    }

    fn set_client_active(&mut self, handle: ClientHandle, active: bool) {
        if active {
            self.activate_client(handle);
        } else {
            self.deactivate_client(handle);
        }
    }

    fn deactivate_client(&mut self, handle: ClientHandle) {
        log::debug!("deactivating client {handle}");
        if self.client_manager.deactivate_client(handle) {
            self.capture.destroy(handle);
            self.broadcast_client(handle);
            log::info!("deactivated client {handle}");
        }
    }

    fn activate_client(&mut self, handle: ClientHandle) {
        log::debug!("activating client");

        /* resolve dns on activate */
        self.resolve(handle);

        /* deactivate potential other client at this position */
        let Some(pos) = self.client_manager.get_pos(handle) else {
            return;
        };

        if let Some(other) = self.client_manager.client_at(pos) {
            if other != handle {
                self.deactivate_client(other);
            }
        }

        /* activate the client */
        if self.client_manager.activate_client(handle) {
            /* notify capture and frontends */
            self.capture.create(handle, pos, CaptureType::Default);
            self.broadcast_client(handle);
            log::info!("activated client {handle} ({pos})");
        }
    }

    fn change_port(&mut self, port: u16) {
        if self.port != port {
            self.emulation.request_port_change(port);
        } else {
            self.notify_frontend(FrontendEvent::PortChanged(self.port, None));
        }
    }

    fn remove_client(&mut self, handle: ClientHandle) {
        if self
            .client_manager
            .remove_client(handle)
            .map(|(_, s)| s.active)
            .unwrap_or(false)
        {
            self.capture.destroy(handle);
        }
        self.notify_frontend(FrontendEvent::Deleted(handle));
    }

    fn update_fix_ips(&mut self, handle: ClientHandle, fix_ips: Vec<IpAddr>) {
        self.client_manager.set_fix_ips(handle, fix_ips);
        self.broadcast_client(handle);
    }

    fn update_hostname(&mut self, handle: ClientHandle, hostname: Option<String>) {
        log::info!("hostname changed: {hostname:?}");
        if self.client_manager.set_hostname(handle, hostname.clone()) {
            self.resolve(handle);
        }
        self.broadcast_client(handle);
    }

    fn update_port(&mut self, handle: ClientHandle, port: u16) {
        self.client_manager.set_port(handle, port);
        self.broadcast_client(handle);
    }

    fn update_pos(&mut self, handle: ClientHandle, pos: Position) {
        // update state in event input emulator & input capture
        if self.client_manager.set_pos(handle, pos) {
            self.deactivate_client(handle);
            self.activate_client(handle);
        }
        self.broadcast_client(handle);
    }

    fn update_enter_hook(&mut self, handle: ClientHandle, enter_hook: Option<String>) {
        self.client_manager.set_enter_hook(handle, enter_hook);
        self.broadcast_client(handle);
    }

    fn broadcast_client(&mut self, handle: ClientHandle) {
        let event = self
            .client_manager
            .get_state(handle)
            .map(|(c, s)| FrontendEvent::State(handle, c, s))
            .unwrap_or(FrontendEvent::NoSuchClient(handle));
        self.notify_frontend(event);
    }

    fn spawn_hook_command(&self, handle: ClientHandle) {
        let Some(cmd) = self.client_manager.get_enter_cmd(handle) else {
            return;
        };
        tokio::task::spawn_local(async move {
            log::info!("spawning command!");
            let mut child = match Command::new("sh").arg("-c").arg(cmd.as_str()).spawn() {
                Ok(c) => c,
                Err(e) => {
                    log::warn!("could not execute cmd: {e}");
                    return;
                }
            };
            match child.wait().await {
                Ok(s) => {
                    if s.success() {
                        log::info!("{cmd} exited successfully");
                    } else {
                        log::warn!("{cmd} exited with {s}");
                    }
                }
                Err(e) => log::warn!("{cmd}: {e}"),
            }
        });
    }
}
