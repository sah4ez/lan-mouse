// Integration tests for X11 input emulation
//
// These tests require a running X11 display and are marked with #[ignore].
// Run them with: cargo test -- --ignored

#[cfg(all(unix, feature = "x11", not(target_os = "macos")))]
mod x11_integration_tests {
    use input_emulation::Backend;
    use input_emulation::InputEmulation;
    use input_event::{
        BTN_BACK, BTN_FORWARD, BTN_LEFT, BTN_MIDDLE, BTN_RIGHT, Event, KeyboardEvent, PointerEvent,
    };

    #[test]
    #[ignore] // Requires X11 display
    fn test_x11_emulation_full_cycle() {
        // Create emulation with X11 backend
        let mut emulation =
            tokio_test::block_on(async { InputEmulation::new(Some(Backend::X11)).await.unwrap() });

        // Create a handle for the test
        let handle = 1;

        // Test cursor creation
        tokio_test::block_on(async {
            emulation.create(handle).await;
        });

        // Test motion event
        let motion_event = Event::Pointer(PointerEvent::Motion {
            time: 0,
            dx: 10.0,
            dy: 5.0,
        });

        tokio_test::block_on(async {
            emulation.consume(motion_event, handle).await.unwrap();
        });

        // Test button press
        let button_press = Event::Pointer(PointerEvent::Button {
            time: 0,
            button: BTN_LEFT,
            state: 1,
        });

        tokio_test::block_on(async {
            emulation.consume(button_press, handle).await.unwrap();
        });

        // Test button release
        let button_release = Event::Pointer(PointerEvent::Button {
            time: 0,
            button: BTN_LEFT,
            state: 0,
        });

        tokio_test::block_on(async {
            emulation.consume(button_release, handle).await.unwrap();
        });

        // Test middle button
        let middle_press = Event::Pointer(PointerEvent::Button {
            time: 0,
            button: BTN_MIDDLE,
            state: 1,
        });

        tokio_test::block_on(async {
            emulation.consume(middle_press, handle).await.unwrap();
        });

        let middle_release = Event::Pointer(PointerEvent::Button {
            time: 0,
            button: BTN_MIDDLE,
            state: 0,
        });

        tokio_test::block_on(async {
            emulation.consume(middle_release, handle).await.unwrap();
        });

        // Test right button
        let right_press = Event::Pointer(PointerEvent::Button {
            time: 0,
            button: BTN_RIGHT,
            state: 1,
        });

        tokio_test::block_on(async {
            emulation.consume(right_press, handle).await.unwrap();
        });

        let right_release = Event::Pointer(PointerEvent::Button {
            time: 0,
            button: BTN_RIGHT,
            state: 0,
        });

        tokio_test::block_on(async {
            emulation.consume(right_release, handle).await.unwrap();
        });

        // Test back button
        let back_press = Event::Pointer(PointerEvent::Button {
            time: 0,
            button: BTN_BACK,
            state: 1,
        });

        tokio_test::block_on(async {
            emulation.consume(back_press, handle).await.unwrap();
        });

        let back_release = Event::Pointer(PointerEvent::Button {
            time: 0,
            button: BTN_BACK,
            state: 0,
        });

        tokio_test::block_on(async {
            emulation.consume(back_release, handle).await.unwrap();
        });

        // Test forward button
        let forward_press = Event::Pointer(PointerEvent::Button {
            time: 0,
            button: BTN_FORWARD,
            state: 1,
        });

        tokio_test::block_on(async {
            emulation.consume(forward_press, handle).await.unwrap();
        });

        let forward_release = Event::Pointer(PointerEvent::Button {
            time: 0,
            button: BTN_FORWARD,
            state: 0,
        });

        tokio_test::block_on(async {
            emulation.consume(forward_release, handle).await.unwrap();
        });

        // Test key press (KEY_A = 30)
        let key_press = Event::Keyboard(KeyboardEvent::Key {
            time: 0,
            key: 30,
            state: 1,
        });

        tokio_test::block_on(async {
            emulation.consume(key_press, handle).await.unwrap();
        });

        // Test key release
        let key_release = Event::Keyboard(KeyboardEvent::Key {
            time: 0,
            key: 30,
            state: 0,
        });

        tokio_test::block_on(async {
            emulation.consume(key_release, handle).await.unwrap();
        });

        // Test scroll event (vertical scroll)
        let scroll_event = Event::Pointer(PointerEvent::Axis {
            time: 0,
            axis: 0, // Vertical axis
            value: 1.0,
        });

        tokio_test::block_on(async {
            emulation.consume(scroll_event, handle).await.unwrap();
        });

        // Test horizontal scroll
        let hscroll_event = Event::Pointer(PointerEvent::Axis {
            time: 0,
            axis: 1, // Horizontal axis
            value: 1.0,
        });

        tokio_test::block_on(async {
            emulation.consume(hscroll_event, handle).await.unwrap();
        });

        // Test discrete scroll (mouse wheel)
        let discrete_scroll = Event::Pointer(PointerEvent::AxisDiscrete120 {
            axis: 0,
            value: 120, // One scroll tick
        });

        tokio_test::block_on(async {
            emulation.consume(discrete_scroll, handle).await.unwrap();
        });

        // Destroy the handle
        tokio_test::block_on(async {
            emulation.destroy(handle).await;
        });
    }

