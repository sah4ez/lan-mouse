use async_trait::async_trait;
use std::ptr;
use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};
use std::sync::Arc;
use x11::{
    xlib::{self, XCloseDisplay, XQueryPointer},
    xtest,
};

use input_event::{
    BTN_BACK, BTN_FORWARD, BTN_LEFT, BTN_MIDDLE, BTN_RIGHT, Event, KeyboardEvent, PointerEvent,
};

use crate::error::EmulationError;

use super::{Emulation, EmulationHandle, error::X11EmulationCreationError};

// ============================================================================
// Constants
// ============================================================================

/// X11 keycode offset relative to Linux scancode
const X11_KEYCODE_OFFSET: u32 = 8;

/// Scroll button codes for XTest
const SCROLL_UP: u32 = 4;
const SCROLL_DOWN: u32 = 5;
const SCROLL_LEFT: u32 = 6;
const SCROLL_RIGHT: u32 = 7;

// ============================================================================
// Display Handle
// ============================================================================

/// Thread-safe wrapper for X11 display pointer
///
/// This struct uses Arc<AtomicPtr> and Arc<AtomicBool> to ensure:
/// - Thread-safe access to the display pointer
/// - Proper cleanup when the display is closed
/// - Prevention of use-after-free by tracking closed state
#[derive(Clone)]
struct DisplayHandle {
    /// Atomic pointer to the X11 display
    ptr: Arc<AtomicPtr<xlib::Display>>,
    /// Flag indicating if the display has been closed
    closed: Arc<AtomicBool>,
}

impl DisplayHandle {
    /// Create a new DisplayHandle from a raw pointer
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

// SAFETY: DisplayHandle uses Arc<AtomicPtr> and Arc<AtomicBool> which are Send and Sync,
// ensuring thread-safe access to the display pointer and closed state.
unsafe impl Send for DisplayHandle {}
unsafe impl Sync for DisplayHandle {}

// ============================================================================
// X11 Emulation
// ============================================================================

/// X11 input emulation backend using XTest extension
///
/// This backend emulates input events on the X server using the XTest extension.
/// It provides methods for emulating mouse motion, button events, scroll events,
/// and keyboard events.
///
/// # Thread Safety
///
/// This struct implements Send, allowing it to be moved between threads.
/// However, X11 display operations are not thread-safe, so callers must ensure
/// proper synchronization when using this struct from multiple threads.
///
/// # Example
///
/// ```no_run
/// use input_emulation::InputEmulation;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let mut emulation = InputEmulation::new(None).await?;
/// // Emulate events...
/// # Ok(())
/// # }
/// ```
pub(crate) struct X11Emulation {
    display: DisplayHandle,
}

// SAFETY: X11Emulation uses DisplayHandle which is thread-safe internally.
// However, X11 display operations must still be synchronized externally.
unsafe impl Send for X11Emulation {}

impl X11Emulation {
    /// Create a new X11 input emulation instance
    ///
    /// This method opens a connection to the X server and initializes the XTest extension
    /// for input emulation.
    ///
    /// # Returns
    ///
    /// * `Ok(X11Emulation)` - A new X11 emulation instance
    /// * `Err(X11EmulationCreationError)` - If the display cannot be opened
    pub(crate) fn new() -> Result<Self, X11EmulationCreationError> {
        log::info!("Initializing X11 input emulation backend");

        // Check DISPLAY environment variable
        let display_env = std::env::var("DISPLAY").unwrap_or_else(|_| ":0".to_string());
        log::info!("Using DISPLAY: {}", display_env);

        let display = unsafe {
            match xlib::XOpenDisplay(ptr::null()) {
                d if std::ptr::eq(d, ptr::null_mut::<xlib::Display>()) => {
                    let error_msg = format!(
                        "Failed to open X11 display for emulation. DISPLAY variable is set to: '{}'. \
                        Make sure you're running in an X11 session. \
                        If you're using Wayland, you may need to run with XWayland or use a Wayland-compatible backend.",
                        display_env
                    );
                    log::error!("{}", error_msg);
                    return Err(X11EmulationCreationError::OpenDisplay { display: display_env });
                }
                display => DisplayHandle::new(display),
            }
        };

        Ok(Self { display })
    }

    /// Validate that the display is still valid
    fn validate_display(&self) -> Result<(), EmulationError> {
        if !self.display.is_valid() {
            return Err(EmulationError::Other("Invalid display handle".to_string()));
        }
        Ok(())
    }

