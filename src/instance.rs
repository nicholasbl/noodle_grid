use std::collections::HashSet;

use crate::{
    domain::{Domain, VoltageSafety},
    dots::GeneratorType,
    utility::roll_free_rotation,
    GeneratorState, LineState, TransformerState,
};

use nalgebra_glm as glm;

/// Struct for extracting simplified per-line metrics used in instance rendering.
pub struct LineGetterResult {
    pub volt_start: f32,
    pub volt_end: f32,
    pub watt: f32,
    pub vars: f32,
    pub line_load: f32,
}

#[allow(dead_code)]
pub struct TfGetterResult {
    pub volt_start: f32,
    pub volt_end: f32,
    pub tap: i32,
    pub tap_change: i32,
}

#[allow(dead_code)]
pub struct GeneratorGetterResult {
    pub voltage: f32,
    pub angle: f32,
    pub real: f32,
    pub react: f32,
    pub ty: GeneratorType,
}

/// Recomputes bus instance transforms and encodes them into a GPU-friendly buffer.
///
/// Each bus is a vertical element placed at a line endpoint, lifted by voltage
/// or line load and encoded with transform, color, and scale data.
pub fn recompute_buses<F>(
    src: &[LineState],
    getter: F,
    d: &Domain,
    offset: glm::Vec3,
    color_band: f32,
    dest: &mut Vec<u8>,
    use_line_load: bool,
) where
    F: Fn(&LineState) -> LineGetterResult,
{
    log::debug!("Recompute buses {}", src.len());

    for state in src {
        let LineGetterResult {
            volt_start,
            volt_end,
            watt,
            vars,
            line_load,
        } = getter(state);

        // Determine the vertical height of each endpoint, based on either voltage or line load
        let (height_a, height_b) = if use_line_load {
            (
                d.line_load_to_height(line_load),
                d.line_load_to_height(line_load),
            )
        } else {
            (
                d.voltage_to_height(volt_start),
                d.voltage_to_height(volt_end),
            )
        };

        let p_a = glm::vec3(
            d.lerp_x(state.loc.sx as f32),
            height_a,
            d.lerp_y(state.loc.sy as f32),
        ) + offset;

        let p_b = glm::vec3(
            d.lerp_x(state.loc.ex as f32),
            height_b,
            d.lerp_y(state.loc.ey as f32),
        ) + offset;

        let v = p_b - p_a;
        let rot = roll_free_rotation(v.normalize());
        let rot_vec = rot.as_vector();

        let center = p_a;

        let width = d.real_power_to_width(watt);
        let height = 1.25 * d.reactive_power_to_width(vars);

        let safety = d.voltage_safety((volt_start + volt_end) / 2.0);
        let saturation = safety_to_saturation(safety);

        // Assign texture coords using a "color band" and safety-based saturation
        let texture = glm::vec2(color_band, saturation);

        // large tube to show tf bounds
        let mat = [
            center.x, center.y, center.z, 0.0, //
            texture.x, texture.y, 1.0, 1.0, //
            rot_vec.x, rot_vec.y, rot_vec.z, rot_vec.w, //
            width, height, width, 0.0, //
        ];

        dest.extend_from_slice(bytemuck::cast_slice(&mat));
    }
}

