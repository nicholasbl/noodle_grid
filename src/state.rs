use std::{
    ops::{Add, Div, Mul, Sub},
    sync::{Arc, Mutex},
    time::Duration,
};

use crate::{
    geometry::{make_cube, make_cyl, make_sphere},
    texture::make_hsv_texture,
    GeneratorState, LineState, PowerSystem, TransformerState,
};
use colabrodo_common::components::{BufferState, TextureRef};
use colabrodo_server::{server::*, server_messages::*};

use nalgebra_glm as glm;

/// Linear interpolation of a value between one range to an output range
#[inline]
fn lerp<T>(x: T, x0: T, x1: T, y0: T, y1: T) -> T
where
    T: Sub<Output = T> + Add<Output = T> + Div<Output = T> + Mul<Output = T> + Copy,
{
    y0 + (x - x0) * ((y1 - y0) / (x1 - x0))
}

/// Does the same as [`lerp`] but also clamps the output to the output range
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

/// Describes how to translate voltage and power to lengths and heights
#[derive(Debug)]
struct Domain {
    x_bounds: glm::DVec2,
    y_bounds: glm::DVec2,

    volt_height_min: f32,
    volt_height_max: f32,

    volt_min: f32,
    volt_max: f32,

    tube_min: f32,
    tube_max: f32,
}

impl Default for Domain {
    fn default() -> Self {
        Self {
            x_bounds: Default::default(),
            y_bounds: Default::default(),
            volt_height_min: 0.0,
            volt_height_max: 1.5,
            volt_min: 0.9,
            volt_max: 1.1,
            tube_min: 0.001,
            tube_max: 0.03,
        }
    }
}

impl Domain {
    fn new(bound_min: glm::DVec2, bound_max: glm::DVec2) -> Self {
        let range = bound_max - bound_min;
        let max_dim = glm::DVec2::repeat(range.max() / 2.0);
        let center = (bound_min + bound_max) / 2.0;

        let nl = center - max_dim;
        let nh = center + max_dim;

        Self {
            x_bounds: glm::DVec2::new(nl.x, nh.x),
            y_bounds: glm::DVec2::new(nl.y, nh.y),
            ..Default::default()
        }
    }

    #[inline]
    fn voltage_to_height(&self, v: f32) -> f32 {
        clamped_lerp(
            v,
            self.volt_min,
            self.volt_max,
            self.volt_height_min,
            self.volt_height_max,
        )
    }

    #[inline]
    fn real_power_to_width(&self, v: f32) -> f32 {
        clamped_lerp(v, 0.0, 704.0, self.tube_min, self.tube_max)
    }

    #[inline]
    fn reactive_power_to_width(&self, v: f32) -> f32 {
        clamped_lerp(v, 0.0, 704.0, self.tube_min, self.tube_max)
    }

    #[inline]
    fn lerp_x(&self, v: f32) -> f32 {
        lerp(
            v as f64,
            self.x_bounds.x,
            self.x_bounds.y,
            -1.0_f64,
            1.0_f64,
        ) as f32
    }

    #[inline]
    fn lerp_y(&self, v: f32) -> f32 {
        lerp(
            v as f64,
            self.y_bounds.x,
            self.y_bounds.y,
            1.0_f64, // flip for now
            -1.0_f64,
        ) as f32
    }
}

/// To avoid overlap of phases (such as on transformers), we use a minute offset
const PHASE_OFFSET: glm::Vec3 = glm::Vec3::new(0.001, 0.0, -0.001);

pub struct GridState {
    state: ServerStatePtr,

    system: PowerSystem,
    time_step: usize,
    max_time_step: usize,

    domain: Domain,

    lower_hazard: EntityReference,
    upper_hazard: EntityReference,

    line_entity: EntityReference,
    transformer_entity: EntityReference,
    gen_entity: EntityReference,

    line_geometry: GeometryReference,
    transformer_geometry: GeometryReference,
    gen_geometry: GeometryReference,

