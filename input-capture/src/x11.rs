use std::{
    collections::HashSet,
    ptr,
    sync::Arc,
    task::Poll,
    thread,
};

use async_trait::async_trait;
use futures_core::Stream;
use tokio::sync::{mpsc, Mutex};
use x11::{
    xlib::{self, XCloseDisplay, XDisplayHeight, XDisplayWidth, XQueryPointer},
    xrecord::{self, XRecordInterceptData},
};

use input_event::{Event, KeyboardEvent, PointerEvent};

use super::{Capture, CaptureError, CaptureEvent, Position, error::X11InputCaptureCreationError};

/// Wrapper for X11 display pointer that is Send-safe
///
/// X11 display pointers are not thread-safe, but we need to send them across threads
/// for the XRecord callback. This wrapper implements Send to allow this, but we must
/// ensure proper synchronization by only using the display in the thread it was sent to.
struct SendDisplay(*mut xlib::Display);

// SAFETY: X11 display pointers are not thread-safe, but we implement Send to allow
// moving the display pointer to another thread. We ensure that each display is only
// used in a single thread at a time.
unsafe impl Send for SendDisplay {}

// SAFETY: Cloning creates a new reference to the same display pointer.
// This is safe as long as each clone is used in a separate thread and the display
// is properly closed only once.
impl Clone for SendDisplay {
    fn clone(&self) -> Self {
        Self(self.0)
    }
}

/// X11 input capture backend using XRecord extension
pub struct X11InputCapture {
    /// X11 display connection
    display: SendDisplay,
    /// XRecord display connection (separate from main display)
    record_display: SendDisplay,
    /// XRecord context for event capture
    record_context: xrecord::XRecordContext,
    /// Receiver for captured events
    event_rx: mpsc::Receiver<Result<(Position, CaptureEvent), CaptureError>>,
    /// Active capture positions
    active_clients: Arc<Mutex<HashSet<Position>>>,
    /// Current cursor position
    cursor_pos: Arc<Mutex<(i32, i32)>>,
    /// Screen bounds
    screen_width: i32,
    screen_height: i32,
    /// Thread handle for XRecord callback
    record_thread: Option<thread::JoinHandle<()>>,
}

