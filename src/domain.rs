use nalgebra_glm::{self as glm};

use crate::utility::LerpTrait;

/// Represents the safety status of a voltage value.
pub enum VoltageSafety {
    Safe,
    Low,
    High,
}

/// Describes how to translate voltage and power values into visual dimensions (lengths, heights, and widths).
///
/// This struct holds calibration parameters and scaling information
/// for visualizing electrical properties like voltage and wattage.
#[derive(Debug)]
#[allow(dead_code)]
pub struct Domain {
    /// Data domain minimum and maximum along X axis.
    pub data_x: glm::DVec2,
    /// Data domain minimum and maximum along Y axis.
    pub data_y: glm::DVec2,

    /// Visual bounds along X axis after normalization.
    pub x_bounds: glm::DVec2,
    /// Visual bounds along Y axis after normalization.
    pub y_bounds: glm::DVec2,

    /// Minimum height (visual) corresponding to minimum voltage.
    pub volt_height_min: f32,
    /// Maximum height (visual) corresponding to maximum voltage.
    pub volt_height_max: f32,

    /// Minimum "safe" voltage value.
    pub volt_min: f32,
    /// Maximum "safe" voltage value.
    pub volt_max: f32,

    /// Minimum tube width for visualization.
    pub tube_min: f32,
    /// Maximum tube width for visualization.
    pub tube_max: f32,

    /// Maximum real or reactive power used for normalization.
    pub watt_bounds: f32,
}

impl Default for Domain {
    fn default() -> Self {
        Self {
            data_x: Default::default(),
            data_y: Default::default(),
            x_bounds: Default::default(),
            y_bounds: Default::default(),
            volt_height_min: 0.0,
            volt_height_max: 1.5,
            volt_min: 0.9,
            volt_max: 1.1,
            tube_min: 0.001,
            tube_max: 0.03,
            watt_bounds: 1700.0,
        }
    }
}

impl Domain {
    /// Create a `Domain` from raw data bounds, setting up normalized visual bounds.
    pub fn new(bound_min: glm::DVec2, bound_max: glm::DVec2) -> Self {
        let range = bound_max - bound_min;
        let max_dim = glm::DVec2::repeat(range.max() / 2.0);
        let center = (bound_min + bound_max) / 2.0;

        // Create a normalized square centered around the data center
        let nl = center - max_dim;
        let nh = center + max_dim;

        Self {
            data_x: glm::DVec2::new(bound_min.x, bound_max.x),
            data_y: glm::DVec2::new(bound_min.y, bound_max.y),
            x_bounds: glm::DVec2::new(nl.x, nh.x),
            y_bounds: glm::DVec2::new(nl.y, nh.y),
            ..Default::default()
        }
    }

    /// Maps a voltage value to a visual height, using clamped linear interpolation.
    #[inline]
    pub fn voltage_to_height(&self, v: f32) -> f32 {
        v.abs().clamped_lerp(
            self.volt_min,
            self.volt_max,
            self.volt_height_min,
            self.volt_height_max,
        )
    }

    /// Maps a line load (normalized) to a visual height.
    #[inline]
    pub fn line_load_to_height(&self, v: f32) -> f32 {
        v.abs()
            .clamped_lerp(0.0, 2.0, self.volt_height_min, self.volt_height_max)
    }

    /// Determines if a given voltage is within a safe range.
    #[inline]
    pub fn voltage_safety(&self, v: f32) -> VoltageSafety {
        if v < 0.95 {
            VoltageSafety::Low
        } else if v > 1.05 {
            VoltageSafety::High
        } else {
            VoltageSafety::Safe
        }
    }

    /// Maps real power (watts) to a visual width.
    #[inline]
    pub fn real_power_to_width(&self, v: f32) -> f32 {
        v.abs()
            .clamped_lerp(0.0, self.watt_bounds, self.tube_min, self.tube_max)
    }

    /// Maps reactive power (VARs) to a visual width.
    #[inline]
    pub fn reactive_power_to_width(&self, v: f32) -> f32 {
        v.abs()
            .clamped_lerp(0.0, self.watt_bounds, self.tube_min, self.tube_max)
    }

    /// Maps a normalized X coordinate [-1, 1] back into real-world bounds.
    #[inline]
    pub fn lerp_x(&self, v: f32) -> f32 {
        (v as f64).lerp(self.x_bounds.x, self.x_bounds.y, -1.0_f64, 1.0_f64) as f32
    }

    /// Maps a normalized Y coordinate [-1, 1] back into real-world bounds.
    ///
    /// Note: Y-axis is flipped (positive is downward visually).
    #[inline]
    pub fn lerp_y(&self, v: f32) -> f32 {
        (v as f64).lerp(
            self.y_bounds.x,
            self.y_bounds.y,
            1.0_f64, // Flip for now
            -1.0_f64,
        ) as f32
    }
}
