// Integration tests for X11Emulation backend
//
// These tests require an X11 server to be running.
// They are designed to work with Xvfb (X Virtual Framebuffer) for CI/CD.

use input_event::{Event, KeyboardEvent, PointerEvent};

use super::X11Emulation;
use crate::Emulation;

/// Helper function to check if X11 is available
fn is_x11_available() -> bool {
    std::env::var("DISPLAY").is_ok()
}

/// Helper function to skip tests if X11 is not available
macro_rules! skip_if_no_x11 {
    () => {
        if !is_x11_available() {
            eprintln!("Skipping X11 tests: DISPLAY not set or X11 not available");
            return;
        }
    };
}

#[test]
fn test_x11_emulation_creation() {
    skip_if_no_x11!();

    // Initialize tracing for better test output
    let _ = tracing_subscriber::fmt().with_test_writer().try_init();

    // Test creating a new X11Emulation instance
    let result = X11Emulation::new();

    assert!(
        result.is_ok(),
        "Failed to create X11Emulation: {:?}",
        result.err()
    );

    let emulation = result.unwrap();

    // The instance should be valid
    // We can't directly check the display handle, but we can verify it was created
    // by checking that it doesn't panic when dropped
    drop(emulation);
}

#[test]
fn test_x11_emulation_motion_event() {
    skip_if_no_x11!();

    let _ = tracing_subscriber::fmt().with_test_writer().try_init();

    let mut emulation = X11Emulation::new().expect("Failed to create X11Emulation");

    // Create a motion event
    let event = Event::Pointer(PointerEvent::Motion {
        time: 0,
        dx: 10.0,
        dy: 5.0,
    });

    // Consume the event
    let result = tokio_test::block_on(async { emulation.consume(event, 0).await });

    assert!(
        result.is_ok(),
        "Failed to consume motion event: {:?}",
        result.err()
    );
}

#[test]
fn test_x11_emulation_button_event() {
    skip_if_no_x11!();

    let _ = tracing_subscriber::fmt().with_test_writer().try_init();

    let mut emulation = X11Emulation::new().expect("Failed to create X11Emulation");

    // Test button press
    let press_event = Event::Pointer(PointerEvent::Button {
        time: 0,
        button: input_event::BTN_LEFT,
        state: 1,
    });

    let result = tokio_test::block_on(async { emulation.consume(press_event, 0).await });

    assert!(
        result.is_ok(),
        "Failed to consume button press event: {:?}",
        result.err()
    );

    // Test button release
    let release_event = Event::Pointer(PointerEvent::Button {
        time: 0,
        button: input_event::BTN_LEFT,
        state: 0,
    });

    let result = tokio_test::block_on(async { emulation.consume(release_event, 0).await });

    assert!(
        result.is_ok(),
        "Failed to consume button release event: {:?}",
        result.err()
    );
}

#[test]
fn test_x11_emulation_scroll_event() {
    skip_if_no_x11!();

    let _ = tracing_subscriber::fmt().with_test_writer().try_init();

    let mut emulation = X11Emulation::new().expect("Failed to create X11Emulation");

    // Test vertical scroll
    let scroll_event = Event::Pointer(PointerEvent::Axis {
        time: 0,
        axis: 0, // Vertical axis
        value: 1.0,
    });

    let result = tokio_test::block_on(async { emulation.consume(scroll_event, 0).await });

    assert!(
        result.is_ok(),
        "Failed to consume scroll event: {:?}",
        result.err()
    );
}

#[test]
fn test_x11_emulation_scroll_discrete_event() {
    skip_if_no_x11!();

    let _ = tracing_subscriber::fmt().with_test_writer().try_init();

    let mut emulation = X11Emulation::new().expect("Failed to create X11Emulation");

    // Test discrete scroll (e.g., from mouse wheel)
    let scroll_event = Event::Pointer(PointerEvent::AxisDiscrete120 {
        axis: 0, // Vertical axis
        value: 120,
    });

    let result = tokio_test::block_on(async { emulation.consume(scroll_event, 0).await });

    assert!(
        result.is_ok(),
        "Failed to consume discrete scroll event: {:?}",
        result.err()
    );
}

