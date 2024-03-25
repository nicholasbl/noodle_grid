use std::{
    ops::{Add, Div, Mul, Sub},
    sync::{Arc, Mutex},
};

use crate::{
    geometry::{make_cube, make_sphere},
    GeneratorState, LineState, PowerSystem, TransformerState,
};
use colabrodo_common::components::BufferState;
use colabrodo_server::{
    server::{self, *},
    server_messages::*,
};

use nalgebra_glm as glm;

#[inline]
fn lerp<T>(x: T, x0: T, x1: T, y0: T, y1: T) -> T
where
    T: Sub<Output = T> + Add<Output = T> + Div<Output = T> + Mul<Output = T> + Copy,
{
    y0 + (x - x0) * ((y1 - y0) / (x1 - x0))
}

#[inline]
fn clamped_lerp<T>(x: T, x0: T, x1: T, y0: T, y1: T) -> T
where
    T: Sub<Output = T>
        + Add<Output = T>
        + Div<Output = T>
        + Mul<Output = T>
        + Copy
        + Sized
        + PartialOrd,
{
    num_traits::clamp(lerp(x, x0, x1, y0, y1), y0, y1)
}

const VOLT_MIN: f32 = 108.0;
const VOLT_MAX: f32 = 132.0;

const TUBE_MIN: f32 = 0.001;
const TUBE_MAX: f32 = 0.015;

#[inline]
fn voltage_to_height(v: f32) -> f32 {
    clamped_lerp(v, VOLT_MIN, VOLT_MAX, 0.0, 1.5)
}

#[inline]
fn real_power_to_width(v: f32) -> f32 {
    clamped_lerp(v, 0.0, 704.0, TUBE_MIN, TUBE_MAX)
}

#[inline]
fn reactive_power_to_width(v: f32) -> f32 {
    clamped_lerp(v, 0.0, 704.0, TUBE_MIN, TUBE_MAX)
}

#[inline]
fn lerp_x(v: f32) -> f32 {
    lerp(v, 18401.9, 18694.2, -1.0, 1.0)
}

#[inline]
fn lerp_y(v: f32) -> f32 {
    lerp(v, -10117.4, -9818.12, -1.0, 1.0)
}

const PHASE_OFFSET: glm::Vec3 = glm::Vec3::new(0.001, 0.0, -0.001);

pub struct GridState {
    state: ServerStatePtr,

    system: PowerSystem,
    time_step: usize,
    max_time_step: usize,

    line_a_entity: EntityReference,
    line_b_entity: EntityReference,
    line_c_entity: EntityReference,
    gen_entity: EntityReference,

    line_a_geometry: GeometryReference,
    line_b_geometry: GeometryReference,
    line_c_geometry: GeometryReference,
    gen_geometry: GeometryReference,

    line_a_buffer: Vec<u8>,
    line_b_buffer: Vec<u8>,
    line_c_buffer: Vec<u8>,
    gen_buffer: Vec<u8>,
}

pub type GridStatePtr = Arc<Mutex<GridState>>;