    /// Emulate relative mouse motion
    ///
    /// This method moves the cursor by the specified delta using XTestFakeRelativeMotionEvent.
    ///
    /// # Arguments
    ///
    /// * `dx` - The x delta to move the cursor
    /// * `dy` - The y delta to move the cursor
    ///
    /// # Returns
    ///
    /// * `Ok(())` - The motion was emulated successfully
    /// * `Err(EmulationError)` - If the emulation failed
    fn relative_motion(&self, dx: i32, dy: i32) -> Result<(), EmulationError> {
        self.validate_display()?;

        // Query cursor position before applying motion
        let (before_x, before_y) = self.query_cursor_position()?;

        let result = unsafe {
            xtest::XTestFakeRelativeMotionEvent(self.display.get(), dx, dy, 0, 0)
        };

        if result == 0 {
            log::error!("XTestFakeRelativeMotionEvent failed");
            return Err(EmulationError::Other("Failed to emulate motion".to_string()));
        }

        // Query cursor position after applying motion
        let (after_x, after_y) = self.query_cursor_position()?;

        if log::log_enabled!(log::Level::Trace) {
            log::trace!("X11 emulation: relative motion applied - delta: ({}, {}), cursor position: ({}, {}) -> ({}, {})",
                        dx, dy, before_x, before_y, after_x, after_y);
        }

        Ok(())
    }

    /// Query the current cursor position from the X server
    ///
    /// This function uses XQueryPointer to get the current cursor position.
    ///
    /// # Returns
    ///
    /// * `Ok((x, y))` - The cursor position in window coordinates
    /// * `Err(EmulationError)` - If the query fails
    ///
    /// # Safety
    ///
    /// This function is unsafe because it calls XQueryPointer which is an FFI function.
    fn query_cursor_position(&self) -> Result<(i32, i32), EmulationError> {
        self.validate_display()?;

        let mut root_x: i32 = 0;
        let mut root_y: i32 = 0;
        let mut win_x: i32 = 0;
        let mut win_y: i32 = 0;
        let mut mask: u32 = 0;
        let mut root_return: xlib::Window = 0;
        let mut child_return: xlib::Window = 0;

        let success = unsafe {
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
            )
        };

        if success == 0 {
            return Err(EmulationError::Other("XQueryPointer failed".to_string()));
        }

        Ok((win_x, win_y))
    }

    /// Emulate a mouse button event
    ///
    /// This method emulates a mouse button press or release using XTestFakeButtonEvent.
    ///
    /// # Arguments
    ///
    /// * `button` - The button number (BTN_LEFT, BTN_RIGHT, etc.)
    /// * `state` - The button state (1 = pressed, 0 = released)
    ///
    /// # Returns
    ///
    /// * `Ok(())` - The button event was emulated successfully
    /// * `Err(EmulationError)` - If the emulation failed
    fn emulate_mouse_button(&self, button: u32, state: u32) -> Result<(), EmulationError> {
        self.validate_display()?;

        // Query cursor position before button event
        let (cursor_x, cursor_y) = self.query_cursor_position()?;

        let x11_button = match button {
            BTN_RIGHT => 3,
            BTN_MIDDLE => 2,
            BTN_BACK => 8,
            BTN_FORWARD => 9,
            BTN_LEFT => 1,
            _ => 1,
        };

        let result = unsafe {
            xtest::XTestFakeButtonEvent(self.display.get(), x11_button, state as i32, 0)
        };

        if result == 0 {
            log::error!("XTestFakeButtonEvent failed for button {}", x11_button);
            return Err(EmulationError::Other(format!("Failed to emulate button {}", x11_button)));
        }

        if log::log_enabled!(log::Level::Trace) {
            log::trace!("X11 emulation: mouse button event - button: {}, state: {}, cursor position: ({}, {})",
                        x11_button, if state == 1 { "pressed" } else { "released" }, cursor_x, cursor_y);
        }

        Ok(())
    }

