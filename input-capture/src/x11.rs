use std::{
    collections::HashSet,
    ptr,
    sync::atomic::{AtomicBool, AtomicI32, AtomicPtr, Ordering},
    sync::{Arc, Mutex as StdMutex},
    task::Poll,
    thread,
};

use async_trait::async_trait;
use futures_core::Stream;
use tokio::sync::{mpsc, Mutex};
use tokio::task::spawn_blocking;
use x11::{
    xlib::{self, XCloseDisplay, XDisplayHeight, XDisplayWidth, XQueryPointer, XWarpPointer, XDefaultRootWindow, XFlush},
    xrecord::{self, XRecordInterceptData},
};

use input_event::{Event, KeyboardEvent, PointerEvent};

use super::{Capture, CaptureError, CaptureEvent, Position, error::X11InputCaptureCreationError};

// ============================================================================
// XKB Constants
// ============================================================================

/// XkbUseCoreKbd - Use the core keyboard device
/// This constant is defined as 0x0100 in the X11 XKB extension specification
const XkbUseCoreKbd: u32 = 0x0100;

// ============================================================================
// X11 Error Handler
// ============================================================================

/// X11 error handler for catching and logging X11 protocol errors
///
/// This prevents X11 from printing errors to stderr and allows
/// proper error handling in the application.
static mut X11_ERROR_HANDLER_INSTALLED: bool = false;

/// X11 error handler function
///
/// This function is called by X11 when a protocol error occurs.
/// It logs the error details and returns 0 to prevent X11 from
/// printing to stderr.
unsafe extern "C" fn x11_error_handler(
    _display: *mut xlib::Display,
    error_event: *mut xlib::XErrorEvent,
) -> i32 {
    if error_event.is_null() {
        return 0;
    }
    
    let event = &*error_event;
    log::error!(
        "X11 Error: type={}, serial={}, error_code={}, request_code={}, minor_code={}",
        event.type_,
        event.serial,
        event.error_code,
        event.request_code,
        event.minor_code
    );
    0 // Return 0 to prevent X11 from printing to stderr
}

/// X11 I/O error handler function
///
/// This function is called by X11 when a fatal I/O error occurs
/// (e.g., connection to X server lost).
unsafe extern "C" fn x11_io_error_handler(
    _display: *mut xlib::Display,
) -> i32 {
    log::error!("X11 I/O Error: display connection lost or X server terminated");
    0 // Return 0 to prevent X11 from printing to stderr
}

/// Install X11 error handlers
///
/// This function installs error and I/O error handlers for the X11 connection.
/// It should be called once during initialization.
unsafe fn install_x11_error_handlers() {
    // Only install once
    if X11_ERROR_HANDLER_INSTALLED {
        return;
    }
    
    xlib::XSetErrorHandler(Some(x11_error_handler));
    xlib::XSetIOErrorHandler(Some(x11_io_error_handler));
    X11_ERROR_HANDLER_INSTALLED = true;
    
    log::debug!("X11 error handlers installed");
}

// ============================================================================
// Constants
// ============================================================================

/// X11 keycode offset relative to Linux scancode
const X11_KEYCODE_OFFSET: u32 = 8;

/// Edge detection thresholds
const EDGE_OFFSET_PIXELS: i32 = 1;
const EDGE_COUNTER_THRESHOLD: u32 = 2;

/// Cursor cache TTL (milliseconds)
const CURSOR_CACHE_TTL_MS: u64 = 16; // ~60 FPS

/// Channel buffer size for events
const EVENT_CHANNEL_BUFFER: usize = 100;

/// XRecord version requirement
const XRECORD_REQUIRED_VERSION: &'static str = "1.13";

/// XRecord event buffer sizes (in bytes)
/// These are the minimum required sizes for each event type
const XRECORD_KEY_EVENT_SIZE: u64 = 2;  // event_type (1) + keycode (1)
const XRECORD_BUTTON_EVENT_SIZE: u64 = 2;  // event_type (1) + button (1)
const XRECORD_MOTION_EVENT_SIZE: u64 = 5;  // event_type (1) + x (2) + y (2)

/// Wrapper for X11 display pointer that is Send-safe
///
/// X11 display pointers are not thread-safe, but we need to send them across threads
/// for the XRecord callback. This wrapper implements Send to allow this, but we must
/// ensure proper synchronization by only using the display in the thread it was sent to.
///
/// # Safety
///
/// This struct uses Arc<AtomicPtr> and Arc<AtomicBool> to ensure:
/// - Thread-safe access to the display pointer
/// - Proper cleanup when the display is closed
/// - Prevention of use-after-free by tracking closed state
#[derive(Clone)]
struct SendDisplay {
    /// Atomic pointer to the X11 display
    ptr: Arc<AtomicPtr<xlib::Display>>,
    /// Flag indicating if the display has been closed
    closed: Arc<AtomicBool>,
}

