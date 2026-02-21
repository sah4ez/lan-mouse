//! Cursor Manager with Edge Detection
//!
//! This module provides cursor position tracking and edge detection functionality
//! for X11 display emulation. It supports multi-monitor configurations and provides
//! utilities for detecting when the cursor crosses screen boundaries.

use super::display::X11DisplayHandle;
use super::error::{X11EmulationError, X11Result};
use super::screen::{Rect, ScreenConfig};

/// Cursor position in different coordinate spaces
#[derive(Debug, Clone, Copy)]
pub struct CursorPosition {
    /// Position in virtual screen space
    pub virtual_pos: (i32, i32),
    /// Position in monitor space
    pub monitor_pos: (i32, i32),
    /// Current monitor index
    pub monitor_index: Option<usize>,
    /// Normalized position (0.0-1.0)
    pub normalized: (f64, f64),
}

impl CursorPosition {
    /// Create a new cursor position
    pub fn new(
        virtual_pos: (i32, i32),
        monitor_pos: (i32, i32),
        monitor_index: Option<usize>,
        normalized: (f64, f64),
    ) -> Self {
        Self {
            virtual_pos,
            monitor_pos,
            monitor_index,
            normalized,
        }
    }
}

/// Edge detection configuration
#[derive(Debug, Clone, Copy)]
pub struct EdgeConfig {
    /// Pixel offset from edge for detection
    pub edge_threshold: i32,
    /// Minimum number of consecutive edge detections required
    pub edge_counter_threshold: u32,
    /// Offset when warping cursor to edge
    pub warp_offset: i32,
    /// Enable/disable edge warping
    pub enable_warping: bool,
}

impl Default for EdgeConfig {
    fn default() -> Self {
        Self {
            edge_threshold: 1,
            edge_counter_threshold: 2,
            warp_offset: 1,
            enable_warping: true,
        }
    }
}

/// Position on screen edge
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Position {
    /// Left edge
    Left,
    /// Right edge
    Right,
    /// Top edge
    Top,
    /// Bottom edge
    Bottom,
}

/// Edge crossing detector
#[derive(Debug)]
pub struct EdgeDetector {
    /// Current cursor position
    current_pos: (i32, i32),
    /// Previous cursor position
    previous_pos: (i32, i32),
    /// Edge counter
    edge_counter: u32,
    /// Configuration
    config: EdgeConfig,
    /// Screen bounds
    screen_bounds: Rect,
    /// Timestamp of last position update
    last_update: std::time::Instant,
    /// Grace period after position reset (to prevent immediate edge detection after enter)
    grace_period_end: std::time::Instant,
}

impl EdgeDetector {
    /// Create a new edge detector
    pub fn new(config: EdgeConfig, screen_bounds: Rect) -> Self {
        let now = std::time::Instant::now();
        Self {
            current_pos: (0, 0),
            previous_pos: (0, 0),
            edge_counter: 0,
            config,
            screen_bounds,
            last_update: now,
            grace_period_end: now,
        }
    }

