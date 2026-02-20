# Step 6: Mouse Button Emulation - Implementation Summary

**Date:** 2026-02-20  
**Status:** Completed  
**Step Reference:** [`step_06.md`](../plans/refactoring_steps/step_06.md)

---

## Overview

This step implements mouse button emulation for the X11 input emulation layer. The implementation provides button mapping, button state tracking, and button event emulation functionality with support for all standard mouse buttons (left, middle, right, back, forward).

---

## Implementation Details

### 1. Files Created/Modified

#### Created Files:
- [`input-emulation/src/x11/mouse.rs`](src/x11/mouse.rs) - New mouse button emulation module (371 lines)

#### Modified Files:
- [`input-emulation/src/x11/mod.rs`](src/x11/mod.rs) - Added mouse module exports

---

## Components Implemented

### 1.1 Button Constants Module

**Purpose:** Defines internal button identifiers for mouse buttons.

**Constants:**
- [`LEFT`](src/x11/mouse.rs:12) - Left mouse button (0x110)
- [`RIGHT`](src/x11/mouse.rs:14) - Right mouse button (0x111)
- [`MIDDLE`](src/x11/mouse.rs:16) - Middle mouse button (0x112)
- [`BACK`](src/x11/mouse.rs:18) - Back mouse button (0x113)
- [`FORWARD`](src/x11/mouse.rs:20) - Forward mouse button (0x114)

**Rationale:** Using internal constants provides abstraction from X11 button numbers and allows for flexible mapping.

---

### 1.2 ButtonMapper

**Purpose:** Maps internal button identifiers to X11 button numbers with support for custom mappings.

**Key Features:**
- Default mappings for standard mouse buttons
- Custom mapping support for non-standard buttons
- Identity fallback for unknown buttons
- Comprehensive tracing for debugging

**Public Methods:**
- [`new()`](src/x11/mouse.rs:47) - Create mapper with default mappings
- [`to_x11_button()`](src/x11/mouse.rs:67) - Convert internal button to X11 button number
- [`add_custom_mapping()`](src/x11/mouse.rs:81) - Add custom button mapping
- [`remove_custom_mapping()`](src/x11/mouse.rs:92) - Remove custom mapping

**Default Mappings:**
- LEFT (0x110) → X11 button 1
- MIDDLE (0x112) → X11 button 2
- RIGHT (0x111) → X11 button 3
- BACK (0x113) → X11 button 8
- FORWARD (0x114) → X11 button 9

**Rationale:** X11 button numbers are standardized but internal constants provide flexibility and better abstraction.

---

### 1.3 ButtonState

**Purpose:** Tracks the current state of all mouse buttons.

**Key Features:**
- State tracking for all standard buttons
- Pressed count calculation
- Individual button state queries
- State reset capability

**Public Methods:**
- [`new()`](src/x11/mouse.rs:120) - Create new button state with all buttons released
- [`update()`](src/x11/mouse.rs:131) - Update state of a specific button
- [`is_pressed()`](src/x11/mouse.rs:151) - Check if a specific button is pressed
- [`pressed_count()`](src/x11/mouse.rs:163) - Get count of currently pressed buttons
- [`reset()`](src/x11/mouse.rs:174) - Reset all button states to released

**Fields:**
- [`left`](src/x11/mouse.rs:111) - Left button state
- [`middle`](src/x11/mouse.rs:112) - Middle button state
- [`right`](src/x11/mouse.rs:113) - Right button state
- [`back`](src/x11/mouse.rs:114) - Back button state
- [`forward`](src/x11/mouse.rs:115) - Forward button state

**Rationale:** Explicit state tracking enables proper button state management and multi-button drag operations.

---

### 1.4 emulate_button()

**Purpose:** Emulates a mouse button event on the X11 display using XTEST extension.

**Signature:**
```rust
pub fn emulate_button(
    display: &X11DisplayHandle,
    button_mapper: &ButtonMapper,
    button: u32,
    state: u8,
) -> X11Result<()>
```