impl SendDisplay {
    /// Create a new SendDisplay from a raw pointer
    ///
    /// # Safety
    ///
    /// The caller must ensure that the pointer is valid and points to an open X11 display.
    unsafe fn new(ptr: *mut xlib::Display) -> Self {
        Self {
            ptr: Arc::new(AtomicPtr::new(ptr)),
            closed: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Check if the display is still valid (not closed and not null)
    fn is_valid(&self) -> bool {
        !self.closed.load(Ordering::Acquire) && !self.ptr.load(Ordering::Acquire).is_null()
    }

    /// Get the raw pointer to the display
    ///
    /// # Safety
    ///
    /// The caller must ensure that the display is still valid (check with is_valid())
    /// and that the pointer is used correctly according to X11 API requirements.
    unsafe fn get(&self) -> *mut xlib::Display {
        self.ptr.load(Ordering::Acquire)
    }

    /// Close the display and mark it as closed
    ///
    /// This method is idempotent - multiple calls are safe.
    unsafe fn close(&self) {
        if self.closed.swap(true, Ordering::AcqRel) {
            return; // Already closed
        }
        let ptr = self.ptr.swap(std::ptr::null_mut(), Ordering::AcqRel);
        if !ptr.is_null() {
            XCloseDisplay(ptr);
        }
    }
}

// SAFETY: SendDisplay uses Arc<AtomicPtr> and Arc<AtomicBool> which are Send and Sync,
// ensuring thread-safe access to the display pointer and closed state.
unsafe impl Send for SendDisplay {}

// SAFETY: SendDisplay uses Arc<AtomicPtr> and Arc<AtomicBool> which are Send and Sync,
// allowing multiple threads to safely access the display state (though actual X11
// operations must still be synchronized externally).
unsafe impl Sync for SendDisplay {}

/// Closure data for XRecord callback
///
/// This struct holds the data that needs to be passed to the XRecord callback
/// through the closure parameter.
struct RecordCallbackClosure {
    event_tx: mpsc::Sender<Result<(Position, CaptureEvent), CaptureError>>,
    active_clients: Arc<Mutex<HashSet<Position>>>,
    display: SendDisplay,
    modifier_state: Arc<StdMutex<X11ModifierState>>,
}

/// X11 modifier state tracker for layout switching support
#[derive(Debug)]
struct X11ModifierState {
    /// Currently depressed modifiers
    depressed: u32,
    /// Latched modifiers (sticky keys)
    latched: u32,
    /// Locked modifiers (Caps Lock, Num Lock)
    locked: u32,
    /// Current keyboard layout group (0 = default, 1 = first alternative, etc.)
    group: u32,
    /// Currently pressed keys (for tracking modifier state changes)
    pressed_keys: HashSet<u32>,
}

impl Default for X11ModifierState {
    fn default() -> Self {
        Self {
            depressed: 0,
            latched: 0,
            locked: 0,
            group: 0,
            pressed_keys: HashSet::new(),
        }
    }
}

impl X11ModifierState {
    /// Create modifier state synchronized with X server
    ///
    /// This method queries the current modifier state from X11 using XkbGetState,
    /// ensuring that the initial state matches the actual X server state.
    /// This is important for correctly tracking Caps Lock, Num Lock, and layout group.
    fn from_x11(display: &SendDisplay) -> Self {
        let mut state = Self::default();

        unsafe {
            // Get current modifier state using XkbGetState
            let mut xkb_state: x11::xlib::XkbStateRec = std::mem::zeroed();
            let success = x11::xlib::XkbGetState(
                display.get(),
                XkbUseCoreKbd,
                &mut xkb_state
            );

            if success != 0 {
                state.depressed = xkb_state.mods as u32;
                state.latched = xkb_state.base_mods as u32;
                state.locked = xkb_state.locked_mods as u32;
                state.group = xkb_state.group as u32;

                log::info!(
                    "X11: Initial modifier state - depressed={}, latched={}, locked={}, group={}",
                    state.depressed, state.latched, state.locked, state.group
                );
            } else {
                log::warn!("X11: Failed to get initial modifier state from Xkb, using defaults");
            }
        }

        state
    }
}

/// Cursor state for tracking position changes
///
/// This struct maintains the current and previous cursor positions atomically
/// to prevent race conditions when detecting edge crossings.
#[derive(Debug)]
struct AtomicCursorState {
    current_x: AtomicI32,
    current_y: AtomicI32,
    previous_x: AtomicI32,
    previous_y: AtomicI32,
}

impl AtomicCursorState {
    /// Create a new AtomicCursorState with initial position
    fn new(initial: (i32, i32)) -> Self {
        Self {
            current_x: AtomicI32::new(initial.0),
            current_y: AtomicI32::new(initial.1),
            previous_x: AtomicI32::new(initial.0),
            previous_y: AtomicI32::new(initial.1),
        }
    }

    /// Update the cursor position and return the previous position
    ///
    /// This method atomically updates the state, preventing race conditions.
    fn update(&self, new_pos: (i32, i32)) -> (i32, i32) {
        let prev_x = self.current_x.swap(new_pos.0, Ordering::AcqRel);
        let prev_y = self.current_y.swap(new_pos.1, Ordering::AcqRel);
        self.previous_x.store(prev_x, Ordering::Release);
        self.previous_y.store(prev_y, Ordering::Release);
        (prev_x, prev_y)
    }

    /// Get the current cursor position
    fn current(&self) -> (i32, i32) {
        (
            self.current_x.load(Ordering::Acquire),
            self.current_y.load(Ordering::Acquire),
        )
    }

    /// Get the previous cursor position
    fn previous(&self) -> (i32, i32) {
        (
            self.previous_x.load(Ordering::Acquire),
            self.previous_y.load(Ordering::Acquire),
        )
    }
}

/// X11 input capture backend using XRecord extension
///
/// This backend captures input events from the X server using the XRecord extension.
/// It runs a separate thread that processes XRecord events and sends them to the main
/// application through a channel.
///
/// # Thread Safety
///
/// This struct is not thread-safe internally, but implements `Stream` which allows
/// it to be used in async contexts. The XRecord callback runs in a separate thread
/// and communicates with the main thread through a channel.
///
/// # Example
///
/// ```no_run
/// use input_capture::InputCapture;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let mut capture = InputCapture::new(None).await?;
/// capture.create(1, input_capture::Position::Left).await?;
/// # Ok(())
/// # }
/// ```
/// X11 input capture backend using XRecord extension
///
/// # Architecture Notes
///
/// XRecord extension requires a separate display connection from the main display.
/// This is a requirement of the XRecord protocol and cannot be avoided.
///
/// Both connections share the same X server but operate independently:
/// - `display`: Used for XQueryPointer, XWarpPointer, and other operations
/// - `record_display`: Used exclusively for XRecord event capture
///
/// # Resource Usage
///
/// Having two display connections doubles X server connection overhead.
/// This is acceptable given the XRecord protocol requirements.
pub struct X11InputCapture {
    /// X11 display connection (used for XQueryPointer, XWarpPointer, etc.)
    display: SendDisplay,
    /// XRecord display connection (required by XRecord protocol, separate from main display)
    record_display: SendDisplay,
    /// XRecord context for event capture
    record_context: xrecord::XRecordContext,
    /// Receiver for captured events
    event_rx: mpsc::Receiver<Result<(Position, CaptureEvent), CaptureError>>,
    /// Active capture positions
    active_clients: Arc<Mutex<HashSet<Position>>>,
    /// Cursor state (current and previous positions)
    cursor_state: Arc<AtomicCursorState>,
    /// Counter for consecutive edge positions (to detect when cursor tries to cross)
    edge_counter: Arc<Mutex<u32>>,
    /// Screen bounds
    screen_width: i32,
    screen_height: i32,
    /// Thread handle for XRecord callback
    record_thread: Option<thread::JoinHandle<()>>,
    /// Shutdown flag for graceful thread termination
    shutdown_flag: Arc<AtomicBool>,
    /// Currently active capture position (if any)
    current_pos: Arc<Mutex<Option<Position>>>,
    /// Position where cursor was when capture started
    enter_position: Arc<Mutex<Option<(i32, i32)>>>,
}

impl X11InputCapture {
    /// Create a new X11 input capture instance
    pub fn new() -> Result<Self, X11InputCaptureCreationError> {
        log::info!("Initializing X11 input capture backend");

        // Install X11 error handlers
        unsafe {
            install_x11_error_handlers();
        }

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
                    return Err(X11InputCaptureCreationError::OpenDisplay { display: display_env });
                }
                display => SendDisplay::new(display),
            }
        };

        // Get screen dimensions
        let screen_width = unsafe { XDisplayWidth(display.get(), 0) };
        let screen_height = unsafe { XDisplayHeight(display.get(), 0) };
        log::info!("X11 screen dimensions: {}x{}", screen_width, screen_height);