impl X11InputCapture {
    /// Create a new X11 input capture instance
    pub fn new() -> Result<Self, X11InputCaptureCreationError> {
        log::info!("Initializing X11 input capture backend");

        // Check DISPLAY environment variable
        let display_env = std::env::var("DISPLAY").unwrap_or_else(|_| ":0".to_string());
        log::info!("Using DISPLAY: {}", display_env);

        // Open X11 display
        let display = unsafe {
            match xlib::XOpenDisplay(ptr::null()) {
                d if ptr::eq(d, ptr::null_mut::<xlib::Display>()) => {
                    let error_msg = format!(
                        "Failed to open X11 display. DISPLAY variable is set to: '{}'. \
                        Make sure you're running in an X11 session. \
                        If you're using Wayland, you may need to run with XWayland or use a Wayland-compatible backend.",
                        display_env
                    );
                    log::error!("{}", error_msg);
                    return Err(X11InputCaptureCreationError::OpenDisplay);
                }
                display => SendDisplay(display),
            }
        };

        // Get screen dimensions
        let screen_width = unsafe { XDisplayWidth(display.0, 0) };
        let screen_height = unsafe { XDisplayHeight(display.0, 0) };
        log::info!("X11 screen dimensions: {}x{}", screen_width, screen_height);

        // Open separate display for XRecord
        log::debug!("Opening separate display for XRecord");
        let record_display = unsafe {
            match xlib::XOpenDisplay(ptr::null()) {
                d if ptr::eq(d, ptr::null_mut::<xlib::Display>()) => {
                    log::error!("Failed to open X11 display for XRecord");
                    unsafe { XCloseDisplay(display.0) };
                    return Err(X11InputCaptureCreationError::OpenDisplay);
                }
                display => SendDisplay(display),
            }
        };

        // Check XRecord availability
        log::debug!("Checking XRecord extension availability");
        let mut major_version = 0;
        let mut minor_version = 0;
        let record_available = unsafe {
            xrecord::XRecordQueryVersion(
                record_display.0,
                &mut major_version,
                &mut minor_version,
            )
        };

        if record_available == 0 {
            let error_msg = format!(
                "XRecord extension is not available on this X server. \
                This extension is required for input capture on X11. \
                Please ensure your X server has the XRecord extension enabled. \
                You can check this by running: 'xdpyinfo | grep RECORD'"
            );
            log::error!("{}", error_msg);
            unsafe {
                XCloseDisplay(display.0);
                XCloseDisplay(record_display.0);
            }
            return Err(X11InputCaptureCreationError::XRecordNotAvailable);
        }

        log::info!("XRecord version: {}.{}", major_version, minor_version);

        // Create XRecord context
        log::debug!("Creating XRecord context");
        let record_context = Self::create_record_context(record_display.0)
            .map_err(|e| {
                log::error!("Failed to create XRecord context: {:?}", e);
                unsafe {
                    XCloseDisplay(display.0);
                    XCloseDisplay(record_display.0);
                }
                e
            })?;

        // Set up communication channels
        let (event_tx, event_rx) = mpsc::channel(100);
        let active_clients = Arc::new(Mutex::new(HashSet::new()));
        let cursor_pos = Arc::new(Mutex::new((0, 0)));

        // Clone for thread
        let active_clients_clone = Arc::clone(&active_clients);
        let cursor_pos_clone = Arc::clone(&cursor_pos);

        // Clone record_display for the thread
        let record_display_clone = record_display.clone();
        
        // Start XRecord thread
        log::debug!("Starting XRecord thread");
        let record_thread = thread::spawn(move || {
            log::info!("XRecord thread started");
            Self::run_record_callback(
                record_display_clone,
                record_context,
                event_tx,
                active_clients_clone,
                cursor_pos_clone,
            );
        });

        Ok(Self {
            display,
            record_display,
            record_context,
            event_rx,
            active_clients,
            cursor_pos,
            screen_width,
            screen_height,
            record_thread: Some(record_thread),
        })
    }

    /// Create XRecord context for capturing input events
    fn create_record_context(
        display: *mut xlib::Display,
    ) -> Result<xrecord::XRecordContext, X11InputCaptureCreationError> {
        unsafe {
            // Allocate XRecord range
            let record_range: *mut xrecord::XRecordRange =
                xrecord::XRecordAllocRange();

            if record_range.is_null() {
                return Err(X11InputCaptureCreationError::XRecordContext);
            }

            // Set up range to capture all input events
            (*record_range).delivered_events.first = xlib::KeyPress as u8;
            (*record_range).delivered_events.last = xlib::MotionNotify as u8;

            // Create XRecord context
            // XRecordCreateContext signature:
            // XRecordContext XRecordCreateContext(
            //     Display *display,
            //     int intercept_client,
            //     XRecordClientSpec *clients,
            //     int nclients,
            //     XRecordRange *ranges,
            //     int nranges,
            //     XRecordInterceptProc intercept_proc
            // );
            let mut ranges: [*mut xrecord::XRecordRange; 1] = [record_range];
            let context = xrecord::XRecordCreateContext(
                display,
                0, // XRecordAllClients
                ptr::null_mut(), // clients (NULL for all clients)
                0, // nclients
                ranges.as_mut_ptr(),
                1, // nranges
            );

            // Free the range
            libc::free(record_range as *mut libc::c_void);

            if context == 0 {
                return Err(X11InputCaptureCreationError::XRecordContext);
            }

            Ok(context)
        }
    }

