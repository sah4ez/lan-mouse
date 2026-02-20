use crate::x11::{
    X11EmulationError, X11Result,
    screen::{MonitorInfo, ScreenConfig},
};

/// Coordinate transformer
///
/// This struct provides methods to convert between different coordinate spaces:
/// - Virtual coordinates: Coordinates in the combined virtual screen (all monitors as one big screen)
/// - Physical coordinates: Coordinates relative to a specific monitor
/// - Normalized coordinates: Coordinates in the 0.0-1.0 range (useful for network transmission)
#[derive(Debug, Clone)]
#[allow(dead_code)] // Will be used in future steps
pub struct CoordinateTransformer {
    screen_config: ScreenConfig,
}

#[allow(dead_code)] // Will be used in future steps
impl CoordinateTransformer {
    /// Create a new coordinate transformer
    ///
    /// # Arguments
    ///
    /// * `screen_config` - Screen configuration containing monitor information
    pub fn new(screen_config: ScreenConfig) -> Self {
        tracing::debug!(
            target: "x11::transform",
            virtual_bounds = %screen_config.virtual_bounds,
            "creating coordinate transformer"
        );

        Self { screen_config }
    }

    /// Convert virtual coordinates to physical coordinates
    ///
    /// Virtual coordinates are coordinates in the combined virtual screen (all monitors as one big screen).
    /// Physical coordinates are coordinates relative to a specific monitor.
    ///
    /// # Arguments
    ///
    /// * `x` - Virtual X coordinate
    /// * `y` - Virtual Y coordinate
    ///
    /// # Returns
    ///
    /// Physical coordinates (x, y) relative to the monitor containing the point
    ///
    /// # Example
    ///
    /// ```
    /// use input_emulation::x11::{ScreenConfig, CoordinateTransformer};
    ///
    /// let config = ScreenConfig::default_single_monitor(1920, 1080);
    /// let transformer = CoordinateTransformer::new(config);
    ///
    /// let (phys_x, phys_y) = transformer.virtual_to_physical(960, 540);
    /// assert_eq!(phys_x, 960);
    /// assert_eq!(phys_y, 540);
    /// ```
    pub fn virtual_to_physical(&self, x: i32, y: i32) -> (i32, i32) {
        tracing::trace!(
            target: "x11::transform",
            x, y,
            "converting virtual to physical"
        );

        // Find the monitor containing the point
        for (i, monitor) in self.screen_config.monitors.iter().enumerate() {
            if monitor.geometry.contains(x, y) {
                let physical = (x - monitor.geometry.x, y - monitor.geometry.y);

                tracing::trace!(
                    target: "x11::transform",
                    monitor_index = i,
                    monitor_name = %monitor.name,
                    phys_x = physical.0, phys_y = physical.1,
                    "converted to physical"
                );

                return physical;
            }
        }

        // Default: return virtual coordinates (no monitor found)
        tracing::trace!(
            target: "x11::transform",
            "no monitor found, using virtual coordinates"
        );

        (x, y)
    }

    /// Convert physical coordinates to virtual coordinates
    ///
    /// Requires specifying the monitor index.
    ///
    /// # Arguments
    ///
    /// * `monitor_index` - Index of the monitor in the screen configuration
    /// * `x` - Physical X coordinate relative to the monitor
    /// * `y` - Physical Y coordinate relative to the monitor
    ///
    /// # Returns
    ///
    /// Virtual coordinates (x, y) in the combined virtual screen
    ///
    /// # Errors
    ///
    /// Returns an error if the monitor index is out of range
    ///
    /// # Example
    ///
    /// ```
    /// use input_emulation::x11::{ScreenConfig, CoordinateTransformer};
    ///
    /// let config = ScreenConfig::default_single_monitor(1920, 1080);
    /// let transformer = CoordinateTransformer::new(config);
    ///
    /// let (virt_x, virt_y) = transformer.physical_to_virtual(0, 960, 540).unwrap();
    /// assert_eq!(virt_x, 960);
    /// assert_eq!(virt_y, 540);
    /// ```
    pub fn physical_to_virtual(
        &self,
        monitor_index: usize,
        x: i32,
        y: i32,
    ) -> X11Result<(i32, i32)> {
        let monitor = self
            .screen_config
            .monitors
            .get(monitor_index)
            .ok_or_else(|| {
                X11EmulationError::InvalidCoordinates(format!(
                    "monitor index {} out of range",
                    monitor_index
                ))
            })?;

        let virtual_coords = (x + monitor.geometry.x, y + monitor.geometry.y);

        tracing::trace!(
            target: "x11::transform",
            monitor_index,
            phys_x = x, phys_y = y,
            virt_x = virtual_coords.0, virt_y = virtual_coords.1,
            "converted to virtual"
        );

        Ok(virtual_coords)
    }

