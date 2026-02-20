use crate::x11::{X11DisplayHandle, X11EmulationError, X11Result, logging::timed};

/// Scroll direction
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollDirection {
    /// Natural scrolling (content moves with scroll)
    Natural,
    /// Traditional scrolling (content moves opposite direction)
    Traditional,
}

impl ScrollDirection {
    /// Get multiplier for scroll value
    pub fn multiplier(&self) -> f64 {
        match self {
            ScrollDirection::Natural => 1.0,
            ScrollDirection::Traditional => -1.0,
        }
    }
}

/// Scroll configuration
#[derive(Debug, Clone)]
pub struct ScrollConfig {
    /// Current scroll direction
    pub direction: ScrollDirection,
    /// Scroll sensitivity multiplier
    pub sensitivity: f64,
    /// Enable horizontal scrolling
    pub enable_horizontal: bool,
}

impl ScrollConfig {
    /// Create default configuration
    pub fn new() -> Self {
        Self {
            direction: ScrollDirection::Traditional,
            sensitivity: 1.0,
            enable_horizontal: true,
        }
    }

    /// Detect scroll direction from system settings via XInput2
    pub fn detect_from_system(display: &X11DisplayHandle) -> X11Result<Self> {
        // MEDIUM PRIORITY: Validate display before use (prevents use-after-free)
        if !display.is_valid() {
            return Err(X11EmulationError::InvalidDisplay);
        }
        
        tracing::debug!(target: "x11::scroll::detect", "detecting scroll direction");

        unsafe {
            let direction = query_xinput2_scroll_direction(display)?;

            tracing::info!(
                target: "x11::scroll::detect",
                direction = ?direction,
                "scroll direction detected"
            );

            Ok(Self {
                direction,
                sensitivity: 1.0,
                enable_horizontal: true,
            })
        }
    }

    /// Adjust scroll value with direction and sensitivity
    pub fn adjust_scroll_value(&self, value: f64) -> f64 {
        let adjusted = value * self.direction.multiplier() * self.sensitivity;

        tracing::trace!(
            target: "x11::scroll::adjust",
            original = value,
            direction = ?self.direction,
            sensitivity = self.sensitivity,
            adjusted,
            "scroll value adjusted"
        );

        adjusted
    }
}

impl Default for ScrollConfig {
    fn default() -> Self {
        Self::new()
    }
}

/// Query scroll direction via XInput2
///
/// # Safety
///
/// This function is unsafe because it calls XInput2 functions.
unsafe fn query_xinput2_scroll_direction(
    _display: &X11DisplayHandle,
) -> X11Result<ScrollDirection> {
    // Note: XInput2 implementation requires additional dependencies
    // This is a simplified version
    // Full implementation should use XIQueryDevice and XIQueryProperty

    tracing::warn!(
        target: "x11::scroll::detect",
        "XInput2 detection not fully implemented, using Traditional direction"
    );

    // Temporary solution: use Traditional by default
    // TODO: Implement full detection via XInput2
    Ok(ScrollDirection::Traditional)
}

/// Scroll event
#[derive(Debug, Clone, Copy)]
pub struct ScrollEvent {
    /// Axis: 0 = vertical, 1 = horizontal
    pub axis: u8,
    /// Scroll value (positive = down/right, negative = up/left)
    pub value: f64,
    /// Discrete scroll value (120 = one wheel tick)
    pub discrete: Option<i32>,
}

impl ScrollEvent {
    /// Create new scroll event
    pub fn new(axis: u8, value: f64) -> Self {
        Self {
            axis,
            value,
            discrete: None,
        }
    }

    /// Create scroll event with discrete value
    pub fn with_discrete(axis: u8, value: i32) -> Self {
        Self {
            axis,
            value: value as f64 / 120.0, // Normalize to 0.0-1.0 range
            discrete: Some(value),
        }
    }
}

