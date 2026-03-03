/// Logging targets for X11 subsystem
#[allow(dead_code)] // Will be used in future steps
pub mod targets {
    pub const DISPLAY: &str = "x11::display";
    pub const CURSOR: &str = "x11::cursor";
    pub const KEYBOARD: &str = "x11::keyboard";
    pub const MOUSE: &str = "x11::mouse";
    pub const SCROLL: &str = "x11::scroll";
    pub const SCREEN: &str = "x11::screen";
    pub const NETWORK: &str = "x11::network";
}

/// Initialize tracing subscriber for X11
///
/// # Parameters
///
/// * `level` - Maximum logging level
#[allow(dead_code)] // Will be used in future steps
pub fn init_tracing(level: tracing::Level) {
    tracing_subscriber::fmt()
        .with_max_level(level)
        .with_target(true)
        .with_thread_ids(true)
        .with_span_events(
            tracing_subscriber::fmt::format::FmtSpan::NEW
                | tracing_subscriber::fmt::format::FmtSpan::CLOSE,
        )
        .init();
}

/// Wrapper for timing operation execution
#[allow(dead_code)] // Will be used in future steps
pub fn timed<F, R>(target: &'static str, operation: &'static str, f: F) -> R
where
    F: FnOnce() -> R,
{
    let start = std::time::Instant::now();
    let result = f();
    let duration = start.elapsed();

    tracing::trace!(
        target = target,
        operation,
        duration_ms = duration.as_secs_f64() * 1000.0,
        "operation completed"
    );

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_targets_defined() {
        assert!(!targets::DISPLAY.is_empty());
        assert!(!targets::CURSOR.is_empty());
        assert!(!targets::KEYBOARD.is_empty());
        assert!(!targets::MOUSE.is_empty());
        assert!(!targets::SCROLL.is_empty());
        assert!(!targets::SCREEN.is_empty());
        assert!(!targets::NETWORK.is_empty());
    }

    #[test]
    fn test_timed_wrapper() {
        let result = timed("test_target", "test_operation", || {
            std::thread::sleep(std::time::Duration::from_millis(10));
            42
        });

        assert_eq!(result, 42);
    }

    #[test]
    fn test_timed_wrapper_with_closure() {
        let value = 10;
        let result = timed("test_target", "test_operation", || value * 2);

        assert_eq!(result, 20);
    }
}
