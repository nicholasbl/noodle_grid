use nalgebra_glm::{self as glm};
use std::ops::{Add, Div, Mul, Sub};

/// A trait for types that can perform linear interpolation.
///
/// Provides `lerp` and `clamped_lerp` functions.
///
/// Implemented for `f32` and `f64`.
pub trait LerpTrait:
    Sub<Output = Self>
    + Add<Output = Self>
    + Div<Output = Self>
    + Mul<Output = Self>
    + Copy
    + Sized
    + PartialOrd
{
    /// Linearly interpolates a value from an input range (`x0`..`x1`) to an output range (`y0`..`y1`).
    ///
    /// No clamping is performed: the result may be outside the output range if `self` is outside the input range.
    #[inline]
    fn lerp(&self, x0: Self, x1: Self, y0: Self, y1: Self) -> Self {
        y0 + (*self - x0) * ((y1 - y0) / (x1 - x0))
    }

    /// Linearly interpolates a value from an input range to an output range, with clamping.
    ///
    /// The output is guaranteed to stay within `y0..=y1`.
    #[inline]
    fn clamped_lerp(&self, x0: Self, x1: Self, y0: Self, y1: Self) -> Self {
        num_traits::clamp(self.lerp(x0, x1, y0, y1), y0, y1)
    }
}

// Provide LerpTrait implementations for basic floats
impl LerpTrait for f32 {}
impl LerpTrait for f64 {}

/// Creates a roll-free quaternion rotation pointing along the given direction.
///
/// This constructs an orthonormal basis with the given direction as the forward vector,
/// and returns the equivalent rotation quaternion.
pub fn roll_free_rotation(direction: glm::Vec3) -> glm::Quat {
    let up = glm::vec3(0.0, 1.0, 0.0); // World "up" direction

    // Construct orthonormal basis vectors
    let a = up.cross(&direction).normalize();
    let b = direction.cross(&a).normalize();

    // Assemble basis into rotation matrix
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

    // Convert rotation matrix to quaternion
    glm::mat3_to_quat(&m)
}

/// Transforms a point `[x, y, z]` by a 4x4 transformation matrix.
///
/// The point is treated as a position (i.e., homogeneous coordinate w = 1.0).
pub fn transform_p(p: [f32; 3], tf: &glm::Mat4) -> [f32; 3] {
    let lp: glm::Vec3 = p.into();
    let lp = glm::vec4(lp.x, lp.y, lp.z, 1.0);
    let lp = tf * lp;
    (lp.xyz() / lp.w).into()
}

/// Transforms a normal vector `[x, y, z]` by a 3x3 matrix.
///
/// The vector is normalized after transformation.
pub fn transform_n(p: [f32; 3], tf: &glm::Mat3) -> [f32; 3] {
    let lp: glm::Vec3 = p.into();
    let lp = (tf * lp).normalize();
    lp.into()
}