    line_buffer: Vec<u8>,
    transformer_buffer: Vec<u8>,
    gen_buffer: Vec<u8>,

    active_timer: Option<tokio::sync::oneshot::Sender<bool>>,
    send_back: Option<tokio::sync::mpsc::Sender<bool>>,
}

pub type GridStatePtr = Arc<Mutex<GridState>>;

impl GridState {
    pub fn new(state: ServerStatePtr, system: PowerSystem) -> GridStatePtr {
        let mut state_lock = state.lock().unwrap();

        // load color texture for instances
        let texture = make_hsv_texture(&mut state_lock);

        // build a material for lines
        let line_mat = state_lock.materials.new_component(ServerMaterialState {
            name: Some("Line Material".into()),
            mutable: ServerMaterialStateUpdatable {
                pbr_info: Some(ServerPBRInfo {
                    base_color: [1.0, 1.0, 1.0, 1.0],
                    base_color_texture: Some(TextureRef {
                        texture,
                        transform: None,
                        texture_coord_slot: None,
                    }),
                    metallic: Some(1.0),
                    roughness: Some(0.5),
                    ..Default::default()
                }),
                ..Default::default()
            },
        });

        // Create geometry for the lines
        let line_geometry = make_cube(&mut state_lock, glm::identity(), line_mat.clone());

        // Create geometry for the tfs
        let transformer_geometry = make_cyl(&mut state_lock, glm::identity(), line_mat.clone());

        // Create geometry for the generators
        let gen_geometry = make_sphere(&mut state_lock, glm::Vec3::new(1.0, 1.0, 0.0));

        // Create an entity to render the lines
        let line_entity = state_lock.entities.new_component(ServerEntityState {
            name: Some("Lines".to_string()),
            mutable: ServerEntityStateUpdatable {
                parent: None,
                transform: None,
                representation: Some(ServerEntityRepresentation::new_render(
                    ServerRenderRepresentation {
                        mesh: line_geometry.clone(),
                        instances: None,
                    },
                )),
                ..Default::default()
            },
        });

        // Create an entity to render the tfs
        let transformer_entity = state_lock.entities.new_component(ServerEntityState {
            name: Some("Transformers".to_string()),
            mutable: ServerEntityStateUpdatable {
                parent: None,
                transform: None,
                representation: Some(ServerEntityRepresentation::new_render(
                    ServerRenderRepresentation {
                        mesh: line_geometry.clone(),
                        instances: None,
                    },
                )),
                ..Default::default()
            },
        });

        // Create an entity to render the gens
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

        // determine bounding box
        let mut bounds_min = glm::DVec2::new(1E9, 1E9);
        let mut bounds_max = glm::DVec2::new(-1E9, -1E9);

        for time_step in &system.lines {
            for line in time_step {
                let pa = glm::DVec2::new(line.loc.sx, line.loc.sy);
                let pb = glm::DVec2::new(line.loc.ex, line.loc.ey);

                bounds_min = glm::min2(&glm::min2(&bounds_min, &pa), &pb);
                bounds_max = glm::max2(&glm::max2(&bounds_max, &pa), &pb);
            }
        }

        let domain = Domain::new(bounds_min, bounds_max);

        log::info!("Loaded powersystem with {ts_len} timesteps");
        log::info!("Bounds {bounds_min:?} {bounds_max:?}");
        log::info!("Domain {domain:?}");

        // set up hazard planes
        let lower_hazard_coord = glm::vec3(0.0, domain.voltage_to_height(0.95), 0.0);
        let upper_hazard_coord = glm::vec3(0.0, domain.voltage_to_height(1.05), 0.0);

        let hazard_mat = state_lock.materials.new_component(ServerMaterialState {
            name: None,
            mutable: ServerMaterialStateUpdatable {
                pbr_info: Some(ServerPBRInfo {
                    base_color: [1.0, 1.0, 1.0, 0.75],
                    metallic: Some(0.0),
                    roughness: Some(1.0),
                    ..Default::default()
                }),
                use_alpha: Some(true),
                ..Default::default()
            },
        });

        let hazard_geom = make_cube(
            &mut state_lock,
            glm::scaling(&glm::vec3(2.0, 0.01, 2.0)),
            hazard_mat,
        );

        let lower_hazard_entity = state_lock.entities.new_component(ServerEntityState {
            name: Some("Lower Voltage Hazard".into()),
            mutable: ServerEntityStateUpdatable {
                parent: None,
                transform: Some(
                    glm::translation(&lower_hazard_coord)
                        .data
                        .as_slice()
                        .try_into()
                        .unwrap(),
                ),
                representation: Some(ServerEntityRepresentation::new_render(
                    ServerRenderRepresentation {
                        mesh: hazard_geom.clone(),
                        instances: None,
                    },
                )),
                ..Default::default()
            },
        });

        let upper_hazard_entity = state_lock.entities.new_component(ServerEntityState {
            name: Some("Upper Voltage Hazard".into()),
            mutable: ServerEntityStateUpdatable {
                parent: None,
                transform: Some(
                    glm::translation(&upper_hazard_coord)
                        .data
                        .as_slice()
                        .try_into()
                        .unwrap(),
                ),
                representation: Some(ServerEntityRepresentation::new_render(
                    ServerRenderRepresentation {
                        mesh: hazard_geom,
                        instances: None,
                    },
                )),
                ..Default::default()
            },
        });

        let ret = Arc::new(Mutex::new(GridState {
            state: state.clone(),
            system,
            time_step: (ts_len / 2).clamp(0, ts_len),
            max_time_step: ts_len,
            line_geometry,
            transformer_geometry,
            gen_geometry,
            line_buffer: vec![],
            transformer_buffer: vec![],
            gen_buffer: vec![],
            line_entity,
            transformer_entity,
            gen_entity,
            domain,
            lower_hazard: lower_hazard_entity,
            upper_hazard: upper_hazard_entity,
            active_timer: None,
            send_back: None,
        }));

        {
            let (tx, rx) = tokio::sync::mpsc::channel(16);

            tokio::spawn(advance_watcher(ret.clone(), rx));

            let mut lock = ret.lock().unwrap();

            lock.send_back = Some(tx);
        }

        ret
    }