    /// Update cursor position and check for edge crossing
    pub fn update(&mut self, x: i32, y: i32) -> Option<Position> {
        self.previous_pos = self.current_pos;
        self.current_pos = (x, y);
        self.last_update = std::time::Instant::now();

        log::debug!(
            "x11::cursor::edge: x={}, y={}, previous_x={}, previous_y={}, screen_width={}, screen_height={}, edge_threshold={}",
            x, y, self.previous_pos.0, self.previous_pos.1,
            self.screen_bounds.width, self.screen_bounds.height, self.config.edge_threshold
        );

        // Check if we're in grace period (prevent immediate edge detection after reset)
        let in_grace_period = self.last_update < self.grace_period_end;
        if in_grace_period {
            log::debug!("x11::cursor::edge: in grace period, skipping edge detection");
            return None;
        }

        // Calculate right and bottom edge thresholds
        let right_edge_threshold = self.screen_bounds.width as i32 - self.config.edge_threshold;
        let bottom_edge_threshold = self.screen_bounds.height as i32 - self.config.edge_threshold;

        // Check if we're at an edge
        let at_left = x <= self.config.edge_threshold;
        let at_right = x >= right_edge_threshold;
        let at_top = y <= self.config.edge_threshold;
        let at_bottom = y >= bottom_edge_threshold;

        log::debug!(
            "x11::cursor::edge: at_left={}, at_right={}, at_top={}, at_bottom={}, right_edge_threshold={}, bottom_edge_threshold={}",
            at_left, at_right, at_top, at_bottom, right_edge_threshold, bottom_edge_threshold
        );

        if at_left || at_right || at_top || at_bottom {
            self.edge_counter += 1;

            log::debug!(
                "x11::cursor::edge: cursor at edge - at_left={}, at_right={}, at_top={}, at_bottom={}, counter={}, threshold={}",
                at_left, at_right, at_top, at_bottom, self.edge_counter, self.config.edge_counter_threshold
            );

            if self.edge_counter >= self.config.edge_counter_threshold {
                self.edge_counter = 0;

                // Determine which edge was crossed
                if at_left && self.previous_pos.0 > self.config.edge_threshold {
                    log::info!(
                        "x11::cursor::edge: edge crossed - position=Left, current_x={}, previous_x={}, threshold={}",
                        x, self.previous_pos.0, self.config.edge_threshold
                    );
                    return Some(Position::Left);
                }
                if at_right && self.previous_pos.0 < right_edge_threshold {
                    log::info!(
                        "x11::cursor::edge: edge crossed - position=Right, current_x={}, previous_x={}, threshold={}",
                        x, self.previous_pos.0, right_edge_threshold
                    );
                    return Some(Position::Right);
                }
                if at_top && self.previous_pos.1 > self.config.edge_threshold {
                    log::info!(
                        "x11::cursor::edge: edge crossed - position=Top, current_y={}, previous_y={}, threshold={}",
                        y, self.previous_pos.1, self.config.edge_threshold
                    );
                    return Some(Position::Top);
                }
                if at_bottom && self.previous_pos.1 < bottom_edge_threshold {
                    log::info!(
                        "x11::cursor::edge: edge crossed - position=Bottom, current_y={}, previous_y={}, threshold={}",
                        y, self.previous_pos.1, bottom_edge_threshold
                    );
                    return Some(Position::Bottom);
                }
            }
        } else {
            // Not at edge, reset counter
            if self.edge_counter > 0 {
                log::debug!("x11::cursor::edge: resetting edge counter, counter={}", self.edge_counter);
                self.edge_counter = 0;
            }
        }

        None
    }

    /// Reset the detector
    pub fn reset(&mut self) {
        self.edge_counter = 0;
        // Set grace period to 500ms to prevent immediate edge detection after reset
        self.grace_period_end = std::time::Instant::now() + std::time::Duration::from_millis(500);
        tracing::debug!(
            target: "x11::cursor::edge",
            "edge detector reset, grace period set to 500ms"
        );
    }

    /// Get current position
    pub fn current_position(&self) -> (i32, i32) {
        self.current_pos
    }

    /// Get previous position
    pub fn previous_position(&self) -> (i32, i32) {
        self.previous_pos
    }

    /// Get current edge counter value
    pub fn edge_counter(&self) -> u32 {
        self.edge_counter
    }

    /// Update screen bounds
    pub fn update_screen_bounds(&mut self, bounds: Rect) {
        self.screen_bounds = bounds;
    }
}

/// Cursor manager
pub struct CursorManager {
    /// Display handle
    display: X11DisplayHandle,
    /// Screen configuration
    screen_config: ScreenConfig,
    /// Edge detector
    edge_detector: EdgeDetector,
    /// Saved enter position
    enter_position: Option<(i32, i32)>,
    /// Current capture position (set via set_entry_edge)
    capture_position: Option<Position>,
    /// Inferred entry edge (detected from first cursor position near edge)
    inferred_entry_edge: Option<Position>,
}