    #[test]
    #[ignore] // Requires X11 display
    fn test_auto_repeat_suppression() {
        let mut emulation =
            tokio_test::block_on(async { InputEmulation::new(Some(Backend::X11)).await.unwrap() });

        let handle = 1;

        // Create handle
        tokio_test::block_on(async {
            emulation.create(handle).await;
        });

        // Press key
        let key_press = Event::Keyboard(KeyboardEvent::Key {
            time: 0,
            key: 30,
            state: 1,
        });

        tokio_test::block_on(async {
            emulation.consume(key_press.clone(), handle).await.unwrap();
        });

        // Send another press immediately (simulating auto-repeat)
        // This should be suppressed by the keyboard state tracking
        tokio_test::block_on(async {
            emulation.consume(key_press.clone(), handle).await.unwrap();
        });

        // Release key
        let key_release = Event::Keyboard(KeyboardEvent::Key {
            time: 0,
            key: 30,
            state: 0,
        });

        tokio_test::block_on(async {
            emulation.consume(key_release, handle).await.unwrap();
        });

        // Destroy handle
        tokio_test::block_on(async {
            emulation.destroy(handle).await;
        });
    }

    #[test]
    #[ignore] // Requires X11 display
    fn test_multiple_handles() {
        let mut emulation =
            tokio_test::block_on(async { InputEmulation::new(Some(Backend::X11)).await.unwrap() });

        let handle1 = 1;
        let handle2 = 2;

        // Create multiple handles
        tokio_test::block_on(async {
            emulation.create(handle1).await;
            emulation.create(handle2).await;
        });

        // Send events from different handles
        let motion_event = Event::Pointer(PointerEvent::Motion {
            time: 0,
            dx: 10.0,
            dy: 5.0,
        });

        tokio_test::block_on(async {
            emulation.consume(motion_event, handle1).await.unwrap();
            emulation.consume(motion_event, handle2).await.unwrap();
        });

        // Destroy handles
        tokio_test::block_on(async {
            emulation.destroy(handle1).await;
            emulation.destroy(handle2).await;
        });
    }

    #[test]
    #[ignore] // Requires X11 display
    fn test_key_state_tracking() {
        let mut emulation =
            tokio_test::block_on(async { InputEmulation::new(Some(Backend::X11)).await.unwrap() });

        let handle = 1;

        // Create handle
        tokio_test::block_on(async {
            emulation.create(handle).await;
        });

        // Press multiple keys
        let keys = [30, 31, 32]; // A, S, D

        for key in keys {
            let key_press = Event::Keyboard(KeyboardEvent::Key {
                time: 0,
                key,
                state: 1,
            });
            tokio_test::block_on(async {
                emulation.consume(key_press, handle).await.unwrap();
            });
        }

        // Check that keys are tracked as pressed
        assert!(emulation.has_pressed_keys(handle));

        // Release all keys
        for key in keys {
            let key_release = Event::Keyboard(KeyboardEvent::Key {
                time: 0,
                key,
                state: 0,
            });
            tokio_test::block_on(async {
                emulation.consume(key_release, handle).await.unwrap();
            });
        }

        // Check that no keys are pressed
        assert!(!emulation.has_pressed_keys(handle));

        // Destroy handle
        tokio_test::block_on(async {
            emulation.destroy(handle).await;
        });
    }

