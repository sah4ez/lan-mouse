use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    task::{Context, Poll},
    time::{Duration, Instant},
};

use futures::{Stream, StreamExt};
use input_capture::{
    CaptureError, CaptureEvent, CaptureHandle, InputCapture, InputCaptureError, Position,
};
use input_event::scancode;
use lan_mouse_proto::ProtoEvent;
use local_channel::mpsc::{Receiver, Sender, channel};
use tokio::task::{JoinHandle, spawn_local};
use tokio_util::sync::CancellationToken;

use crate::connect::LanMouseConnection;

pub(crate) struct Capture {
    cancellation_token: CancellationToken,
    request_tx: Sender<CaptureRequest>,
    task: JoinHandle<()>,
    event_rx: Receiver<ICaptureEvent>,
}

pub(crate) enum ICaptureEvent {
    /// a client was entered
    CaptureBegin(CaptureHandle),
    /// capture disabled
    CaptureDisabled,
    /// capture disabled
    CaptureEnabled,
    /// A (new) client was entered.
    /// In contrast to [`ICaptureEvent::CaptureBegin`] this
    /// event is only triggered when the capture was
    /// explicitly released in the meantime by
    /// either the remote client leaving its device region,
    /// a new device entering the screen or the release bind.
    ClientEntered(u64),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CaptureType {
    /// a normal input capture
    Default,
    /// A capture only interested in [`CaptureEvent::Begin`] events.
    /// The capture is released immediately, if there is no
    /// Default capture at the same position.
    EnterOnly,
}

#[derive(Clone, Copy, Debug)]
enum CaptureRequest {
    /// capture must release the mouse
    Release,
    /// add a capture client
    Create(CaptureHandle, Position, CaptureType),
    /// destory a capture client
    Destroy(CaptureHandle),
    /// reenable input capture
    Reenable,
}

impl Capture {
    pub(crate) fn new(
        backend: Option<input_capture::Backend>,
        conn: LanMouseConnection,
        release_bind: Vec<scancode::Linux>,
    ) -> Self {
        let (request_tx, request_rx) = channel();
        let (event_tx, event_rx) = channel();
        let cancellation_token = CancellationToken::new();
        let capture_task = CaptureTask {
            active_client: None,
            incoming_client: None, // Track the client that's sending events TO us
            backend,
            cancellation_token: cancellation_token.clone(),
            captures: Default::default(),
            conn,
            event_tx,
            request_rx,
            release_bind: Rc::new(RefCell::new(release_bind)),
            state: Default::default(),
            prevent_capture_recreation: false,
            wait_for_ack_since: None,
            ack_timeout: Duration::from_secs(5),
        };
        let task = spawn_local(capture_task.run());
        Self {
            cancellation_token,
            request_tx,
            task,
            event_rx,
        }
    }

    pub(crate) fn reenable(&self) {
        self.request_tx
            .send(CaptureRequest::Reenable)
            .expect("channel closed");
    }

    pub(crate) async fn terminate(&mut self) {
        self.cancellation_token.cancel();
        log::debug!("terminating capture");
        if let Err(e) = (&mut self.task).await {
            log::warn!("{e}");
        }
    }

    pub(crate) fn create(
        &self,
        handle: CaptureHandle,
        pos: lan_mouse_ipc::Position,
        capture_type: CaptureType,
    ) {
        let pos = to_capture_pos(pos);
        self.request_tx
            .send(CaptureRequest::Create(handle, pos, capture_type))
            .expect("channel closed");
    }

    pub(crate) fn destroy(&self, handle: CaptureHandle) {
        self.request_tx
            .send(CaptureRequest::Destroy(handle))
            .expect("channel closed");
    }

    pub(crate) fn release(&self) {
        self.request_tx
            .send(CaptureRequest::Release)
            .expect("channel closed");
    }

    pub(crate) async fn event(&mut self) -> ICaptureEvent {
        self.event_rx.recv().await.expect("channel closed")
    }
}

/// debounce a statement `$st`, i.e. the statement is executed only if the
/// time since the previous execution is at least `$dur`.
/// `$prev` is used to keep track of this timestamp
macro_rules! debounce {
    ($prev:ident, $dur:expr, $st:stmt) => {
        let exec = match $prev.get() {
            None => true,
            Some(instant) if instant.elapsed() > $dur => true,
            _ => false,
        };
        if exec {
            $prev.replace(Some(Instant::now()));
            $st
        }
    };
}

struct CaptureTask {
    active_client: Option<CaptureHandle>,
    incoming_client: Option<CaptureHandle>, // Track the client that's sending events TO us
    backend: Option<input_capture::Backend>,
    cancellation_token: CancellationToken,
    captures: Vec<(CaptureHandle, Position, CaptureType)>,
    conn: LanMouseConnection,
    event_tx: Sender<ICaptureEvent>,
    release_bind: Rc<RefCell<Vec<scancode::Linux>>>,
    request_rx: Receiver<CaptureRequest>,
    state: State,
    /// Flag to prevent capture recreation when a client has just entered the device
    prevent_capture_recreation: bool,
    /// Timestamp when we entered WaitingForAck state (for timeout detection)
    wait_for_ack_since: Option<Instant>,
    /// Timeout for WaitingForAck state before resetting
    ack_timeout: Duration,
}

impl CaptureTask {
    fn add_capture(&mut self, handle: CaptureHandle, pos: Position, capture_type: CaptureType) {
        self.captures.push((handle, pos, capture_type));
    }