#[test]
fn test_x11_emulation_keyboard_event() {
    skip_if_no_x11!();

    let _ = tracing_subscriber::fmt().with_test_writer().try_init();

    let mut emulation = X11Emulation::new().expect("Failed to create X11Emulation");

    // Test key press (using a common key like 'A' which has scancode 30)
    let press_event = Event::Keyboard(KeyboardEvent::Key {
        time: 0,
        key: 30, // Linux scancode for 'A'
        state: 1,
    });

    let result = tokio_test::block_on(async { emulation.consume(press_event, 0).await });

    assert!(
        result.is_ok(),
        "Failed to consume key press event: {:?}",
        result.err()
    );

    // Test key release
    let release_event = Event::Keyboard(KeyboardEvent::Key {
        time: 0,
        key: 30,
        state: 0,
    });

    let result = tokio_test::block_on(async { emulation.consume(release_event, 0).await });

    assert!(
        result.is_ok(),
        "Failed to consume key release event: {:?}",
        result.err()
    );
}

#[test]
fn test_x11_emulation_auto_repeat_suppression() {
    skip_if_no_x11!();

    let _ = tracing_subscriber::fmt().with_test_writer().try_init();

    let mut emulation = X11Emulation::new().expect("Failed to create X11Emulation");

    // Send a key press
    let press_event = Event::Keyboard(KeyboardEvent::Key {
        time: 0,
        key: 30,
        state: 1,
    });

    let result = tokio_test::block_on(async { emulation.consume(press_event, 0).await });

    assert!(
        result.is_ok(),
        "Failed to consume key press event: {:?}",
        result.err()
    );

    // Send the same key press again (auto-repeat)
    // This should be suppressed by the keyboard state
    let repeat_event = Event::Keyboard(KeyboardEvent::Key {
        time: 0,
        key: 30,
        state: 1,
    });

    let result = tokio_test::block_on(async { emulation.consume(repeat_event, 0).await });

    assert!(
        result.is_ok(),
        "Failed to consume auto-repeat event: {:?}",
        result.err()
    );

    // Send key release
    let release_event = Event::Keyboard(KeyboardEvent::Key {
        time: 0,
        key: 30,
        state: 0,
    });

    let result = tokio_test::block_on(async { emulation.consume(release_event, 0).await });

    assert!(
        result.is_ok(),
        "Failed to consume key release event: {:?}",
        result.err()
    );
}

#[test]
fn test_x11_emulation_multiple_button_states() {
    skip_if_no_x11!();

    let _ = tracing_subscriber::fmt().with_test_writer().try_init();

    let mut emulation = X11Emulation::new().expect("Failed to create X11Emulation");

    // Press left button
    let left_press = Event::Pointer(PointerEvent::Button {
        time: 0,
        button: input_event::BTN_LEFT,
        state: 1,
    });

    tokio_test::block_on(async { emulation.consume(left_press, 0).await }).unwrap();

    // Press right button while left is still pressed
    let right_press = Event::Pointer(PointerEvent::Button {
        time: 0,
        button: input_event::BTN_RIGHT,
        state: 1,
    });

    tokio_test::block_on(async { emulation.consume(right_press, 0).await }).unwrap();

    // Release right button
    let right_release = Event::Pointer(PointerEvent::Button {
        time: 0,
        button: input_event::BTN_RIGHT,
        state: 0,
    });

    tokio_test::block_on(async { emulation.consume(right_release, 0).await }).unwrap();

    // Release left button
    let left_release = Event::Pointer(PointerEvent::Button {
        time: 0,
        button: input_event::BTN_LEFT,
        state: 0,
    });

    tokio_test::block_on(async { emulation.consume(left_release, 0).await }).unwrap();
}