/// Converts a line state into a 4x4 matrix with color and orientation metadata.
///
/// This is used for generating line flow or voltage/power bar representations.
#[inline]
fn state_to_line<F, T, C>(
    state: &LineState,
    getter: &F,
    texture: T,
    mut callback: C,
    d: &Domain,
    offset: glm::Vec3,
    use_line_load: bool,
) -> Option<[f32; 16]>
where
    F: Fn(&LineState) -> LineGetterResult,
    T: Fn(&LineGetterResult, f32) -> glm::Vec4,
    C: FnMut(&LineGetterResult, glm::Vec3, glm::Vec3),
{
    let result = getter(state);
    let LineGetterResult {
        volt_start,
        volt_end,
        watt,
        vars,
        line_load,
    } = result;

    let (height_a, height_b) = if use_line_load {
        (
            d.line_load_to_height(line_load),
            d.line_load_to_height(line_load),
        )
    } else {
        (
            d.voltage_to_height(volt_start),
            d.voltage_to_height(volt_end),
        )
    };

    let p_a = glm::vec3(
        d.lerp_x(state.loc.sx as f32),
        height_a,
        d.lerp_y(state.loc.sy as f32),
    ) + offset;

    let p_b = glm::vec3(
        d.lerp_x(state.loc.ex as f32),
        height_b,
        d.lerp_y(state.loc.ey as f32),
    ) + offset;

    callback(&result, p_a, p_b);

    let mut v = p_b - p_a;

    // Flip flow direction based on power direction (flow = negative watt)
    if 0.0 > watt {
        v = -v;
    }

    let rot = roll_free_rotation(v.normalize());

    let center = (p_a + p_b) / 2.0;

    let watt_size = d.real_power_to_width(watt);
    let vars_size = d.reactive_power_to_width(vars);
    let rot_vec = rot.as_vector();

    if p_a.y < 0.000001 || p_b.y < 0.000001 {
        return None;
    }

    let texture = texture(&result, v.magnitude());

    // Construct instance matrix with position, texture info, rotation, and scale
    Some([
        center.x,
        center.y,
        center.z,
        0.0, // 3
        texture.x,
        texture.y,
        texture.z,
        texture.w, // 7
        rot_vec.x,
        rot_vec.y,
        rot_vec.z,
        rot_vec.w, // 11
        watt_size,
        vars_size,
        v.magnitude(),
        0.0, // 15
    ])
}

fn safety_to_saturation(v: VoltageSafety) -> f32 {
    match v {
        crate::domain::VoltageSafety::Safe => 0.5,
        crate::domain::VoltageSafety::Low => 0.2,
        crate::domain::VoltageSafety::High => 0.8,
    }
}

/// Detects hazard line intersections with horizontal voltage bands
///
/// This discretizes intersections and stores them for later instance creation.
struct HazardCheck {
    snap: f32,
    v_min_height: f32,
    v_max_height: f32,
    map_intersect: HashSet<(i32, i32, i32)>,
}

fn line_plane_intersection(a: glm::Vec3, b: glm::Vec3, plane_h: f32) -> Option<glm::Vec3> {
    let plane_normal: glm::Vec3 = glm::vec3(0.0, 1.0, 0.0);
    let plane_point = glm::vec3(0.0, plane_h, 0.0);

    let u = b - a;
    let dot = plane_normal.dot(&u);

    if dot.abs() < f32::EPSILON {
        return None;
    }

    let w = a - plane_point;
    let fac = -(plane_normal.dot(&w)) / dot;

    if !(0.0..1.0).contains(&fac) {
        return None;
    }

    let u = u * fac;

    Some(a + u)
}

impl HazardCheck {
    fn new(d: &Domain) -> Self {
        // we are scaling the data to a 2 meter square. we want X cells

        Self {
            snap: 2.0 / 20.0,
            v_min_height: d.voltage_to_height(0.95),
            v_max_height: d.voltage_to_height(1.05),
            map_intersect: Default::default(),
        }
    }

    fn check(&mut self, a: glm::Vec3, b: glm::Vec3) {
        // Snap point to grid and record whether it intersects upper or lower band

        if let Some(point) = line_plane_intersection(a, b, self.v_min_height) {
            let point: glm::IVec3 = glm::round(&(point / self.snap)).try_cast().unwrap();
            self.map_intersect.insert((point.x, point.z, 0));
        }

        if let Some(point) = line_plane_intersection(a, b, self.v_max_height) {
            let point: glm::IVec3 = glm::round(&(point / self.snap)).try_cast().unwrap();
            self.map_intersect.insert((point.x, point.z, 1));
        }
    }