    fn remove_capture(&mut self, handle: CaptureHandle) {
        self.captures.retain(|&(h, ..)| handle != h);
    }

    fn is_default_capture_at(&self, pos: Position) -> bool {
        self.captures
            .iter()
            .any(|&(_, p, t)| p == pos && t == CaptureType::Default)
    }

    fn get_pos(&self, handle: CaptureHandle) -> Position {
        self.captures
            .iter()
            .find(|(h, ..)| *h == handle)
            .expect("no such capture")
            .1
    }

    fn get_type(&self, handle: CaptureHandle) -> CaptureType {
        self.captures
            .iter()
            .find(|(h, ..)| *h == handle)
            .expect("no such capture")
            .2
    }

    async fn run(mut self) {
        loop {
            if let Err(e) = self.do_capture().await {
                log::warn!("input capture exited: {e}");
            }
            loop {
                tokio::select! {
                    r = self.request_rx.recv() => match r.expect("channel closed") {
                        CaptureRequest::Reenable => break,
                        CaptureRequest::Create(h, p, t) => self.add_capture(h, p, t),
                        CaptureRequest::Destroy(h) => self.remove_capture(h),
                        CaptureRequest::Release => { /* nothing to do */ }
                    },
                    _ = self.cancellation_token.cancelled() => return,
                }
            }
        }
    }

    async fn do_capture(&mut self) -> Result<(), InputCaptureError> {
        /* allow cancelling capture request */
        let mut capture = tokio::select! {
            r = InputCapture::new(self.backend) => r?,
            _ = self.cancellation_token.cancelled() => return Ok(()),
        };

        let _capture_guard = DropGuard::new(
            self.event_tx.clone(),
            ICaptureEvent::CaptureEnabled,
            ICaptureEvent::CaptureDisabled,
        );

        /* create barriers for active clients */
        /* prevent capture recreation when a client has just entered the device */
        if !self.prevent_capture_recreation {
            let r = self.create_captures(&mut capture).await;
            if let Err(e) = r {
                capture.terminate().await?;
                return Err(e.into());
            }
        }
        self.prevent_capture_recreation = false;

        let r = self.do_capture_session(&mut capture).await;

        // FIXME replace with async drop when stabilized
        capture.terminate().await?;

        r
    }

    async fn create_captures(&mut self, capture: &mut InputCapture) -> Result<(), CaptureError> {
        let captures = self.captures.clone();
        log::debug!("creating {} capture barriers", captures.len());
        for (handle, pos, _type) in captures {
            log::debug!("creating capture barrier: handle={handle}, pos={pos:?}, type={_type:?}");
            tokio::select! {
                r = capture.create(handle, pos) => r?,
                _ = self.cancellation_token.cancelled() => return Ok(()),
            }
        }
        log::debug!("all capture barriers created successfully");
        Ok(())
    }

