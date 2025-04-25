use nalgebra_glm::{self as glm};

use crate::utility::LerpTrait;

pub enum VoltageSafety {
    Safe,
    Low,
    High,
}

/// Describes how to translate voltage and power to lengths and heights
#[derive(Debug)]
#[allow(dead_code)]
pub struct Domain {
    pub data_x: glm::DVec2,
    pub data_y: glm::DVec2,

    pub x_bounds: glm::DVec2,
    pub y_bounds: glm::DVec2,

    pub volt_height_min: f32,
    pub volt_height_max: f32,

    pub volt_min: f32,
    pub volt_max: f32,

    pub tube_min: f32,
    pub tube_max: f32,

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
    pub fn new(bound_min: glm::DVec2, bound_max: glm::DVec2) -> Self {
        let range = bound_max - bound_min;
        let max_dim = glm::DVec2::repeat(range.max() / 2.0);
        let center = (bound_min + bound_max) / 2.0;

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

    #[inline]
    pub fn voltage_to_height(&self, v: f32) -> f32 {
        v.abs().clamped_lerp(
            self.volt_min,
            self.volt_max,
            self.volt_height_min,
            self.volt_height_max,
        )
    }

    #[inline]
    pub fn line_load_to_height(&self, v: f32) -> f32 {
        v.abs()
            .clamped_lerp(0.0, 2.0, self.volt_height_min, self.volt_height_max)
    }

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

    #[inline]
    pub fn real_power_to_width(&self, v: f32) -> f32 {
        v.abs()
            .clamped_lerp(0.0, self.watt_bounds, self.tube_min, self.tube_max)
    }

    #[inline]
    pub fn reactive_power_to_width(&self, v: f32) -> f32 {
        v.abs()
            .clamped_lerp(0.0, self.watt_bounds, self.tube_min, self.tube_max)
    }

    #[inline]
    pub fn lerp_x(&self, v: f32) -> f32 {
        (v as f64).lerp(self.x_bounds.x, self.x_bounds.y, -1.0_f64, 1.0_f64) as f32
    }

    #[inline]
    pub fn lerp_y(&self, v: f32) -> f32 {
        (v as f64).lerp(
            self.y_bounds.x,
            self.y_bounds.y,
            1.0_f64, // flip for now
            -1.0_f64,
        ) as f32
    }
}