    pub fn post_setup(state: &ServerStatePtr, app_state: &GridStatePtr) {
        let mut state_lock = state.lock().unwrap();
        let comp_set_time = state_lock
            .methods
            .new_owned_component(create_set_time(app_state.clone()));

        let comp_step_time = state_lock
            .methods
            .new_owned_component(create_step_time(app_state.clone()));

        let comp_adv_time = state_lock
            .methods
            .new_owned_component(create_play_time(app_state.clone()));

        state_lock.update_document(ServerDocumentUpdate {
            methods_list: Some(vec![comp_set_time, comp_step_time, comp_adv_time]),
            signals_list: None,
        })
    }
}

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

fn recompute_lines<F>(
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

        // reverse?

        let rot = roll_free_rotation(v.normalize());

        let center = (p_a + p_b) / 2.0;

        let watt_size = d.real_power_to_width(watt);
        let vars_size = d.reactive_power_to_width(vars);
        let rot_vec = rot.as_vector();

        if p_a.y < 0.000001 || p_b.y < 0.000001 {
            //log::debug!("SKIP {volt_start} {volt_end} {} {}", p_a.y, p_b.y);
            continue;
        }

        let texture = glm::vec2(color_band, 0.5);
        //let texture = glm::vec2(0.5, 0.5);
        //log::info!("TEX {texture:?}");

        let mat = [
            center.x,
            center.y,
            center.z,
            texture.x, //
            1.0,
            1.0,
            1.0,
            1.0, //
            rot_vec.x,
            rot_vec.y,
            rot_vec.z,
            rot_vec.w, //
            watt_size,
            vars_size,
            v.magnitude(),
            texture.y, //
        ];

        //log::info!("NEW MAT {mat:?}");

        dest.extend_from_slice(bytemuck::cast_slice(&mat));
    }
}

