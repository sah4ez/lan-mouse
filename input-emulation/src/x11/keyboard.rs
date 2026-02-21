use std::collections::HashSet;
use std::time::{Duration, Instant};

use x11::xlib;

use crate::x11::{X11DisplayHandle, X11EmulationError, X11Result, logging::timed};

/// Смещение X11 keycode относительно Linux scancode
const X11_KEYCODE_OFFSET: u32 = 8;

/// Маппер сканкодов
#[derive(Debug, Clone)]
pub struct ScancodeMapper {
    /// Linux to X11 keycode offset
    pub keycode_offset: u32,
    /// Кастомные маппинги сканкодов
    pub custom_mappings: std::collections::HashMap<u32, u32>,
    /// Расширенные сканкоды (требуют E0 prefix)
    pub extended_scancodes: HashSet<u32>,
}

impl ScancodeMapper {
    /// Создать новый маппер с настройками по умолчанию
    pub fn new() -> Self {
        Self {
            keycode_offset: X11_KEYCODE_OFFSET,
            custom_mappings: std::collections::HashMap::new(),
            extended_scancodes: Self::default_extended_scancodes(),
        }
    }

    /// Преобразовать Linux scancode в X11 keycode
    pub fn linux_to_x11(&self, linux_scancode: u32) -> u32 {
        // Проверяем кастомный маппинг сначала
        if let Some(&keycode) = self.custom_mappings.get(&linux_scancode) {
            tracing::trace!(
                target: "x11::keyboard::mapping",
                linux = linux_scancode,
                x11 = keycode,
                "custom scancode mapping"
            );
            return keycode;
        }

        // Применяем стандартное смещение
        let x11_keycode = linux_scancode + self.keycode_offset;

        tracing::trace!(
            target: "x11::keyboard::mapping",
            linux = linux_scancode,
            x11 = x11_keycode,
            offset = self.keycode_offset,
            "standard scancode mapping"
        );

        x11_keycode
    }

    /// Проверить, является ли скancode расширенным (требует E0 prefix)
    pub fn is_extended(&self, linux_scancode: u32) -> bool {
        self.extended_scancodes.contains(&linux_scancode)
    }

    /// Добавить кастомный маппинг
    pub fn add_custom_mapping(&mut self, linux_scancode: u32, x11_keycode: u32) {
        tracing::debug!(
            target: "x11::keyboard::mapping",
            linux = linux_scancode,
            x11 = x11_keycode,
            "adding custom mapping"
        );
        self.custom_mappings.insert(linux_scancode, x11_keycode);
    }

    /// Удалить кастомный маппинг
    pub fn remove_custom_mapping(&mut self, linux_scancode: u32) {
        tracing::debug!(
            target: "x11::keyboard::mapping",
            linux = linux_scancode,
            "removing custom mapping"
        );
        self.custom_mappings.remove(&linux_scancode);
    }

    /// Получить расширенные сканкоды по умолчанию
    fn default_extended_scancodes() -> HashSet<u32> {
        let mut set = HashSet::new();
        // Правый Ctrl, Правый Alt, и т.д.
        set.insert(97); // KEY_RIGHTCTRL
        set.insert(100); // KEY_RIGHTALT
        set.insert(126); // KEY_RIGHTMETA
        set
    }
}

impl Default for ScancodeMapper {
    fn default() -> Self {
        Self::new()
    }
}

/// Состояние модификаторов
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModifierState {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub super_key: bool,
    pub caps_lock: bool,
    pub num_lock: bool,
}

impl ModifierState {
    /// Создать новое состояние модификаторов
    pub fn new() -> Self {
        Self {
            shift: false,
            ctrl: false,
            alt: false,
            super_key: false,
            caps_lock: false,
            num_lock: false,
        }
    }

