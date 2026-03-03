use crate::x11::{X11DisplayHandle, X11EmulationError, X11Result, logging::timed};
use std::collections::HashMap;

/// Mouse button constants (internal identifiers)
pub mod buttons {
    /// Left mouse button
    pub const LEFT: u32 = 0x110;
    /// Right mouse button
    pub const RIGHT: u32 = 0x111;
    /// Middle mouse button
    pub const MIDDLE: u32 = 0x112;
    /// Back mouse button
    pub const BACK: u32 = 0x113;
    /// Forward mouse button
    pub const FORWARD: u32 = 0x114;
}

/// Mouse button mapper for converting internal button IDs to X11 button numbers
#[derive(Debug, Clone)]
pub struct ButtonMapper {
    /// Mapping from internal button IDs to X11 button numbers
    pub mappings: HashMap<u32, u32>,
}

impl ButtonMapper {
    /// Create a new button mapper with default mappings
    pub fn new() -> Self {
        let mut mappings = HashMap::new();

        // Standard X11 button numbers
        mappings.insert(buttons::LEFT, 1);
        mappings.insert(buttons::MIDDLE, 2);
        mappings.insert(buttons::RIGHT, 3);
        mappings.insert(buttons::BACK, 8);
        mappings.insert(buttons::FORWARD, 9);

        tracing::debug!(
            target: "x11::mouse::mapping",
            mappings_count = mappings.len(),
            "button mapper created"
        );

        Self { mappings }
    }

    /// Convert internal button ID to X11 button number
    pub fn to_x11_button(&self, button: u32) -> u32 {
        let x11_button = *self.mappings.get(&button).unwrap_or(&button);

        tracing::trace!(
            target: "x11::mouse::mapping",
            internal = button,
            x11 = x11_button,
            "button mapped"
        );

        x11_button
    }

    /// Add a custom button mapping
    pub fn add_custom_mapping(&mut self, internal: u32, x11: u32) {
        tracing::debug!(
            target: "x11::mouse::mapping",
            internal,
            x11,
            "adding custom button mapping"
        );
        self.mappings.insert(internal, x11);
    }

    /// Remove a custom button mapping
    pub fn remove_custom_mapping(&mut self, internal: u32) {
        tracing::debug!(
            target: "x11::mouse::mapping",
            internal,
            "removing custom button mapping"
        );
        self.mappings.remove(&internal);
    }
}

impl Default for ButtonMapper {
    fn default() -> Self {
        Self::new()
    }
}

/// Mouse button state tracker
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ButtonState {
    /// Left button pressed state
    pub left: bool,
    /// Middle button pressed state
    pub middle: bool,
    /// Right button pressed state
    pub right: bool,
    /// Back button pressed state
    pub back: bool,
    /// Forward button pressed state
    pub forward: bool,
}

impl ButtonState {
    /// Create a new button state with all buttons released
    pub fn new() -> Self {
        Self {
            left: false,
            middle: false,
            right: false,
            back: false,
            forward: false,
        }
    }

    /// Update the state of a specific button
    pub fn update(&mut self, button: u32, pressed: bool) {
        match button {
            buttons::LEFT => self.left = pressed,
            buttons::MIDDLE => self.middle = pressed,
            buttons::RIGHT => self.right = pressed,
            buttons::BACK => self.back = pressed,
            buttons::FORWARD => self.forward = pressed,
            _ => {}
        }

        tracing::trace!(
            target: "x11::mouse::state",
            button,
            pressed,
            state = ?self,
            "button state updated"
        );
    }

    /// Check if a specific button is pressed
    pub fn is_pressed(&self, button: u32) -> bool {
        match button {
            buttons::LEFT => self.left,
            buttons::MIDDLE => self.middle,
            buttons::RIGHT => self.right,
            buttons::BACK => self.back,
            buttons::FORWARD => self.forward,
            _ => false,
        }
    }

    /// Get the count of currently pressed buttons
    pub fn pressed_count(&self) -> u32 {
        let mut count = 0;
        if self.left {
            count += 1;
        }
        if self.middle {
            count += 1;
        }
        if self.right {
            count += 1;
        }
        if self.back {
            count += 1;
        }
        if self.forward {
            count += 1;
        }
        count
    }

    /// Reset all button states to released
    pub fn reset(&mut self) {
        tracing::debug!(
            target: "x11::mouse::state",
            "resetting button state"
        );
        *self = Self::new();
    }
}

impl Default for ButtonState {
    fn default() -> Self {
        Self::new()
    }
}