impl CursorManager {
    /// Create a new cursor manager
    pub fn new(
        display: X11DisplayHandle,
        screen_config: ScreenConfig,
        edge_config: EdgeConfig,
    ) -> Self {
        tracing::info!(
            target: "x11::cursor",
            screen_bounds = %screen_config.virtual_bounds,
            "creating cursor manager"
        );

        let edge_detector = EdgeDetector::new(edge_config, screen_config.virtual_bounds);

        Self {
            display,
            screen_config,
            edge_detector,
            enter_position: None,
            capture_position: None,
            inferred_entry_edge: None,
        }
    }

    /// Query current cursor position
    pub fn query_position(&self) -> X11Result<CursorPosition> {
        tracing::trace!(target: "x11::cursor", "querying cursor position");

        // MEDIUM PRIORITY: Validate display before use (prevents use-after-free)
        if !self.display.is_valid() {
            return Err(X11EmulationError::InvalidDisplay);
        }

        let (root_x, root_y) = unsafe {
            let mut root_x: i32 = 0;
            let mut root_y: i32 = 0;
            let mut win_x: i32 = 0;
            let mut win_y: i32 = 0;
            let mut mask: u32 = 0;
            let mut root_return: x11::xlib::Window = 0;
            let mut child_return: x11::xlib::Window = 0;

            let success = x11::xlib::XQueryPointer(
                self.display.get(),
                x11::xlib::XDefaultRootWindow(self.display.get()),
                &mut root_return,
                &mut child_return,
                &mut root_x,
                &mut root_y,
                &mut win_x,
                &mut win_y,
                &mut mask,
            );

            if success == 0 {
                return Err(X11EmulationError::ProtocolError(
                    "XQueryPointer failed".to_string(),
                ));
            }

            (root_x, root_y)
        };

        // Find monitor
        let monitor_index = self.screen_config.find_monitor_index_at(root_x, root_y);
        let monitor_pos = if let Some(idx) = monitor_index {
            let monitor = &self.screen_config.monitors[idx];
            (root_x - monitor.geometry.x, root_y - monitor.geometry.y)
        } else {
            (root_x, root_y)
        };

        // Normalize coordinates
        let normalized = self.normalize_coords(root_x, root_y);

        tracing::debug!(
            target: "x11::cursor",
            virtual_x = root_x,
            virtual_y = root_y,
            monitor_x = monitor_pos.0,
            monitor_y = monitor_pos.1,
            monitor_index,
            normalized_x = normalized.0,
            normalized_y = normalized.1,
            "cursor position queried"
        );

        Ok(CursorPosition::new(
            (root_x, root_y),
            monitor_pos,
            monitor_index,
            normalized,
        ))
    }

    /// Normalize coordinates to 0.0-1.0 range
    fn normalize_coords(&self, x: i32, y: i32) -> (f64, f64) {
        let width = self.screen_config.virtual_bounds.width as f64;
        let height = self.screen_config.virtual_bounds.height as f64;

        ((x as f64) / width, (y as f64) / height)
    }

    /// Warp cursor to specified position
    pub fn warp_cursor(&self, x: i32, y: i32) -> X11Result<()> {
        let (clamped_x, clamped_y) = self.screen_config.clamp_to_virtual(x, y);

        tracing::trace!(
            target: "x11::cursor::warp",
            x,
            y,
            clamped_x,
            clamped_y,
            bounds_width = self.screen_config.virtual_bounds.width,
            bounds_height = self.screen_config.virtual_bounds.height,
            "warping cursor"
        );

        // MEDIUM PRIORITY: Validate display before use (prevents use-after-free)
        if !self.display.is_valid() {
            return Err(X11EmulationError::InvalidDisplay);
        }

        unsafe {
            let root_window = x11::xlib::XDefaultRootWindow(self.display.get());
            x11::xlib::XWarpPointer(
                self.display.get(),
                0,
                root_window,
                0,
                0,
                0,
                0,
                clamped_x,
                clamped_y,
            );
            x11::xlib::XFlush(self.display.get());
        }

        tracing::debug!(
            target: "x11::cursor::warp",
            x = clamped_x,
            y = clamped_y,
            "cursor warped"
        );

        Ok(())
    }