    /// Обновить состояние модификаторов на основе события клавиши
    pub fn update(&mut self, linux_scancode: u32, pressed: bool) {
        match linux_scancode {
            x if x == input_event::scancode::Linux::KeyLeftShift as u32
                || x == input_event::scancode::Linux::KeyRightShift as u32 =>
            {
                self.shift = pressed;
            }
            x if x == input_event::scancode::Linux::KeyLeftCtrl as u32
                || x == input_event::scancode::Linux::KeyRightCtrl as u32 =>
            {
                self.ctrl = pressed;
            }
            x if x == input_event::scancode::Linux::KeyLeftAlt as u32
                || x == input_event::scancode::Linux::KeyRightalt as u32 =>
            {
                self.alt = pressed;
            }
            x if x == input_event::scancode::Linux::KeyLeftMeta as u32
                || x == input_event::scancode::Linux::KeyRightmeta as u32 =>
            {
                self.super_key = pressed;
            }
            x if x == input_event::scancode::Linux::KeyCapsLock as u32 => {
                if pressed {
                    self.caps_lock = !self.caps_lock;
                }
            }
            x if x == input_event::scancode::Linux::KeyNumlock as u32 => {
                if pressed {
                    self.num_lock = !self.num_lock;
                }
            }
            _ => {}
        }

        tracing::trace!(
            target: "x11::keyboard::modifiers",
            scancode = linux_scancode,
            pressed,
            state = ?self,
            "modifier state updated"
        );
    }

    /// Получить маску модификаторов для X11
    pub fn as_x11_mask(&self) -> u32 {
        let mut mask = 0u32;

        if self.shift {
            mask |= xlib::ShiftMask;
        }
        if self.ctrl {
            mask |= xlib::ControlMask;
        }
        if self.alt {
            mask |= xlib::Mod1Mask; // Alt
        }
        if self.super_key {
            mask |= xlib::Mod4Mask; // Super
        }
        if self.caps_lock {
            mask |= xlib::LockMask;
        }
        if self.num_lock {
            mask |= xlib::Mod2Mask; // NumLock
        }

        mask
    }
}

impl Default for ModifierState {
    fn default() -> Self {
        Self::new()
    }
}

/// Результат обработки события клавиши
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyEventResult {
    /// Нормальное событие
    Normal,
    /// Авто-повтор (должен быть подавлен)
    AutoRepeat,
}

/// Менеджер состояния клавиатуры
#[derive(Debug)]
pub struct KeyboardState {
    /// Текущее состояние модификаторов
    pub modifiers: ModifierState,
    /// Нажатые клавиши
    pub pressed_keys: HashSet<u32>,
    /// Время последнего нажатия (для детекции авто-повтора)
    pub last_press_time: Option<Instant>,
    /// Последняя нажатая клавиша (для детекции авто-повтора)
    pub last_pressed_key: Option<u32>,
    /// Порог времени для детекции авто-повтора
    pub auto_repeat_threshold: Duration,
    /// Current keyboard layout group (0 = default, 1 = first alternative, etc.)
    pub current_group: u32,
}

impl KeyboardState {
    /// Создать новое состояние клавиатуры
    pub fn new() -> Self {
        Self {
            modifiers: ModifierState::new(),
            pressed_keys: HashSet::new(),
            last_press_time: None,
            last_pressed_key: None,
            auto_repeat_threshold: Duration::from_millis(50),
            current_group: 0,
        }
    }