        // Open separate display for XRecord
        log::debug!("Opening separate display for XRecord");
        let record_display = unsafe {
            match xlib::XOpenDisplay(ptr::null()) {
                d if ptr::eq(d, ptr::null_mut::<xlib::Display>()) => {
                    log::error!("Failed to open X11 display for XRecord");
                    display.close();
                    return Err(X11InputCaptureCreationError::OpenDisplay { display: display_env });
                }
                display => SendDisplay::new(display),
            }
        };

        // Check XRecord availability
        log::debug!("Checking XRecord extension availability");
        let mut xrecord_major = 0;
        let mut xrecord_minor = 0;
        let record_available = unsafe {
            xrecord::XRecordQueryVersion(
                record_display.get(),
                &mut xrecord_major,
                &mut xrecord_minor,
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
                display.close();
                record_display.close();
            }
            return Err(X11InputCaptureCreationError::XRecordNotAvailable {
                required: XRECORD_REQUIRED_VERSION,
                major: xrecord_major,
                minor: xrecord_minor,
            });
        }

        log::info!("XRecord version: {}.{}", xrecord_major, xrecord_minor);

        // Create XRecord context
        log::debug!("Creating XRecord context");
        let record_context = Self::create_record_context(record_display.clone())
            .map_err(|e| {
                log::error!("Failed to create XRecord context: {:?}", e);
                unsafe {
                    display.close();
                    record_display.close();
                }
                e
            })?;

        // Set up communication channels
        let (event_tx, event_rx) = mpsc::channel(EVENT_CHANNEL_BUFFER);
        let active_clients = Arc::new(Mutex::new(HashSet::new()));
        let cursor_state = Arc::new(AtomicCursorState::new((0, 0)));
        let edge_counter = Arc::new(Mutex::new(0));
        let current_pos = Arc::new(Mutex::new(None));
        let enter_position = Arc::new(Mutex::new(None));
        let shutdown_flag = Arc::new(AtomicBool::new(false));