/// Emulate scroll event
pub fn emulate_scroll(
    display: &X11DisplayHandle,
    scroll_config: &ScrollConfig,
    event: ScrollEvent,
) -> X11Result<()> {
    // MEDIUM PRIORITY: Validate display before use (prevents use-after-free)
    if !display.is_valid() {
        return Err(X11EmulationError::InvalidDisplay);
    }
    
    let adjusted_value = scroll_config.adjust_scroll_value(event.value);

    // Determine scroll button
    let button = match (event.axis, adjusted_value < 0.0) {
        (0, true) => 4,  // Scroll up
        (0, false) => 5, // Scroll down
        (1, true) => 6,  // Scroll left
        (1, false) => 7, // Scroll right
        _ => {
            return Err(X11EmulationError::InvalidScrollAxis);
        }
    };

    let direction_str = match button {
        4 => "up",
        5 => "down",
        6 => "left",
        7 => "right",
        _ => "unknown",
    };

    tracing::trace!(
        target: "x11::scroll::emulate",
        axis = event.axis,
        original_value = event.value,
        adjusted_value,
        direction = ?scroll_config.direction,
        button,
        direction_str,
        discrete = event.discrete,
        "emulating scroll event"
    );

    // Send button press and release
    timed(
        "x11::scroll::emulate",
        "XTestFakeButtonEvent (scroll)",
        || unsafe {
            let press_result = x11::xtest::XTestFakeButtonEvent(
                display.get(),
                button,
                1, // pressed
                0, // delay
            );

            let release_result = x11::xtest::XTestFakeButtonEvent(
                display.get(),
                button,
                0, // released
                0, // delay
            );

            if press_result == 0 || release_result == 0 {
                tracing::error!(
                    target: "x11::scroll::emulate",
                    button,
                    direction_str,
                    "XTestFakeButtonEvent failed for scroll"
                );
                return Err(X11EmulationError::EmulationFailed {
                    operation: "XTestFakeButtonEvent".to_string(),
                    detail: format!("scroll button {} ({})", button, direction_str),
                });
            }

            Ok(())
        },
    )?;

    tracing::debug!(
        target: "x11::scroll::emulate",
        axis = event.axis,
        value = event.value,
        adjusted_value,
        direction = ?scroll_config.direction,
        direction_str,
        "scroll event emulated successfully"
    );

    Ok(())
}

/// Get axis name for logging
pub fn axis_name(axis: u8) -> &'static str {
    match axis {
        0 => "vertical",
        1 => "horizontal",
        _ => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scroll_direction_multiplier() {
        assert_eq!(ScrollDirection::Natural.multiplier(), 1.0);
        assert_eq!(ScrollDirection::Traditional.multiplier(), -1.0);
    }

    #[test]
    fn test_scroll_config_adjust() {
        let config = ScrollConfig {
            direction: ScrollDirection::Natural,
            sensitivity: 1.0,
            enable_horizontal: true,
        };

        assert_eq!(config.adjust_scroll_value(1.0), 1.0);
        assert_eq!(config.adjust_scroll_value(-1.0), -1.0);
    }

    #[test]
    fn test_scroll_config_adjust_traditional() {
        let config = ScrollConfig {
            direction: ScrollDirection::Traditional,
            sensitivity: 1.0,
            enable_horizontal: true,
        };

        // Traditional inverts direction
        assert_eq!(config.adjust_scroll_value(1.0), -1.0);
        assert_eq!(config.adjust_scroll_value(-1.0), 1.0);
    }

    #[test]
    fn test_scroll_config_sensitivity() {
        let config = ScrollConfig {
            direction: ScrollDirection::Natural,
            sensitivity: 2.0,
            enable_horizontal: true,
        };

        assert_eq!(config.adjust_scroll_value(1.0), 2.0);
        assert_eq!(config.adjust_scroll_value(-1.0), -2.0);
    }

    #[test]
    fn test_scroll_event() {
        let event = ScrollEvent::new(0, 1.0);
        assert_eq!(event.axis, 0);
        assert_eq!(event.value, 1.0);
        assert!(event.discrete.is_none());
    }

    #[test]
    fn test_scroll_event_discrete() {
        let event = ScrollEvent::with_discrete(0, 120);
        assert_eq!(event.axis, 0);
        assert_eq!(event.value, 1.0);
        assert_eq!(event.discrete, Some(120));
    }

    #[test]
    fn test_axis_name() {
        assert_eq!(axis_name(0), "vertical");
        assert_eq!(axis_name(1), "horizontal");
        assert_eq!(axis_name(2), "unknown");
    }
}