    /// Normalize coordinates to the 0.0-1.0 range
    ///
    /// Normalized coordinates are useful for network transmission as they are
    /// resolution-independent.
    ///
    /// # Arguments
    ///
    /// * `x` - Virtual X coordinate
    /// * `y` - Virtual Y coordinate
    ///
    /// # Returns
    ///
    /// Normalized coordinates (x, y) in the 0.0-1.0 range
    ///
    /// # Example
    ///
    /// ```
    /// use input_emulation::x11::{ScreenConfig, CoordinateTransformer};
    ///
    /// let config = ScreenConfig::default_single_monitor(1920, 1080);
    /// let transformer = CoordinateTransformer::new(config);
    ///
    /// let (norm_x, norm_y) = transformer.normalize(960, 540);
    /// assert!((norm_x - 0.5).abs() < 0.01);
    /// assert!((norm_y - 0.5).abs() < 0.01);
    /// ```
    pub fn normalize(&self, x: i32, y: i32) -> (f64, f64) {
        let width = self.screen_config.virtual_bounds.width as f64;
        let height = self.screen_config.virtual_bounds.height as f64;

        let normalized = ((x as f64) / width, (y as f64) / height);

        tracing::trace!(
            target: "x11::transform",
            input_x = x, input_y = y,
            bounds_width = width, bounds_height = height,
            norm_x = normalized.0, norm_y = normalized.1,
            "normalized coordinates"
        );

        normalized
    }

    /// Denormalize coordinates from the 0.0-1.0 range
    ///
    /// Reverse operation of normalize.
    ///
    /// # Arguments
    ///
    /// * `x` - Normalized X coordinate in the 0.0-1.0 range
    /// * `y` - Normalized Y coordinate in the 0.0-1.0 range
    ///
    /// # Returns
    ///
    /// Virtual coordinates (x, y)
    ///
    /// # Example
    ///
    /// ```
    /// use input_emulation::x11::{ScreenConfig, CoordinateTransformer};
    ///
    /// let config = ScreenConfig::default_single_monitor(1920, 1080);
    /// let transformer = CoordinateTransformer::new(config);
    ///
    /// let (denorm_x, denorm_y) = transformer.denormalize(0.5, 0.5);
    /// assert_eq!(denorm_x, 960);
    /// assert_eq!(denorm_y, 540);
    /// ```
    pub fn denormalize(&self, x: f64, y: f64) -> (i32, i32) {
        let width = self.screen_config.virtual_bounds.width as i32;
        let height = self.screen_config.virtual_bounds.height as i32;

        let denormalized = ((x * width as f64) as i32, (y * height as f64) as i32);

        tracing::trace!(
            target: "x11::transform",
            input_x = x, input_y = y,
            bounds_width = width, bounds_height = height,
            denorm_x = denormalized.0, denorm_y = denormalized.1,
            "denormalized coordinates"
        );

        denormalized
    }

    /// Clamp coordinates to virtual bounds
    ///
    /// Ensures that coordinates are within the virtual screen boundaries.
    ///
    /// # Arguments
    ///
    /// * `x` - Virtual X coordinate
    /// * `y` - Virtual Y coordinate
    ///
    /// # Returns
    ///
    /// Clamped coordinates (x, y) within virtual bounds
    ///
    /// # Example
    ///
    /// ```
    /// use input_emulation::x11::{ScreenConfig, CoordinateTransformer};
    ///
    /// let config = ScreenConfig::default_single_monitor(1920, 1080);
    /// let transformer = CoordinateTransformer::new(config);
    ///
    /// assert_eq!(transformer.clamp_to_virtual(-10, 100), (0, 100));
    /// assert_eq!(transformer.clamp_to_virtual(2000, 100), (1919, 100));
    /// ```
    pub fn clamp_to_virtual(&self, x: i32, y: i32) -> (i32, i32) {
        let clamped = self.screen_config.clamp_to_virtual(x, y);

        tracing::trace!(
            target: "x11::transform",
            input_x = x, input_y = y,
            clamped_x = clamped.0, clamped_y = clamped.1,
            "clamped to virtual bounds"
        );

        clamped
    }

    /// Find monitor by virtual coordinates
    ///
    /// # Arguments
    ///
    /// * `x` - Virtual X coordinate
    /// * `y` - Virtual Y coordinate
    ///
    /// # Returns
    ///
    /// Reference to the monitor containing the point, or None if no monitor found
    pub fn find_monitor(&self, x: i32, y: i32) -> Option<&MonitorInfo> {
        self.screen_config.find_monitor_at(x, y)
    }