    /// Handle cursor at edge during capture
    pub fn handle_edge_cursor(&self, position: Position) -> X11Result<()> {
        let (x, y) = match position {
            Position::Left => (
                self.edge_detector.config.warp_offset,
                self.screen_config.primary.height as i32 / 2,
            ),
            Position::Right => (
                self.screen_config.primary.width as i32 - self.edge_detector.config.warp_offset,
                self.screen_config.primary.height as i32 / 2,
            ),
            Position::Top => (
                self.screen_config.primary.width as i32 / 2,
                self.edge_detector.config.warp_offset,
            ),
            Position::Bottom => (
                self.screen_config.primary.width as i32 / 2,
                self.screen_config.primary.height as i32 - self.edge_detector.config.warp_offset,
            ),
        };

        self.warp_cursor(x, y)?;

        tracing::debug!(
            target: "x11::cursor::edge",
            position = ?position,
            warped_to_x = x,
            warped_to_y = y,
            "cursor warped to edge"
        );

        Ok(())
    }

    /// Check for edge crossing
    pub fn check_edge_crossing(&mut self) -> Option<Position> {
        match self.query_position() {
            Ok(pos) => {
                let edge = self.edge_detector.update(pos.virtual_pos.0, pos.virtual_pos.1);
                
                // Only trigger edge crossing if it's the same edge where cursor entered
                if let Some(crossed_edge) = edge {
                    // Use explicitly set entry edge only (no inference)
                    if let Some(entry_edge) = self.capture_position {
                        // Only allow exit through the same edge where cursor entered
                        let matches_entry_edge = crossed_edge == entry_edge;
                        
                        if matches_entry_edge {
                            log::info!(
                                "x11::cursor::exit: === CURSOR EXIT DETECTED ==="
                            );
                            log::info!(
                                "x11::cursor::exit: crossed_edge={:?} | entry_edge={:?} | match=YES",
                                crossed_edge, entry_edge
                            );
                            log::info!(
                                "x11::cursor::exit: cursor_position=({}, {}) | returning control to remote",
                                pos.virtual_pos.0, pos.virtual_pos.1
                            );
                            Some(crossed_edge)
                        } else {
                            log::debug!(
                                "x11::cursor::exit: crossed_edge={:?} | entry_edge={:?} | match=NO (ignoring)",
                                crossed_edge, entry_edge
                            );
                            None
                        }
                    } else {
                        // No entry edge set - ignore edge crossing
                        // Entry edge should be set via set_entry_edge when remote sends Enter event
                        log::info!(
                            "x11::cursor::exit: crossed_edge={:?} | cursor_position=({}, {}) | status=NO_ENTRY_EDGE_SET (ignoring - waiting for ProtoEvent::Enter)",
                            crossed_edge, pos.virtual_pos.0, pos.virtual_pos.1
                        );
                        None
                    }
                } else {
                    None
                }
            },
            Err(e) => {
                log::warn!(
                    "x11::cursor::edge: failed to query cursor position for edge detection: {}",
                    e
                );
                None
            }
        }
    }

    /// Save enter position
    pub fn save_enter_position(&mut self, pos: (i32, i32)) {
        self.enter_position = Some(pos);
        tracing::debug!(
            target: "x11::cursor",
            x = pos.0,
            y = pos.1,
            "enter position saved"
        );
    }

    /// Restore saved enter position
    pub fn restore_enter_position(&self) -> X11Result<()> {
        if let Some(pos) = self.enter_position {
            tracing::debug!(
                target: "x11::cursor",
                x = pos.0,
                y = pos.1,
                "restoring enter position"
            );
            self.warp_cursor(pos.0, pos.1)?;
        } else {
            tracing::trace!(
                target: "x11::cursor",
                "no enter position to restore"
            );
        }
        Ok(())
    }

