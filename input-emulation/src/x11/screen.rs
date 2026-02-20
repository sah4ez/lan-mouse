use serde::{Deserialize, Serialize};
use std::fmt;

use super::X11DisplayHandle;
use super::error::{X11EmulationError, X11Result};

/// Rectangle geometry
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[allow(dead_code)] // Will be used in future steps
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[allow(dead_code)] // Will be used in future steps
impl Rect {
    /// Create a new rectangle
    pub const fn new(x: i32, y: i32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// Check if rectangle contains a point
    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.x
            && x < (self.x + self.width as i32)
            && y >= self.y
            && y < (self.y + self.height as i32)
    }

    /// Clamp coordinates to rectangle
    pub fn clamp(&self, x: i32, y: i32) -> (i32, i32) {
        let clamped_x = x.max(self.x).min(self.x + self.width as i32 - 1);
        let clamped_y = y.max(self.y).min(self.y + self.height as i32 - 1);
        (clamped_x, clamped_y)
    }

    /// Get right boundary
    pub fn right(&self) -> i32 {
        self.x + self.width as i32
    }

    /// Get bottom boundary
    pub fn bottom(&self) -> i32 {
        self.y + self.height as i32
    }

    /// Get rectangle center
    pub fn center(&self) -> (i32, i32) {
        (
            self.x + (self.width / 2) as i32,
            self.y + (self.height / 2) as i32,
        )
    }
}

impl fmt::Display for Rect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Rect(x={}, y={}, w={}, h={})",
            self.x, self.y, self.width, self.height
        )
    }
}

/// Monitor information
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)] // Will be used in future steps
pub struct MonitorInfo {
    /// Monitor name (e.g., "DP-0", "HDMI-1")
    pub name: String,
    /// Physical geometry
    pub geometry: Rect,
    /// Primary monitor flag
    pub is_primary: bool,
    /// Output connection (e.g., "DisplayPort-0")
    pub output: String,
    /// Supported refresh rates
    pub refresh_rates: Vec<f64>,
}

#[allow(dead_code)] // Will be used in future steps
impl MonitorInfo {
    /// Create new monitor information
    pub fn new(name: String, geometry: Rect, is_primary: bool, output: String) -> Self {
        Self {
            name,
            geometry,
            is_primary,
            output,
            refresh_rates: Vec::new(),
        }
    }

    /// Get monitor width
    pub fn width(&self) -> i32 {
        self.geometry.width as i32
    }

    /// Get monitor height
    pub fn height(&self) -> i32 {
        self.geometry.height as i32
    }
}

/// Screen configuration
#[derive(Debug, Clone)]
#[allow(dead_code)] // Will be used in future steps
pub struct ScreenConfig {
    /// Primary screen dimensions
    pub primary: Rect,
    /// All connected monitors
    pub monitors: Vec<MonitorInfo>,
    /// Virtual screen bounds (union of all monitors)
    pub virtual_bounds: Rect,
}

#[allow(dead_code)] // Will be used in future steps
impl ScreenConfig {
    /// Create default screen configuration (single monitor)
    pub fn default_single_monitor(width: i32, height: i32) -> Self {
        let primary = Rect::new(0, 0, width as u32, height as u32);
        let monitors = vec![MonitorInfo::new(
            "default".to_string(),
            primary,
            true,
            "default".to_string(),
        )];

        Self {
            primary,
            monitors,
            virtual_bounds: primary,
        }
    }

    /// Find monitor containing a point
    pub fn find_monitor_at(&self, x: i32, y: i32) -> Option<&MonitorInfo> {
        self.monitors.iter().find(|m| m.geometry.contains(x, y))
    }

    /// Find monitor index containing a point
    pub fn find_monitor_index_at(&self, x: i32, y: i32) -> Option<usize> {
        self.monitors.iter().position(|m| m.geometry.contains(x, y))
    }

    /// Get primary monitor
    pub fn primary_monitor(&self) -> Option<&MonitorInfo> {
        self.monitors.iter().find(|m| m.is_primary)
    }

    /// Clamp coordinates to virtual bounds
    pub fn clamp_to_virtual(&self, x: i32, y: i32) -> (i32, i32) {
        self.virtual_bounds.clamp(x, y)
    }
}

/// XRandR configuration
#[derive(Debug, Clone, Default)]
#[allow(dead_code)] // Will be used in future steps
pub struct XRandRConfig {
    /// XRandR version
    pub major_version: i32,
    pub minor_version: i32,
    /// Available monitors
    pub available_monitors: Vec<String>,
}