    /// Обработать событие клавиши и детектировать авто-повтор
    pub fn process_key_event(&mut self, linux_scancode: u32, pressed: bool) -> KeyEventResult {
        let now = Instant::now();

        if pressed {
            // Проверяем на авто-повтор
            if let (Some(last_key), Some(last_time)) = (self.last_pressed_key, self.last_press_time)
            {
                if last_key == linux_scancode
                    && now.duration_since(last_time) < self.auto_repeat_threshold
                {
                    tracing::trace!(
                        target: "x11::keyboard::autorepeat",
                        scancode = linux_scancode,
                        time_since_last_ms = now.duration_since(last_time).as_millis(),
                        threshold_ms = self.auto_repeat_threshold.as_millis(),
                        "auto-repeat detected, suppressing"
                    );
                    return KeyEventResult::AutoRepeat;
                }
            }

            self.pressed_keys.insert(linux_scancode);
            self.modifiers.update(linux_scancode, true);
            self.last_pressed_key = Some(linux_scancode);
            self.last_press_time = Some(now);

            tracing::debug!(
                target: "x11::keyboard",
                scancode = linux_scancode,
                state = "pressed",
                modifiers = ?self.modifiers,
                "key event processed"
            );

            KeyEventResult::Normal
        } else {
            self.pressed_keys.remove(&linux_scancode);
            self.modifiers.update(linux_scancode, false);

            if self.last_pressed_key == Some(linux_scancode) {
                self.last_pressed_key = None;
                self.last_press_time = None;
            }

            tracing::debug!(
                target: "x11::keyboard",
                scancode = linux_scancode,
                state = "released",
                modifiers = ?self.modifiers,
                "key event processed"
            );

            KeyEventResult::Normal
        }
    }

    /// Проверить, нажата ли клавиша
    pub fn is_key_pressed(&self, linux_scancode: u32) -> bool {
        self.pressed_keys.contains(&linux_scancode)
    }

    /// Получить количество нажатых клавиш
    pub fn pressed_count(&self) -> usize {
        self.pressed_keys.len()
    }

    /// Сбросить состояние
    pub fn reset(&mut self) {
        tracing::debug!(
            target: "x11::keyboard",
            "resetting keyboard state"
        );

        self.modifiers = ModifierState::new();
        self.pressed_keys.clear();
        self.last_press_time = None;
        self.last_pressed_key = None;
    }

    /// Set the full modifier state (for layout synchronization)
    pub fn set_modifier_state(&mut self, depressed: u32, latched: u32, locked: u32, group: u32) {
        tracing::debug!(
            target: "x11::keyboard::modifiers",
            depressed = depressed,
            latched = latched,
            locked = locked,
            group = group,
            "setting full modifier state"
        );
        
        // Update modifier state from the wire format
        // depressed: currently pressed modifiers
        // latched: modifiers that will be released after next key press (like sticky keys)
        // locked: toggle modifiers (Caps Lock, Num Lock)
        // group: keyboard layout group (0 = default, 1 = first alternative, etc.)
        
        // Parse standard X11 modifier masks
        self.modifiers.shift = (depressed & xlib::ShiftMask) != 0;
        self.modifiers.ctrl = (depressed & xlib::ControlMask) != 0;
        self.modifiers.alt = (depressed & xlib::Mod1Mask) != 0;
        self.modifiers.super_key = (depressed & xlib::Mod4Mask) != 0;
        self.modifiers.caps_lock = (locked & xlib::LockMask) != 0;
        self.modifiers.num_lock = (locked & xlib::Mod2Mask) != 0;
        
        // Store the current layout group
        self.current_group = group;
        
        tracing::debug!(
            target: "x11::keyboard::modifiers",
            shift = self.modifiers.shift,
            ctrl = self.modifiers.ctrl,
            alt = self.modifiers.alt,
            super_key = self.modifiers.super_key,
            caps_lock = self.modifiers.caps_lock,
            num_lock = self.modifiers.num_lock,
            group = group,
            "modifier state updated"
        );
    }
}

impl Default for KeyboardState {
    fn default() -> Self {
        Self::new()
    }
}