        // Clone for thread
        let active_clients_clone = Arc::clone(&active_clients);
        let current_pos_clone = Arc::clone(&current_pos);
        let enter_position_clone = Arc::clone(&enter_position);
        let shutdown_flag_clone = Arc::clone(&shutdown_flag);

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
                current_pos_clone,
                enter_position_clone,
                shutdown_flag_clone,
            );
        });

        Ok(Self {
            display,
            record_display,
            record_context,
            event_rx,
            active_clients,
            cursor_state,
            edge_counter,
            screen_width,
            screen_height,
            record_thread: Some(record_thread),
            shutdown_flag,
            current_pos,
            enter_position,
        })
    }

    /// Create XRecord context for capturing input events
    ///
    /// This method creates an XRecord context that captures all input events
    /// from the X server. It uses RAII guards to ensure proper resource cleanup.
    ///
    /// # Arguments
    ///
    /// * `display` - The X11 display connection (wrapped in SendDisplay)
    ///
    /// # Returns
    ///
    /// * `Ok(context)` - The XRecord context handle
    /// * `Err(X11InputCaptureCreationError)` - If context creation fails
    fn create_record_context(
        display: SendDisplay,
    ) -> Result<xrecord::XRecordContext, X11InputCaptureCreationError> {
        unsafe {
            // Allocate XRecord range
            let record_range: *mut xrecord::XRecordRange = xrecord::XRecordAllocRange();

            if record_range.is_null() {
                return Err(X11InputCaptureCreationError::XRecordContext {
                    reason: "Failed to allocate XRecord range".to_string(),
                });
            }

            // Use scopeguard to ensure the range is freed even if an error occurs
            let _range_guard = scopeguard::guard(record_range, |range| {
                libc::free(range as *mut libc::c_void);
            });

            // Set up range to capture all input events
            (*record_range).delivered_events.first = xlib::KeyPress as u8;
            (*record_range).delivered_events.last = xlib::MotionNotify as u8;

            log::debug!(
                "XRecord range: delivered_events first={} (KeyPress), last={} (MotionNotify)",
                (*record_range).delivered_events.first,
                (*record_range).delivered_events.last
            );

            // Create XRecord context
            // XRecordCreateContext signature:
            // XRecordContext XRecordCreateContext(
            //     Display *display,
            //     int intercept_client,
            //     XRecordClientSpec *clients,
            //     int nclients,
            //     XRecordRange *ranges,
            //     int nranges,
            // );
            let mut ranges: [*mut xrecord::XRecordRange; 1] = [record_range];
            let context = xrecord::XRecordCreateContext(
                display.get(),
                0, // XRecordAllClients
                ptr::null_mut(), // clients (NULL for all clients)
                0, // nclients
                ranges.as_mut_ptr(),
                1, // nranges
            );

            // The range will be freed automatically by the scopeguard when this function returns

            if context == 0 {
                return Err(X11InputCaptureCreationError::XRecordContext {
                    reason: "XRecordCreateContext returned 0".to_string(),
                });
            }

            log::debug!("XRecord context created successfully: {}", context);

            Ok(context)
        }
    }

    /// Run XRecord callback in a separate thread
    ///
    /// This method runs the XRecord event processing loop in a dedicated thread.
    /// It checks for shutdown signals and channel closure to gracefully terminate.
    ///
    /// # Arguments
    ///
    /// * `display` - The X11 display connection for XRecord
    /// * `context` - The XRecord context
    /// * `event_tx` - Channel for sending captured events
    /// * `active_clients` - Set of active capture positions
    /// * `current_pos` - Currently active capture position
    /// * `enter_position` - Position where cursor was when capture started
    /// * `shutdown_flag` - Flag for graceful shutdown
    fn run_record_callback(
        display: SendDisplay,
        context: xrecord::XRecordContext,
        event_tx: mpsc::Sender<Result<(Position, CaptureEvent), CaptureError>>,
        active_clients: Arc<Mutex<HashSet<Position>>>,
        _current_pos: Arc<Mutex<Option<Position>>>,
        _enter_position: Arc<Mutex<Option<(i32, i32)>>>,
        shutdown_flag: Arc<AtomicBool>,
    ) {
        log::info!("XRecord thread started");

        // Create modifier state tracker, synchronized with X server
        let modifier_state = Arc::new(StdMutex::new(X11ModifierState::from_x11(&display)));

        // Create closure data
        let closure = RecordCallbackClosure {
            event_tx,
            active_clients,
            display: display.clone(),
            modifier_state: modifier_state.clone(),
        };

        // Enable XRecord context
        let result = unsafe {
            xrecord::XRecordEnableContextAsync(
                display.get(),
                context,
                Some(Self::record_callback),
                &closure as *const _ as *mut libc::c_char,
            )
        };

        if result == 0 {
            log::error!("Failed to enable XRecord context");
            return;
        }

        log::info!("XRecord context enabled successfully");

        // Process XRecord events using XRecordProcessReplies
        // This is the correct way to process XRecord events
        loop {
            // Check for shutdown signal
            if shutdown_flag.load(Ordering::Acquire) {
                log::info!("XRecord thread received shutdown signal");
                break;
            }

            // Check if channel is closed
            if closure.event_tx.is_closed() {
                log::info!("XRecord channel closed, stopping thread");
                break;
            }

            // Process XRecord replies - this triggers the callback
            unsafe {
                xrecord::XRecordProcessReplies(display.get());
            }
        }

        log::info!("XRecord thread stopped");
    }

    /// XRecord callback function
    ///
    /// This function is called by the XRecord extension for each captured event.
    /// It processes the event and sends it to the main thread through a channel.
    ///
    /// # Safety
    ///
    /// This function is unsafe because:
    /// - It's called from C code (XRecord)
    /// - It dereferences raw pointers
    /// - It accesses X11 data structures
    ///
    /// The caller must ensure that:
    /// - `closure` is a valid pointer to a RecordCallbackClosure
    /// - `intercept_data` is a valid pointer to XRecordInterceptData
    unsafe extern "C" fn record_callback(
        closure: *mut libc::c_char,
        intercept_data: *mut XRecordInterceptData,
    ) {
        // Validate pointers before dereferencing
        if intercept_data.is_null() {
            log::trace!("XRecord callback: intercept_data is null");
            return;
        }

        if closure.is_null() {
            log::error!("XRecord callback: closure pointer is null");
            return;
        }

        let closure = &*(closure as *const RecordCallbackClosure);
        let data = &*intercept_data;

        // Only process events from the X server
        if data.category != xrecord::XRecordFromServer {
            log::trace!("XRecord callback: category {} is not XRecordFromServer", data.category);
            return;
        }

        let event_data = data.data;
        
        // Validate buffer size before dereferencing
        if data.data_len < 1 {
            log::warn!("XRecord callback: data_len is 0, cannot read event_type");
            return;
        }
        
        let event_type = *(event_data as *const u8);

        log::trace!("XRecord callback: event_type = {}", event_type);

        match event_type as i32 {
            xlib::KeyPress | xlib::KeyRelease => {
                // Validate buffer size for keyboard event
                if data.data_len < XRECORD_KEY_EVENT_SIZE {
                    log::warn!(
                        "XRecord callback: insufficient data for keyboard event: got {} bytes, need {} bytes",
                        data.data_len, XRECORD_KEY_EVENT_SIZE
                    );
                    return;
                }
                log::debug!("XRecord callback: processing keyboard event, type = {}", event_type);
                Self::process_keyboard_event(&closure.event_tx, event_data, event_type, &closure.active_clients, &closure.modifier_state);
            }
            xlib::ButtonPress | xlib::ButtonRelease => {
                // Validate buffer size for button event
                if data.data_len < XRECORD_BUTTON_EVENT_SIZE {
                    log::warn!(
                        "XRecord callback: insufficient data for button event: got {} bytes, need {} bytes",
                        data.data_len, XRECORD_BUTTON_EVENT_SIZE
                    );
                    return;
                }
                log::trace!("XRecord callback: processing button event, type = {}", event_type);
                Self::process_button_event(&closure.event_tx, event_data, event_type);
            }
            xlib::MotionNotify => {
                // Validate buffer size for motion event
                if data.data_len < XRECORD_MOTION_EVENT_SIZE {
                    log::warn!(
                        "XRecord callback: insufficient data for motion event: got {} bytes, need {} bytes",
                        data.data_len, XRECORD_MOTION_EVENT_SIZE
                    );
                    return;
                }
                log::trace!("XRecord callback: processing motion event");
                Self::process_motion_event(closure.display.clone(), &closure.event_tx, event_data);
            }
            _ => {
                log::trace!("XRecord callback: unknown event type {}", event_type);
            }
        }
    }

    /// Process keyboard events
    ///
    /// # Safety
    ///
    /// This function is unsafe because it dereferences raw pointers to event data.
    /// The caller must ensure that event_data points to valid memory.
    unsafe fn process_keyboard_event(
        event_tx: &mpsc::Sender<Result<(Position, CaptureEvent), CaptureError>>,
        event_data: *const u8,
        event_type: u8,
        active_clients: &Arc<Mutex<HashSet<Position>>>,
        modifier_state: &Arc<StdMutex<X11ModifierState>>,
    ) {
        // Validate pointer before dereferencing
        if event_data.is_null() {
            log::error!("XRecord: process_keyboard_event - event_data is null");
            return;
        }
        
        let keycode = *(event_data.offset(1) as *const u8);
        let pressed = event_type as i32 == xlib::KeyPress;
        let state = if pressed { 1 } else { 0 };

        // X11 keycodes are shifted by 8 relative to Linux scancodes
        let linux_scancode = (keycode as u32).saturating_sub(X11_KEYCODE_OFFSET);

        log::debug!(
            "X11: Keyboard event - keycode: {}, linux_scancode: {}, state: {}",
            keycode,
            linux_scancode,
            if state == 1 { "pressed" } else { "released" }
        );

        // Update modifier state and check for layout changes
        let modifier_event = {
            let mut mods = modifier_state.lock().unwrap();
            let old_group = mods.group;
            
            // Track pressed keys
            if pressed {
                mods.pressed_keys.insert(linux_scancode);
            } else {
                mods.pressed_keys.remove(&linux_scancode);
            }
            
            // Update modifier state based on scancode
            Self::update_modifier_state(&mut mods, linux_scancode, pressed);
            
            // If group changed, send a modifier event
            if mods.group != old_group {
                log::info!(
                    "X11: Keyboard layout group changed from {} to {}",
                    old_group,
                    mods.group
                );
                Some(KeyboardEvent::Modifiers {
                    depressed: mods.depressed,
                    latched: mods.latched,
                    locked: mods.locked,
                    group: mods.group,
                })
            } else {
                None
            }
        };

        // Send modifier event if layout changed
        if let Some(mods_event) = modifier_event {
            let event = Event::Keyboard(mods_event);
            if let Ok(clients) = active_clients.try_lock() {
                for &position in clients.iter() {
                    match event_tx.try_send(Ok((position, CaptureEvent::Input(event.clone())))) {
                        Ok(_) => {}
                        Err(mpsc::error::TrySendError::Full(_)) => {
                            log::warn!("X11: event channel full, dropping modifier event");
                        }
                        Err(mpsc::error::TrySendError::Closed(_)) => {
                            log::error!("X11: event channel closed");
                            return;
                        }
                    }
                }
            }
        }

        let event = Event::Keyboard(KeyboardEvent::Key {
            time: 0,
            key: linux_scancode,
            state,
        });

        // Send keyboard events to all active capture positions
        // This ensures the release bind can be detected regardless of which
        // position has an active capture
        if let Ok(clients) = active_clients.try_lock() {
            if clients.is_empty() {
                log::debug!("X11: No active captures, sending keyboard event to Left as placeholder");
                // If no active captures, send to Left as placeholder
                // This ensures pressed_keys is still updated
                match event_tx.try_send(Ok((Position::Left, CaptureEvent::Input(event.clone())))) {
                    Ok(_) => {}
                    Err(mpsc::error::TrySendError::Full(_)) => {
                        log::warn!("X11: event channel full, dropping keyboard event");
                    }
                    Err(mpsc::error::TrySendError::Closed(_)) => {
                        log::error!("X11: event channel closed, cannot send keyboard event");
                    }
                }
            } else {
                log::debug!("X11: Active captures at positions: {:?}", clients);
                // Send to all active capture positions
                for &position in clients.iter() {
                    match event_tx.try_send(Ok((position, CaptureEvent::Input(event.clone())))) {
                        Ok(_) => {}
                        Err(mpsc::error::TrySendError::Full(_)) => {
                            log::warn!("X11: event channel full, dropping keyboard event for position {:?}", position);
                        }
                        Err(mpsc::error::TrySendError::Closed(_)) => {
                            log::error!("X11: event channel closed, cannot send keyboard event for position {:?}", position);
                            break;
                        }
                    }
                }
            }
        } else {
            log::debug!("X11: Failed to lock active_clients, sending keyboard event to Left as placeholder");
            // If lock fails, send to Left as placeholder
            match event_tx.try_send(Ok((Position::Left, CaptureEvent::Input(event)))) {
                Ok(_) => {}
                Err(mpsc::error::TrySendError::Full(_)) => {
                    log::warn!("X11: event channel full, dropping keyboard event");
                }
                Err(mpsc::error::TrySendError::Closed(_)) => {
                    log::error!("X11: event channel closed, cannot send keyboard event");
                }
            }
        }
    }

    /// Update modifier state based on key event
    fn update_modifier_state(mods: &mut X11ModifierState, scancode: u32, pressed: bool) {
        use input_event::scancode::Linux;
        
        // Standard X11 modifier masks
        const SHIFT_MASK: u32 = 1;
        const LOCK_MASK: u32 = 2;  // Caps Lock
        const CONTROL_MASK: u32 = 4;
        const MOD1_MASK: u32 = 8;  // Alt
        const MOD2_MASK: u32 = 16; // Num Lock
        const MOD4_MASK: u32 = 64; // Super/Windows key
        
        let scancode_enum = Linux::try_from(scancode);
        
        if let Ok(key) = scancode_enum {
            match key {
                // Shift keys
                Linux::KeyLeftShift | Linux::KeyRightShift => {
                    if pressed {
                        mods.depressed |= SHIFT_MASK;
                    } else {
                        mods.depressed &= !SHIFT_MASK;
                    }
                }
                // Control keys
                Linux::KeyLeftCtrl | Linux::KeyRightCtrl => {
                    if pressed {
                        mods.depressed |= CONTROL_MASK;
                    } else {
                        mods.depressed &= !CONTROL_MASK;
                    }
                }
                // Alt keys
                Linux::KeyLeftAlt | Linux::KeyRightalt => {
                    if pressed {
                        mods.depressed |= MOD1_MASK;
                    } else {
                        mods.depressed &= !MOD1_MASK;
                    }
                }
                // Super/Windows keys
                Linux::KeyLeftMeta | Linux::KeyRightmeta => {
                    if pressed {
                        mods.depressed |= MOD4_MASK;
                    } else {
                        mods.depressed &= !MOD4_MASK;
                    }
                }
                // Caps Lock (toggle) - also used for layout switching in some configurations
                Linux::KeyCapsLock => {
                    if pressed {
                        // Toggle Caps Lock state
                        mods.locked ^= LOCK_MASK;
                        log::debug!("X11: Caps Lock toggled, locked = {}", mods.locked);
                        
                        // Note: When Caps Lock is configured as layout switch key,
                        // the XKB configuration will change the group. We can't detect
                        // this from XRecord events alone, but we handle it by tracking
                        // the modifier state and sending it to the target.
                    }
                }
                // Num Lock (toggle)
                Linux::KeyNumlock => {
                    if pressed {
                        mods.locked ^= MOD2_MASK;
                        log::debug!("X11: Num Lock toggled, locked = {}", mods.locked);
                    }
                }
                _ => {}
            }
        }
    }

    /// Process mouse button events
    ///
    /// # Safety
    ///
    /// This function is unsafe because it dereferences raw pointers to event data.
    /// The caller must ensure that event_data points to valid memory.
    unsafe fn process_button_event(
        event_tx: &mpsc::Sender<Result<(Position, CaptureEvent), CaptureError>>,
        event_data: *const u8,
        event_type: u8,
    ) {
        // Validate pointer before dereferencing
        if event_data.is_null() {
            log::error!("XRecord: process_button_event - event_data is null");
            return;
        }
        
        let button = *(event_data.offset(1) as *const u8);
        let state = if event_type as i32 == xlib::ButtonPress { 1 } else { 0 };

        let event = Event::Pointer(PointerEvent::Button {
            time: 0,
            button: button as u32,
            state,
        });

        // Send event (position will be determined by edge detection)
        // For now, we send to Left as placeholder
        match event_tx.try_send(Ok((Position::Left, CaptureEvent::Input(event)))) {
            Ok(_) => {}
            Err(mpsc::error::TrySendError::Full(_)) => {
                log::warn!("X11: event channel full, dropping button event");
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
                log::error!("X11: event channel closed, cannot send button event");
            }
        }
    }

    /// Process mouse motion events
    ///
    /// # Safety
    ///
    /// This function is unsafe because it dereferences raw pointers to event data.
    /// The caller must ensure that event_data points to valid memory.
    unsafe fn process_motion_event(
        display: SendDisplay,
        event_tx: &mpsc::Sender<Result<(Position, CaptureEvent), CaptureError>>,
        event_data: *const u8,
    ) {
        // Validate pointer before dereferencing
        if event_data.is_null() {
            log::error!("XRecord: process_motion_event - event_data is null");
            return;
        }
        
        let x = *(event_data.offset(1) as *const i16) as i32;
        let y = *(event_data.offset(3) as *const i16) as i32;

        let event = Event::Pointer(PointerEvent::Motion {
            time: 0,
            dx: x as f64,
            dy: y as f64,
        });

        // Get current cursor position from X server
        let (cursor_x, cursor_y) = match Self::query_cursor_position(display) {
            Ok(pos) => pos,
            Err(e) => {
                log::warn!("X11: failed to query cursor position: {}, using (0, 0)", e);
                (0, 0)
            }
        };

        // Log actual cursor position from system
        if log::log_enabled!(log::Level::Trace) {
            log::trace!("X11: motion event - delta: ({}, {}), cursor position: ({}, {})", x, y, cursor_x, cursor_y);
        }

        // Send event (position will be determined by edge detection)
        // For now, we send to Left as placeholder
        match event_tx.try_send(Ok((Position::Left, CaptureEvent::Input(event)))) {
            Ok(_) => {}
            Err(mpsc::error::TrySendError::Full(_)) => {
                log::warn!("X11: event channel full, dropping motion event");
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
                log::error!("X11: event channel closed, cannot send motion event");
            }
        }
    }

    /// Query current cursor position from X server
    ///
    /// This function uses XQueryPointer to get the current cursor position.
    /// It returns the window-relative coordinates.
    ///
    /// # Arguments
    ///
    /// * `display` - The X11 display connection
    ///
    /// # Returns
    ///
    /// * `Ok((x, y))` - The cursor position in window coordinates
    /// * `Err(String)` - An error message if the query fails
    ///
    /// # Safety
    ///
    /// This function is unsafe because it calls XQueryPointer which is an FFI function.
    /// The caller must ensure that the display pointer is valid.
    unsafe fn query_cursor_position(display: SendDisplay) -> Result<(i32, i32), String> {
        // Validate display before use
        if !display.is_valid() {
            return Err("Display is not valid (closed or null)".to_string());
        }
        
        let mut root_x: i32 = 0;
        let mut root_y: i32 = 0;
        let mut win_x: i32 = 0;
        let mut win_y: i32 = 0;
        let mut mask: u32 = 0;
        let mut root_return: xlib::Window = 0;
        let mut child_return: xlib::Window = 0;

        // Query pointer position from X server
        let success = XQueryPointer(
            display.get(),
            xlib::XDefaultRootWindow(display.get()),
            &mut root_return,
            &mut child_return,
            &mut root_x,
            &mut root_y,
            &mut win_x,
            &mut win_y,
            &mut mask,
        );

        if success == 0 {
            return Err("XQueryPointer failed".to_string());
        }

        if log::log_enabled!(log::Level::Trace) {
            log::trace!("X11: cursor position query - root: ({}, {}), win: ({}, {}), mask: {}", root_x, root_y, win_x, win_y, mask);
        }

        Ok((win_x, win_y))
    }

    /// Warp cursor to specified position
    ///
    /// This function moves the cursor to the specified position using XWarpPointer.
    ///
    /// # Arguments
    ///
    /// * `x` - The x coordinate to warp to
    /// * `y` - The y coordinate to warp to
    fn warp_cursor(&self, x: i32, y: i32) {
        log::trace!("X11: warp_cursor - warping cursor to ({}, {})", x, y);
        log::trace!("X11: warp_cursor - screen bounds: {}x{}, target position within bounds: x=[0, {}], y=[0, {}]",
                    self.screen_width, self.screen_height, self.screen_width - 1, self.screen_height - 1);
        
        // Validate display before use
        if !self.display.is_valid() {
            log::error!("X11: warp_cursor - display is not valid, cannot warp cursor");
            return;
        }
        
        unsafe {
            let root_window = XDefaultRootWindow(self.display.get());
            XWarpPointer(
                self.display.get(),
                0,
                root_window,
                0,
                0,
                0,
                0,
                x,
                y,
            );
            XFlush(self.display.get());
        }
        log::trace!("X11: warp_cursor - cursor warped to ({}, {})", x, y);
    }

    /// Reset cursor to the position where capture started
    fn reset_cursor(&self) {
        if let Ok(pos) = self.enter_position.try_lock() {
            if let Some((x, y)) = *pos {
                log::trace!("X11: Resetting cursor to saved position: ({}, {})", x, y);
                log::trace!("X11: Cursor reset - returning from edge to original position ({}, {})", x, y);
                self.warp_cursor(x, y);
            } else {
                log::trace!("X11: No saved enter position to reset to");
            }
        } else {
            log::trace!("X11: Failed to acquire lock for cursor reset");
        }
    }

    /// Start capture at the specified position
    ///
    /// This method saves the current cursor position, sets the capture position,
    /// and warps the cursor to the edge of the screen.
    ///
    /// # Arguments
    ///
    /// * `position` - The position where capture is starting
    /// * `root_x` - The current cursor x position
    /// * `root_y` - The current cursor y position
    fn start_capture(&self, position: Position, root_x: i32, root_y: i32) {
        log::info!("X11: Starting capture at position: {:?}, cursor position: ({}, {})", position, root_x, root_y);
        log::trace!("X11: Screen dimensions: {}x{}", self.screen_width, self.screen_height);

        // Calculate safe enter position - offset from edge to prevent infinite loop
        // If cursor is at edge, save position slightly away from edge
        let safe_offset = 10; // pixels away from edge
        let (enter_x, enter_y) = match position {
            Position::Left => {
                // If at left edge, save position slightly to the right
                (safe_offset.max(root_x), root_y)
            }
            Position::Right => {
                // If at right edge, save position slightly to the left
                ((self.screen_width - 1 - safe_offset).min(root_x), root_y)
            }
            Position::Top => {
                // If at top edge, save position slightly below
                (root_x, safe_offset.max(root_y))
            }
            Position::Bottom => {
                // If at bottom edge, save position slightly above
                (root_x, (self.screen_height - 1 - safe_offset).min(root_y))
            }
        };

        // Save the safe enter position (not at edge)
        if let Ok(mut pos) = self.enter_position.try_lock() {
            *pos = Some((enter_x, enter_y));
            log::debug!("Saved enter position: ({}, {}) (original: ({}, {}))", enter_x, enter_y, root_x, root_y);
            log::trace!("X11: Enter position saved, cursor will return to ({}, {}) when capture is released", enter_x, enter_y);
        }

        // Set current capture position
        if let Ok(mut current) = self.current_pos.try_lock() {
            *current = Some(position);
            log::debug!("Set current capture position: {:?}", position);
            log::trace!("X11: Current capture position set to {:?}, cursor will be constrained to edge", position);
        }

        // Warp cursor to the edge with a small offset
        let (new_x, new_y) = match position {
            Position::Left => (EDGE_OFFSET_PIXELS, root_y),
            Position::Right => (self.screen_width - 1 - EDGE_OFFSET_PIXELS, root_y),
            Position::Top => (root_x, EDGE_OFFSET_PIXELS),
            Position::Bottom => (root_x, self.screen_height - 1 - EDGE_OFFSET_PIXELS),
        };
        log::info!("X11: Warping cursor from ({}, {}) to edge position: ({}, {})", root_x, root_y, new_x, new_y);
        log::trace!("X11: Edge offset: {}, target position: ({}, {}), screen bounds: {}x{}", EDGE_OFFSET_PIXELS, new_x, new_y, self.screen_width, self.screen_height);
        self.warp_cursor(new_x, new_y);
    }

    /// Check if cursor has crossed an edge
    ///
    /// This method queries the current cursor position and checks if it has crossed
    /// any screen edge. It uses atomic updates to prevent race conditions.
    ///
    /// # Returns
    ///
    /// * `Some(position)` - The edge that was crossed
    /// * `None` - No edge was crossed
    fn check_edge_crossing(&self) -> Option<Position> {
        // Validate display before use
        if !self.display.is_valid() {
            log::error!("X11: check_edge_crossing - display is not valid");
            return None;
        }
        
        // Query current cursor position
        let (root_x, root_y) = unsafe {
            let mut root_x: i32 = 0;
            let mut root_y: i32 = 0;
            let mut win_x: i32 = 0;
            let mut win_y: i32 = 0;
            let mut mask: u32 = 0;
            let mut root_return: xlib::Window = 0;
            let mut child_return: xlib::Window = 0;

            XQueryPointer(
                self.display.get(),
                xlib::XDefaultRootWindow(self.display.get()),
                &mut root_return,
                &mut child_return,
                &mut root_x,
                &mut root_y,
                &mut win_x,
                &mut win_y,
                &mut mask,
            );

            (root_x, root_y)
        };

        if log::log_enabled!(log::Level::Trace) {
            log::trace!("X11: check_edge_crossing - cursor position: ({}, {}), screen bounds: {}x{}",
                        root_x, root_y, self.screen_width, self.screen_height);
        }

        // Atomically update cursor state and get previous position
        let (prev_x, prev_y) = self.cursor_state.update((root_x, root_y));

        // Check if we're currently in capture mode
        let is_capturing = self.current_pos.try_lock().ok()?.is_some();

        // If we're capturing, keep the cursor at the edge
        if is_capturing {
            if log::log_enabled!(log::Level::Trace) {
                log::trace!("X11: Capturing mode active, keeping cursor at edge, cursor position: ({}, {})", root_x, root_y);
            }
            self.reset_cursor();
            return None;
        }

        // Check if cursor crossed any screen edge
        if log::log_enabled!(log::Level::Trace) {
            log::trace!("X11: Edge detection - cursor: ({}, {}), prev: ({}, {}), screen: {}x{}, thresholds: left=0, right={}, top=0, bottom={}",
                        root_x, root_y, prev_x, prev_y, self.screen_width, self.screen_height,
                        self.screen_width - 1, self.screen_height - 1);
        }

        // Detect edge crossing by checking if cursor is at edge AND was moving towards it
        // This handles the case where X11 clamps the cursor at the edge
        // Also use edge counter to detect when cursor stays at edge (trying to cross)
        let at_left_edge = root_x <= 0;
        let at_right_edge = root_x >= self.screen_width - 1;
        let at_top_edge = root_y <= 0;
        let at_bottom_edge = root_y >= self.screen_height - 1;

        // Check which edge we're at (if any)
        let edge_position = if at_left_edge {
            Some(Position::Left)
        } else if at_right_edge {
            Some(Position::Right)
        } else if at_top_edge {
            Some(Position::Top)
        } else if at_bottom_edge {
            Some(Position::Bottom)
        } else {
            None
        };

        // Check if this edge is an active capture position
        // Only trigger capture if the edge is configured as a client
        let should_capture = if let Some(edge) = edge_position {
            if let Ok(clients) = self.active_clients.try_lock() {
                clients.contains(&edge)
            } else {
                false
            }
        } else {
            false
        };

        if log::log_enabled!(log::Level::Trace) {
            if let Some(edge) = edge_position {
                log::trace!("X11: Cursor at edge {:?}, should_capture: {}, active_clients: {:?}", 
                            edge, should_capture, 
                            self.active_clients.try_lock().map(|c| c.clone()).unwrap_or_default());
            }
        }

        // Check if cursor is at any edge AND should trigger capture
        if at_left_edge || at_right_edge || at_top_edge || at_bottom_edge {
            // Prevent edge detection if cursor just entered from this edge
            // This prevents immediate edge crossing after cursor enters from a client
            let just_entered = if let Ok(guard) = self.current_pos.try_lock() {
                guard.as_ref().map(|pos| {
                    // Check if we're at the same edge as current capture position
                    match pos {
                        Position::Left if at_left_edge => true,
                        Position::Right if at_right_edge => true,
                        Position::Top if at_top_edge => true,
                        Position::Bottom if at_bottom_edge => true,
                        _ => false,
                    }
                }).unwrap_or(false)
            } else {
                false
            };
            
            if just_entered {
                if log::log_enabled!(log::Level::Debug) {
                    log::debug!("X11: Cursor just entered from this edge, skipping edge detection");
                }
                return None;
            }
            
            // Increment edge counter
            if let Ok(mut counter) = self.edge_counter.try_lock() {
                *counter += 1;
                if log::log_enabled!(log::Level::Trace) {
                    log::trace!("X11: Cursor at edge, counter: {}, pos: ({}, {})", *counter, root_x, root_y);
                }
                
                // If counter reaches threshold AND edge is active, trigger edge crossing
                if *counter >= EDGE_COUNTER_THRESHOLD && should_capture {
                    // Determine which edge and return
                    if at_left_edge {
                        log::info!("X11: Cursor crossed left edge at ({}, {}), preparing to return to client", root_x, root_y);
                        *counter = 0; // Reset counter
                        return Some(Position::Left);
                    } else if at_right_edge {
                        log::info!("X11: Cursor crossed right edge at ({}, {}), preparing to return to client", root_x, root_y);
                        *counter = 0; // Reset counter
                        return Some(Position::Right);
                    } else if at_top_edge {
                        log::info!("X11: Cursor crossed top edge at ({}, {}), preparing to return to client", root_x, root_y);
                        *counter = 0; // Reset counter
                        return Some(Position::Top);
                    } else if at_bottom_edge {
                        log::info!("X11: Cursor crossed bottom edge at ({}, {}), preparing to return to client", root_x, root_y);
                        *counter = 0; // Reset counter
                        return Some(Position::Bottom);
                    }
                }
            }
        } else {
            // Not at edge, reset counter
            if let Ok(mut counter) = self.edge_counter.try_lock() {
                if *counter > 0 {
                    if log::log_enabled!(log::Level::Trace) {
                        log::trace!("X11: Cursor left edge, resetting counter from {}", *counter);
                    }
                    *counter = 0;
                }
            }
        }

        // Also detect edge crossing when cursor moves towards edge
        // Only trigger capture if the edge is in active_clients
        if root_x <= 0 && prev_x > 0 {
            // Check if left edge is active
            let should_capture = if let Ok(clients) = self.active_clients.try_lock() {
                clients.contains(&Position::Left)
            } else {
                false
            };
            
            if should_capture {
                log::info!("X11: Cursor crossed left edge at ({}, {}) from ({}, {}), preparing to return to client", root_x, root_y, prev_x, prev_y);
                if log::log_enabled!(log::Level::Trace) {
                    log::trace!("X11: Edge check - left edge crossed (x={}, prev_x={})", root_x, prev_x);
                }
                return Some(Position::Left);
            }
        } else if root_x >= self.screen_width - 1 && root_x > prev_x {
            // Check if right edge is active
            let should_capture = if let Ok(clients) = self.active_clients.try_lock() {
                clients.contains(&Position::Right)
            } else {
                false
            };
            
            if should_capture {
                log::info!("X11: Cursor crossed right edge at ({}, {}) from ({}, {}), preparing to return to client", root_x, root_y, prev_x, prev_y);
                if log::log_enabled!(log::Level::Trace) {
                    log::trace!("X11: Edge check - right edge crossed (x={}, prev_x={}, threshold={})", root_x, prev_x, self.screen_width - 1);
                }
                return Some(Position::Right);
            }
        } else if root_y <= 0 && prev_y > 0 {
            // Check if top edge is active
            let should_capture = if let Ok(clients) = self.active_clients.try_lock() {
                clients.contains(&Position::Top)
            } else {
                false
            };
            
            if should_capture {
                log::info!("X11: Cursor crossed top edge at ({}, {}) from ({}, {}), preparing to return to client", root_x, root_y, prev_x, prev_y);
                if log::log_enabled!(log::Level::Trace) {
                    log::trace!("X11: Edge check - top edge crossed (y={}, prev_y={})", root_y, prev_y);
                }
                return Some(Position::Top);
            }
        } else if root_y >= self.screen_height - 1 && root_y > prev_y {
            // Check if bottom edge is active
            let should_capture = if let Ok(clients) = self.active_clients.try_lock() {
                clients.contains(&Position::Bottom)
            } else {
                false
            };
            
            if should_capture {
                log::info!("X11: Cursor crossed bottom edge at ({}, {}) from ({}, {}), preparing to return to client", root_x, root_y, prev_x, prev_y);
                if log::log_enabled!(log::Level::Trace) {
                    log::trace!("X11: Edge check - bottom edge crossed (y={}, prev_y={}, threshold={})", root_y, prev_y, self.screen_height - 1);
                }
                return Some(Position::Bottom);
            }
        }

        if log::log_enabled!(log::Level::Trace) {
            log::trace!("X11: check_edge_crossing - cursor at ({}, {}), no edge crossed", root_x, root_y);
        }
        None
    }
}

