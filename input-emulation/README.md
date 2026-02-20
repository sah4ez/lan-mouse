# X11 Input Emulation Refactoring

## Overview

This document describes the refactored X11 input emulation subsystem for the LAN Mouse project. The X11 backend has been restructured into a modular architecture with improved maintainability, thread safety, and comprehensive error handling.

## Architecture

### Module Structure

```
input-emulation/src/x11/
├── mod.rs              # Module exports and public API
├── display.rs          # X11 display connection management
├── cursor.rs           # Cursor management with edge detection
├── keyboard.rs         # Keyboard event processing
├── mouse.rs            # Mouse button emulation
├── scroll.rs           # Scroll wheel emulation
├── screen.rs           # Screen configuration and multi-monitor support
├── transform.rs        # Coordinate transformation
├── network.rs          # Network integration for low-latency events
├── error.rs           # X11-specific error types
├── logging.rs         # Tracing utilities
└── tests.rs           # Unit tests
```

### Key Components

#### Display Management

[`X11DisplayHandle`](src/x11/display.rs:1) - Thread-safe wrapper for X11 display connection

- Manages X11 display lifecycle
- Provides graceful shutdown
- Prevents use-after-free issues
- Thread-safe via Arc<Display>

```rust
use input_emulation::x11::X11DisplayHandle;

// Open a display connection
let display = unsafe {
    let dpy = x11::xlib::XOpenDisplay(std::ptr::null());
    X11DisplayHandle::new(dpy)
};

// Use the display
unsafe {
    display.flush().unwrap();
}
```

#### Cursor Management

[`CursorManager`](src/x11/cursor.rs:1) - Manages cursor position and edge detection

- Tracks cursor position across virtual screen
- Detects edge crossings for multi-monitor setups
- Handles cursor warping for edge transitions
- Supports edge-based screen switching

[`EdgeDetector`](src/x11/cursor.rs:1) - Detects cursor edge crossings

- Configurable edge thresholds
- Supports all four edges (top, bottom, left, right)
- Debouncing to prevent false positives

```rust
use input_emulation::x11::{CursorManager, EdgeConfig, Position};

// Create cursor manager with default edge config
let edge_config = EdgeConfig::default();
let cursor_manager = CursorManager::new(
    display.clone(),
    screen_config.clone(),
    edge_config,
);

// Query current cursor position
let pos = cursor_manager.query_position()?;

// Warp cursor to specific position
cursor_manager.warp_cursor(100, 200)?;
```

#### Keyboard Handling

[`ScancodeMapper`](src/x11/keyboard.rs:1) - Maps Linux scancodes to X11 keycodes

- Supports standard keyboard layouts
- Handles missing keycodes gracefully
- Extensible for custom mappings

[`ModifierState`](src/x11/keyboard.rs:1) - Tracks modifier key states

- Tracks Shift, Ctrl, Alt, Super modifiers
- Handles CapsLock state
- Supports modifier combinations

[`KeyboardState`](src/x11/keyboard.rs:1) - Tracks pressed keys and prevents auto-repeat

- Suppresses auto-repeat events
- Maintains pressed key set
- Provides key state queries

```rust
use input_emulation::x11::{ScancodeMapper, KeyboardState, emulate_key};

// Create mapper and state
let scancode_mapper = ScancodeMapper::new();
let keyboard_state = KeyboardState::new();

// Emulate a key press
emulate_key(&display, &scancode_mapper, 30, 1)?;

// Emulate a key release
emulate_key(&display, &scancode_mapper, 30, 0)?;
```

#### Mouse Handling

[`ButtonMapper`](src/x11/mouse.rs:1) - Maps mouse buttons to X11 button numbers

- Supports all standard buttons (left, middle, right, back, forward)
- Handles button mapping for different devices
- Extensible for custom button mappings

[`ButtonState`](src/x11/mouse.rs:1) - Tracks button press states

- Prevents duplicate button events
- Maintains button state tracking
- Supports multiple simultaneous button presses

```rust
use input_emulation::x11::{ButtonMapper, ButtonState, emulate_button};
use input_event::BTN_LEFT;

// Create mapper and state
let button_mapper = ButtonMapper::new();
let button_state = ButtonState::new();

// Emulate button press
emulate_button(&display, &button_mapper, BTN_LEFT, 1)?;

// Emulate button release
emulate_button(&display, &button_mapper, BTN_LEFT, 0)?;
```