fn recompute_tfs<F>(
    src: &[TransformerState],
    getter: F,
    d: &Domain,
    offset: glm::Vec3,
    color_band: f32,
    dest: &mut Vec<u8>,
) where
    F: Fn(&TransformerState) -> TfGetterResult,
{
    //log::debug!("Recompute tfs {}", src.len());
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

        let texture = glm::vec2(color_band, 0.85);

        // large tube to show tf bounds
        let mat = [
            center.x, center.y, center.z, texture.x, //
            1.0, 1.0, 1.0, 1.0, //
            0.0, 0.0, 0.0, 1.0, //
            d.tube_max, height, d.tube_max, texture.y, //
        ];

        dest.extend_from_slice(bytemuck::cast_slice(&mat));

        let hx = d.volt_height_max - d.volt_height_min;

        // thinner tube to show tf to map
        let mat = [
            center.x,
            hx / 2.0,
            center.z,
            texture.x, //
            1.0,
            1.0,
            1.0,
            1.0, //
            0.0,
            0.0,
            0.0,
            1.0, //
            d.tube_min,
            hx,
            d.tube_min,
            texture.y, //
        ];

        dest.extend_from_slice(bytemuck::cast_slice(&mat));
    }
}

fn recompute_gens<F>(
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
            p_a.x, p_a.y, p_a.z, 0.25, //
            1.0, 1.0, 1.0, 1.0, //
            0.0, 0.0, 0.0, 1.0, //
            width, height, width, 0.5, //
        ];

        dest.extend_from_slice(bytemuck::cast_slice(&mat));
    }
}

pub fn recompute_all(gstate: &mut GridState, server_state: &mut ServerState) {
    log::debug!("Recomputing all");
    gstate.line_buffer.clear();
    gstate.transformer_buffer.clear();
    gstate.gen_buffer.clear();

    let line_ts = &gstate.system.lines[gstate.time_step];
    let tf_ts = &gstate.system.tfs[gstate.time_step];
    let gen_ts = &gstate.system.pvs[gstate.time_step];

    // ===

    const BAND_RED: f32 = 0.0;
    const BAND_GREEN: f32 = 0.33;
    const BAND_BLUE: f32 = 0.66;

    recompute_lines(
        line_ts,
        |s| LineGetterResult {
            volt_start: s.voltage.sa,
            volt_end: s.voltage.ea,
            watt: s.real_power.average_a().abs(),
            vars: s.reactive_power.average_a().abs(),
        },
        &gstate.domain,
        PHASE_OFFSET * 0.0,
        BAND_RED,
        &mut gstate.line_buffer,
    );

    recompute_lines(
        line_ts,
        |s| LineGetterResult {
            volt_start: s.voltage.sb,
            volt_end: s.voltage.eb,
            watt: s.real_power.average_b().abs(),
            vars: s.reactive_power.average_b().abs(),
        },
        &gstate.domain,
        PHASE_OFFSET * 1.0,
        BAND_GREEN,
        &mut gstate.line_buffer,
    );

    recompute_lines(
        line_ts,
        |s| LineGetterResult {
            volt_start: s.voltage.sc,
            volt_end: s.voltage.ec,
            watt: s.real_power.average_c().abs(),
            vars: s.reactive_power.average_c().abs(),
        },
        &gstate.domain,
        PHASE_OFFSET * 2.0,
        BAND_BLUE,
        &mut gstate.line_buffer,
    );

    // ===

    recompute_tfs(
        tf_ts,
        |s| TfGetterResult {
            volt_start: s.voltage.sa,
            volt_end: s.voltage.ea,
            tap: s.tap.a,
            tap_change: s.tap_changes.a,
        },
        &gstate.domain,
        PHASE_OFFSET * 0.0,
        BAND_RED,
        &mut gstate.transformer_buffer,
    );

    recompute_tfs(
        tf_ts,
        |s| TfGetterResult {
            volt_start: s.voltage.sb,
            volt_end: s.voltage.eb,
            tap: s.tap.b,
            tap_change: s.tap_changes.b,
        },
        &gstate.domain,
        PHASE_OFFSET * 1.0,
        BAND_GREEN,
        &mut gstate.transformer_buffer,
    );

    recompute_tfs(
        tf_ts,
        |s| TfGetterResult {
            volt_start: s.voltage.sc,
            volt_end: s.voltage.ec,
            tap: s.tap.c,
            tap_change: s.tap_changes.c,
        },
        &gstate.domain,
        PHASE_OFFSET * 2.0,
        BAND_BLUE,
        &mut gstate.transformer_buffer,
    );

    // ===

    recompute_gens(
        gen_ts,
        |s| GeneratorGetterResult {
            voltage: s.voltage.a,
            angle: s.angle.a,
            real: s.real,
            react: s.react,
        },
        &gstate.domain,
        PHASE_OFFSET * 0.0,
        &mut gstate.gen_buffer,
    );

    // ===

    update_buffers(
        server_state,
        &gstate.line_buffer,
        &gstate.line_geometry,
        &gstate.line_entity,
    );

    update_buffers(
        server_state,
        &gstate.transformer_buffer,
        &gstate.transformer_geometry,
        &gstate.transformer_entity,
    );

    update_buffers(
        server_state,
        &gstate.gen_buffer,
        &gstate.gen_geometry,
        &gstate.gen_entity,
    );
}

