// Performance benchmarks for X11 input emulation
//
// These tests require a running X11 display and are marked with #[ignore].
// Run them with: cargo test --release -- --ignored benchmark

#[cfg(all(unix, feature = "x11", not(target_os = "macos")))]
mod x11_benchmarks {
    use input_emulation::Backend;
    use input_emulation::InputEmulation;
    use input_event::{BTN_LEFT, BTN_MIDDLE, BTN_RIGHT, Event, KeyboardEvent, PointerEvent};
    use std::time::Instant;

    #[test]
    #[ignore] // Requires X11 display
    fn benchmark_key_emulation() {
        let mut emulation =
            tokio_test::block_on(async { InputEmulation::new(Some(Backend::X11)).await.unwrap() });

        let handle = 1;

        // Create handle
        tokio_test::block_on(async {
            emulation.create(handle).await;
        });

        let key_press = Event::Keyboard(KeyboardEvent::Key {
            time: 0,
            key: 30,
            state: 1,
        });

        let key_release = Event::Keyboard(KeyboardEvent::Key {
            time: 0,
            key: 30,
            state: 0,
        });

        let start = Instant::now();
        let iterations = 1000;

        for _ in 0..iterations {
            tokio_test::block_on(async {
                emulation.consume(key_press.clone(), handle).await.unwrap();
                emulation
                    .consume(key_release.clone(), handle)
                    .await
                    .unwrap();
            });
        }

        let duration = start.elapsed();
        let avg_latency = duration.as_micros() as f64 / (iterations as f64 * 2.0);

        println!("Key press+release benchmark:");
        println!("  Total time: {:?}", duration);
        println!("  Iterations: {}", iterations);
        println!("  Average latency: {:.2} μs", avg_latency);
        println!(
            "  Throughput: {:.2} events/sec",
            (iterations * 2) as f64 / duration.as_secs_f64()
        );

        assert!(
            avg_latency < 1000.0,
            "Key emulation too slow: {:.2} μs",
            avg_latency
        );

        // Destroy handle
        tokio_test::block_on(async {
            emulation.destroy(handle).await;
        });
    }

    #[test]
    #[ignore] // Requires X11 display
    fn benchmark_button_emulation() {
        let mut emulation =
            tokio_test::block_on(async { InputEmulation::new(Some(Backend::X11)).await.unwrap() });

        let handle = 1;

        // Create handle
        tokio_test::block_on(async {
            emulation.create(handle).await;
        });

        let button_press = Event::Pointer(PointerEvent::Button {
            time: 0,
            button: BTN_LEFT,
            state: 1,
        });

        let button_release = Event::Pointer(PointerEvent::Button {
            time: 0,
            button: BTN_LEFT,
            state: 0,
        });

        let start = Instant::now();
        let iterations = 1000;

        for _ in 0..iterations {
            tokio_test::block_on(async {
                emulation
                    .consume(button_press.clone(), handle)
                    .await
                    .unwrap();
                emulation
                    .consume(button_release.clone(), handle)
                    .await
                    .unwrap();
            });
        }

        let duration = start.elapsed();
        let avg_latency = duration.as_micros() as f64 / (iterations as f64 * 2.0);

        println!("Button press+release benchmark:");
        println!("  Total time: {:?}", duration);
        println!("  Iterations: {}", iterations);
        println!("  Average latency: {:.2} μs", avg_latency);
        println!(
            "  Throughput: {:.2} events/sec",
            (iterations * 2) as f64 / duration.as_secs_f64()
        );

        assert!(
            avg_latency < 1000.0,
            "Button emulation too slow: {:.2} μs",
            avg_latency
        );

        // Destroy handle
        tokio_test::block_on(async {
            emulation.destroy(handle).await;
        });
    }