#[allow(dead_code)] // Will be used in future steps
impl XRandRConfig {
    /// Create new XRandR configuration
    pub fn new(major: i32, minor: i32) -> Self {
        Self {
            major_version: major,
            minor_version: minor,
            available_monitors: Vec::new(),
        }
    }
}

/// Query screen configuration
///
/// This function queries information about connected monitors
/// and calculates virtual screen bounds.
pub fn query_screen_config(display: &X11DisplayHandle) -> X11Result<ScreenConfig> {
    tracing::debug!(target: "x11::screen", "querying screen configuration");

    unsafe {
        let screen = x11::xlib::XScreenOfDisplay(display.get(), 0);
        let primary_width = (*screen).width as i32;
        let primary_height = (*screen).height as i32;

        tracing::info!(
            target: "x11::screen",
            primary_width,
            primary_height,
            "primary screen dimensions: {}x{}",
            primary_width, primary_height
        );

        // Try to query XRandR for multi-monitor configuration
        let monitors = match query_xrandr_monitors(display) {
            Ok(monitors) => {
                tracing::info!(
                    target: "x11::screen",
                    monitor_count = monitors.len(),
                    "XRandR monitors detected"
                );

                monitors
            }
            Err(e) => {
                tracing::warn!(
                    target: "x11::screen",
                    error = %e,
                    "XRandR query failed, using single monitor mode"
                );
                // Create configuration with single monitor
                return Ok(ScreenConfig::default_single_monitor(
                    primary_width,
                    primary_height,
                ));
            }
        };

        // Calculate virtual bounds
        let virtual_bounds = calculate_virtual_bounds(&monitors);

        tracing::info!(
            target: "x11::screen",
            virtual_width = virtual_bounds.width,
            virtual_height = virtual_bounds.height,
            monitor_count = monitors.len(),
            "screen configuration loaded"
        );

        Ok(ScreenConfig {
            primary: Rect::new(0, 0, primary_width as u32, primary_height as u32),
            monitors,
            virtual_bounds,
        })
    }
}

/// Query monitor information via XRandR
#[allow(dead_code)] // Will be used in future steps
unsafe fn query_xrandr_monitors(display: &X11DisplayHandle) -> X11Result<Vec<MonitorInfo>> {
    // Validate display before use (HIGH PRIORITY: prevents use-after-free)
    if !display.is_valid() {
        return Err(X11EmulationError::InvalidDisplay);
    }
    
    // Check for XRandR availability
    let mut major = 0;
    let mut minor = 0;
    let has_xrandr = x11::xrandr::XRRQueryVersion(display.get(), &mut major, &mut minor);

    if has_xrandr == 0 {
        return Err(X11EmulationError::XRandRError(
            "XRandR extension not available".to_string(),
        ));
    }

    tracing::debug!(
        target: "x11::screen",
        major_version = major,
        minor_version = minor,
        "XRandR version detected"
    );

    // Get monitor information
    let root_window = x11::xlib::XDefaultRootWindow(display.get());
    let mut n_monitors: i32 = 0;
    let monitors_ptr = x11::xrandr::XRRGetMonitors(
        display.get(),
        root_window,
        1, // get_active
        &mut n_monitors,
    );

    if monitors_ptr.is_null() {
        return Err(X11EmulationError::XRandRError(
            "XRRGetMonitors returned null".to_string(),
        ));
    }

    tracing::debug!(
        target: "x11::screen",
        n_monitors,
        "monitors retrieved from XRandR"
    );

    let mut monitors = Vec::new();
    for i in 0..n_monitors {
        let monitor = *monitors_ptr.offset(i as isize);

        // CRITICAL FIX: monitor.name is an X11 Atom (32-bit identifier), not a pointer
        // We need to use XGetAtomName to get the actual string representation
        let name = if monitor.name == 0 {
            tracing::warn!(
                target: "x11::screen",
                index = i,
                "monitor name atom is None (0), using default name"
            );
            format!("monitor-{}", i)
        } else {
            // Use XGetAtomName to convert Atom to string
            unsafe {
                let atom_name_ptr = x11::xlib::XGetAtomName(display.get(), monitor.name as x11::xlib::Atom);
                if atom_name_ptr.is_null() {
                    tracing::warn!(
                        target: "x11::screen",
                        index = i,
                        atom = monitor.name,
                        "XGetAtomName returned null, using default name"
                    );
                    format!("monitor-{}", i)
                } else {
                    // Convert C string to Rust string
                    let c_str = std::ffi::CStr::from_ptr(atom_name_ptr);
                    let name = c_str.to_string_lossy().into_owned();
                    // Free the string allocated by XGetAtomName
                    x11::xlib::XFree(atom_name_ptr as *mut _);
                    name
                }
            }
        };

        let geometry = Rect::new(
            monitor.x,
            monitor.y,
            monitor.width as u32,
            monitor.height as u32,
        );

        let is_primary = monitor.primary != 0;

        tracing::trace!(
            target: "x11::screen",
            index = i,
            name,
            geometry = %geometry,
            is_primary,
            "monitor info"
        );

        monitors.push(MonitorInfo::new(
            name,
            geometry,
            is_primary,
            format!("output-{}", i),
        ));
    }

    // Free memory
    x11::xrandr::XRRFreeMonitors(monitors_ptr);

    Ok(monitors)
}