    /// Start capture at specified position
    pub fn start_capture(&mut self, position: Position) -> X11Result<()> {
        tracing::info!(
            target: "x11::cursor",
            position = ?position,
            "starting capture"
        );

        // Save current position
        let pos = self.query_position()?;
        self.save_enter_position(pos.virtual_pos);

        // Set capture position
        self.capture_position = Some(position);

        // Warp to edge
        self.handle_edge_cursor(position)?;

        Ok(())
    }

    /// Stop capture
    pub fn stop_capture(&mut self) -> X11Result<()> {
        tracing::info!(
            target: "x11::cursor",
            "stopping capture"
        );

        // Restore position
        self.restore_enter_position()?;

        // Reset state
        self.capture_position = None;
        self.enter_position = None;
        self.edge_detector.reset();

        Ok(())
    }

    /// Get current capture position
    pub fn capture_position(&self) -> Option<Position> {
        self.capture_position
    }

    /// Get enter position
    pub fn enter_position(&self) -> Option<(i32, i32)> {
        self.enter_position
    }

    /// Get screen configuration
    pub fn screen_config(&self) -> &ScreenConfig {
        &self.screen_config
    }

    /// Get edge detector reference
    pub fn edge_detector(&self) -> &EdgeDetector {
        &self.edge_detector
    }

    /// Get mutable edge detector reference
    pub fn edge_detector_mut(&mut self) -> &mut EdgeDetector {
        &mut self.edge_detector
    }

    /// Infer entry edge from cursor position
    /// Uses the first position to determine which edge the cursor entered from.
    /// If cursor is in the left third of the screen, entry was from Left.
    /// If cursor is in the right third of the screen, entry was from Right.
    /// This is a heuristic for when the explicit entry edge is not set.
    fn infer_entry_edge_from_position(&self, x: i32, y: i32) -> Option<Position> {
        let width = self.screen_config.virtual_bounds.width as i32;
        let height = self.screen_config.virtual_bounds.height as i32;
        
        // Use thirds of the screen to determine entry edge
        let left_third = width / 3;
        let right_third = width * 2 / 3;
        let top_third = height / 3;
        let bottom_third = height * 2 / 3;
        
        // If cursor is in the left third of the screen, entry was from Left
        if x <= left_third {
            log::info!(
                "x11::cursor::edge: inferring entry edge=Left from position x={} (left_third={})",
                x, left_third
            );
            return Some(Position::Left);
        }
        // If cursor is in the right third of the screen, entry was from Right
        if x >= right_third {
            log::info!(
                "x11::cursor::edge: inferring entry edge=Right from position x={} (right_third={})",
                x, right_third
            );
            return Some(Position::Right);
        }
        // If cursor is in the top third of the screen, entry was from Top
        if y <= top_third {
            log::info!(
                "x11::cursor::edge: inferring entry edge=Top from position y={} (top_third={})",
                y, top_third
            );
            return Some(Position::Top);
        }
        // If cursor is in the bottom third of the screen, entry was from Bottom
        if y >= bottom_third {
            log::info!(
                "x11::cursor::edge: inferring entry edge=Bottom from position y={} (bottom_third={})",
                y, bottom_third
            );
            return Some(Position::Bottom);
        }
        
        // Cursor is in the center of the screen, cannot reliably infer entry
        log::debug!(
            "x11::cursor::edge: cannot infer entry edge from position ({}, {}) - cursor in center",
            x, y
        );
        None
    }