impl Drop for X11InputCapture {
    fn drop(&mut self) {
        log::info!("Cleaning up X11 input capture");

        // Signal the XRecord thread to shutdown FIRST
        self.shutdown_flag.store(true, Ordering::Release);

        // Disable XRecord context to stop new events
        if self.record_context != 0 {
            unsafe {
                xrecord::XRecordDisableContext(self.record_display.get(), self.record_context);
                xrecord::XRecordFreeContext(self.record_display.get(), self.record_context);
            }
        }

        // Wait for XRecord thread to finish BEFORE closing displays
        // This prevents use-after-free if the thread is still using the display
        if let Some(handle) = self.record_thread.take() {
            if !handle.is_finished() {
                log::debug!("Waiting for XRecord thread to finish...");
                // Give the thread 2 seconds to gracefully shutdown
                let _ = handle.join();
                log::debug!("XRecord thread finished");
            }
        }

        // Now it's safe to close displays
        unsafe {
            self.record_display.close();
            self.display.close();
        }

        log::info!("X11 input capture cleanup complete");
    }
}

#[async_trait]
impl Capture for X11InputCapture {
    async fn create(&mut self, pos: Position) -> Result<(), CaptureError> {
        log::info!("Creating capture at position: {pos}");
        let active_clients = Arc::clone(&self.active_clients);
        spawn_blocking(move || {
            let mut clients = active_clients.blocking_lock();
            clients.insert(pos);
        })
        .await
        .map_err(|e| CaptureError::Other(e.to_string()))?;
        Ok(())
    }