    #[test]
    #[ignore] // Requires X11 display
    fn benchmark_scroll_emulation() {
        let mut emulation =
            tokio_test::block_on(async { InputEmulation::new(Some(Backend::X11)).await.unwrap() });

        let handle = 1;

        // Create handle
        tokio_test::block_on(async {
            emulation.create(handle).await;
        });

        let scroll_event = Event::Pointer(PointerEvent::Axis {
            time: 0,
            axis: 0,
            value: 1.0,
        });

        let start = Instant::now();
        let iterations = 1000;

        for _ in 0..iterations {
            tokio_test::block_on(async {
                emulation
                    .consume(scroll_event.clone(), handle)
                    .await
                    .unwrap();
            });
        }

        let duration = start.elapsed();
        let avg_latency = duration.as_micros() as f64 / iterations as f64;

        println!("Scroll emulation benchmark:");
        println!("  Total time: {:?}", duration);
        println!("  Iterations: {}", iterations);
        println!("  Average latency: {:.2} μs", avg_latency);
        println!(
            "  Throughput: {:.2} events/sec",
            iterations as f64 / duration.as_secs_f64()
        );

        assert!(
            avg_latency < 1000.0,
            "Scroll emulation too slow: {:.2} μs",
            avg_latency
        );

        // Destroy handle
        tokio_test::block_on(async {
            emulation.destroy(handle).await;
        });
    }

    #[test]
    #[ignore] // Requires X11 display
    fn benchmark_motion_emulation() {
        let mut emulation =
            tokio_test::block_on(async { InputEmulation::new(Some(Backend::X11)).await.unwrap() });

        let handle = 1;

        // Create handle
        tokio_test::block_on(async {
            emulation.create(handle).await;
        });

        let motion_event = Event::Pointer(PointerEvent::Motion {
            time: 0,
            dx: 1.0,
            dy: 1.0,
        });

        let start = Instant::now();
        let iterations = 1000;

        for _ in 0..iterations {
            tokio_test::block_on(async {
                emulation
                    .consume(motion_event.clone(), handle)
                    .await
                    .unwrap();
            });
        }

        let duration = start.elapsed();
        let avg_latency = duration.as_micros() as f64 / iterations as f64;

        println!("Motion emulation benchmark:");
        println!("  Total time: {:?}", duration);
        println!("  Iterations: {}", iterations);
        println!("  Average latency: {:.2} μs", avg_latency);
        println!(
            "  Throughput: {:.2} events/sec",
            iterations as f64 / duration.as_secs_f64()
        );

        assert!(
            avg_latency < 1000.0,
            "Motion emulation too slow: {:.2} μs",
            avg_latency
        );

        // Destroy handle
        tokio_test::block_on(async {
            emulation.destroy(handle).await;
        });
    }

    #[test]
    #[ignore] // Requires X11 display
    fn benchmark_discrete_scroll_emulation() {
        let mut emulation =
            tokio_test::block_on(async { InputEmulation::new(Some(Backend::X11)).await.unwrap() });

        let handle = 1;

        // Create handle
        tokio_test::block_on(async {
            emulation.create(handle).await;
        });

        let discrete_scroll = Event::Pointer(PointerEvent::AxisDiscrete120 {
            axis: 0,
            value: 120,
        });

        let start = Instant::now();
        let iterations = 1000;

        for _ in 0..iterations {
            tokio_test::block_on(async {
                emulation
                    .consume(discrete_scroll.clone(), handle)
                    .await
                    .unwrap();
            });
        }

        let duration = start.elapsed();
        let avg_latency = duration.as_micros() as f64 / iterations as f64;

        println!("Discrete scroll emulation benchmark:");
        println!("  Total time: {:?}", duration);
        println!("  Iterations: {}", iterations);
        println!("  Average latency: {:.2} μs", avg_latency);
        println!(
            "  Throughput: {:.2} events/sec",
            iterations as f64 / duration.as_secs_f64()
        );

        assert!(
            avg_latency < 1000.0,
            "Discrete scroll emulation too slow: {:.2} μs",
            avg_latency
        );

        // Destroy handle
        tokio_test::block_on(async {
            emulation.destroy(handle).await;
        });
    }