    /// Run XRecord callback in a separate thread
    fn run_record_callback(
        display: SendDisplay,
        context: xrecord::XRecordContext,
        event_tx: mpsc::Sender<Result<(Position, CaptureEvent), CaptureError>>,
        active_clients: Arc<Mutex<HashSet<Position>>>,
        cursor_pos: Arc<Mutex<(i32, i32)>>,
    ) {
        log::info!("XRecord thread started");

        // Enable XRecord context
        let result = unsafe {
            xrecord::XRecordEnableContextAsync(
                display.0,
                context,
                Some(Self::record_callback),
                &event_tx as *const _ as *mut libc::c_char,
            )
        };

        if result == 0 {
            log::error!("Failed to enable XRecord context");
            return;
        }

        // Process X11 events
        loop {
            unsafe {
                // Check if channel is closed
                if event_tx.is_closed() {
                    log::info!("XRecord channel closed, stopping thread");
                    break;
                }

                // Process X11 events
                let mut event: xlib::XEvent = std::mem::zeroed();
                xlib::XNextEvent(display.0, &mut event);
            }
        }

        log::info!("XRecord thread stopped");
    }

    /// XRecord callback function
    unsafe extern "C" fn record_callback(
        closure: *mut libc::c_char,
        intercept_data: *mut XRecordInterceptData,
    ) {
        // Check if intercept_data is null before dereferencing
        if intercept_data.is_null() {
            return;
        }

        let event_tx = &*(closure as *const mpsc::Sender<Result<(Position, CaptureEvent), CaptureError>>);
        let data = &*intercept_data;

        // Only process events from the X server
        if data.category != xrecord::XRecordFromServer {
            return;
        }

        let event_data = data.data;
        let event_type = *(event_data as *const u8);

        match event_type as i32 {
            xlib::KeyPress | xlib::KeyRelease => {
                Self::process_keyboard_event(event_tx, event_data, event_type);
            }
            xlib::ButtonPress | xlib::ButtonRelease => {
                Self::process_button_event(event_tx, event_data, event_type);
            }
            xlib::MotionNotify => {
                Self::process_motion_event(event_tx, event_data);
            }
            _ => {}
        }
    }

    /// Process keyboard events
    unsafe fn process_keyboard_event(
        event_tx: &mpsc::Sender<Result<(Position, CaptureEvent), CaptureError>>,
        event_data: *const u8,
        event_type: u8,
    ) {
        let keycode = *(event_data.offset(1) as *const u8);
        let state = if event_type as i32 == xlib::KeyPress { 1 } else { 0 };

        // X11 keycodes are shifted by 8 relative to Linux scancodes
        let linux_scancode = (keycode as u32) - 8;

        let event = Event::Keyboard(KeyboardEvent::Key {
            time: 0,
            key: linux_scancode,
            state,
        });

        // Send event (position will be determined by edge detection)
        // For now, we send to Left as placeholder
        let _ = event_tx.try_send(Ok((Position::Left, CaptureEvent::Input(event))));
    }

    /// Process mouse button events
    unsafe fn process_button_event(
        event_tx: &mpsc::Sender<Result<(Position, CaptureEvent), CaptureError>>,
        event_data: *const u8,
        event_type: u8,
    ) {
        let button = *(event_data.offset(1) as *const u8);
        let state = if event_type as i32 == xlib::ButtonPress { 1 } else { 0 };

        let event = Event::Pointer(PointerEvent::Button {
            time: 0,
            button: button as u32,
            state,
        });

        let _ = event_tx.try_send(Ok((Position::Left, CaptureEvent::Input(event))));
    }

    /// Process mouse motion events
    unsafe fn process_motion_event(
        event_tx: &mpsc::Sender<Result<(Position, CaptureEvent), CaptureError>>,
        event_data: *const u8,
    ) {
        let x = *(event_data.offset(1) as *const i16) as i32;
        let y = *(event_data.offset(3) as *const i16) as i32;

        let event = Event::Pointer(PointerEvent::Motion {
            time: 0,
            dx: x as f64,
            dy: y as f64,
        });

        let _ = event_tx.try_send(Ok((Position::Left, CaptureEvent::Input(event))));
    }