    fn create_matrices(&self, dest: &mut Vec<u8>) {
        for &(x, y, level) in &self.map_intersect {
            let scale = glm::vec3(self.snap, 1.0, self.snap);

            let point = glm::vec3(
                x as f32 * self.snap,
                glm::mix_scalar(self.v_min_height, self.v_max_height, level as f32),
                y as f32 * self.snap,
            );

            let mat = [
                point.x, point.y, point.z, 0.0, //
                0.0, 0.0, 1.0, 1.0, //
                0.0, 0.0, 0.0, 1.0, //
                scale.x, scale.y, scale.z, 0.0, //
            ];

            dest.extend_from_slice(bytemuck::cast_slice(&mat));
        }
    }
}

/// Builds per-instance transforms for all power lines and detects hazard zones.
///
/// Outputs both instance matrices and, if applicable, intersection hazard boxes.
#[allow(clippy::too_many_arguments)]
pub fn recompute_lines<F>(
    src: &[LineState],
    getter: F,
    d: &Domain,
    offset: glm::Vec3,
    color_band: f32,
    dest: &mut Vec<u8>,
    hazard_parts: &mut Vec<u8>,
    line_load: bool,
) where
    F: Fn(&LineState) -> LineGetterResult,
{
    log::debug!("Recompute line {}", src.len());

    let mut checker = HazardCheck::new(d);

    for state in src.iter() {
        // Process each line, converting to instance data and checking for hazards

        let Some(matrix) = state_to_line(
            state,
            &getter,
            |st, _len| {
                let safety = d.voltage_safety((st.volt_start + st.volt_end) / 2.0);

                glm::vec4(color_band, safety_to_saturation(safety), 1.0, 1.0)
            },
            |_, a, b| {
                checker.check(a, b);
            },
            d,
            offset,
            line_load,
        ) else {
            continue;
        };

        dest.extend_from_slice(bytemuck::cast_slice(&matrix));
    }

    if !line_load {
        // Generate hazard geometry for intersections with voltage limits

        checker.create_matrices(hazard_parts);
    }
}

/// Creates low-lying "ground lines" that visually represent line topology on the ground.
pub fn recompute_gound_lines(src: &[LineState], d: &Domain, dest: &mut Vec<u8>) {
    log::debug!("Recompute ground line {}", src.len());

    for state in src {
        let p_a = glm::vec3(
            d.lerp_x(state.loc.sx as f32),
            0.0,
            d.lerp_y(state.loc.sy as f32),
        );

        let p_b = glm::vec3(
            d.lerp_x(state.loc.ex as f32),
            0.0,
            d.lerp_y(state.loc.ey as f32),
        );

        let mut v = p_b - p_a;

        if p_a.y > p_b.y {
            v = -v;
        }

        let rot = roll_free_rotation(v.normalize());
        let rot_vec = rot.as_vector();

        let center = (p_a + p_b) / 2.0;

        // Basic blue line with uniform thin tube and neutral transform
        let matrix: [f32; 16] = [
            center.x,
            center.y,
            center.z,
            0.0, // 3
            0.1,
            0.5,
            1.0,
            1.0, // 7
            rot_vec.x,
            rot_vec.y,
            rot_vec.z,
            rot_vec.w, // 11
            0.005,
            0.005,
            v.magnitude(),
            0.0, // 15
        ];

        dest.extend_from_slice(bytemuck::cast_slice(&matrix));
    }
}

/// Generates flowing visual instances based on power or voltage.
///
/// Encodes flow rate into texture scale and slight geometry padding for effect.
pub fn recompute_line_flows<F>(
    src: &[LineState],
    getter: F,
    domain: &Domain,
    offset: glm::Vec3,
    dest: &mut Vec<u8>,
    use_line_load: bool,
) where
    F: Fn(&LineState) -> LineGetterResult,
{
    log::debug!("Recompute line flows {}", src.len());

    for state in src {
        let Some(mut matrix) = state_to_line(
            state,
            &getter,
            |_, len| glm::vec4(0.0, 0.0, 30.0 * len, 1.0),
            |_, _, _| {},
            domain,
            offset,
            use_line_load,
        ) else {
            continue;
        };

        // Nudge tube width/height slightly for visual separation
        matrix[12] += 0.002;
        matrix[13] += 0.002;

        dest.extend_from_slice(bytemuck::cast_slice(&matrix));
    }
}