**Parameters:**
- `display` - X11 display handle
- `button_mapper` - Button mapper for conversion
- `button` - Internal button identifier to emulate
- `state` - Button state (1 = pressed, 0 = released)

**Implementation:**
1. Converts internal button to X11 button number using mapper
2. Determines if button is pressed or released
3. Calls `XTestFakeButtonEvent` with appropriate parameters
4. Returns error if XTEST operation fails
5. Uses [`timed()`](src/x11/logging.rs) wrapper for performance logging

**Error Handling:**
- Returns `X11EmulationError::EmulationFailed` if XTEST operation fails

---

### 1.5 button_name()

**Purpose:** Returns human-readable name of a button for logging purposes.

**Signature:**
```rust
pub fn button_name(button: u32) -> &'static str
```

**Returns:**
- "left" for LEFT button
- "right" for RIGHT button
- "middle" for MIDDLE button
- "back" for BACK button
- "forward" for FORWARD button
- "unknown" for any other button

---

## Testing

### Unit Tests (8 tests total)

All tests pass successfully.

#### ButtonMapper Tests:
1. [`test_button_mapping`](src/x11/mouse.rs:262) - Default button mapping verification
2. [`test_custom_button_mapping`](src/x11/mouse.rs:273) - Custom mapping add/remove functionality
3. [`test_button_mapper_default`](src/x11/mouse.rs:343) - Default trait implementation

#### ButtonState Tests:
4. [`test_button_state`](src/x11/mouse.rs:285) - Button state update and tracking
5. [`test_button_state_reset`](src/x11/mouse.rs:301) - Button state reset functionality
6. [`test_button_state_all_buttons`](src/x11/mouse.rs:314) - All buttons state tracking
7. [`test_button_state_default`](src/x11/mouse.rs:353) - Default trait implementation

#### Utility Tests:
8. [`test_button_name`](src/x11/mouse.rs:297) - Button name function for all buttons

---

## Code Quality

### Compilation
- ✅ All code compiles without errors
- ✅ All tests pass (85/85 including previous steps)

### Formatting
- ✅ Code formatted with `cargo fmt`
- ✅ Follows Rust style guidelines

### Clippy
- ✅ No clippy warnings specific to mouse.rs module
- Note: Some warnings in cursor.rs, transform.rs, and keyboard.rs are expected as they're from previous steps and will be used in future implementations

---

## Logging

The implementation uses structured tracing with three levels:

### TRACE Level
- Button mapping operations
- Button state updates

### DEBUG Level
- Button mapper creation
- Custom mapping additions/removals
- Button state reset
- Button event emulation details

### ERROR Level
- XTEST emulation failures

**Example Log Output:**
```
[DEBUG x11::mouse::mapping] button mapper created: mappings_count=5
[TRACE x11::mouse::mapping] button mapped: internal=272, x11=1
[TRACE x11::mouse::state] button state updated: button=272, pressed=true, state=ButtonState { left: true, middle: false, right: false, back: false, forward: false }
[DEBUG x11::mouse::emulate] button event emulated successfully: internal_button=272, x11_button=1, state=pressed
```

---

## Dependencies

### External Dependencies:
- `x11` crate - X11 library bindings (xlib, xtest)
- `std::collections::HashMap` - For button mappings

### Internal Dependencies:
- [`X11DisplayHandle`](src/x11/display.rs) - Display handle type
- [`X11EmulationError`](src/x11/error.rs) - Error type
- [`X11Result`](src/x11/error.rs) - Result type alias
- [`timed()`](src/x11/logging.rs) - Performance logging wrapper

---

## Integration Points

### With Previous Steps:
- Uses [`X11DisplayHandle`](src/x11/display.rs) from Step 1
- Uses error types from Step 1
- Uses logging utilities from Step 1