/// Эмулировать событие клавиши
pub fn emulate_key(
    display: &X11DisplayHandle,
    mapper: &ScancodeMapper,
    linux_scancode: u32,
    state: u8,
) -> X11Result<()> {
    // MEDIUM PRIORITY: Validate display before use (prevents use-after-free)
    if !display.is_valid() {
        return Err(X11EmulationError::InvalidDisplay);
    }
    
    let x11_keycode = mapper.linux_to_x11(linux_scancode);
    let pressed = state == 1;

    tracing::trace!(
        target: "x11::keyboard::emulate",
        linux_scancode,
        x11_keycode,
        state = if pressed { "pressed" } else { "released" },
        "emulating key event"
    );

    timed("x11::keyboard::emulate", "XTestFakeKeyEvent", || unsafe {
        let result = x11::xtest::XTestFakeKeyEvent(
            display.get(),
            x11_keycode,
            state as i32,
            0, // delay
        );

        if result == 0 {
            tracing::error!(
                target: "x11::keyboard::emulate",
                x11_keycode,
                "XTestFakeKeyEvent failed"
            );
            return Err(X11EmulationError::EmulationFailed {
                operation: "XTestFakeKeyEvent".to_string(),
                detail: format!("keycode {}", x11_keycode),
            });
        }

        Ok(())
    })?;

    tracing::debug!(
        target: "x11::keyboard::emulate",
        linux_scancode,
        x11_keycode,
        state = if pressed { "pressed" } else { "released" },
        "key event emulated successfully"
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use x11::xlib;

    #[test]
    fn test_scancode_mapping() {
        let mapper = ScancodeMapper::new();

        // Тест стандартного маппинга
        assert_eq!(mapper.linux_to_x11(30), 38); // KEY_A
        assert_eq!(mapper.linux_to_x11(1), 9); // KEY_ESC
    }

    #[test]
    fn test_custom_mapping() {
        let mut mapper = ScancodeMapper::new();
        mapper.add_custom_mapping(30, 100);

        assert_eq!(mapper.linux_to_x11(30), 100);

        mapper.remove_custom_mapping(30);
        assert_eq!(mapper.linux_to_x11(30), 38);
    }

    #[test]
    fn test_extended_scancode() {
        let mapper = ScancodeMapper::new();

        // KEY_RIGHTCTRL is extended
        assert!(mapper.is_extended(97));
        // KEY_A is not extended
        assert!(!mapper.is_extended(30));
    }

    #[test]
    fn test_modifier_state() {
        let mut state = ModifierState::new();

        state.update(42, true); // KEY_LEFTSHIFT
        assert!(state.shift);

        state.update(42, false);
        assert!(!state.shift);
    }

    #[test]
    fn test_ctrl_modifier() {
        let mut state = ModifierState::new();

        state.update(29, true); // KEY_LEFTCTRL
        assert!(state.ctrl);

        state.update(97, true); // KEY_RIGHTCTRL
        assert!(state.ctrl);

        state.update(29, false);
        state.update(97, false);
        assert!(!state.ctrl);
    }

    #[test]
    fn test_alt_modifier() {
        let mut state = ModifierState::new();

        state.update(56, true); // KEY_LEFTALT
        assert!(state.alt);

        state.update(56, false);
        assert!(!state.alt);
    }

    #[test]
    fn test_caps_lock_toggle() {
        let mut state = ModifierState::new();

        assert!(!state.caps_lock);

        state.update(58, true); // KEY_CAPSLOCK
        assert!(state.caps_lock);

        state.update(58, true); // Нажатие CapsLock снова
        assert!(!state.caps_lock);
    }

    #[test]
    fn test_num_lock_toggle() {
        let mut state = ModifierState::new();

        assert!(!state.num_lock);

        state.update(69, true); // KEY_NUMLOCK
        assert!(state.num_lock);

        state.update(69, true); // Нажатие NumLock снова
        assert!(!state.num_lock);
    }

    #[test]
    fn test_x11_mask() {
        let mut state = ModifierState::new();

        let mask = state.as_x11_mask();
        assert_eq!(mask, 0);

        state.shift = true;
        let mask = state.as_x11_mask();
        assert_ne!(mask & xlib::ShiftMask, 0);

        state.ctrl = true;
        let mask = state.as_x11_mask();
        assert_ne!(mask & xlib::ControlMask, 0);

        state.alt = true;
        let mask = state.as_x11_mask();
        assert_ne!(mask & xlib::Mod1Mask, 0);

        state.super_key = true;
        let mask = state.as_x11_mask();
        assert_ne!(mask & xlib::Mod4Mask, 0);

        state.caps_lock = true;
        let mask = state.as_x11_mask();
        assert_ne!(mask & xlib::LockMask, 0);

        state.num_lock = true;
        let mask = state.as_x11_mask();
        assert_ne!(mask & xlib::Mod2Mask, 0);
    }

    #[test]
    fn test_keyboard_state_press() {
        let mut state = KeyboardState::new();

        let result = state.process_key_event(30, true);
        assert_eq!(result, KeyEventResult::Normal);
        assert!(state.is_key_pressed(30));
        assert_eq!(state.pressed_count(), 1);
    }

    #[test]
    fn test_keyboard_state_release() {
        let mut state = KeyboardState::new();

        state.process_key_event(30, true);
        assert!(state.is_key_pressed(30));

        let result = state.process_key_event(30, false);
        assert_eq!(result, KeyEventResult::Normal);
        assert!(!state.is_key_pressed(30));
        assert_eq!(state.pressed_count(), 0);
    }

    #[test]
    fn test_auto_repeat_detection() {
        let mut state = KeyboardState::new();

        // Первое нажатие
        let result1 = state.process_key_event(30, true);
        assert_eq!(result1, KeyEventResult::Normal);

        // Авто-повтор (быстрое повторное нажатие)
        std::thread::sleep(Duration::from_millis(10));
        let result2 = state.process_key_event(30, true);
        assert_eq!(result2, KeyEventResult::AutoRepeat);

        // Отпускание
        let result3 = state.process_key_event(30, false);
        assert_eq!(result3, KeyEventResult::Normal);
    }

    #[test]
    fn test_no_auto_repeat_after_threshold() {
        let mut state = KeyboardState::new();

        // Первое нажатие
        state.process_key_event(30, true);

        // Ждем дольше порога
        std::thread::sleep(Duration::from_millis(60));

        // Это не авто-повтор, а новое нажатие
        let result = state.process_key_event(30, true);
        assert_eq!(result, KeyEventResult::Normal);
    }

    #[test]
    fn test_different_keys_no_auto_repeat() {
        let mut state = KeyboardState::new();

        // Нажимаем одну клавишу
        state.process_key_event(30, true);

        // Сразу нажимаем другую - не авто-повтор
        std::thread::sleep(Duration::from_millis(10));
        let result = state.process_key_event(31, true);
        assert_eq!(result, KeyEventResult::Normal);
    }

    #[test]
    fn test_multiple_keys() {
        let mut state = KeyboardState::new();

        state.process_key_event(30, true);
        state.process_key_event(31, true);
        state.process_key_event(32, true);

        assert_eq!(state.pressed_count(), 3);
        assert!(state.is_key_pressed(30));
        assert!(state.is_key_pressed(31));
        assert!(state.is_key_pressed(32));
    }

    #[test]
    fn test_reset() {
        let mut state = KeyboardState::new();

        state.process_key_event(30, true);
        state.process_key_event(31, true);
        assert_eq!(state.pressed_count(), 2);

        state.reset();
        assert_eq!(state.pressed_count(), 0);
        assert!(!state.modifiers.shift);
        assert!(!state.modifiers.ctrl);
    }

    #[test]
    fn test_modifier_tracking() {
        let mut state = KeyboardState::new();

        // Нажимаем Shift
        state.process_key_event(42, true);
        assert!(state.modifiers.shift);

        // Нажимаем клавишу с Shift
        state.process_key_event(30, true);
        assert!(state.modifiers.shift);

        // Отпускаем клавишу
        state.process_key_event(30, false);
        assert!(state.modifiers.shift);

        // Отпускаем Shift
        state.process_key_event(42, false);
        assert!(!state.modifiers.shift);
    }
}
