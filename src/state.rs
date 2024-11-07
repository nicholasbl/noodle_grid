use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use crate::{
    basemap::make_basemap,
    domain::Domain,
    instance::*,
    instanced_item::{
        make_bus_element, make_generator_element, make_hazard_element, make_line_element,
        make_line_flow_element, make_transformer_element, InstancedItem,
    },
    ruler::make_ruler,
    texture::{make_chevron_texture, make_hsv_texture},
    PowerSystem,
};
use colabrodo_common::components::{BufferState, TextureRef};
use colabrodo_server::{server::*, server_messages::*};

use nalgebra_glm as glm;

/// To avoid overlap of phases (such as on transformers), we use a minute offset
const PHASE_OFFSET: glm::Vec3 = glm::Vec3::new(0.001, 0.0, -0.001);

pub struct GridState {
    state: ServerStatePtr,

    system: PowerSystem,
    time_step: usize,
    max_time_step: usize,

    domain: Domain,

    hazard: InstancedItem,

    _base_map: Option<EntityReference>,

    ruler: EntityReference,

    bus: InstancedItem,
    line: InstancedItem,
    line_flow: InstancedItem,
    transformer: InstancedItem,
    generator: InstancedItem,

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

        let flow_texture = make_chevron_texture(&mut state_lock);

        // build a material for line flow
        let line_flow_mat = state_lock.materials.new_component(ServerMaterialState {
            name: Some("Line Flow Material".into()),
            mutable: ServerMaterialStateUpdatable {
                pbr_info: Some(ServerPBRInfo {
                    base_color: [1.0, 1.0, 1.0, 1.0],
                    base_color_texture: Some(TextureRef {
                        texture: flow_texture,
                        transform: None,
                        texture_coord_slot: None,
                    }),
                    metallic: Some(0.0),
                    roughness: Some(0.2),
                    ..Default::default()
                }),
                use_alpha: Some(true),
                ..Default::default()
            },
        });

        // build a material for hazard blocks
        let hazard_mat = state_lock.materials.new_component(ServerMaterialState {
            name: None,
            mutable: ServerMaterialStateUpdatable {
                pbr_info: Some(ServerPBRInfo {
                    base_color: [0.0, 0.0, 1.0, 1.0],
                    metallic: Some(0.0),
                    roughness: Some(1.0),
                    ..Default::default()
                }),
                //use_alpha: Some(true),
                ..Default::default()
            },
        });

        let bus = make_bus_element(&mut state_lock, line_mat.clone());
        let line = make_line_element(&mut state_lock, line_mat.clone());
        let line_flow = make_line_flow_element(&mut state_lock, line_flow_mat);
        let transformer = make_transformer_element(&mut state_lock, line_mat);
        let generator = make_generator_element(&mut state_lock);
        let hazard = make_hazard_element(&mut state_lock, hazard_mat);

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

        //let (lower_hazard, upper_hazard) = make_hazard_planes(&mut state_lock, &domain);

        let base_map = make_basemap(&mut state_lock, &system, &domain);

        let ruler = make_ruler(&mut state_lock, &system, &domain);

        let ret = Arc::new(Mutex::new(GridState {
            state: state.clone(),
            system,
            time_step: (ts_len / 2).clamp(0, ts_len),
            max_time_step: ts_len,
            bus,
            line,
            line_flow,
            transformer,
            generator,
            domain,
            hazard,
            _base_map: base_map,
            ruler,
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

pub fn recompute_all(gstate: &mut GridState, server_state: &mut ServerState) {
    log::debug!("Recomputing all");
    gstate.bus.buffer.clear();
    gstate.line.buffer.clear();
    gstate.line_flow.buffer.clear();
    gstate.hazard.buffer.clear();
    gstate.transformer.buffer.clear();
    gstate.generator.buffer.clear();

    let line_ts = &gstate.system.lines[gstate.time_step];
    let tf_ts = &gstate.system.tfs[gstate.time_step];
    let gen_ts = &gstate.system.pvs[gstate.time_step];

    // ===

    const BAND_RED: f32 = 0.0;
    const BAND_GREEN: f32 = 0.33;
    const BAND_BLUE: f32 = 0.66;

    recompute_buses(
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
        &mut gstate.bus.buffer,
    );

    // ===

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
        &mut gstate.line.buffer,
        &mut gstate.hazard.buffer,
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
        &mut gstate.line.buffer,
        &mut gstate.hazard.buffer,
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
        &mut gstate.line.buffer,
        &mut gstate.hazard.buffer,
    );

    // ===

    recompute_line_flows(
        line_ts,
        |s| LineGetterResult {
            volt_start: s.voltage.sa,
            volt_end: s.voltage.ea,
            watt: s.real_power.average_a().abs(),
            vars: s.reactive_power.average_a().abs(),
        },
        &gstate.domain,
        PHASE_OFFSET * 0.0,
        //BAND_RED,
        &mut gstate.line_flow.buffer,
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
        &mut gstate.transformer.buffer,
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
        &mut gstate.transformer.buffer,
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
        &mut gstate.transformer.buffer,
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
        &mut gstate.generator.buffer,
    );

    // ===

    for element in [
        &gstate.bus,
        &gstate.line,
        &gstate.line_flow,
        &gstate.hazard,
        &gstate.transformer,
        &gstate.generator,
    ] {
        update_buffers(server_state, element);
    }
}

/// Update instances with a buffer
fn update_buffers(lock: &mut ServerState, element: &InstancedItem) {
    let line_buffer = lock
        .buffers
        .new_component(BufferState::new_from_bytes(element.buffer.clone()));

    let view = lock
        .buffer_views
        .new_component(ServerBufferViewState::new_from_whole_buffer(line_buffer));

    let update = ServerEntityStateUpdatable {
        representation: Some(ServerEntityRepresentation::new_render(
            ServerRenderRepresentation {
                mesh: element.geometry.clone(),
                instances: Some(ServerGeometryInstance {
                    view,
                    stride: None,
                    bb: None,
                }),
            },
        )),
        ..Default::default()
    };

    update.patch(&element.entity);
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