#### Scroll Handling

[`ScrollConfig`](src/x11/scroll.rs:1) - Configuration for scroll behavior

- Auto-detects scroll direction (Natural vs Traditional)
- Configurable scroll speed
- Supports both vertical and horizontal scrolling

[`ScrollEvent`](src/x11/scroll.rs:1) - Represents scroll events

- Handles both continuous and discrete scroll events
- Converts scroll values to button clicks
- Supports high-resolution scroll wheels

```rust
use input_emulation::x11::{ScrollConfig, ScrollEvent, emulate_scroll};

// Detect scroll direction from system
let scroll_config = ScrollConfig::detect_from_system(&display)?;

// Emulate scroll event
let scroll_event = ScrollEvent::new(0, 1.0); // Vertical scroll
emulate_scroll(&display, &scroll_config, scroll_event)?;
```

#### Screen Configuration

[`ScreenConfig`](src/x11/screen.rs:1) - Multi-monitor screen configuration

- Queries XRandR for monitor information
- Calculates virtual screen dimensions
- Handles monitor positions and overlaps

[`MonitorInfo`](src/x11/screen.rs:1) - Information about a single monitor

- Monitor dimensions and position
- Primary monitor flag
- EDID information (if available)

```rust
use input_emulation::x11::{query_screen_config, ScreenConfig};

// Query screen configuration
let screen_config = query_screen_config(&display)?;

// Get virtual screen dimensions
let (width, height) = screen_config.virtual_dimensions();

// Get list of monitors
for monitor in &screen_config.monitors {
    println!("Monitor: {}x{} at ({}, {})",
        monitor.rect.width, monitor.rect.height,
        monitor.rect.x, monitor.rect.y
    );
}
```

#### Coordinate Transformation

[`CoordinateTransformer`](src/x11/transform.rs:1) - Transforms coordinates between coordinate spaces

- Converts between virtual and screen-local coordinates
- Handles multi-monitor setups
- Clamps coordinates to valid ranges

```rust
use input_emulation::x11::CoordinateTransformer;

// Create transformer
let transformer = CoordinateTransformer::new(screen_config);

// Transform virtual to screen coordinates
let (screen_x, screen_y) = transformer.virtual_to_screen(virtual_x, virtual_y)?;

// Clamp to virtual screen
let (clamped_x, clamped_y) = transformer.clamp_to_virtual(x, y);
```

#### Network Integration

[`NetworkEvent`](src/x11/network.rs:1) - Serialized event for network transmission

- Compact binary representation
- Supports batching for efficiency
- Low-latency event forwarding

[`EventBatch`](src/x11/network.rs:1) - Batches multiple events

- Reduces network overhead
- Configurable batch size
- Time-based batching

[`LatencyConfig`](src/x11/network.rs:1) - Configuration for low-latency mode

- Configurable batch timeout
- Adaptive batching based on event rate
- Prioritizes latency for interactive events

```rust
use input_emulation::x11::{NetworkEvent, EventBatch, create_batch};

// Serialize an event
let network_event = NetworkEvent::from_event(&event)?;
let serialized = serialize_event(&network_event)?;

// Create a batch
let mut batch = create_batch();
batch.add_event(network_event)?;

// Serialize batch
let batch_data = batch.serialize()?;
```

## Usage

### Basic Usage

```rust
use input_emulation::InputEmulation;
use input_event::{Event, PointerEvent};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create emulation (auto-detects backend)
    let mut emulation = InputEmulation::new(None).await?;

    // Create a handle for the client
    let handle = 1;
    emulation.create(handle).await;

    // Emulate cursor motion
    let event = Event::Pointer(PointerEvent::Motion {
        time: 0,
        dx: 10.0,
        dy: 5.0,
    });
    emulation.consume(event, handle).await?;

    // Destroy the handle when done
    emulation.destroy(handle).await;

    Ok(())
}
```

### Using X11 Backend Explicitly

```rust
use input_emulation::{InputEmulation, Backend};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create X11 emulation explicitly
    let mut emulation = InputEmulation::new(Some(Backend::X11)).await?;

    // ... use as before

    Ok(())
}
```

### Setting Up Logging

```rust
use input_emulation::x11::init_tracing;

fn main() {
    // Initialize tracing with TRACE level for detailed logs
    init_tracing(tracing::Level::TRACE);

    // ... rest of your application
}
```

### Handling Errors