    /// Check if cursor has crossed an edge
    fn check_edge_crossing(&self) -> Option<Position> {
        let mut root_x: i32 = 0;
        let mut root_y: i32 = 0;
        let mut win_x: i32 = 0;
        let mut win_y: i32 = 0;
        let mut mask: u32 = 0;
        let mut root_return: xlib::Window = 0;
        let mut child_return: xlib::Window = 0;

        unsafe {
            XQueryPointer(
                self.display.0,
                xlib::XDefaultRootWindow(self.display.0),
                &mut root_return,
                &mut child_return,
                &mut root_x,
                &mut root_y,
                &mut win_x,
                &mut win_y,
                &mut mask,
            );
        }

        // Update cursor position using blocking lock
        // Note: This is called from poll_next which is not async, so we use try_lock
        // to avoid blocking. If the lock is contended, we skip the update this time.
        if let Ok(mut pos) = self.cursor_pos.try_lock() {
            *pos = (root_x, root_y);
        }

        // Check for edge crossing using blocking lock
        if let Ok(active_clients) = self.active_clients.try_lock() {
            for &position in active_clients.iter() {
                match position {
                    Position::Left if root_x <= 0 => return Some(Position::Left),
                    Position::Right if root_x >= self.screen_width - 1 => return Some(Position::Right),
                    Position::Top if root_y <= 0 => return Some(Position::Top),
                    Position::Bottom if root_y >= self.screen_height - 1 => return Some(Position::Bottom),
                    _ => {}
                }
            }
        }

        None
    }
}

impl Drop for X11InputCapture {
    fn drop(&mut self) {
        log::info!("Cleaning up X11 input capture");

        // Disable XRecord context
        if self.record_context != 0 {
            unsafe {
                xrecord::XRecordDisableContext(self.record_display.0, self.record_context);
                xrecord::XRecordFreeContext(self.record_display.0, self.record_context);
            }
        }

        // Close displays
        if !self.record_display.0.is_null() {
            unsafe {
                XCloseDisplay(self.record_display.0);
            }
        }

        if !self.display.0.is_null() {
            unsafe {
                XCloseDisplay(self.display.0);
            }
        }

        // Join thread
        if let Some(handle) = self.record_thread.take() {
            let _ = handle.join();
        }
    }
}

#[async_trait]
impl Capture for X11InputCapture {
    async fn create(&mut self, pos: Position) -> Result<(), CaptureError> {
        log::info!("Creating capture at position: {pos}");
        let mut clients = self.active_clients.lock().await;
        clients.insert(pos);
        Ok(())
    }

    async fn destroy(&mut self, pos: Position) -> Result<(), CaptureError> {
        log::info!("Destroying capture at position: {pos}");
        let mut clients = self.active_clients.lock().await;
        clients.remove(&pos);
        Ok(())
    }

    async fn release(&mut self) -> Result<(), CaptureError> {
        log::debug!("Releasing capture");
        // Release any pressed keys
        Ok(())
    }

    async fn terminate(&mut self) -> Result<(), CaptureError> {
        log::info!("Terminating X11 capture");
        Ok(())
    }
}

impl Stream for X11InputCapture {
    type Item = Result<(Position, CaptureEvent), CaptureError>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        // Check for edge crossing periodically
        if let Some(pos) = self.as_mut().check_edge_crossing() {
            log::debug!("Edge crossed: {pos}");
            return Poll::Ready(Some(Ok((pos, CaptureEvent::Begin))));
        }

        // Poll for captured events
        match self.event_rx.poll_recv(cx) {
            Poll::Ready(Some(event)) => Poll::Ready(Some(event)),
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}