    #[test]
    #[ignore] // Requires X11 display
    fn test_release_keys_on_destroy() {
        let mut emulation =
            tokio_test::block_on(async { InputEmulation::new(Some(Backend::X11)).await.unwrap() });

        let handle = 1;

        // Create handle
        tokio_test::block_on(async {
            emulation.create(handle).await;
        });

        // Press a key
        let key_press = Event::Keyboard(KeyboardEvent::Key {
            time: 0,
            key: 30,
            state: 1,
        });

        tokio_test::block_on(async {
            emulation.consume(key_press, handle).await.unwrap();
        });

        // Key should be pressed
        assert!(emulation.has_pressed_keys(handle));

        // Destroy handle - this should release all keys
        tokio_test::block_on(async {
            emulation.destroy(handle).await;
        });

        // Handle should no longer exist
        assert!(!emulation.has_pressed_keys(handle));
    }

    #[test]
    #[ignore] // Requires X11 display
    fn test_modifiers_event() {
        let mut emulation =
            tokio_test::block_on(async { InputEmulation::new(Some(Backend::X11)).await.unwrap() });

        let handle = 1;

        // Create handle
        tokio_test::block_on(async {
            emulation.create(handle).await;
        });

        // Send modifiers event
        let modifiers_event = Event::Keyboard(KeyboardEvent::Modifiers {
            depressed: 0,
            latched: 0,
            locked: 0,
            group: 0,
        });

        tokio_test::block_on(async {
            emulation.consume(modifiers_event, handle).await.unwrap();
        });

        // Destroy handle
        tokio_test::block_on(async {
            emulation.destroy(handle).await;
        });
    }

    #[test]
    #[ignore] // Requires X11 display
    fn test_rapid_events() {
        let mut emulation =
            tokio_test::block_on(async { InputEmulation::new(Some(Backend::X11)).await.unwrap() });

        let handle = 1;

        // Create handle
        tokio_test::block_on(async {
            emulation.create(handle).await;
        });

        // Send many rapid events
        for _ in 0..100 {
            let motion_event = Event::Pointer(PointerEvent::Motion {
                time: 0,
                dx: 1.0,
                dy: 1.0,
            });

            tokio_test::block_on(async {
                emulation.consume(motion_event, handle).await.unwrap();
            });
        }

        // Destroy handle
        tokio_test::block_on(async {
            emulation.destroy(handle).await;
        });
    }

    #[test]
    #[ignore] // Requires X11 display
    fn test_terminate() {
        let mut emulation =
            tokio_test::block_on(async { InputEmulation::new(Some(Backend::X11)).await.unwrap() });

        let handle1 = 1;
        let handle2 = 2;

        // Create multiple handles
        tokio_test::block_on(async {
            emulation.create(handle1).await;
            emulation.create(handle2).await;
        });

        // Press some keys
        let key_press = Event::Keyboard(KeyboardEvent::Key {
            time: 0,
            key: 30,
            state: 1,
        });

        tokio_test::block_on(async {
            emulation.consume(key_press, handle1).await.unwrap();
        });

        // Terminate - should release all keys and destroy all handles
        tokio_test::block_on(async {
            emulation.terminate().await;
        });

        // No handles should exist
        assert!(!emulation.has_pressed_keys(handle1));
        assert!(!emulation.has_pressed_keys(handle2));
    }

    #[test]
    #[ignore] // Requires X11 display
    fn test_backend_detection() {
        // Test that X11 backend can be created
        let emulation =
            tokio_test::block_on(async { InputEmulation::new(Some(Backend::X11)).await });

        assert!(emulation.is_ok(), "Failed to create X11 backend");
    }
}

#[cfg(not(all(unix, feature = "x11", not(target_os = "macos"))))]
mod x11_integration_tests {
    #[test]
    #[ignore]
    fn test_x11_emulation_full_cycle() {
        // X11 not available on this platform
        println!("X11 integration tests skipped - not available on this platform");
    }
}
