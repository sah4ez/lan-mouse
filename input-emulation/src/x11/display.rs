use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};
use x11::xlib::{self, XCloseDisplay};

use super::error::{X11EmulationError, X11Result};

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
