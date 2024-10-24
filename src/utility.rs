use nalgebra_glm::{self as glm};
use std::ops::{Add, Div, Mul, Sub};

pub trait LerpTrait:
    Sub<Output = Self>
    + Add<Output = Self>
    + Div<Output = Self>
    + Mul<Output = Self>
    + Copy
    + Sized
    + PartialOrd
{
    /// Linear interpolation of a value between one range to an output range
    #[inline]
    fn lerp(&self, x0: Self, x1: Self, y0: Self, y1: Self) -> Self {
        y0 + (*self - x0) * ((y1 - y0) / (x1 - x0))
    }

    /// Does the same as [`lerp`] but also clamps the output to the output range
    #[inline]
    fn clamped_lerp(&self, x0: Self, x1: Self, y0: Self, y1: Self) -> Self {
        num_traits::clamp(self.lerp(x0, x1, y0, y1), y0, y1)
    }
}

impl LerpTrait for f32 {}
impl LerpTrait for f64 {}

pub fn roll_free_rotation(direction: glm::Vec3) -> glm::Quat {
    let up = glm::vec3(0.0, 1.0, 0.0);

    let a = up.cross(&direction).normalize();
    let b = direction.cross(&a).normalize();

    let m = glm::mat3(
        a.x,
        b.x,
        direction.x,
        a.y,
        b.y,
        direction.y,
        a.z,
        b.z,
        direction.z,
    );

    glm::mat3_to_quat(&m)
}

pub fn transform_p(p: [f32; 3], tf: &glm::Mat4) -> [f32; 3] {
    let lp: glm::Vec3 = p.into();
    let lp = glm::vec4(lp.x, lp.y, lp.z, 1.0);
    let lp = tf * lp;
    (lp.xyz() / lp.w).into()
}

pub fn transform_n(p: [f32; 3], tf: &glm::Mat3) -> [f32; 3] {
    let lp: glm::Vec3 = p.into();
    let lp = (tf * lp).normalize();
    lp.into()
}
