use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};
use x11::xlib::{self, XCloseDisplay, XErrorEvent};

use super::error::{X11EmulationError, X11Result};

// ============================================================================
// X11 Error Handler
// ============================================================================

/// Flag to track if error handlers have been installed
static mut X11_ERROR_HANDLER_INSTALLED: bool = false;

/// X11 error handler function
///
/// This function is called by X11 when a protocol error occurs.
/// It logs the error details and returns 0 to prevent X11 from
/// printing to stderr.
unsafe extern "C" fn x11_error_handler(
    _display: *mut x11::xlib::Display,
    error_event: *mut XErrorEvent,
) -> i32 {
    if error_event.is_null() {
        return 0;
    }
    
    let event = &*error_event;
    tracing::error!(
        target: "x11::error",
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
    _display: *mut x11::xlib::Display,
) -> i32 {
    tracing::error!(
        target: "x11::error",
        "X11 I/O Error: display connection lost or X server terminated"
    );
    0 // Return 0 to prevent X11 from printing to stderr
}

/// Install X11 error handlers
///
/// This function installs error and I/O error handlers for the X11 connection.
/// It should be called once during initialization.
pub(crate) unsafe fn install_x11_error_handlers() {
    // Only install once
    if X11_ERROR_HANDLER_INSTALLED {
        return;
    }
    
    x11::xlib::XSetErrorHandler(Some(x11_error_handler));
    x11::xlib::XSetIOErrorHandler(Some(x11_io_error_handler));
    X11_ERROR_HANDLER_INSTALLED = true;
    
    tracing::debug!(target: "x11::display", "X11 error handlers installed");
}

/// Safe wrapper for X11 Display pointer
///
/// Uses Arc<AtomicPtr> and Arc<AtomicBool> to ensure:
/// - Thread-safe access to display pointer
/// - Proper cleanup when display is closed
/// - Prevention of use-after-free via closed state tracking
#[derive(Clone)]
#[allow(dead_code)] // Will be used in future steps
pub struct X11DisplayHandle {
    /// Atomic pointer to X11 display
    ptr: Arc<AtomicPtr<xlib::Display>>,
    /// Flag indicating whether display is closed
    closed: Arc<AtomicBool>,
}

#[allow(dead_code)] // Will be used in future steps
impl X11DisplayHandle {
    /// Create a new X11DisplayHandle from a raw pointer
    ///
    /// # Safety
    ///
    /// Caller must ensure that the pointer is valid and points to an open X11 display.
    pub unsafe fn new(ptr: *mut xlib::Display) -> Self {
        Self {
            ptr: Arc::new(AtomicPtr::new(ptr)),
            closed: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Check if display is valid (not closed and not null)
    pub fn is_valid(&self) -> bool {
        !self.closed.load(Ordering::Acquire) && !self.ptr.load(Ordering::Acquire).is_null()
    }

    /// Get raw pointer to display
    ///
    /// # Safety
    ///
    /// Caller must ensure that display is still valid (check via is_valid())
    /// and that pointer is used correctly according to X11 API requirements.
    pub unsafe fn get(&self) -> *mut xlib::Display {
        self.ptr.load(Ordering::Acquire)
    }

    /// Close display and mark as closed
    ///
    /// This method is idempotent - multiple calls are safe.
    pub unsafe fn close(&self) {
        if self.closed.swap(true, Ordering::AcqRel) {
            return; // Already closed
        }
        let ptr = self.ptr.swap(std::ptr::null_mut(), Ordering::AcqRel);
        if !ptr.is_null() {
            XCloseDisplay(ptr);
        }
    }

    /// Flush display (send all buffered requests)
    pub unsafe fn flush(&self) -> X11Result<()> {
        if !self.is_valid() {
            return Err(X11EmulationError::InvalidDisplay);
        }
        xlib::XFlush(self.get());
        Ok(())
    }
}

// SAFETY: X11DisplayHandle uses Arc<AtomicPtr> and Arc<AtomicBool>, which are Send and Sync,
// ensuring thread-safe access to display pointer and closed state.
unsafe impl Send for X11DisplayHandle {}
unsafe impl Sync for X11DisplayHandle {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ptr;

    #[test]
    fn test_display_handle_creation() {
        // Create a mock display pointer (null for testing)
        let ptr = ptr::null_mut::<xlib::Display>();
        let handle = unsafe { X11DisplayHandle::new(ptr) };

        // Should be invalid because pointer is null
        assert!(!handle.is_valid());
    }

    #[test]
    fn test_display_handle_validity() {
        let ptr = ptr::null_mut::<xlib::Display>();
        let handle = unsafe { X11DisplayHandle::new(ptr) };

        assert!(!handle.is_valid());
    }

    #[test]
    fn test_display_handle_close_idempotent() {
        let ptr = ptr::null_mut::<xlib::Display>();
        let handle = unsafe { X11DisplayHandle::new(ptr) };

        // First close
        unsafe { handle.close() };
        assert!(!handle.is_valid());

        // Second close should be safe (idempotent)
        unsafe { handle.close() };
        assert!(!handle.is_valid());
    }

    #[test]
    fn test_display_handle_clone() {
        let ptr = ptr::null_mut::<xlib::Display>();
        let handle1 = unsafe { X11DisplayHandle::new(ptr) };
        let handle2 = handle1.clone();

        // Both handles should share the same state
        assert_eq!(handle1.is_valid(), handle2.is_valid());

        // Close via handle1
        unsafe { handle1.close() };

        // handle2 should also see the closed state
        assert!(!handle1.is_valid());
        assert!(!handle2.is_valid());
    }

    #[test]
    fn test_flush_invalid_display() {
        let ptr = ptr::null_mut::<xlib::Display>();
        let handle = unsafe { X11DisplayHandle::new(ptr) };

        let result = unsafe { handle.flush() };
        assert!(matches!(result, Err(X11EmulationError::InvalidDisplay)));
    }

    #[test]
    fn test_get_raw_pointer() {
        let ptr = ptr::null_mut::<xlib::Display>();
        let handle = unsafe { X11DisplayHandle::new(ptr) };

        let raw_ptr = unsafe { handle.get() };
        assert!(raw_ptr.is_null());
    }
}