    #[test]
    #[ignore] // Requires X11 display
    fn benchmark_mixed_events() {
        let mut emulation =
            tokio_test::block_on(async { InputEmulation::new(Some(Backend::X11)).await.unwrap() });

        let handle = 1;

        // Create handle
        tokio_test::block_on(async {
            emulation.create(handle).await;
        });

        let events = vec![
            Event::Pointer(PointerEvent::Motion {
                time: 0,
                dx: 1.0,
                dy: 1.0,
            }),
            Event::Pointer(PointerEvent::Button {
                time: 0,
                button: BTN_LEFT,
                state: 1,
            }),
            Event::Pointer(PointerEvent::Motion {
                time: 0,
                dx: 2.0,
                dy: 2.0,
            }),
            Event::Pointer(PointerEvent::Axis {
                time: 0,
                axis: 0,
                value: 1.0,
            }),
            Event::Pointer(PointerEvent::Motion {
                time: 0,
                dx: 3.0,
                dy: 3.0,
            }),
            Event::Pointer(PointerEvent::Button {
                time: 0,
                button: BTN_LEFT,
                state: 0,
            }),
            Event::Keyboard(KeyboardEvent::Key {
                time: 0,
                key: 30,
                state: 1,
            }),
            Event::Keyboard(KeyboardEvent::Key {
                time: 0,
                key: 30,
                state: 0,
            }),
            Event::Pointer(PointerEvent::Button {
                time: 0,
                button: BTN_RIGHT,
                state: 1,
            }),
            Event::Pointer(PointerEvent::Button {
                time: 0,
                button: BTN_RIGHT,
                state: 0,
            }),
        ];

        let start = Instant::now();
        let iterations = 1000;

        for _ in 0..iterations {
            for event in &events {
                tokio_test::block_on(async {
                    emulation.consume(event.clone(), handle).await.unwrap();
                });
            }
        }

        let duration = start.elapsed();
        let total_events = iterations * events.len();
        let avg_latency = duration.as_micros() as f64 / total_events as f64;

        println!("Mixed events benchmark:");
        println!("  Total time: {:?}", duration);
        println!("  Iterations: {}", iterations);
        println!("  Events per iteration: {}", events.len());
        println!("  Total events: {}", total_events);
        println!("  Average latency: {:.2} μs", avg_latency);
        println!(
            "  Throughput: {:.2} events/sec",
            total_events as f64 / duration.as_secs_f64()
        );

        assert!(
            avg_latency < 1000.0,
            "Mixed events too slow: {:.2} μs",
            avg_latency
        );

        // Destroy handle
        tokio_test::block_on(async {
            emulation.destroy(handle).await;
        });
    }

    #[test]
    #[ignore] // Requires X11 display
    fn benchmark_create_destroy() {
        let mut emulation =
            tokio_test::block_on(async { InputEmulation::new(Some(Backend::X11)).await.unwrap() });

        let start = Instant::now();
        let iterations = 1000;

        for i in 0..iterations {
            let handle = i as u64;
            tokio_test::block_on(async {
                emulation.create(handle).await;
            });
        }

        let create_duration = start.elapsed();

        for i in 0..iterations {
            let handle = i as u64;
            tokio_test::block_on(async {
                emulation.destroy(handle).await;
            });
        }

        let total_duration = start.elapsed();
        let destroy_duration = total_duration - create_duration;

        let avg_create_time = create_duration.as_micros() as f64 / iterations as f64;
        let avg_destroy_time = destroy_duration.as_micros() as f64 / iterations as f64;

        println!("Create/Destroy benchmark:");
        println!("  Total time: {:?}", total_duration);
        println!("  Create time: {:?}", create_duration);
        println!("  Destroy time: {:?}", destroy_duration);
        println!("  Iterations: {}", iterations);
        println!("  Average create time: {:.2} μs", avg_create_time);
        println!("  Average destroy time: {:.2} μs", avg_destroy_time);

        assert!(
            avg_create_time < 100.0,
            "Create too slow: {:.2} μs",
            avg_create_time
        );
        assert!(
            avg_destroy_time < 100.0,
            "Destroy too slow: {:.2} μs",
            avg_destroy_time
        );
    }