#[test]
fn test_x11_emulation_terminate() {
    skip_if_no_x11!();

    let _ = tracing_subscriber::fmt().with_test_writer().try_init();

    let mut emulation = X11Emulation::new().expect("Failed to create X11Emulation");

    // Test terminate method
    tokio_test::block_on(async { emulation.terminate().await });

    // Should not panic
}

#[test]
fn test_x11_emulation_create_destroy() {
    skip_if_no_x11!();

    let _ = tracing_subscriber::fmt().with_test_writer().try_init();

    let mut emulation = X11Emulation::new().expect("Failed to create X11Emulation");

    // Test create and destroy methods
    tokio_test::block_on(async { emulation.create(0).await });

    tokio_test::block_on(async { emulation.destroy(0).await });

    // Should not panic
}

// ============================================================================
// Unit tests for internal methods
// ============================================================================

#[test]
fn test_relative_motion() {
    skip_if_no_x11!();

    let _ = tracing_subscriber::fmt().with_test_writer().try_init();

    let emulation = X11Emulation::new().expect("Failed to create X11Emulation");

    // Test relative motion
    let result = emulation.relative_motion(10, 5);

    assert!(
        result.is_ok(),
        "Failed to emulate relative motion: {:?}",
        result.err()
    );
}

#[test]
fn test_absolute_motion() {
    skip_if_no_x11!();

    let _ = tracing_subscriber::fmt().with_test_writer().try_init();

    let emulation = X11Emulation::new().expect("Failed to create X11Emulation");

    // Test absolute motion
    let result = emulation.absolute_motion(100, 200);

    assert!(
        result.is_ok(),
        "Failed to emulate absolute motion: {:?}",
        result.err()
    );
}

#[test]
fn test_absolute_motion_clamping() {
    skip_if_no_x11!();

    let _ = tracing_subscriber::fmt().with_test_writer().try_init();

    let emulation = X11Emulation::new().expect("Failed to create X11Emulation");

    // Test absolute motion with coordinates that should be clamped
    let result = emulation.absolute_motion(-100, -100);

    assert!(
        result.is_ok(),
        "Failed to emulate clamped absolute motion: {:?}",
        result.err()
    );

    let result = emulation.absolute_motion(10000, 10000);

    assert!(
        result.is_ok(),
        "Failed to emulate clamped absolute motion: {:?}",
        result.err()
    );
}

#[test]
fn test_send_trait() {
    // X11Emulation should implement Send
    fn assert_send<T: Send>() {}
    assert_send::<X11Emulation>();
}

#[test]
fn test_display_env_var() {
    // Test that DISPLAY environment variable is used correctly
    let display_env = std::env::var("DISPLAY");

    if display_env.is_ok() {
        let display = display_env.unwrap();
        assert!(!display.is_empty(), "DISPLAY should not be empty");
    }
}

// ============================================================================
// Tests for all button types
// ============================================================================

#[test]
fn test_all_mouse_buttons() {
    skip_if_no_x11!();

    let _ = tracing_subscriber::fmt().with_test_writer().try_init();

    let mut emulation = X11Emulation::new().expect("Failed to create X11Emulation");

    // Test all supported mouse buttons
    let buttons = [
        input_event::BTN_LEFT,
        input_event::BTN_RIGHT,
        input_event::BTN_MIDDLE,
        input_event::BTN_BACK,
        input_event::BTN_FORWARD,
    ];

    for button in buttons {
        // Press
        let press = Event::Pointer(PointerEvent::Button {
            time: 0,
            button,
            state: 1,
        });

        let result = tokio_test::block_on(async { emulation.consume(press, 0).await });
        assert!(
            result.is_ok(),
            "Failed to press button {:?}: {:?}",
            button,
            result.err()
        );

        // Release
        let release = Event::Pointer(PointerEvent::Button {
            time: 0,
            button,
            state: 0,
        });

        let result = tokio_test::block_on(async { emulation.consume(release, 0).await });
        assert!(
            result.is_ok(),
            "Failed to release button {:?}: {:?}",
            button,
            result.err()
        );
    }
}