impl GridState {
    pub fn new(state: ServerStatePtr, system: PowerSystem) -> GridStatePtr {
        let mut state_lock = state.lock().unwrap();

        // Create a cube
        let line_a_geometry = make_cube(&mut state_lock, glm::Vec3::new(1.0, 0.5, 0.5));
        let line_b_geometry = make_cube(&mut state_lock, glm::Vec3::new(0.5, 1.0, 0.5));
        let line_c_geometry = make_cube(&mut state_lock, glm::Vec3::new(0.5, 0.5, 1.0));

        let gen_geometry = make_sphere(&mut state_lock, glm::Vec3::new(1.0, 1.0, 1.0));

        let line_a_entity = state_lock.entities.new_component(ServerEntityState {
            name: Some("Lines A".to_string()),
            mutable: ServerEntityStateUpdatable {
                parent: None,
                transform: None,
                representation: Some(ServerEntityRepresentation::new_render(
                    ServerRenderRepresentation {
                        mesh: line_a_geometry.clone(),
                        instances: None,
                    },
                )),
                ..Default::default()
            },
        });
        let line_b_entity = state_lock.entities.new_component(ServerEntityState {
            name: Some("Lines B".to_string()),
            mutable: ServerEntityStateUpdatable {
                parent: None,
                transform: None,
                representation: Some(ServerEntityRepresentation::new_render(
                    ServerRenderRepresentation {
                        mesh: line_b_geometry.clone(),
                        instances: None,
                    },
                )),
                ..Default::default()
            },
        });
        let line_c_entity = state_lock.entities.new_component(ServerEntityState {
            name: Some("Lines C".to_string()),
            mutable: ServerEntityStateUpdatable {
                parent: None,
                transform: None,
                representation: Some(ServerEntityRepresentation::new_render(
                    ServerRenderRepresentation {
                        mesh: line_c_geometry.clone(),
                        instances: None,
                    },
                )),
                ..Default::default()
            },
        });

        let gen_entity = state_lock.entities.new_component(ServerEntityState {
            name: Some("Generator".to_string()),
            mutable: ServerEntityStateUpdatable {
                parent: None,
                transform: None,
                representation: Some(ServerEntityRepresentation::new_render(
                    ServerRenderRepresentation {
                        mesh: gen_geometry.clone(),
                        instances: None,
                    },
                )),
                ..Default::default()
            },
        });

        let ts_len = system.lines.len();

        log::info!("Loaded powersystem with {ts_len} timesteps");

        Arc::new(Mutex::new(GridState {
            state: state.clone(),
            system,
            time_step: (ts_len / 2).clamp(0, ts_len),
            max_time_step: ts_len,
            line_a_geometry,
            line_b_geometry,
            line_c_geometry,
            gen_geometry,
            line_a_buffer: vec![],
            line_b_buffer: vec![],
            line_c_buffer: vec![],
            gen_buffer: vec![],
            line_a_entity,
            line_b_entity,
            line_c_entity,
            gen_entity,
        }))
    }

    pub fn post_setup(state: &ServerStatePtr, app_state: &GridStatePtr) {
        let mut state_lock = state.lock().unwrap();
        let comp_set_time = state_lock
            .methods
            .new_owned_component(create_set_time(app_state.clone()));

        let comp_step_time = state_lock
            .methods
            .new_owned_component(create_step_time(app_state.clone()));

        state_lock.update_document(ServerDocumentUpdate {
            methods_list: Some(vec![comp_set_time, comp_step_time]),
            signals_list: None,
        })
    }
}

// fn roll_free_rotation(direction: Vec3) -> Quat {
//     let rot = {
//         let m1 = 0.0;
//         let m2 = direction.z / direction.x;
//         ((m1 - m2) / (1.0 + m1 * m2)).abs().atan()
//     };

//     let tilt = {
//         let yp = Vec3::new(0.0, 1.0, 0.0);
//         (PI / 2.0) - direction.dot(&yp).acos()
//     };

//     let y_unit = UnitVector3::new_normalize(Vec3::new(0.0, 1.0, 0.0));
//     let z_unit = UnitVector3::new_normalize(Vec3::new(0.0, 0.0, 1.0));

//     UnitQuaternion::from_axis_angle(&z_unit, tilt).quaternion()
//         + UnitQuaternion::from_axis_angle(&y_unit, rot).quaternion()
// }