/// Builds transformer visual elements using height-based scaling.
///
/// Includes both the main transformer and a "link" tube to the baseline.
pub fn recompute_tfs<F>(
    src: &[TransformerState],
    getter: F,
    d: &Domain,
    offset: glm::Vec3,
    color_band: f32,
    dest: &mut Vec<u8>,
) where
    F: Fn(&TransformerState) -> TfGetterResult,
{
    log::debug!("Recompute tfs {}", src.len());
    for state in src {
        let TfGetterResult {
            volt_start,
            volt_end,
            tap: _,
            tap_change: _,
        } = getter(state);

        let p_a = glm::vec3(
            d.lerp_x(state.loc.sx as f32),
            d.voltage_to_height(volt_start),
            d.lerp_y(state.loc.sy as f32),
        ) + offset;

        let p_b = glm::vec3(
            d.lerp_x(state.loc.sx as f32),
            d.voltage_to_height(volt_end),
            d.lerp_y(state.loc.sy as f32),
        ) + offset;

        // log::debug!(
        //     "Recompute: {volt_start} {volt_end} {} {} {p_a} {p_b}",
        //     state.loc.sx,
        //     state.loc.sy
        // );

        //let v = p_b - p_a;

        // reverse?

        //let rot = roll_free_rotation(v.normalize());

        let center = (p_a + p_b) / 2.0;

        let height = (p_b.y - p_a.y).abs();

        if p_a.y < 0.000001 || p_b.y < 0.000001 {
            //log::debug!("SKIP TF");
            continue;
        }

        let texture = glm::vec2(color_band, 0.6);

        // First tube: transformer height bounds
        let mat = [
            center.x, center.y, center.z, 0.0, //
            texture.x, texture.y, 1.0, 1.0, //
            0.0, 0.0, 0.0, 1.0, //
            d.tube_max, height, d.tube_max, 0.0, //
        ];

        dest.extend_from_slice(bytemuck::cast_slice(&mat));

        // Second tube: thin baseline connection to floor
        let hx = d.volt_height_max - d.volt_height_min;

        // thinner tube to show tf to map
        let mat = [
            center.x,
            hx / 2.0,
            center.z,
            0.0, //
            texture.x,
            texture.y,
            1.0,
            1.0, //
            0.0,
            0.0,
            0.0,
            1.0, //
            d.tube_min,
            hx,
            d.tube_min,
            0.0, //
        ];

        dest.extend_from_slice(bytemuck::cast_slice(&mat));
    }
}

/// Builds generator instance transforms with voltage-aware height and width.
pub fn recompute_gens<F>(
    src: &[GeneratorState],
    getter: F,
    d: &Domain,
    offset: glm::Vec3,
    dest: &mut Vec<u8>,
    use_line_load: bool,
) where
    F: Fn(&GeneratorState) -> GeneratorGetterResult,
{
    log::debug!("Recompute gens {}", src.len());
    for state in src {
        let GeneratorGetterResult {
            voltage,
            angle: _,
            real,
            react,
            ty,
        } = getter(state);

        let height = if use_line_load {
            0.0
        } else {
            d.voltage_to_height(voltage)
        };

        let p_a = glm::vec3(
            d.lerp_x(state.loc.sx as f32),
            height,
            d.lerp_y(state.loc.sy as f32),
        ) + offset;

        let width = d.real_power_to_width(real.abs()) * 2.0;
        //let height = d.reactive_power_to_width(react.abs()) * 2.0;
        let height = width;

        let hue = match ty {
            GeneratorType::Unknown => 0.0,
            GeneratorType::Solar => 0.17,
            GeneratorType::Battery => 0.3125,
        };

        let sat = match ty {
            GeneratorType::Unknown => 1.0,
            _ => 0.5,
        };

        log::debug!("GEN {p_a:?} {real} {width} | {react} {height} | {hue} {sat}");

        let mat = [
            p_a.x, p_a.y, p_a.z, 0.0, //
            hue, sat, 1.0, 1.0, //
            0.0, 0.0, 0.0, 1.0, //
            width, height, width, 0.0, //
        ];

        dest.extend_from_slice(bytemuck::cast_slice(&mat));
    }
}