    async fn destroy(&mut self, pos: Position) -> Result<(), CaptureError> {
        log::info!("Destroying capture at position: {pos}");
        let active_clients = Arc::clone(&self.active_clients);
        spawn_blocking(move || {
            let mut clients = active_clients.blocking_lock();
            clients.remove(&pos);
        })
        .await
        .map_err(|e| CaptureError::Other(e.to_string()))?;
        Ok(())
    }

    async fn release(&mut self) -> Result<(), CaptureError> {
        log::info!("X11: Releasing capture");
        // Clear capture state
        let current_pos = Arc::clone(&self.current_pos);
        let enter_position = Arc::clone(&self.enter_position);
        let edge_counter = Arc::clone(&self.edge_counter);
        
        spawn_blocking(move || {
            if let Ok(mut current) = current_pos.try_lock() {
                log::debug!("Clearing current capture position");
                log::trace!("X11: Current capture position was: {:?}", *current);
                *current = None;
                log::trace!("X11: Current capture position cleared, cursor is no longer constrained");
            }
            if let Ok(mut pos) = enter_position.try_lock() {
                log::debug!("Clearing enter position");
                log::trace!("X11: Enter position was: {:?}", *pos);
                *pos = None;
                log::trace!("X11: Enter position cleared, cursor will not be reset on next capture");
            }
            // CRITICAL FIX: Reset edge counter when capture is released
            // This prevents edge counter from accumulating across capture sessions
            if let Ok(mut counter) = edge_counter.try_lock() {
                if *counter > 0 {
                    log::debug!("Resetting edge counter from {} to 0", *counter);
                    *counter = 0;
                }
            }
        })
        .await
        .map_err(|e| CaptureError::Other(e.to_string()))?;
        
        log::debug!("X11: Capture released, cursor can now move freely");
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
            if log::log_enabled!(log::Level::Trace) {
                log::trace!("X11: poll_next - edge crossed at position {:?}", pos);
            }

            // Get current cursor position
            let (root_x, root_y) = self.cursor_state.current();

            if log::log_enabled!(log::Level::Trace) {
                log::trace!("X11: poll_next - starting capture at position {:?} with cursor at ({}, {})", pos, root_x, root_y);
            }

            // Start capture at this position
            self.start_capture(pos, root_x, root_y);

            return Poll::Ready(Some(Ok((pos, CaptureEvent::Begin))));
        }

        // Poll for captured events
        match self.event_rx.poll_recv(cx) {
            Poll::Ready(Some(event)) => {
                if log::log_enabled!(log::Level::Trace) {
                    log::trace!("X11: poll_next - received event from event_rx");
                }
                Poll::Ready(Some(event))
            }
            Poll::Ready(None) => {
                if log::log_enabled!(log::Level::Trace) {
                    log::trace!("X11: poll_next - event_rx stream ended");
                }
                Poll::Ready(None)
            }
            Poll::Pending => {
                if log::log_enabled!(log::Level::Trace) {
                    log::trace!("X11: poll_next - no events available, pending");
                }
                Poll::Pending
            }
        }
    }
}