// ============================================================================
// Tests for horizontal and vertical scroll
// ============================================================================

#[test]
fn test_horizontal_scroll() {
    skip_if_no_x11!();

    let _ = tracing_subscriber::fmt().with_test_writer().try_init();

    let mut emulation = X11Emulation::new().expect("Failed to create X11Emulation");

    // Test horizontal scroll (axis 1)
    let scroll_right = Event::Pointer(PointerEvent::Axis {
        time: 0,
        axis: 1,
        value: 1.0,
    });

    let result = tokio_test::block_on(async { emulation.consume(scroll_right, 0).await });
    assert!(result.is_ok(), "Failed to scroll right: {:?}", result.err());

    let scroll_left = Event::Pointer(PointerEvent::Axis {
        time: 0,
        axis: 1,
        value: -1.0,
    });

    let result = tokio_test::block_on(async { emulation.consume(scroll_left, 0).await });
    assert!(result.is_ok(), "Failed to scroll left: {:?}", result.err());
}

#[test]
fn test_vertical_scroll() {
    skip_if_no_x11!();

    let _ = tracing_subscriber::fmt().with_test_writer().try_init();

    let mut emulation = X11Emulation::new().expect("Failed to create X11Emulation");

    // Test vertical scroll (axis 0)
    let scroll_down = Event::Pointer(PointerEvent::Axis {
        time: 0,
        axis: 0,
        value: 1.0,
    });

    let result = tokio_test::block_on(async { emulation.consume(scroll_down, 0).await });
    assert!(result.is_ok(), "Failed to scroll down: {:?}", result.err());

    let scroll_up = Event::Pointer(PointerEvent::Axis {
        time: 0,
        axis: 0,
        value: -1.0,
    });

    let result = tokio_test::block_on(async { emulation.consume(scroll_up, 0).await });
    assert!(result.is_ok(), "Failed to scroll up: {:?}", result.err());
}

// ============================================================================
// Stress tests
// ============================================================================

#[test]
fn test_rapid_motion_events() {
    skip_if_no_x11!();

    let _ = tracing_subscriber::fmt().with_test_writer().try_init();

    let mut emulation = X11Emulation::new().expect("Failed to create X11Emulation");

    // Send many motion events rapidly
    for i in 0..100 {
        let event = Event::Pointer(PointerEvent::Motion {
            time: i,
            dx: 1.0,
            dy: 1.0,
        });

        let result = tokio_test::block_on(async { emulation.consume(event, 0).await });
        assert!(
            result.is_ok(),
            "Failed at iteration {}: {:?}",
            i,
            result.err()
        );
    }
}

#[test]
fn test_rapid_key_events() {
    skip_if_no_x11!();

    let _ = tracing_subscriber::fmt().with_test_writer().try_init();

    let mut emulation = X11Emulation::new().expect("Failed to create X11Emulation");

    // Press and release multiple keys rapidly
    for key in [30, 48, 46, 32, 18] {
        // A, B, C, D, E scancodes
        // Press
        let press = Event::Keyboard(KeyboardEvent::Key {
            time: 0,
            key,
            state: 1,
        });

        let result = tokio_test::block_on(async { emulation.consume(press, 0).await });
        assert!(
            result.is_ok(),
            "Failed to press key {}: {:?}",
            key,
            result.err()
        );

        // Release
        let release = Event::Keyboard(KeyboardEvent::Key {
            time: 0,
            key,
            state: 0,
        });

        let result = tokio_test::block_on(async { emulation.consume(release, 0).await });
        assert!(
            result.is_ok(),
            "Failed to release key {}: {:?}",
            key,
            result.err()
        );
    }
}