    async fn do_capture_session(
        &mut self,
        capture: &mut InputCapture,
    ) -> Result<(), InputCaptureError> {
        log::debug!("starting capture session, state: {:?}, active_client: {:?}", self.state, self.active_client);
        log::debug!("active captures: {:?}", self.captures);

        // Create a timer to periodically poll for edge detection
        // This ensures edge detection works even when there are no XRecord events
        // (e.g., when cursor is being emulated by remote client)
        let mut edge_check_interval = tokio::time::interval(tokio::time::Duration::from_millis(10));
        // Timer for checking ACK timeout
        let mut ack_timeout_check_interval = tokio::time::interval(tokio::time::Duration::from_millis(500));

        loop {
            tokio::select! {
                event = capture.next() => match event {
                    Some(event) => {
                        let event = event?;
                        log::trace!("capture event received: handle={}, event={:?}", event.0, event.1);
                        self.handle_capture_event(capture, event).await?
                    },
                    None => {
                        log::debug!("capture stream ended");
                        return Ok(())
                    },
                },
                (handle, event) = self.conn.recv() => {
                    if let Some(active) = self.active_client {
                        if handle != active {
                            // we only care about events coming from the client we are currently connected to
                            // only `Ack` and `Leave` are relevant
                            continue
                        }
                    }

                    match event {
                        // connection acknowlegded => set state to Sending
                        ProtoEvent::Ack(_) => {
                            log::info!("client {handle} acknowledged the connection!");
                            self.state = State::Sending;
                            self.wait_for_ack_since = None;
                        }
                        // client disconnected
                        ProtoEvent::Leave(_) => {
                            log::info!("releasing capture: left remote client device region or connection lost");
                            self.release_capture(capture).await?;
                            // Reset state to WaitingForAck when client disconnects
                            self.state = State::WaitingForAck;
                            self.wait_for_ack_since = None;
                        },
                        _ => {}
                    }
                },
                e = self.request_rx.recv() => match e.expect("channel closed") {
                    CaptureRequest::Reenable => { log::debug!("reenable request received (already active)"); },
                    CaptureRequest::Release => {
                        log::debug!("release request received");
                        self.release_capture(capture).await?
                    },
                    CaptureRequest::Create(h, p, t) => {
                        log::debug!("create capture request: handle={h}, pos={p:?}, type={t:?}");
                        self.add_capture(h, p, t);
                        capture.create(h, p).await?;
                    }
                    CaptureRequest::Destroy(h) => {
                        log::debug!("destroy capture request: handle={h}");
                        self.remove_capture(h);
                        capture.destroy(h).await?;
                    }
                },
                _ = edge_check_interval.tick() => {
                    // Periodically poll the capture stream for edge detection
                    // This is needed because XRecord only generates events for real input,
                    // not for emulated cursor movement
                    if let Poll::Ready(Some(event)) = Pin::new(&mut *capture).poll_next(&mut Context::from_waker(futures::task::noop_waker_ref())) {
                        if let Ok((pos, event)) = event {
                            log::trace!("periodic edge check: handle={}, event={:?}", pos, event);
                            self.handle_capture_event(capture, (pos, event)).await?;
                        }
                    }
                },
                _ = ack_timeout_check_interval.tick() => {
                    // Check for ACK timeout - if we've been waiting too long, reset state
                    if let Some(since) = self.wait_for_ack_since {
                        if since.elapsed() > self.ack_timeout {
                            log::warn!("ACK timeout after {:?}, resetting capture state", since.elapsed());
                            self.release_capture(capture).await?;
                            self.state = State::WaitingForAck;
                            self.wait_for_ack_since = None;
                        }
                    }
                },
                _ = self.cancellation_token.cancelled() => break,
            }
        }
        Ok(())
    }

