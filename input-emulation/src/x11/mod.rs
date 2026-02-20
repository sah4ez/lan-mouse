pub mod cursor;
pub mod display;
pub mod error;
pub mod keyboard;
pub mod logging;
pub mod mouse;
pub mod network;
pub mod screen;
pub mod scroll;
pub mod transform;

#[cfg(test)]
mod tests;

// Re-export public types from all modules
pub use cursor::{CursorManager, CursorPosition, EdgeConfig, EdgeDetector, Position};
pub use display::X11DisplayHandle;
pub use error::{ErrorContext, X11EmulationError, X11Result};
pub use keyboard::{KeyEventResult, KeyboardState, ModifierState, ScancodeMapper, emulate_key};
pub use logging::{init_tracing, targets, timed};
pub use mouse::{ButtonMapper, ButtonState, button_name, buttons, emulate_button};
pub use network::{
    EventBatch, LatencyConfig, NetworkEvent, NetworkEventData, NetworkEventType, create_batch,
    deserialize_event, serialize_event,
};
pub use screen::{MonitorInfo, Rect, ScreenConfig, XRandRConfig, query_screen_config};
pub use scroll::{ScrollConfig, ScrollDirection, ScrollEvent, axis_name, emulate_scroll};
pub use transform::CoordinateTransformer;

use async_trait::async_trait;
use std::sync::Arc;

use input_event::{
    BTN_BACK, BTN_FORWARD, BTN_LEFT, BTN_MIDDLE, BTN_RIGHT, Event, KeyboardEvent, PointerEvent,
};

use crate::{
    Emulation, EmulationCreationError, EmulationError, EmulationHandle,
    error::X11EmulationCreationError,
};

// ============================================================================
// X11 Emulation Backend
// ============================================================================

/// X11 input emulation backend using XTest extension
///
/// Этот backend эмулирует события ввода на X сервере используя расширение XTest.
/// Он предоставляет методы для эмуляции движения курсора, событий кнопок,
/// событий прокрутки и событий клавиатуры.
///
/// # Thread Safety
///
/// Этот struct реализует Send, позволяя перемещать его между потоками.
/// Однако операции X11 display не thread-safe, поэтому вызывающие должны обеспечить
/// правильную синхронизацию при использовании этого struct из нескольких потоков.
pub struct X11Emulation {
    /// Display handle
    display: X11DisplayHandle,
    /// Менеджер курсора
    cursor_manager: CursorManager,
    /// Трансформер координат
    coord_transformer: CoordinateTransformer,
    /// Маппер сканкодов
    scancode_mapper: ScancodeMapper,
    /// Маппер кнопок мыши
    button_mapper: ButtonMapper,
    /// Конфигурация прокрутки
    scroll_config: ScrollConfig,
    /// Состояние клавиатуры
    keyboard_state: KeyboardState,
    /// Состояние кнопок мыши
    button_state: ButtonState,
}

impl X11Emulation {
    /// Создать новый экземпляр X11 эмуляции ввода
    ///
    /// Этот метод открывает соединение с X сервером и инициализирует
    /// все необходимые компоненты для эмуляции ввода.
    ///
    /// # Returns
    ///
    /// * `Ok(X11Emulation)` - Новый экземпляр X11 эмуляции
    /// * `Err(X11EmulationCreationError)` - Если не удалось открыть display
    pub fn new() -> Result<Self, X11EmulationCreationError> {
        tracing::info!(target: "x11::display", "Initializing X11 input emulation backend");

        // Проверяем переменную окружения DISPLAY
        let display_env = std::env::var("DISPLAY").unwrap_or_else(|_| ":0".to_string());
        tracing::info!(target: "x11::display", "Using DISPLAY: {}", display_env);

        // Открываем X11 display
        let display = unsafe {
            match x11::xlib::XOpenDisplay(std::ptr::null()) {
                d if std::ptr::eq(d, std::ptr::null_mut::<x11::xlib::Display>()) => {
                    let error_msg = format!(
                        "Failed to open X11 display for emulation. DISPLAY variable is set to: '{}'. \
                        Make sure you're running in an X11 session. \
                        If you're using Wayland, you may need to run with XWayland or use a Wayland-compatible backend.",
                        display_env
                    );
                    tracing::error!(target: "x11::display", "{}", error_msg);
                    return Err(X11EmulationCreationError::OpenDisplay {
                        display: display_env,
                    });
                }
                display => X11DisplayHandle::new(display),
            }
        };

        // Запрашиваем конфигурацию экрана
        let screen_config = screen::query_screen_config(&display).map_err(|e| {
            X11EmulationCreationError::Other {
                message: format!("Failed to query screen config: {}", e),
            }
        })?;

        // Создаем трансформер координат
        let coord_transformer = CoordinateTransformer::new(screen_config.clone());

        // Создаем менеджер курсора
        let edge_config = EdgeConfig::default();
        let cursor_manager =
            CursorManager::new(display.clone(), screen_config.clone(), edge_config);

        // Создаем маппер сканкодов
        let scancode_mapper = ScancodeMapper::new();

        // Создаем маппер кнопок
        let button_mapper = ButtonMapper::new();

        // Создаем конфигурацию прокрутки
        let scroll_config = ScrollConfig::detect_from_system(&display).unwrap_or_else(|_| {
            tracing::warn!(
                target: "x11::scroll::detect",
                "Failed to detect scroll direction, using default"
            );
            ScrollConfig::default()
        });

        // Создаем состояние клавиатуры
        let keyboard_state = KeyboardState::new();

        // Создаем состояние кнопок
        let button_state = ButtonState::new();

        tracing::info!(
            target: "x11::display",
            "X11 input emulation backend initialized successfully"
        );

        Ok(Self {
            display,
            cursor_manager,
            coord_transformer,
            scancode_mapper,
            button_mapper,
            scroll_config,
            keyboard_state,
            button_state,
        })
    }

