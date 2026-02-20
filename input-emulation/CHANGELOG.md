# Changelog

All notable changes to the input-emulation crate will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

#### X11 Backend Refactoring
- Modular architecture for X11 input emulation
- Thread-safe `X11DisplayHandle` wrapper for X11 display connection
- `CursorManager` with edge detection for multi-monitor setups
- `ScancodeMapper` for Linux scancode to X11 keycode conversion
- `ModifierState` for tracking modifier key states
- `KeyboardState` for pressed key tracking and auto-repeat suppression
- `ButtonMapper` for mouse button mapping
- `ButtonState` for tracking button press states
- `ScrollConfig` with automatic scroll direction detection
- `ScrollEvent` handling for both continuous and discrete scroll events
- `ScreenConfig` for multi-monitor configuration via XRandR
- `CoordinateTransformer` for coordinate space conversions
- `NetworkEvent` for low-latency event serialization
- `EventBatch` for event batching to reduce network overhead
- `LatencyConfig` for adaptive low-latency mode
- Comprehensive tracing integration with `init_tracing()`
- `ErrorContext` for detailed error reporting
- Custom X11-specific error types

#### Testing
- Comprehensive unit tests for all X11 modules
- Integration tests requiring X11 display (marked with `#[ignore]`)
- Performance benchmarks for all event types
- Tests for auto-repeat suppression
- Tests for multi-handle support
- Tests for key state tracking
- Tests for rapid event sequences

#### Documentation
- Comprehensive README.md with usage examples
- Detailed inline documentation for all public APIs
- Architecture diagrams and component descriptions
- Performance benchmarks and target metrics
- Debugging guide with common issues and solutions

### Changed

- Refactored X11 backend to use new modular architecture
- Improved error handling with context-aware error types
- Enhanced multi-monitor support via XRandR
- Better thread safety with Arc-based display handle
- More efficient event batching for network transmission
- Improved auto-repeat detection and suppression

### Fixed

- Fixed thread-safety issues with X11 display access
- Fixed resource leaks on display connection closure
- Fixed auto-repeat suppression for keyboard events
- Fixed coordinate clamping for multi-monitor setups
- Fixed scroll direction detection for various devices

### Removed

- Removed old monolithic X11 backend code
- Removed deprecated X11-specific helper functions

## [0.3.0] - 2026-02-20

### Added

- Initial X11 input emulation implementation
- Support for XTest extension
- Basic keyboard event emulation
- Basic mouse button emulation
- Basic scroll wheel emulation
- Cursor motion emulation

## [0.2.0] - 2025-XX-XX

### Added

- Wayland wlroots backend
- libei backend
- XDG Desktop Portal backend
- macOS backend
- Windows backend
- Dummy backend for testing

## [0.1.0] - 2025-XX-XX

### Added

- Initial release
- Basic input emulation framework
- Backend selection and auto-detection