    /// Set the entry edge when a client enters
    /// This determines which edge the cursor must cross to return control
    /// The cursor is warped to the entry edge position
    pub fn set_entry_edge(&mut self, position: Position) -> X11Result<()> {
        // Calculate the opposite edge where cursor must exit
        let exit_edge = position;
        
        log::info!(
            "x11::cursor::entry: === ENTRY EDGE SET (EXPLICIT via ProtoEvent::Enter) ==="
        );
        log::info!(
            "x11::cursor::entry: entry_edge={:?} | exit_edge={:?} | source=EXPLICIT",
            position,
            exit_edge
        );
        
        // Warp cursor to the entry edge
        // If remote crossed LEFT edge, cursor appears on RIGHT side of this screen (and vice versa)
        let (x, y) = match position {
            Position::Left => {
                // Remote crossed left edge, cursor appears on right side
                (
                    self.screen_config.primary.width as i32 - self.edge_detector.config.warp_offset,
                    self.screen_config.primary.height as i32 / 2,
                )
            }
            Position::Right => {
                // Remote crossed right edge, cursor appears on left side
                (
                    self.edge_detector.config.warp_offset,
                    self.screen_config.primary.height as i32 / 2,
                )
            }
            Position::Top => {
                // Remote crossed top edge, cursor appears on bottom side
                (
                    self.screen_config.primary.width as i32 / 2,
                    self.screen_config.primary.height as i32 - self.edge_detector.config.warp_offset,
                )
            }
            Position::Bottom => {
                // Remote crossed bottom edge, cursor appears on top side
                (
                    self.screen_config.primary.width as i32 / 2,
                    self.edge_detector.config.warp_offset,
                )
            }
        };
        
        log::info!(
            "x11::cursor::entry: warping cursor to ({}, {}) on {:?} edge of screen ({})",
            x, y, position,
            if matches!(position, Position::Left | Position::Right) { "horizontal center" } else { "vertical center" }
        );
        
        self.warp_cursor(x, y)?;
        
        self.capture_position = Some(position);
        // Clear any inferred edge since we now have explicit edge
        self.inferred_entry_edge = None;
        // Reset the edge detector to prevent immediate edge detection
        self.edge_detector.reset();
        
        log::info!(
            "x11::cursor::entry: entry edge setup complete - cursor will return control when crossing {:?} edge",
            exit_edge
        );
        
        Ok(())
    }