    async fn handle_capture_event(
        &mut self,
        capture: &mut InputCapture,
        event: (CaptureHandle, CaptureEvent),
    ) -> Result<(), CaptureError> {
        let (handle, event) = event;
        let capture_type = self.captures.iter().find(|(h, _, _)| *h == handle).map(|(_, _, t)| t);
        log::debug!("handle_capture_event: handle={handle}, event={event:?}, type={capture_type:?}, state={:?}, active_client={:?}",
            self.state, self.active_client);

        // Check if release bind is pressed
        if capture.keys_pressed(&self.release_bind.borrow()) {
            log::info!("releasing capture: release-bind pressed");
            return self.release_capture(capture).await;
        }

        // Global keyboard handle (u64::MAX) is used only for release bind checking
        // Skip further processing for this special handle
        const GLOBAL_KEYBOARD_HANDLE: u64 = u64::MAX;
        if handle == GLOBAL_KEYBOARD_HANDLE {
            log::trace!("global keyboard handle, skipping further processing");
            return Ok(());
        }

        if event == CaptureEvent::Begin {
            let pos = self.get_pos(handle);
            let cap_type = self.get_type(handle);
            log::info!("cursor reached screen edge: {pos:?} (handle: {handle}, type: {cap_type:?})");
            self.event_tx
                .send(ICaptureEvent::CaptureBegin(handle))
                .expect("channel closed");
        }

        // enter only capture (for incoming connections)
        if self.get_type(handle) == CaptureType::EnterOnly {
            // This is an incoming connection - track it
            self.incoming_client = Some(handle);
            log::info!("set incoming_client: {:?}", handle);
            // if there is no active outgoing connection at the current capture,
            // we release the capture
            if !self.is_default_capture_at(self.get_pos(handle)) {
                log::info!("releasing capture: no active client at this position");
                self.prevent_capture_recreation = true;
                capture.release().await?;
                // Reset prevent_capture_recreation after releasing capture
                self.prevent_capture_recreation = false;
            }
            // we dont care about events from incoming handles except for releasing the capture
            return Ok(());
        }

        // activated a new client
        if event == CaptureEvent::Begin {
            if Some(handle) != self.active_client {
                log::info!("activating new client: handle={handle}, state transition: {:?} -> WaitingForAck", self.state);
                self.state = State::WaitingForAck;
                self.wait_for_ack_since = Some(Instant::now());
                self.active_client.replace(handle);
                self.event_tx
                    .send(ICaptureEvent::ClientEntered(handle))
                    .expect("channel closed");
            } else {
                // Same client re-entering - keep current state
                log::debug!("client {handle} re-entered, keeping current state: {:?}", self.state);
            }
        }

        let opposite_pos = to_proto_pos(self.get_pos(handle).opposite());

        // Determine if we should send Enter or Leave event
        // If we're currently sending input events to a client (state is Sending),
        // and the cursor crosses back towards that client, we should send Leave
        // to tell the client to stop sending input events.
        // Otherwise, we send Enter to tell the client to start sending events.
        let event = match event {
            CaptureEvent::Begin => {
                if self.state == State::Sending && Some(handle) == self.active_client {
                    // We're sending events to this client and cursor is returning to it
                    // Send Leave to tell client to stop sending events
                    log::info!("cursor returning to client {handle}, sending Leave event");
                    ProtoEvent::Leave(0)
                } else {
                    // Cursor is entering client's region
                    log::info!("cursor entering client {handle} region at position: {opposite_pos:?}");
                    ProtoEvent::Enter(opposite_pos)
                }
            }
            CaptureEvent::Input(e) => match self.state {
                // connection not acknowledged, repeat `Enter` event
                State::WaitingForAck => {
                    log::debug!("waiting for ack from client {handle}, sending Enter event");
                    ProtoEvent::Enter(opposite_pos)
                }
                State::Sending => {
                    log::trace!("sending input event to client {handle}");
                    ProtoEvent::Input(e)
                }
            },
        };

        if let Err(e) = self.conn.send(event, handle).await {
            const DUR: Duration = Duration::from_millis(500);
            debounce!(
                PREV_LOG,
                DUR,
                {
                    log::warn!("releasing capture: {e}");
                    log::debug!("Client {handle} is not connected, attempting to establish connection...");
                    log::debug!("This warning will repeat until the connection is established");
                    log::debug!("Check the logs above for connection attempts and any errors");
                    log::debug!("If connection keeps failing, run: ./scripts/diagnose-connection.sh <remote-ip>");
                }
            );
            capture.release().await?;
        }
        Ok(())
    }

    async fn release_capture(&mut self, capture: &mut InputCapture) -> Result<(), CaptureError> {
        log::info!("release_capture() called, active_client: {:?}, state: {:?}", self.active_client, self.state);
        // If we have an active client, notify them we're leaving
        if let Some(handle) = self.active_client.take() {
            log::info!("sending Leave event to client {handle}");
            if let Err(e) = self.conn.send(ProtoEvent::Leave(0), handle).await {
                log::warn!("failed to send Leave to client {handle}: {e}");
            }
        }
        // Reset state to WaitingForAck when capture is released
        self.state = State::WaitingForAck;
        self.wait_for_ack_since = None;
        log::info!("calling capture.release(), state reset to WaitingForAck");
        capture.release().await
    }
}

thread_local! {
    static PREV_LOG: Cell<Option<Instant>> = const { Cell::new(None) };
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum State {
    #[default]
    WaitingForAck,
    Sending,
}

fn to_capture_pos(pos: lan_mouse_ipc::Position) -> input_capture::Position {
    match pos {
        lan_mouse_ipc::Position::Left => input_capture::Position::Left,
        lan_mouse_ipc::Position::Right => input_capture::Position::Right,
        lan_mouse_ipc::Position::Top => input_capture::Position::Top,
        lan_mouse_ipc::Position::Bottom => input_capture::Position::Bottom,
    }
}

fn to_proto_pos(pos: input_capture::Position) -> lan_mouse_proto::Position {
    match pos {
        input_capture::Position::Left => lan_mouse_proto::Position::Left,
        input_capture::Position::Right => lan_mouse_proto::Position::Right,
        input_capture::Position::Top => lan_mouse_proto::Position::Top,
        input_capture::Position::Bottom => lan_mouse_proto::Position::Bottom,
    }
}

struct DropGuard<T> {
    tx: Sender<T>,
    on_drop: Option<T>,
}

impl<T> DropGuard<T> {
    fn new(tx: Sender<T>, on_new: T, on_drop: T) -> Self {
        tx.send(on_new).expect("channel closed");
        let on_drop = Some(on_drop);
        Self { tx, on_drop }
    }
}

impl<T> Drop for DropGuard<T> {
    fn drop(&mut self) {
        self.tx
            .send(self.on_drop.take().expect("item"))
            .expect("channel closed");
    }
}