fn roll_free_rotation(direction: glm::Vec3) -> glm::Quat {
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

struct LineGetterResult {
    volt_start: f32,
    volt_end: f32,
    watt: f32,
    vars: f32,
}

struct TfGetterResult {
    volt_start: f32,
    volt_end: f32,
    tap: i32,
    tap_change: i32,
}

struct GeneratorGetterResult {
    pub voltage: f32,
    pub angle: f32,
    pub real: f32,
    pub react: f32,
}

fn recompute_lines<F>(src: &[LineState], getter: F, offset: glm::Vec3, dest: &mut Vec<u8>)
where
    F: Fn(&LineState) -> LineGetterResult,
{
    for state in src {
        let LineGetterResult {
            volt_start,
            volt_end,
            watt,
            vars,
        } = getter(&state);

        let p_a = glm::vec3(
            lerp_x(state.loc.sx as f32),
            voltage_to_height(volt_start),
            lerp_y(state.loc.sy as f32),
        ) + offset;

        let p_b = glm::vec3(
            lerp_x(state.loc.ex as f32),
            voltage_to_height(volt_end),
            lerp_y(state.loc.ey as f32),
        ) + offset;

        let v = p_b - p_a;

        // reverse?

        let rot = roll_free_rotation(v.normalize());

        let center = (p_a + p_b) / 2.0;

        let watt_size = real_power_to_width(watt);
        let vars_size = reactive_power_to_width(vars);
        let rot_vec = rot.as_vector();

        if p_a.y < 0.000001 || p_b.y < 0.000001 {
            continue;
        }

        let mat = [
            center.x,
            center.y,
            center.z,
            1.0, //
            1.0,
            1.0,
            1.0,
            1.0, //
            rot_vec.x,
            rot_vec.y,
            rot_vec.z,
            rot_vec.w, //
            vars_size,
            watt_size,
            v.magnitude(),
            1.0, //
        ];

        dest.extend_from_slice(bytemuck::cast_slice(&mat));
    }
}

fn recompute_tfs<F>(src: &[TransformerState], getter: F, offset: glm::Vec3, dest: &mut Vec<u8>)
where
    F: Fn(&TransformerState) -> TfGetterResult,
{
    for state in src {
        let TfGetterResult {
            volt_start,
            volt_end,
            tap: _,
            tap_change: _,
        } = getter(&state);

        let p_a = glm::vec3(
            lerp_x(state.loc.sx as f32),
            voltage_to_height(volt_start),
            lerp_y(state.loc.sy as f32),
        ) + offset;

        let p_b = glm::vec3(
            lerp_x(state.loc.sx as f32),
            voltage_to_height(volt_end),
            lerp_y(state.loc.sy as f32),
        ) + offset;

        //let v = p_b - p_a;

        // reverse?

        //let rot = roll_free_rotation(v.normalize());

        let center = (p_a + p_b) / 2.0;

        let height = (p_b.y - p_a.y).abs();

        if p_a.y < 0.000001 || p_b.y < 0.000001 {
            continue;
        }

        let mat = [
            center.x, center.y, center.z, 1.0, //
            1.0, 1.0, 1.0, 1.0, //
            0.0, 0.0, 0.0, 1.0, //
            TUBE_MAX, height, TUBE_MAX, 1.0, //
        ];

        dest.extend_from_slice(bytemuck::cast_slice(&mat));

        let mat = [
            center.x, 0.75, center.z, 1.0, //
            1.0, 1.0, 1.0, 1.0, //
            0.0, 0.0, 0.0, 1.0, //
            TUBE_MIN, 1.5, TUBE_MIN, 1.0, //
        ];

        dest.extend_from_slice(bytemuck::cast_slice(&mat));
    }
}

fn recompute_gens<F>(src: &[GeneratorState], getter: F, offset: glm::Vec3, dest: &mut Vec<u8>)
where
    F: Fn(&GeneratorState) -> GeneratorGetterResult,
{
    for state in src {
        let GeneratorGetterResult {
            voltage,
            angle: _,
            real,
            react: _,
        } = getter(&state);

        let p_a = glm::vec3(
            lerp_x(state.loc.sx as f32),
            voltage_to_height(voltage),
            lerp_y(state.loc.sy as f32),
        ) + offset;

        //let v = p_b - p_a;

        // reverse?

        //let rot = roll_free_rotation(v.normalize());

        let width = real_power_to_width(real) * 2.0;

        let mat = [
            p_a.x, p_a.y, p_a.z, 1.0, //
            1.0, 1.0, 1.0, 1.0, //
            0.0, 0.0, 0.0, 1.0, //
            width, width, width, 1.0, //
        ];

        dest.extend_from_slice(bytemuck::cast_slice(&mat));
    }
}

pub fn recompute_all(gstate: &mut GridState, server_state: &mut ServerState) {
    gstate.line_a_buffer.clear();
    gstate.line_b_buffer.clear();
    gstate.line_c_buffer.clear();
    gstate.gen_buffer.clear();

    let line_ts = &gstate.system.lines[gstate.time_step];
    let tf_ts = &gstate.system.tfs[gstate.time_step];
    let gen_ts = &gstate.system.pvs[gstate.time_step];

    // ===

    recompute_lines(
        &line_ts,
        |s| LineGetterResult {
            volt_start: s.voltage.sa,
            volt_end: s.voltage.ea,
            watt: s.real_power.sa,
            vars: s.reactive_power.sa,
        },
        PHASE_OFFSET * 0.0,
        &mut gstate.line_a_buffer,
    );

    recompute_lines(
        &line_ts,
        |s| LineGetterResult {
            volt_start: s.voltage.sb,
            volt_end: s.voltage.eb,
            watt: s.real_power.sb,
            vars: s.reactive_power.sb,
        },
        PHASE_OFFSET * 1.0,
        &mut gstate.line_b_buffer,
    );

    recompute_lines(
        &line_ts,
        |s| LineGetterResult {
            volt_start: s.voltage.sc,
            volt_end: s.voltage.ec,
            watt: s.real_power.sc,
            vars: s.reactive_power.sc,
        },
        PHASE_OFFSET * 2.0,
        &mut gstate.line_c_buffer,
    );

    // ===

    recompute_tfs(
        &tf_ts,
        |s| TfGetterResult {
            volt_start: s.voltage.sa,
            volt_end: s.voltage.ea,
            tap: s.tap.a,
            tap_change: s.tap_changes.a,
        },
        PHASE_OFFSET * 0.0,
        &mut gstate.line_a_buffer,
    );

    recompute_tfs(
        &tf_ts,
        |s| TfGetterResult {
            volt_start: s.voltage.sb,
            volt_end: s.voltage.eb,
            tap: s.tap.b,
            tap_change: s.tap_changes.b,
        },
        PHASE_OFFSET * 1.0,
        &mut gstate.line_b_buffer,
    );

    recompute_tfs(
        &tf_ts,
        |s| TfGetterResult {
            volt_start: s.voltage.sc,
            volt_end: s.voltage.ec,
            tap: s.tap.c,
            tap_change: s.tap_changes.c,
        },
        PHASE_OFFSET * 2.0,
        &mut gstate.line_c_buffer,
    );

    // ===

    recompute_gens(
        &gen_ts,
        |s| GeneratorGetterResult {
            voltage: s.voltage.a,
            angle: s.angle.a,
            real: s.real,
            react: s.react,
        },
        PHASE_OFFSET * 0.0,
        &mut gstate.gen_buffer,
    );

    // ===

    update_buffers(
        server_state,
        &gstate.line_a_buffer,
        &gstate.line_a_geometry,
        &gstate.line_a_entity,
    );

    update_buffers(
        server_state,
        &gstate.line_b_buffer,
        &gstate.line_b_geometry,
        &gstate.line_b_entity,
    );

    update_buffers(
        server_state,
        &gstate.line_c_buffer,
        &gstate.line_c_geometry,
        &gstate.line_c_entity,
    );

    update_buffers(
        server_state,
        &gstate.gen_buffer,
        &gstate.gen_geometry,
        &gstate.gen_entity,
    );
}

fn update_buffers(
    lock: &mut ServerState,
    input_buffer: &Vec<u8>,
    geometry: &GeometryReference,
    entity: &EntityReference,
) {
    let line_buffer = lock
        .buffers
        .new_component(BufferState::new_from_bytes(input_buffer.clone()));

    let view = lock
        .buffer_views
        .new_component(ServerBufferViewState::new_from_whole_buffer(line_buffer));

    let update = ServerEntityStateUpdatable {
        representation: Some(ServerEntityRepresentation::new_render(
            ServerRenderRepresentation {
                mesh: geometry.clone(),
                instances: Some(ServerGeometryInstance {
                    view,
                    stride: None,
                    bb: None,
                }),
            },
        )),
        ..Default::default()
    };

    update.patch(&entity);
}

make_method_function!(set_time,
GridState,
"noo::set_time",
"Set the time of the visualization",
| time : Value : "Floating point time" |,
{
    let time : f32 = from_cbor(time).unwrap_or_default();
    let time : usize = time as usize;
    let time = time.clamp(0, app.max_time_step - 1);
    app.time_step = time;
    recompute_all(app, state);
    Ok(None)
});

make_method_function!(step_time,
GridState,
"noo::step_time",
"Advance the time of the visualization",
| time : Value : "Integer step direction" |,
{
    let time : i32 = from_cbor(time).unwrap_or_default();
    let time = (app.time_step as i32 + time).clamp(0, app.max_time_step as i32 - 1);

    log::debug!("Stepping time: {time}");

    app.time_step = time as usize;
    recompute_all(app, state);

    log::debug!("All done");
    Ok(None)
});
