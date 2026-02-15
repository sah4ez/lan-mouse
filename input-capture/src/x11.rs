use std::{
    collections::HashSet,
    ptr,
    sync::Arc,
    task::{Context, Poll},
    thread,
    time::Duration,
};

use async_trait::async_trait;
use futures_core::Stream;
use tokio::sync::{mpsc, Mutex};
use x11::{
    xlib::{self, XCloseDisplay, XDisplayHeight, XDisplayWidth, XFlush, XQueryPointer},
    xrecord::{self, XRecordClientInfo, XRecordInterceptData},
};

use input_event::{Event, KeyboardEvent, PointerEvent, scancode};

use super::{Capture, CaptureError, CaptureEvent, Position, error::X11InputCaptureCreationError};

/// X11 input capture backend using XRecord extension
pub struct X11InputCapture {
    /// X11 display connection
    display: *mut xlib::Display,
    /// XRecord display connection (separate from main display)
    record_display: *mut xlib::Display,
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

unsafe impl Send for X11InputCapture {}

impl X11InputCapture {
    /// Create a new X11 input capture instance
    pub fn new() -> Result<Self, X11InputCaptureCreationError> {
        log::info!("Initializing X11 input capture backend");

        // Open X11 display
        let display = unsafe {
            match xlib::XOpenDisplay(ptr::null()) {
                d if ptr::eq(d, ptr::null_mut::<xlib::Display>()) => {
                    return Err(X11InputCaptureCreationError::OpenDisplay);
                }
                display => display,
            }
        };

        // Get screen dimensions
        let screen_width = unsafe { XDisplayWidth(display, 0) };
        let screen_height = unsafe { XDisplayHeight(display, 0) };
        log::info!("X11 screen dimensions: {}x{}", screen_width, screen_height);

        // Open separate display for XRecord
        let record_display = unsafe {
            match xlib::XOpenDisplay(ptr::null()) {
                d if ptr::eq(d, ptr::null_mut::<xlib::Display>()) => {
                    unsafe { XCloseDisplay(display) };
                    return Err(X11InputCaptureCreationError::OpenDisplay);
                }
                display => display,
            }
        };

        // Check XRecord availability
        let mut major_version = 0;
        let mut minor_version = 0;
        let record_available = unsafe {
            xrecord::XRecordQueryVersion(
                record_display,
                &mut major_version,
                &mut minor_version,
            )
        };

        if record_available == 0 {
            unsafe {
                XCloseDisplay(display);
                XCloseDisplay(record_display);
            }
            return Err(X11InputCaptureCreationError::XRecordNotAvailable);
        }

        log::info!("XRecord version: {}.{}", major_version, minor_version);

        // Create XRecord context
        let record_context = Self::create_record_context(record_display)
            .map_err(|e| {
                unsafe {
                    XCloseDisplay(display);
                    XCloseDisplay(record_display);
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

        // Start XRecord thread
        let record_thread = thread::spawn(move || {
            Self::run_record_callback(
                record_display,
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
            let mut record_range: *mut xrecord::XRecordRange =
                xrecord::XRecordAllocRange();

            if record_range.is_null() {
                return Err(X11InputCaptureCreationError::XRecordContext);
            }

            // Set up range to capture all input events
            (*record_range).core_events.first = xlib::KeyPress as u8;
            (*record_range).core_events.last = xlib::MotionNotify as u8;

            // Create XRecord context
            let context = xrecord::XRecordCreateContext(
                display,
                0,
                &mut record_range as *mut _ as *mut *mut xrecord::XRecordClientInfo,
                1,
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
        display: *mut xlib::Display,
        context: xrecord::XRecordContext,
        event_tx: mpsc::Sender<Result<(Position, CaptureEvent), CaptureError>>,
        active_clients: Arc<Mutex<HashSet<Position>>>,
        cursor_pos: Arc<Mutex<(i32, i32)>>,
    ) {
        log::info!("XRecord thread started");

        // Enable XRecord context
        let result = unsafe {
            xrecord::XRecordEnableContextAsync(
                display,
                context,
                Some(Self::record_callback),
                &event_tx as *const _ as *mut libc::c_void,
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
                xlib::XNextEvent(display, &mut event);
            }
        }

        log::info!("XRecord thread stopped");
    }

    /// XRecord callback function
    unsafe extern "C" fn record_callback(
        closure: *mut libc::c_void,
        intercept_data: *mut libc::c_void,
    ) {
        let event_tx = &*(closure as *const mpsc::Sender<Result<(Position, CaptureEvent), CaptureError>>);
        let data = intercept_data as *mut XRecordInterceptData;

        if data.is_null() {
            return;
        }

        let data = &*data;

        // Only process events from the X server
        if data.category != xrecord::XRecordFromServer {
            return;
        }

        let event_data = data.data;
        let event_type = *(event_data as *const u8);

        match event_type {
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
        let state = if event_type == xlib::KeyPress { 1 } else { 0 };

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
        event_tx: &mpsc::Sender<Result<(Position, CaptureError>, CaptureEvent>>,
        event_data: *const u8,
        event_type: u8,
    ) {
        let button = *(event_data.offset(1) as *const u8);
        let state = if event_type == xlib::ButtonPress { 1 } else { 0 };

        let event = Event::Pointer(PointerEvent::Button {
            time: 0,
            button: button as u32,
            state,
        });

        let _ = event_tx.try_send(Ok((Position::Left, CaptureEvent::Input(event))));
    }

    /// Process mouse motion events
    unsafe fn process_motion_event(
        event_tx: &mpsc::Sender<Result<(Position, CaptureError), CaptureEvent>>,
        event_data: *const u8,
    ) {
        let x = *(event_data.offset(1) as *const i16) as i32;
        let y = *(event_data.offset(3) as *const i16) as i32;

        let event = Event::Pointer(PointerEvent::Motion {
            time: 0,
            dx: x,
            dy: y,
        });

        let _ = event_tx.try_send(Ok((Position::Left, CaptureEvent::Input(event))));
    }

    /// Check if cursor has crossed an edge
    async fn check_edge_crossing(&self) -> Option<Position> {
        let mut root_x: i32 = 0;
        let mut root_y: i32 = 0;
        let mut win_x: i32 = 0;
        let mut win_y: i32 = 0;
        let mut mask: u32 = 0;
        let mut root_return: xlib::Window = 0;
        let mut child_return: xlib::Window = 0;

        unsafe {
            XQueryPointer(
                self.display,
                xlib::XDefaultRootWindow(self.display),
                &mut root_return,
                &mut child_return,
                &mut root_x,
                &mut root_y,
                &mut win_x,
                &mut win_y,
                &mut mask,
            );
        }

        // Update cursor position
        let mut pos = self.cursor_pos.lock().await;
        *pos = (root_x, root_y);
        drop(pos);

        // Check for edge crossing
        let active_clients = self.active_clients.lock().await;
        for &position in active_clients.iter() {
            match position {
                Position::Left if root_x <= 0 => return Some(Position::Left),
                Position::Right if root_x >= self.screen_width - 1 => return Some(Position::Right),
                Position::Top if root_y <= 0 => return Some(Position::Top),
                Position::Bottom if root_y >= self.screen_height - 1 => return Some(Position::Bottom),
                _ => {}
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
                xrecord::XRecordDisableContext(self.record_display, self.record_context);
                xrecord::XRecordFreeContext(self.record_display, self.record_context);
            }
        }

        // Close displays
        if !self.record_display.is_null() {
            unsafe {
                XCloseDisplay(self.record_display);
            }
        }

        if !self.display.is_null() {
            unsafe {
                XCloseDisplay(self.display);
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