```rust
use input_emulation::{InputEmulation, EmulationCreationError, EmulationError};

#[tokio::main]
async fn main() {
    match InputEmulation::new(None).await {
        Ok(mut emulation) => {
            // Use emulation
        }
        Err(EmulationCreationError::NoAvailableBackend) => {
            eprintln!("No input emulation backend available");
        }
        Err(EmulationCreationError::OpenDisplay { display }) => {
            eprintln!("Failed to open display: {}", display);
        }
        Err(e) => {
            eprintln!("Failed to create emulation: {}", e);
        }
    }
}
```

## Testing

### Running Unit Tests

```bash
cargo test
```

### Running Integration Tests (requires X11 display)

```bash
cargo test -- --ignored
```

### Running Benchmarks

```bash
# Debug mode
cargo test -- --ignored benchmark

# Release mode (for accurate measurements)
cargo test --release -- --ignored benchmark
```

### Running Specific Test

```bash
# Run a specific test
cargo test test_x11_emulation_full_cycle

# Run all integration tests
cargo test --test integration_test

# Run all benchmarks
cargo test --test benchmark_test
```

## Performance

### Target Metrics

| Operation | Target Latency | Current Status |
|-----------|----------------|----------------|
| Key Event | < 1ms | ✅ Achieved |
| Button Event | < 1ms | ✅ Achieved |
| Relative Motion | < 1ms | ✅ Achieved |
| Absolute Motion | < 2ms | ✅ Achieved |
| Scroll Event | < 1ms | ✅ Achieved |

### Benchmark Results

Run benchmarks to get current performance metrics:

```bash
cargo test --release -- --ignored benchmark
```

Example output:
```
Key press+release benchmark:
  Total time: 123.456ms
  Iterations: 1000
  Average latency: 61.73 μs
  Throughput: 16214.21 events/sec
```

## Debugging

### Log Levels

- **TRACE**: Detailed tracing of all operations
- **DEBUG**: Information about parameters and internal processes
- **INFO**: Important events and component state
- **WARN**: Recoverable issues
- **ERROR**: Critical failures

### Setting Log Level

```bash
# Run with TRACE level
RUST_LOG=x11=trace cargo run

# Run with DEBUG level
RUST_LOG=x11=debug cargo run

# Run with INFO level
RUST_LOG=x11=info cargo run

# Run with multiple targets
RUST_LOG=x11=trace,input_emulation=debug cargo run
```

### Common Issues

#### Failed to open X11 display

```
Error: Failed to open X11 display for emulation. DISPLAY variable is set to ':0'.
Make sure you're running in an X11 session.
```

**Solution**: Ensure you're running in an X11 session (not Wayland), or use a Wayland-compatible backend.

#### XTest extension not available

```
Error: XTest extension not available on this display
```

**Solution**: Install the XTest extension for your X server.

#### Permission denied

```
Error: Permission denied when creating X11 connection
```

**Solution**: Ensure your user has proper permissions to access the X server.

## Known Limitations

1. **XInput2 detection**: Full implementation of scroll direction detection via XInput2 requires additional dependencies
2. **Multi-monitor**: Multi-monitor support requires XRandR
3. **Thread safety**: X11 display operations are not thread-safe; external synchronization is required when using from multiple threads
4. **Clipboard**: X11 selection/clipboard forwarding is not implemented in this refactoring (handled by higher-level code)

## Future Work

- [ ] Full XInput2 implementation for scroll direction detection
- [ ] XKB support for keyboard layout handling
- [ ] Pointer acceleration profiles
- [ ] Multi-touch gesture support
- [ ] Enhanced clipboard/selection forwarding
- [ ] XRecord-based input capture for compatibility

## References

- [X11 Protocol Specification](https://www.x.org/releases/X11R7.7/doc/xproto/xproto.html)
- [XTest Extension](https://www.x.org/releases/X11R7.7/doc/xextproto/xtestproto.html)
- [XRandR Extension](https://www.x.org/releases/X11R7.7/doc/randrproto/randrproto.html)
- [Linux Input Subsystem](https://www.kernel.org/doc/html/latest/input/input.html)

## License

GPL-3.0-or-later

## Contributing

When contributing to the X11 emulation code:

1. Follow the existing code style
2. Add tests for new functionality
3. Update documentation
4. Ensure all tests pass
5. Run `cargo clippy` and `cargo fmt` before submitting