/// Update instances with a buffer
fn update_buffers(
    lock: &mut ServerState,
    input_buffer: &[u8],
    geometry: &GeometryReference,
    entity: &EntityReference,
) {
    let line_buffer = lock
        .buffers
        .new_component(BufferState::new_from_bytes(input_buffer.to_owned()));

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

    update.patch(entity);
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

async fn advance_timer(
    send_back: tokio::sync::mpsc::Sender<bool>,
    mut to_stop: tokio::sync::oneshot::Receiver<bool>,
) {
    loop {
        log::debug!("Advancer");
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs_f32(0.25)) => {
                log::debug!("Sleep done");
                if send_back.send(true).await.is_err() {
                    log::debug!("closing advance timer");
                    return
                }
            },
            _ = &mut to_stop => {
                log::debug!("closing advance timer");
                return
            }
        }
    }
}

fn check_launch_timer(gs: &mut GridState, start_timer: bool) {
    if gs.active_timer.is_some() {
        if start_timer {
            // already have timer going. skip
        } else {
            // timer going and we want to stop. issue stop.
            log::debug!("Issuing stop");
            let sender = gs.active_timer.take().unwrap();
            let _ = sender.send(true);
        }
    } else if start_timer {
        // timer is not running and they want one. start;
        log::debug!("Launching player");
        let (os_tx, os_rx) = tokio::sync::oneshot::channel();

        gs.active_timer = Some(os_tx);
        let send_back = gs.send_back.clone().unwrap();

        tokio::spawn(advance_timer(send_back, os_rx));
    } else {
        // timer not running and they want a stop. skip
    }
    log::debug!("Check launch done");
}

make_method_function!(play_time,
GridState,
"noo::animate_time",
"Play the visualization",
| time : Value : "Integer step direction" |,
{
    let time : i32 = from_cbor(time).unwrap_or_default();
    let time = time.clamp(0, 1) == 1;

    log::debug!("Asking to play time: {time}");

    check_launch_timer(app, time);
    Ok(None)
});

async fn advance_watcher(gs: GridStatePtr, mut rx: tokio::sync::mpsc::Receiver<bool>) {
    while rx.recv().await.is_some() {
        log::debug!("advancing time");
        let mut lock = gs.lock().unwrap();
        lock.time_step = (lock.time_step + 1) % lock.max_time_step;

        let ss_arc = lock.state.clone();
        let mut ss_lock = ss_arc.lock().unwrap();

        recompute_all(&mut lock, &mut ss_lock);
    }
}