    /// Find monitor index by virtual coordinates
    ///
    /// # Arguments
    ///
    /// * `x` - Virtual X coordinate
    /// * `y` - Virtual Y coordinate
    ///
    /// # Returns
    ///
    /// Index of the monitor containing the point, or None if no monitor found
    pub fn find_monitor_index(&self, x: i32, y: i32) -> Option<usize> {
        self.screen_config.find_monitor_index_at(x, y)
    }

    /// Get screen configuration
    ///
    /// # Returns
    ///
    /// Reference to the screen configuration
    pub fn screen_config(&self) -> &ScreenConfig {
        &self.screen_config
    }

    /// Update screen configuration
    ///
    /// # Arguments
    ///
    /// * `screen_config` - New screen configuration
    pub fn update_screen_config(&mut self, screen_config: ScreenConfig) {
        tracing::info!(
            target: "x11::transform",
            old_bounds = %self.screen_config.virtual_bounds,
            new_bounds = %screen_config.virtual_bounds,
            "updating screen config"
        );

        self.screen_config = screen_config;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize() {
        let config = ScreenConfig::default_single_monitor(1920, 1080);
        let transformer = CoordinateTransformer::new(config);

        // Test center of screen
        let normalized = transformer.normalize(960, 540);
        assert!((normalized.0 - 0.5).abs() < 0.01);
        assert!((normalized.1 - 0.5).abs() < 0.01);

        // Test top-left corner
        let normalized = transformer.normalize(0, 0);
        assert_eq!(normalized.0, 0.0);
        assert_eq!(normalized.1, 0.0);

        // Test bottom-right corner
        let normalized = transformer.normalize(1919, 1079);
        assert!((normalized.0 - 1.0).abs() < 0.001);
        assert!((normalized.1 - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_denormalize() {
        let config = ScreenConfig::default_single_monitor(1920, 1080);
        let transformer = CoordinateTransformer::new(config);

        // Test center of screen
        let denormalized = transformer.denormalize(0.5, 0.5);
        assert_eq!(denormalized, (960, 540));

        // Test top-left corner
        let denormalized = transformer.denormalize(0.0, 0.0);
        assert_eq!(denormalized, (0, 0));

        // Test bottom-right corner
        let denormalized = transformer.denormalize(1.0, 1.0);
        assert_eq!(denormalized, (1920, 1080));
    }

    #[test]
    fn test_clamp_to_virtual() {
        let config = ScreenConfig::default_single_monitor(1920, 1080);
        let transformer = CoordinateTransformer::new(config);

        // Test clamping by X
        assert_eq!(transformer.clamp_to_virtual(-10, 100), (0, 100));
        assert_eq!(transformer.clamp_to_virtual(2000, 100), (1919, 100));

        // Test clamping by Y
        assert_eq!(transformer.clamp_to_virtual(100, -10), (100, 0));
        assert_eq!(transformer.clamp_to_virtual(100, 1200), (100, 1079));

        // Test clamping by both axes
        assert_eq!(transformer.clamp_to_virtual(-10, -10), (0, 0));
        assert_eq!(transformer.clamp_to_virtual(2000, 1200), (1919, 1079));

        // Test no clamping needed
        assert_eq!(transformer.clamp_to_virtual(100, 100), (100, 100));
    }

    #[test]
    fn test_virtual_to_physical_single_monitor() {
        let config = ScreenConfig::default_single_monitor(1920, 1080);
        let transformer = CoordinateTransformer::new(config);

        // Test various points
        assert_eq!(transformer.virtual_to_physical(0, 0), (0, 0));
        assert_eq!(transformer.virtual_to_physical(960, 540), (960, 540));
        assert_eq!(transformer.virtual_to_physical(1919, 1079), (1919, 1079));
    }

    #[test]
    fn test_virtual_to_physical_multi_monitor() {
        // Create a multi-monitor configuration
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

        let config = ScreenConfig {
            primary: monitor1.geometry,
            monitors: vec![monitor1, monitor2],
            virtual_bounds: Rect::new(0, 0, 3840, 1080),
        };

        let transformer = CoordinateTransformer::new(config);

        // Test point in first monitor
        let (phys_x, phys_y) = transformer.virtual_to_physical(960, 540);
        assert_eq!(phys_x, 960);
        assert_eq!(phys_y, 540);

        // Test point in second monitor
        let (phys_x, phys_y) = transformer.virtual_to_physical(2880, 540);
        assert_eq!(phys_x, 960); // 2880 - 1920
        assert_eq!(phys_y, 540);

        // Test point on boundary
        let (phys_x, phys_y) = transformer.virtual_to_physical(1920, 540);
        assert_eq!(phys_x, 0); // Should be in second monitor (1920 - 1920)
        assert_eq!(phys_y, 540);
    }

    #[test]
    fn test_physical_to_virtual_valid_index() {
        let config = ScreenConfig::default_single_monitor(1920, 1080);
        let transformer = CoordinateTransformer::new(config);

        // Test various points
        let result = transformer.physical_to_virtual(0, 0, 0);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), (0, 0));

        let result = transformer.physical_to_virtual(0, 960, 540);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), (960, 540));

        let result = transformer.physical_to_virtual(0, 1919, 1079);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), (1919, 1079));
    }

    #[test]
    fn test_physical_to_virtual_invalid_index() {
        let config = ScreenConfig::default_single_monitor(1920, 1080);
        let transformer = CoordinateTransformer::new(config);

        // Test with invalid monitor index
        let result = transformer.physical_to_virtual(5, 100, 100);
        assert!(result.is_err());

        if let Err(X11EmulationError::InvalidCoordinates(msg)) = result {
            assert!(msg.contains("monitor index 5 out of range"));
        } else {
            panic!("Expected InvalidCoordinates error");
        }
    }

    #[test]
    fn test_physical_to_virtual_multi_monitor() {
        // Create a multi-monitor configuration
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

        let config = ScreenConfig {
            primary: monitor1.geometry,
            monitors: vec![monitor1, monitor2],
            virtual_bounds: Rect::new(0, 0, 3840, 1080),
        };

        let transformer = CoordinateTransformer::new(config);

        // Test point in first monitor
        let result = transformer.physical_to_virtual(0, 960, 540);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), (960, 540));

        // Test point in second monitor
        let result = transformer.physical_to_virtual(1, 960, 540);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), (2880, 540)); // 960 + 1920
    }

    #[test]
    fn test_find_monitor() {
        let config = ScreenConfig::default_single_monitor(1920, 1080);
        let transformer = CoordinateTransformer::new(config);

        // Test finding monitor
        let monitor = transformer.find_monitor(100, 100);
        assert!(monitor.is_some());
        assert_eq!(monitor.unwrap().name, "default");

        // Test point outside monitor
        let monitor = transformer.find_monitor(-10, 100);
        assert!(monitor.is_none());
    }

    #[test]
    fn test_find_monitor_index() {
        let config = ScreenConfig::default_single_monitor(1920, 1080);
        let transformer = CoordinateTransformer::new(config);

        // Test finding monitor index
        let index = transformer.find_monitor_index(100, 100);
        assert_eq!(index, Some(0));

        // Test point outside monitor
        let index = transformer.find_monitor_index(-10, 100);
        assert_eq!(index, None);
    }

    #[test]
    fn test_screen_config() {
        let config = ScreenConfig::default_single_monitor(1920, 1080);
        let transformer = CoordinateTransformer::new(config.clone());

        let retrieved_config = transformer.screen_config();
        assert_eq!(retrieved_config.virtual_bounds.width, 1920);
        assert_eq!(retrieved_config.virtual_bounds.height, 1080);
    }

    #[test]
    fn test_update_screen_config() {
        let config1 = ScreenConfig::default_single_monitor(1920, 1080);
        let mut transformer = CoordinateTransformer::new(config1);

        let config2 = ScreenConfig::default_single_monitor(2560, 1440);
        transformer.update_screen_config(config2);

        let retrieved_config = transformer.screen_config();
        assert_eq!(retrieved_config.virtual_bounds.width, 2560);
        assert_eq!(retrieved_config.virtual_bounds.height, 1440);
    }

    #[test]
    fn test_normalize_denormalize_roundtrip() {
        let config = ScreenConfig::default_single_monitor(1920, 1080);
        let transformer = CoordinateTransformer::new(config);

        // Test roundtrip conversion
        let original = (100, 200);
        let normalized = transformer.normalize(original.0, original.1);
        let denormalized = transformer.denormalize(normalized.0, normalized.1);

        assert_eq!(denormalized, original);
    }

    #[test]
    fn test_virtual_physical_roundtrip() {
        let config = ScreenConfig::default_single_monitor(1920, 1080);
        let transformer = CoordinateTransformer::new(config);

        // Test roundtrip conversion
        let original = (100, 200);
        let physical = transformer.virtual_to_physical(original.0, original.1);

        // Find the monitor index for the original point
        let monitor_index = transformer
            .find_monitor_index(original.0, original.1)
            .unwrap();
        let virtual_coords = transformer
            .physical_to_virtual(monitor_index, physical.0, physical.1)
            .unwrap();

        assert_eq!(virtual_coords, original);
    }
}