/// Emulate a mouse button event
pub fn emulate_button(
    display: &X11DisplayHandle,
    button_mapper: &ButtonMapper,
    button: u32,
    state: u8,
) -> X11Result<()> {
    // MEDIUM PRIORITY: Validate display before use (prevents use-after-free)
    if !display.is_valid() {
        return Err(X11EmulationError::InvalidDisplay);
    }
    
    let x11_button = button_mapper.to_x11_button(button);
    let pressed = state == 1;

    tracing::trace!(
        target: "x11::mouse::emulate",
        internal_button = button,
        x11_button,
        state = if pressed { "pressed" } else { "released" },
        "emulating button event"
    );

    timed("x11::mouse::emulate", "XTestFakeButtonEvent", || unsafe {
        let result = x11::xtest::XTestFakeButtonEvent(
            display.get(),
            x11_button,
            state as i32,
            0, // delay
        );

        if result == 0 {
            tracing::error!(
                target: "x11::mouse::emulate",
                x11_button,
                "XTestFakeButtonEvent failed"
            );
            return Err(X11EmulationError::EmulationFailed {
                operation: "XTestFakeButtonEvent".to_string(),
                detail: format!("button {}", x11_button),
            });
        }

        Ok(())
    })?;

    tracing::debug!(
        target: "x11::mouse::emulate",
        internal_button = button,
        x11_button,
        state = if pressed { "pressed" } else { "released" },
        "button event emulated successfully"
    );

    Ok(())
}

/// Get the name of a button for logging purposes
pub fn button_name(button: u32) -> &'static str {
    match button {
        buttons::LEFT => "left",
        buttons::MIDDLE => "middle",
        buttons::RIGHT => "right",
        buttons::BACK => "back",
        buttons::FORWARD => "forward",
        _ => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_button_mapping() {
        let mapper = ButtonMapper::new();

        assert_eq!(mapper.to_x11_button(buttons::LEFT), 1);
        assert_eq!(mapper.to_x11_button(buttons::MIDDLE), 2);
        assert_eq!(mapper.to_x11_button(buttons::RIGHT), 3);
        assert_eq!(mapper.to_x11_button(buttons::BACK), 8);
        assert_eq!(mapper.to_x11_button(buttons::FORWARD), 9);
    }

    #[test]
    fn test_custom_button_mapping() {
        let mut mapper = ButtonMapper::new();

        // Add custom mapping
        mapper.add_custom_mapping(999, 10);
        assert_eq!(mapper.to_x11_button(999), 10);

        // Remove custom mapping
        mapper.remove_custom_mapping(999);
        assert_eq!(mapper.to_x11_button(999), 999); // Falls back to identity
    }

    #[test]
    fn test_button_state() {
        let mut state = ButtonState::new();

        // Test left button
        state.update(buttons::LEFT, true);
        assert!(state.left);
        assert_eq!(state.pressed_count(), 1);

        // Test right button
        state.update(buttons::RIGHT, true);
        assert!(state.right);
        assert_eq!(state.pressed_count(), 2);

        // Release left button
        state.update(buttons::LEFT, false);
        assert!(!state.left);
        assert_eq!(state.pressed_count(), 1);

        // Test is_pressed
        assert!(!state.is_pressed(buttons::LEFT));
        assert!(state.is_pressed(buttons::RIGHT));
    }

    #[test]
    fn test_button_state_reset() {
        let mut state = ButtonState::new();

        state.update(buttons::LEFT, true);
        state.update(buttons::RIGHT, true);
        state.update(buttons::MIDDLE, true);
        assert_eq!(state.pressed_count(), 3);

        state.reset();
        assert_eq!(state.pressed_count(), 0);
        assert!(!state.left);
        assert!(!state.right);
        assert!(!state.middle);
    }

    #[test]
    fn test_button_name() {
        assert_eq!(button_name(buttons::LEFT), "left");
        assert_eq!(button_name(buttons::RIGHT), "right");
        assert_eq!(button_name(buttons::MIDDLE), "middle");
        assert_eq!(button_name(buttons::BACK), "back");
        assert_eq!(button_name(buttons::FORWARD), "forward");
        assert_eq!(button_name(999), "unknown");
    }

    #[test]
    fn test_button_state_all_buttons() {
        let mut state = ButtonState::new();

        // Press all buttons
        state.update(buttons::LEFT, true);
        state.update(buttons::MIDDLE, true);
        state.update(buttons::RIGHT, true);
        state.update(buttons::BACK, true);
        state.update(buttons::FORWARD, true);

        assert_eq!(state.pressed_count(), 5);
        assert!(state.is_pressed(buttons::LEFT));
        assert!(state.is_pressed(buttons::MIDDLE));
        assert!(state.is_pressed(buttons::RIGHT));
        assert!(state.is_pressed(buttons::BACK));
        assert!(state.is_pressed(buttons::FORWARD));
    }

    #[test]
    fn test_button_mapper_default() {
        let mapper = ButtonMapper::default();

        // Should have same mappings as new()
        assert_eq!(mapper.to_x11_button(buttons::LEFT), 1);
        assert_eq!(mapper.to_x11_button(buttons::RIGHT), 3);
    }

    #[test]
    fn test_button_state_default() {
        let state = ButtonState::default();

        // Should have all buttons released
        assert_eq!(state.pressed_count(), 0);
        assert!(!state.left);
        assert!(!state.right);
        assert!(!state.middle);
        assert!(!state.back);
        assert!(!state.forward);
    }
}