### With Future Steps:
- Will integrate with scroll emulation (Step 7)
- Will be used by main emulation loop
- Will work with cursor position tracking (Step 3)

---

## Technical Decisions

### 1. Internal Button Constants
Using internal constants (0x110-0x114) instead of X11 button numbers:
```rust
pub const LEFT: u32 = 0x110;
```

**Rationale:** Provides abstraction layer and allows for flexible mapping between internal representation and X11 button numbers.

### 2. HashMap for Button Mappings
Using HashMap for button mappings:
```rust
pub mappings: HashMap<u32, u32>,
```

**Rationale:** Enables efficient O(1) lookup and easy addition of custom mappings without code changes.

### 3. Identity Fallback for Unknown Buttons
Falling back to identity mapping for unknown buttons:
```rust
let x11_button = *self.mappings.get(&button).unwrap_or(&button);
```

**Rationale:** Provides reasonable default behavior for non-standard buttons while allowing custom mapping.

### 4. Explicit Button State Fields
Using explicit boolean fields for each button:
```rust
pub struct ButtonState {
    pub left: bool,
    pub middle: bool,
    pub right: bool,
    pub back: bool,
    pub forward: bool,
}
```

**Rationale:** Provides clear, type-safe state tracking and enables efficient multi-button operations.

---

## Known Limitations

1. **Fixed Button Set:** Currently supports only 5 standard buttons. Additional buttons would require extending the ButtonState structure.

2. **No Double-Click Detection:** Double-click detection is not implemented. This could be added at a higher level if needed.

3. **No Button Hold Detection:** No detection of button hold duration. This could be added for advanced features.

4. **X11 Button Numbers:** Default mappings use standard X11 button numbers (1-3, 8-9). Some systems may have different button mappings.

---

## Performance Characteristics

### Time Complexity:
- Button mapping: O(1) with HashMap
- Button state update: O(1)
- Button state query: O(1)
- Pressed count: O(1) - fixed number of buttons

### Memory Usage:
- ButtonMapper: O(n) where n is number of custom mappings
- ButtonState: O(1) - fixed size structure

---

## Future Enhancements

1. **Dynamic Button Support:** Allow dynamic addition of buttons to ButtonState for non-standard mice.

2. **Double-Click Detection:** Add double-click detection with configurable timeout.

3. **Button Hold Detection:** Add detection of button hold duration for long-press actions.

4. **Button Chording:** Support for button chording (multiple buttons pressed simultaneously) for advanced features.

5. **Configurable Mappings:** Allow runtime configuration of button mappings via configuration file.

---

## Compliance with Requirements

### Code Acceptance Criteria:
- ✅ ButtonMapper implemented with to_x11_button, add_custom_mapping methods
- ✅ ButtonState implemented with update, is_pressed, pressed_count methods
- ✅ emulate_button implemented with XTestFakeButtonEvent
- ✅ button_name implemented for all standard buttons
- ✅ All methods have appropriate documentation
- ✅ Code passes cargo clippy (no warnings in mouse.rs)
- ✅ Code passes cargo fmt

### Test Acceptance Criteria:
- ✅ Unit tests for ButtonMapper (3 tests)
- ✅ Unit tests for ButtonState (4 tests)
- ✅ Unit tests for button_name (1 test)
- ✅ All tests pass (8/8 for mouse module, 85/85 total)

### Documentation Acceptance Criteria:
- ✅ All public types have documentation
- ✅ Examples in documentation are executable

---

## Conclusion

Step 6 has been successfully implemented, providing a robust mouse button emulation system for the X11 input emulation layer. The implementation includes comprehensive testing, proper error handling, and structured logging. All acceptance criteria have been met.

The mouse button module is now ready for integration with the main emulation loop and will be used in conjunction with scroll emulation in future steps.

---

**Next Steps:**
- Step 7: Scroll Emulation with direction detection
- Step 8: Integration of all components into main X11 backend