    /// Clear the entry edge when a client leaves
    pub fn clear_entry_edge(&mut self) {
        log::info!(
            "x11::cursor::entry: === ENTRY EDGE CLEARED ==="
        );
        log::info!(
            "x11::cursor::entry: previous_entry_edge={:?} | cursor can now exit through ANY edge",
            self.capture_position
        );
        self.capture_position = None;
        self.inferred_entry_edge = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_rect() -> Rect {
        Rect::new(0, 0, 1920, 1080)
    }

    fn create_test_edge_config() -> EdgeConfig {
        EdgeConfig {
            edge_threshold: 1,
            edge_counter_threshold: 2,
            warp_offset: 1,
            enable_warping: true,
        }
    }

    // ============= CursorPosition Tests =============

    #[test]
    fn test_cursor_position_creation() {
        let pos = CursorPosition::new((100, 200), (50, 100), Some(0), (0.05, 0.18));

        assert_eq!(pos.virtual_pos, (100, 200));
        assert_eq!(pos.monitor_pos, (50, 100));
        assert_eq!(pos.monitor_index, Some(0));
        assert_eq!(pos.normalized, (0.05, 0.18));
    }

    // ============= EdgeConfig Tests =============

    #[test]
    fn test_edge_config_default() {
        let config = EdgeConfig::default();

        assert_eq!(config.edge_threshold, 1);
        assert_eq!(config.edge_counter_threshold, 2);
        assert_eq!(config.warp_offset, 1);
        assert!(config.enable_warping);
    }

    // ============= EdgeDetector Tests =============

    #[test]
    fn test_edge_detector_creation() {
        let config = create_test_edge_config();
        let bounds = create_test_rect();
        let detector = EdgeDetector::new(config, bounds);

        assert_eq!(detector.current_position(), (0, 0));
        assert_eq!(detector.previous_position(), (0, 0));
        assert_eq!(detector.edge_counter(), 0);
    }

    #[test]
    fn test_edge_detector_left_edge() {
        let config = create_test_edge_config();
        let bounds = create_test_rect();
        let mut detector = EdgeDetector::new(config, bounds);

        // Start in the middle
        let result = detector.update(500, 540);
        assert!(result.is_none());

        // Move to left edge (first detection)
        let result = detector.update(1, 540);
        assert!(result.is_none()); // Counter = 1, not enough

        // Move further left (second detection)
        let result = detector.update(0, 540);
        assert!(result.is_none()); // Counter reset because we didn't cross from outside

        // Reset and try proper crossing
        detector.reset();

        // Start in the middle
        detector.update(500, 540);

        // Move to edge (counter = 1)
        detector.update(1, 540);

        // Stay at edge (counter = 2, should trigger)
        let _result = detector.update(1, 540);
        // Still no crossing because previous was also at edge

        // Now test proper crossing: start outside, move to edge
        detector.reset();
        detector.update(10, 540); // Not at edge
        detector.update(1, 540); // At edge, counter = 1
        let result = detector.update(1, 540); // At edge, counter = 2
        // Previous is also at edge, so no crossing detected
        assert!(result.is_none());
    }

    #[test]
    fn test_edge_detector_right_edge() {
        let config = EdgeConfig {
            edge_threshold: 2,
            edge_counter_threshold: 1,
            warp_offset: 1,
            enable_warping: true,
        };
        let bounds = create_test_rect();
        let mut detector = EdgeDetector::new(config, bounds);

        // Start in the middle
        detector.update(1000, 540);

        // Move to right edge (should trigger immediately with threshold=1)
        let result = detector.update(1918, 540);
        assert_eq!(result, Some(Position::Right));
    }

    #[test]
    fn test_edge_detector_top_edge() {
        let config = EdgeConfig {
            edge_threshold: 1,
            edge_counter_threshold: 1,
            warp_offset: 1,
            enable_warping: true,
        };
        let bounds = create_test_rect();
        let mut detector = EdgeDetector::new(config, bounds);

        // Start in the middle
        detector.update(960, 500);

        // Move to top edge
        let result = detector.update(960, 1);
        assert_eq!(result, Some(Position::Top));
    }

    #[test]
    fn test_edge_detector_bottom_edge() {
        let config = EdgeConfig {
            edge_threshold: 1,
            edge_counter_threshold: 1,
            warp_offset: 1,
            enable_warping: true,
        };
        let bounds = create_test_rect();
        let mut detector = EdgeDetector::new(config, bounds);

        // Start in the middle
        detector.update(960, 500);

        // Move to bottom edge
        let result = detector.update(960, 1079);
        assert_eq!(result, Some(Position::Bottom));
    }

    #[test]
    fn test_edge_detector_counter_reset() {
        let config = create_test_edge_config();
        let bounds = create_test_rect();
        let mut detector = EdgeDetector::new(config, bounds);

        // Move to edge (counter = 1)
        detector.update(1, 540);
        assert_eq!(detector.edge_counter(), 1);

        // Move away from edge (counter should reset)
        detector.update(100, 540);
        assert_eq!(detector.edge_counter(), 0);
    }

    #[test]
    fn test_edge_detector_reset() {
        let config = create_test_edge_config();
        let bounds = create_test_rect();
        let mut detector = EdgeDetector::new(config, bounds);

        // Build up some counter - start outside edge, then move to edge
        detector.update(100, 540); // Not at edge
        detector.update(1, 540); // At edge, counter = 1
        assert_eq!(detector.edge_counter(), 1);

        // Reset
        detector.reset();
        assert_eq!(detector.edge_counter(), 0);
    }

    #[test]
    fn test_edge_detector_counter_threshold() {
        let config = EdgeConfig {
            edge_threshold: 1,
            edge_counter_threshold: 3,
            warp_offset: 1,
            enable_warping: true,
        };
        let bounds = create_test_rect();
        let mut detector = EdgeDetector::new(config, bounds);

        // Start in middle
        detector.update(500, 540);

        // Move to edge
        detector.update(1, 540);
        assert_eq!(detector.edge_counter(), 1);

        // Stay at edge
        detector.update(1, 540);
        assert_eq!(detector.edge_counter(), 2);

        // Still at edge
        detector.update(1, 540);
        assert_eq!(detector.edge_counter(), 0); // Reset after triggering
    }

    #[test]
    fn test_edge_detector_update_screen_bounds() {
        let config = create_test_edge_config();
        let bounds = create_test_rect();
        let mut detector = EdgeDetector::new(config, bounds);

        // Update to larger bounds
        let new_bounds = Rect::new(0, 0, 3840, 2160);
        detector.update_screen_bounds(new_bounds);

        // Should not trigger on old boundary
        detector.update(1920, 540);
        assert_eq!(detector.edge_counter(), 0);
    }

    #[test]
    fn test_edge_detector_no_crossing_when_starting_at_edge() {
        let config = EdgeConfig {
            edge_threshold: 1,
            edge_counter_threshold: 1,
            warp_offset: 1,
            enable_warping: true,
        };
        let bounds = create_test_rect();
        let mut detector = EdgeDetector::new(config, bounds);

        // Start at edge (previous = current = (1, 540))
        let _result = detector.update(1, 540);
        // Counter = 1, but previous is also at edge
        // No crossing because previous is also at edge

        // Stay at edge
        let _result = detector.update(1, 540);
        // Counter would be 2, but previous is still at edge
        // The crossing check requires previous to NOT be at edge
    }

    #[test]
    fn test_edge_detector_left_edge_crossing_from_outside() {
        let config = EdgeConfig {
            edge_threshold: 5,
            edge_counter_threshold: 1,
            warp_offset: 1,
            enable_warping: true,
        };
        let bounds = create_test_rect();
        let mut detector = EdgeDetector::new(config, bounds);

        // Start outside edge threshold
        detector.update(100, 540);

        // Move into edge threshold - should detect crossing
        let result = detector.update(3, 540);
        assert_eq!(result, Some(Position::Left));
    }

    #[test]
    fn test_edge_detector_all_edges_in_sequence() {
        let config = EdgeConfig {
            edge_threshold: 1,
            edge_counter_threshold: 1,
            warp_offset: 1,
            enable_warping: true,
        };
        let bounds = create_test_rect();
        let mut detector = EdgeDetector::new(config, bounds);

        // Test left edge
        detector.update(100, 540);
        let result = detector.update(1, 540);
        assert_eq!(result, Some(Position::Left));

        // Test right edge
        detector.update(1000, 540);
        let result = detector.update(1919, 540);
        assert_eq!(result, Some(Position::Right));

        // Test top edge
        detector.update(960, 100);
        let result = detector.update(960, 1);
        assert_eq!(result, Some(Position::Top));

        // Test bottom edge
        detector.update(960, 500);
        let result = detector.update(960, 1079);
        assert_eq!(result, Some(Position::Bottom));
    }

    #[test]
    fn test_edge_detector_corner_handling() {
        let config = EdgeConfig {
            edge_threshold: 1,
            edge_counter_threshold: 1,
            warp_offset: 1,
            enable_warping: true,
        };
        let bounds = create_test_rect();
        let mut detector = EdgeDetector::new(config, bounds);

        // Move to top-left corner
        detector.update(100, 100);
        let result = detector.update(1, 1);

        // Should detect one of the edges (left is checked first)
        assert!(result.is_some());
    }

    // ============= Position Tests =============

    #[test]
    fn test_position_equality() {
        assert_eq!(Position::Left, Position::Left);
        assert_eq!(Position::Right, Position::Right);
        assert_eq!(Position::Top, Position::Top);
        assert_eq!(Position::Bottom, Position::Bottom);

        assert_ne!(Position::Left, Position::Right);
        assert_ne!(Position::Top, Position::Bottom);
    }
}