/// Calculate virtual screen bounds
fn calculate_virtual_bounds(monitors: &[MonitorInfo]) -> Rect {
    if monitors.is_empty() {
        return Rect::new(0, 0, 0, 0);
    }

    let mut min_x = i32::MAX;
    let mut min_y = i32::MAX;
    let mut max_x = i32::MIN;
    let mut max_y = i32::MIN;

    for monitor in monitors {
        min_x = min_x.min(monitor.geometry.x);
        min_y = min_y.min(monitor.geometry.y);
        max_x = max_x.max(monitor.geometry.right());
        max_y = max_y.max(monitor.geometry.bottom());
    }

    Rect::new(min_x, min_y, (max_x - min_x) as u32, (max_y - min_y) as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rect_creation() {
        let rect = Rect::new(10, 20, 100, 200);
        assert_eq!(rect.x, 10);
        assert_eq!(rect.y, 20);
        assert_eq!(rect.width, 100);
        assert_eq!(rect.height, 200);
    }

    #[test]
    fn test_rect_contains() {
        let rect = Rect::new(10, 20, 100, 200);
        assert!(rect.contains(10, 20)); // Top-left corner
        assert!(rect.contains(109, 219)); // Bottom-right corner
        assert!(rect.contains(60, 120)); // Center
        assert!(!rect.contains(9, 20)); // Left of rect
        assert!(!rect.contains(10, 19)); // Above rect
        assert!(!rect.contains(110, 20)); // Right of rect
        assert!(!rect.contains(10, 220)); // Below rect
    }

    #[test]
    fn test_rect_clamp() {
        let rect = Rect::new(10, 20, 100, 200);
        let (x, y) = rect.clamp(5, 15);
        assert_eq!(x, 10); // Clamped to left edge
        assert_eq!(y, 20); // Clamped to top edge

        let (x, y) = rect.clamp(120, 230);
        assert_eq!(x, 109); // Clamped to right edge
        assert_eq!(y, 219); // Clamped to bottom edge

        let (x, y) = rect.clamp(60, 120);
        assert_eq!(x, 60); // Inside rect
        assert_eq!(y, 120); // Inside rect
    }

    #[test]
    fn test_rect_right() {
        let rect = Rect::new(10, 20, 100, 200);
        assert_eq!(rect.right(), 110);
    }

    #[test]
    fn test_rect_bottom() {
        let rect = Rect::new(10, 20, 100, 200);
        assert_eq!(rect.bottom(), 220);
    }

    #[test]
    fn test_rect_center() {
        let rect = Rect::new(10, 20, 100, 200);
        let (x, y) = rect.center();
        assert_eq!(x, 60);
        assert_eq!(y, 120);
    }

    #[test]
    fn test_monitor_info_creation() {
        let geometry = Rect::new(0, 0, 1920, 1080);
        let monitor = MonitorInfo::new(
            "DP-0".to_string(),
            geometry,
            true,
            "DisplayPort-0".to_string(),
        );
        assert_eq!(monitor.name, "DP-0");
        assert_eq!(monitor.geometry, geometry);
        assert!(monitor.is_primary);
        assert_eq!(monitor.output, "DisplayPort-0");
        assert!(monitor.refresh_rates.is_empty());
    }

    #[test]
    fn test_monitor_info_width_height() {
        let geometry = Rect::new(0, 0, 1920, 1080);
        let monitor = MonitorInfo::new(
            "DP-0".to_string(),
            geometry,
            true,
            "DisplayPort-0".to_string(),
        );
        assert_eq!(monitor.width(), 1920);
        assert_eq!(monitor.height(), 1080);
    }

    #[test]
    fn test_screen_config_default_single_monitor() {
        let config = ScreenConfig::default_single_monitor(1920, 1080);
        assert_eq!(config.primary, Rect::new(0, 0, 1920, 1080));
        assert_eq!(config.monitors.len(), 1);
        assert_eq!(config.monitors[0].name, "default");
        assert!(config.monitors[0].is_primary);
        assert_eq!(config.virtual_bounds, Rect::new(0, 0, 1920, 1080));
    }

    #[test]
    fn test_screen_config_find_monitor_at() {
        let config = ScreenConfig::default_single_monitor(1920, 1080);
        let monitor = config.find_monitor_at(100, 100);
        assert!(monitor.is_some());
        assert_eq!(monitor.unwrap().name, "default");

        let monitor = config.find_monitor_at(-10, 100);
        assert!(monitor.is_none());
    }

    #[test]
    fn test_screen_config_find_monitor_index_at() {
        let config = ScreenConfig::default_single_monitor(1920, 1080);
        let index = config.find_monitor_index_at(100, 100);
        assert_eq!(index, Some(0));

        let index = config.find_monitor_index_at(-10, 100);
        assert_eq!(index, None);
    }

    #[test]
    fn test_screen_config_primary_monitor() {
        let config = ScreenConfig::default_single_monitor(1920, 1080);
        let monitor = config.primary_monitor();
        assert!(monitor.is_some());
        assert!(monitor.unwrap().is_primary);
    }

    #[test]
    fn test_screen_config_clamp_to_virtual() {
        let config = ScreenConfig::default_single_monitor(1920, 1080);
        let (x, y) = config.clamp_to_virtual(-10, -10);
        assert_eq!(x, 0);
        assert_eq!(y, 0);

        let (x, y) = config.clamp_to_virtual(2000, 1200);
        assert_eq!(x, 1919);
        assert_eq!(y, 1079);
    }

    #[test]
    fn test_xrandr_config_creation() {
        let config = XRandRConfig::new(1, 6);
        assert_eq!(config.major_version, 1);
        assert_eq!(config.minor_version, 6);
        assert!(config.available_monitors.is_empty());
    }

    #[test]
    fn test_calculate_virtual_bounds_single_monitor() {
        let geometry = Rect::new(0, 0, 1920, 1080);
        let monitors = vec![MonitorInfo::new(
            "DP-0".to_string(),
            geometry,
            true,
            "DisplayPort-0".to_string(),
        )];
        let bounds = calculate_virtual_bounds(&monitors);
        assert_eq!(bounds, Rect::new(0, 0, 1920, 1080));
    }

    #[test]
    fn test_calculate_virtual_bounds_multiple_monitors() {
        let monitor1 = MonitorInfo::new(
            "DP-0".to_string(),
            Rect::new(0, 0, 1920, 1080),
            true,
            "DisplayPort-0".to_string(),
        );
        let monitor2 = MonitorInfo::new(
            "HDMI-1".to_string(),
            Rect::new(1920, 0, 1920, 1080),
            false,
            "HDMI-1".to_string(),
        );
        let monitors = vec![monitor1, monitor2];
        let bounds = calculate_virtual_bounds(&monitors);
        assert_eq!(bounds, Rect::new(0, 0, 3840, 1080));
        assert_eq!(bounds.x, 0);
        assert_eq!(bounds.y, 0);
        assert_eq!(bounds.width, 3840);
        assert_eq!(bounds.height, 1080);
    }

    #[test]
    fn test_calculate_virtual_bounds_empty_monitors() {
        let monitors: Vec<MonitorInfo> = vec![];
        let bounds = calculate_virtual_bounds(&monitors);
        assert_eq!(bounds, Rect::new(0, 0, 0, 0));
    }

    #[test]
    fn test_calculate_virtual_bounds_offset_monitors() {
        let monitor1 = MonitorInfo::new(
            "DP-0".to_string(),
            Rect::new(-100, -50, 1920, 1080),
            true,
            "DisplayPort-0".to_string(),
        );
        let monitor2 = MonitorInfo::new(
            "HDMI-1".to_string(),
            Rect::new(1920, 100, 1920, 1080),
            false,
            "HDMI-1".to_string(),
        );
        let monitors = vec![monitor1, monitor2];
        let bounds = calculate_virtual_bounds(&monitors);
        // Monitor 1: x=-100, y=-50, width=1920, height=1080, right=1820, bottom=1030
        // Monitor 2: x=1920, y=100, width=1920, height=1080, right=3840, bottom=1180
        // Virtual: min_x=-100, min_y=-50, max_x=3840, max_y=1180
        // Width = 3840 - (-100) = 3940, Height = 1180 - (-50) = 1230
        assert_eq!(bounds, Rect::new(-100, -50, 3940, 1230));
    }
}
