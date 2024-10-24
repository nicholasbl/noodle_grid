use crate::{
    domain::{Domain, VoltageSafety},
    utility::roll_free_rotation,
    GeneratorState, LineState, TransformerState,
};

use nalgebra_glm as glm;

pub struct LineGetterResult {
    pub volt_start: f32,
    pub volt_end: f32,
    pub watt: f32,
    pub vars: f32,
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
}

pub fn recompute_buses<F>(
    src: &[LineState],
    getter: F,
    d: &Domain,
    offset: glm::Vec3,
    color_band: f32,
    dest: &mut Vec<u8>,
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
        } = getter(state);

        let p_a = glm::vec3(
            d.lerp_x(state.loc.sx as f32),
            d.voltage_to_height(volt_start),
            d.lerp_y(state.loc.sy as f32),
        ) + offset;

        let p_b = glm::vec3(
            d.lerp_x(state.loc.ex as f32),
            d.voltage_to_height(volt_end),
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

#[inline]
fn state_to_line<F, T>(
    state: &LineState,
    getter: &F,
    texture: T,
    d: &Domain,
    offset: glm::Vec3,
) -> Option<[f32; 16]>
where
    F: Fn(&LineState) -> LineGetterResult,
    T: Fn(&LineGetterResult, f32) -> glm::Vec4,
{
    let result = getter(state);
    let LineGetterResult {
        volt_start,
        volt_end,
        watt,
        vars,
    } = result;

    let p_a = glm::vec3(
        d.lerp_x(state.loc.sx as f32),
        d.voltage_to_height(volt_start),
        d.lerp_y(state.loc.sy as f32),
    ) + offset;

    let p_b = glm::vec3(
        d.lerp_x(state.loc.ex as f32),
        d.voltage_to_height(volt_end),
        d.lerp_y(state.loc.ey as f32),
    ) + offset;

    let mut v = p_b - p_a;

    if p_a.y > p_b.y {
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

pub fn recompute_lines<F>(
    src: &[LineState],
    getter: F,
    d: &Domain,
    offset: glm::Vec3,
    color_band: f32,
    dest: &mut Vec<u8>,
) where
    F: Fn(&LineState) -> LineGetterResult,
{
    log::debug!("Recompute line {}", src.len());

    for state in src {
        let Some(matrix) = state_to_line(
            state,
            &getter,
            |st, _len| {
                let safety = d.voltage_safety((st.volt_start + st.volt_end) / 2.0);

                return glm::vec4(color_band, safety_to_saturation(safety), 1.0, 1.0);
            },
            d,
            offset,
        ) else {
            continue;
        };

        dest.extend_from_slice(bytemuck::cast_slice(&matrix));
    }
}

pub fn recompute_line_flows<F>(
    src: &[LineState],
    getter: F,
    domain: &Domain,
    offset: glm::Vec3,
    dest: &mut Vec<u8>,
) where
    F: Fn(&LineState) -> LineGetterResult,
{
    log::debug!("Recompute line flows {}", src.len());

    for state in src {
        let Some(mut matrix) = state_to_line(
            state,
            &getter,
            |_, len| {
                return glm::vec4(0.0, 0.0, 30.0 * len, 1.0);
            },
            domain,
            offset,
        ) else {
            continue;
        };

        matrix[12] += 0.002;
        matrix[13] += 0.002;

        dest.extend_from_slice(bytemuck::cast_slice(&matrix));
    }
}

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

        // large tube to show tf bounds
        let mat = [
            center.x, center.y, center.z, 0.0, //
            texture.x, texture.y, 1.0, 1.0, //
            0.0, 0.0, 0.0, 1.0, //
            d.tube_max, height, d.tube_max, 0.0, //
        ];

        dest.extend_from_slice(bytemuck::cast_slice(&mat));

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

pub fn recompute_gens<F>(
    src: &[GeneratorState],
    getter: F,
    d: &Domain,
    offset: glm::Vec3,
    dest: &mut Vec<u8>,
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
        } = getter(state);

        let p_a = glm::vec3(
            d.lerp_x(state.loc.sx as f32),
            d.voltage_to_height(voltage),
            d.lerp_y(state.loc.sy as f32),
        ) + offset;

        let width = d.real_power_to_width(real.abs()) * 2.0;
        let height = d.reactive_power_to_width(react.abs()) * 2.0;

        log::debug!("GEN {p_a:?} {real} {width}");

        let mat = [
            p_a.x, p_a.y, p_a.z, 0.0, //
            0.25, 0.5, 1.0, 1.0, //
            0.0, 0.0, 0.0, 1.0, //
            width, height, width, 0.0, //
        ];

        dest.extend_from_slice(bytemuck::cast_slice(&mat));
    }
}