    /// Эмулировать относительное движение курсора
    fn relative_motion(&self, dx: i32, dy: i32) -> Result<(), EmulationError> {
        tracing::trace!(
            target: "x11::cursor::emulate",
            dx = dx,
            dy = dy,
            "emulating relative motion"
        );

        // Запрашиваем позицию до движения
        let before = self
            .cursor_manager
            .query_position()
            .map_err(|e| EmulationError::Other(e.to_string()))?;

        // Эмулируем относительное движение
        unsafe {
            let result = x11::xtest::XTestFakeRelativeMotionEvent(self.display.get(), dx, dy, 0, 0);

            if result == 0 {
                tracing::error!(
                    target: "x11::cursor::emulate",
                    dx = dx,
                    dy = dy,
                    "XTestFakeRelativeMotionEvent failed"
                );
                return Err(EmulationError::Other(format!(
                    "Failed to emulate motion: ({}, {})",
                    dx, dy
                )));
            }
        }

        // Запрашиваем позицию после движения
        let after = self
            .cursor_manager
            .query_position()
            .map_err(|e| EmulationError::Other(e.to_string()))?;

        tracing::debug!(
            target: "x11::cursor::emulate",
            dx = dx,
            dy = dy,
            before_x = before.virtual_pos.0,
            before_y = before.virtual_pos.1,
            after_x = after.virtual_pos.0,
            after_y = after.virtual_pos.1,
            "relative motion emulated"
        );

        Ok(())
    }

    /// Эмулировать абсолютное движение курсора
    #[cfg(test)]
    fn absolute_motion(&self, x: i32, y: i32) -> Result<(), EmulationError> {
        tracing::trace!(
            target: "x11::cursor::emulate",
            x = x,
            y = y,
            "emulating absolute motion"
        );

        // Ограничиваем координаты
        let (clamped_x, clamped_y) = self.coord_transformer.clamp_to_virtual(x, y);

        // Warp курсор к позиции
        self.cursor_manager
            .warp_cursor(clamped_x, clamped_y)
            .map_err(|e| EmulationError::Other(e.to_string()))?;

        tracing::debug!(
            target: "x11::cursor::emulate",
            original_x = x,
            original_y = y,
            clamped_x = clamped_x,
            clamped_y = clamped_y,
            "absolute motion emulated"
        );

        Ok(())
    }
}

// SAFETY: X11Emulation использует X11DisplayHandle который thread-safe.
// Однако операции X11 display должны быть синхронизированы внешне.
unsafe impl Send for X11Emulation {}

impl Drop for X11Emulation {
    fn drop(&mut self) {
        tracing::info!(target: "x11::display", "Cleaning up X11 input emulation");

        unsafe {
            self.display.close();
        }

        tracing::info!(target: "x11::display", "X11 input emulation cleanup complete");
    }
}

#[async_trait]
impl Emulation for X11Emulation {
    async fn consume(&mut self, event: Event, _: EmulationHandle) -> Result<(), EmulationError> {
        match event {
            Event::Pointer(pointer_event) => match pointer_event {
                PointerEvent::Motion { time: _, dx, dy } => {
                    self.relative_motion(dx as i32, dy as i32)?;
                }
                PointerEvent::Button {
                    time: _,
                    button,
                    state,
                } => {
                    // Обновляем состояние кнопок
                    self.button_state.update(button, state != 0);

                    // Эмулируем событие кнопки
                    emulate_button(&self.display, &self.button_mapper, button, state as u8)
                        .map_err(|e| EmulationError::Other(e.to_string()))?;
                }
                PointerEvent::Axis {
                    time: _,
                    axis,
                    value,
                } => {
                    let scroll_event = ScrollEvent::new(axis, value);
                    emulate_scroll(&self.display, &self.scroll_config, scroll_event)
                        .map_err(|e| EmulationError::Other(e.to_string()))?;
                }
                PointerEvent::AxisDiscrete120 { axis, value } => {
                    let scroll_event = ScrollEvent::with_discrete(axis, value);
                    emulate_scroll(&self.display, &self.scroll_config, scroll_event)
                        .map_err(|e| EmulationError::Other(e.to_string()))?;
                }
            },
            Event::Keyboard(KeyboardEvent::Key {
                time: _,
                key,
                state,
            }) => {
                // Обрабатываем событие клавиши
                let result = self.keyboard_state.process_key_event(key, state != 0);

                // Пропускаем авто-повтор
                if result == KeyEventResult::AutoRepeat {
                    tracing::trace!(
                        target: "x11::keyboard",
                        scancode = key,
                        "auto-repeat suppressed"
                    );
                    return Ok(());
                }

                // Эмулируем событие клавиши
                emulate_key(&self.display, &self.scancode_mapper, key, state)
                    .map_err(|e| EmulationError::Other(e.to_string()))?;
            }
            _ => {}
        }

        // Flush X display для гарантированной отправки событий
        unsafe {
            self.display
                .flush()
                .map_err(|e| EmulationError::Other(e.to_string()))?;
        }

        Ok(())
    }

    async fn create(&mut self, _: EmulationHandle) {
        // Для наших целей не важно, какой клиент отправил событие
    }

    async fn destroy(&mut self, _: EmulationHandle) {
        // Для наших целей не важно, какой клиент отправил событие
    }

    async fn terminate(&mut self) {
        // Нечего делать
    }
}