    /// Emulate a scroll event
    ///
    /// This method emulates a scroll event by sending button press and release events
    /// for the appropriate scroll direction.
    ///
    /// # Arguments
    ///
    /// * `axis` - The scroll axis (0 = vertical, 1 = horizontal)
    /// * `value` - The scroll value (positive = down/right, negative = up/left)
    ///
    /// # Returns
    ///
    /// * `Ok(())` - The scroll event was emulated successfully
    /// * `Err(EmulationError)` - If the emulation failed
    fn emulate_scroll(&self, axis: u8, value: f64) -> Result<(), EmulationError> {
        self.validate_display()?;

        // Query cursor position before scroll event
        let (cursor_x, cursor_y) = self.query_cursor_position()?;

        let direction = match axis {
            1 => {
                if value < 0.0 {
                    SCROLL_LEFT
                } else {
                    SCROLL_RIGHT
                }
            }
            _ => {
                if value < 0.0 {
                    SCROLL_UP
                } else {
                    SCROLL_DOWN
                }
            }
        };

        // Send button press and release
        unsafe {
            let result1 = xtest::XTestFakeButtonEvent(self.display.get(), direction, 1, 0);
            let result2 = xtest::XTestFakeButtonEvent(self.display.get(), direction, 0, 0);

            if result1 == 0 || result2 == 0 {
                log::error!("XTestFakeButtonEvent failed for scroll direction {}", direction);
                return Err(EmulationError::Other(format!("Failed to emulate scroll direction {}", direction)));
            }
        }

        let direction_str = match direction {
            SCROLL_UP => "up",
            SCROLL_DOWN => "down",
            SCROLL_LEFT => "left",
            SCROLL_RIGHT => "right",
            _ => "unknown",
        };

        if log::log_enabled!(log::Level::Trace) {
            log::trace!("X11 emulation: scroll event - direction: {}, value: {}, cursor position: ({}, {})",
                        direction_str, value, cursor_x, cursor_y);
        }

        Ok(())
    }

    /// Emulate a keyboard event
    ///
    /// This method emulates a key press or release using XTestFakeKeyEvent.
    ///
    /// # Arguments
    ///
    /// * `key` - The Linux scancode
    /// * `state` - The key state (1 = pressed, 0 = released)
    ///
    /// # Returns
    ///
    /// * `Ok(())` - The key event was emulated successfully
    /// * `Err(EmulationError)` - If the emulation failed
    fn emulate_key(&self, key: u32, state: u8) -> Result<(), EmulationError> {
        self.validate_display()?;

        // Query cursor position before key event
        let (cursor_x, cursor_y) = self.query_cursor_position()?;

        // X11 keycodes are shifted by 8 relative to Linux scancodes
        let x11_keycode = key + X11_KEYCODE_OFFSET;

        let result = unsafe {
            xtest::XTestFakeKeyEvent(self.display.get(), x11_keycode, state as i32, 0)
        };

        if result == 0 {
            log::error!("XTestFakeKeyEvent failed for keycode {}", x11_keycode);
            return Err(EmulationError::Other(format!("Failed to emulate key {}", x11_keycode)));
        }

        if log::log_enabled!(log::Level::Trace) {
            log::trace!("X11 emulation: key event - keycode: {}, state: {}, cursor position: ({}, {})",
                        x11_keycode, if state == 1 { "pressed" } else { "released" }, cursor_x, cursor_y);
        }

        Ok(())
    }
}

impl Drop for X11Emulation {
    #[allow(dead_code)]
    fn drop(&mut self) {
        log::info!("Cleaning up X11 input emulation");
        unsafe {
            self.display.close();
        }
        log::info!("X11 input emulation cleanup complete");
    }
}

#[async_trait]
impl Emulation for X11Emulation {
    async fn consume(&mut self, event: Event, _: EmulationHandle) -> Result<(), EmulationError> {
        match event {
            Event::Pointer(pointer_event) => match pointer_event {
                PointerEvent::Motion { time: _, dx, dy } => {
                    self.relative_motion(dx as i32, dy as i32)?;
                }
                PointerEvent::Button {
                    time: _,
                    button,
                    state,
                } => {
                    self.emulate_mouse_button(button, state)?;
                }
                PointerEvent::Axis {
                    time: _,
                    axis,
                    value,
                } => {
                    self.emulate_scroll(axis, value)?;
                }
                PointerEvent::AxisDiscrete120 { axis, value } => {
                    self.emulate_scroll(axis, value as f64)?;
                }
            },
            Event::Keyboard(KeyboardEvent::Key {
                time: _,
                key,
                state,
            }) => {
                self.emulate_key(key, state)?;
            }
            _ => {}
        }

        // Flush the X display to ensure events are sent
        unsafe {
            xlib::XFlush(self.display.get());
        }

        // FIXME: XFlush may fail silently. Consider checking for errors
        // and handling them appropriately. The X protocol may have
        // encountered an error that should be propagated to the caller.

        Ok(())
    }

    async fn create(&mut self, _: EmulationHandle) {
        // For our purposes it does not matter what client sent the event
    }

    async fn destroy(&mut self, _: EmulationHandle) {
        // For our purposes it does not matter what client sent the event
    }

    async fn terminate(&mut self) {
        // Nothing to do
    }
}