    #[test]
    #[ignore] // Requires X11 display
    fn benchmark_rapid_key_sequence() {
        let mut emulation =
            tokio_test::block_on(async { InputEmulation::new(Some(Backend::X11)).await.unwrap() });

        let handle = 1;

        // Create handle
        tokio_test::block_on(async {
            emulation.create(handle).await;
        });

        // Simulate typing a word (e.g., "hello")
        let keys = [35, 23, 38, 38, 24]; // h, e, l, l, o

        let start = Instant::now();
        let iterations = 1000;

        for _ in 0..iterations {
            for &key in &keys {
                let key_press = Event::Keyboard(KeyboardEvent::Key {
                    time: 0,
                    key,
                    state: 1,
                });
                let key_release = Event::Keyboard(KeyboardEvent::Key {
                    time: 0,
                    key,
                    state: 0,
                });
                tokio_test::block_on(async {
                    emulation.consume(key_press, handle).await.unwrap();
                    emulation.consume(key_release, handle).await.unwrap();
                });
            }
        }

        let duration = start.elapsed();
        let total_events = iterations * keys.len() * 2;
        let avg_latency = duration.as_micros() as f64 / total_events as f64;

        println!(
            "Rapid key sequence benchmark (typing 'hello' {} times):",
            iterations
        );
        println!("  Total time: {:?}", duration);
        println!("  Total events: {}", total_events);
        println!("  Average latency: {:.2} μs", avg_latency);
        println!(
            "  Throughput: {:.2} events/sec",
            total_events as f64 / duration.as_secs_f64()
        );

        assert!(
            avg_latency < 1000.0,
            "Rapid key sequence too slow: {:.2} μs",
            avg_latency
        );

        // Destroy handle
        tokio_test::block_on(async {
            emulation.destroy(handle).await;
        });
    }

    #[test]
    #[ignore] // Requires X11 display
    fn benchmark_all_buttons() {
        let mut emulation =
            tokio_test::block_on(async { InputEmulation::new(Some(Backend::X11)).await.unwrap() });

        let handle = 1;

        // Create handle
        tokio_test::block_on(async {
            emulation.create(handle).await;
        });

        let buttons = [BTN_LEFT, BTN_MIDDLE, BTN_RIGHT];

        let start = Instant::now();
        let iterations = 1000;

        for _ in 0..iterations {
            for &button in &buttons {
                let press = Event::Pointer(PointerEvent::Button {
                    time: 0,
                    button,
                    state: 1,
                });
                let release = Event::Pointer(PointerEvent::Button {
                    time: 0,
                    button,
                    state: 0,
                });
                tokio_test::block_on(async {
                    emulation.consume(press, handle).await.unwrap();
                    emulation.consume(release, handle).await.unwrap();
                });
            }
        }

        let duration = start.elapsed();
        let total_events = iterations * buttons.len() * 2;
        let avg_latency = duration.as_micros() as f64 / total_events as f64;

        println!("All buttons benchmark:");
        println!("  Total time: {:?}", duration);
        println!("  Total events: {}", total_events);
        println!("  Average latency: {:.2} μs", avg_latency);
        println!(
            "  Throughput: {:.2} events/sec",
            total_events as f64 / duration.as_secs_f64()
        );

        assert!(
            avg_latency < 1000.0,
            "All buttons too slow: {:.2} μs",
            avg_latency
        );

        // Destroy handle
        tokio_test::block_on(async {
            emulation.destroy(handle).await;
        });
    }
}

#[cfg(not(all(unix, feature = "x11", not(target_os = "macos"))))]
mod x11_benchmarks {
    #[test]
    #[ignore]
    fn benchmark_key_emulation() {
        println!("X11 benchmarks skipped - not available on this platform");
    }
}
