use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

use crate::{
    basemap::make_basemap,
    domain::Domain,
    instance::*,
    instanced_item::{
        make_bus_element, make_generator_element, make_hazard_element, make_line_element,
        make_line_flow_element, make_transformer_element, InstancedItem,
    },
    methods::*,
    probe::Probe,
    ruler::{
        make_ruler,
        RulerType::{LineLoad, Voltage},
    },
    summary::SummaryItem,
    texture::{make_chevron_texture, make_hsv_texture},
    PowerSystem,
};

use colabrodo_common::components::{BufferState, TextureRef};
use colabrodo_server::{server::*, server_messages::*};

use nalgebra_glm::{self as glm};

/// Small offset used to visually separate phases (A/B/C) in space.
///
/// Prevents overlapping geometry like lines and transformers.
const PHASE_OFFSET: glm::Vec3 = glm::Vec3::new(0.001, 0.0, -0.001);

/// Core application state used for visualization logic and render management.
///
/// Holds all system data, instance containers, control signals, and configuration state.
pub struct GridState {
    pub state: ServerStatePtr,

    pub system: Arc<PowerSystem>,
    pub time_step: usize,
    pub time_step_direction: i32,
    pub max_time_step: usize,
    pub previous_show_line_load: bool,
    pub show_line_load: bool,

    pub domain: Domain,

    pub hazard: InstancedItem,

    _base_map: Option<EntityReference>,

    ruler: EntityReference,
    line_load_ruler: EntityReference,

    //pub axis_selector: VerticalAxisSelector,
    pub summary: SummaryItem,

    pub move_func: Option<MethodReference>,
    pub activate_func: Option<MethodReference>,

    pub probes: VecDeque<Probe>,

    bus: InstancedItem,
    line: InstancedItem,
    line_flow: InstancedItem,
    transformer: InstancedItem,
    generator: InstancedItem,

    pub active_timer: Option<tokio::sync::oneshot::Sender<bool>>,
    pub send_back: Option<tokio::sync::mpsc::Sender<bool>>,

    pub probe_move_request_signal: tokio::sync::mpsc::UnboundedSender<bool>,
}

pub type GridStatePtr = Arc<Mutex<GridState>>;

impl GridState {
    /// Initializes a new GridState with geometry, materials, and system configuration.
    ///
    /// Sets up all buffers, entities, materials, rulers, and spawns background tasks.
    pub fn new(state: ServerStatePtr, system: PowerSystem) -> GridStatePtr {
        let mut state_lock = state.lock().unwrap();

        // Load texture and build material for color-mapped lines
        let hsv_texture = make_hsv_texture(&mut state_lock);

        // Build a material for lines
        let line_mat = state_lock.materials.new_component(ServerMaterialState {
            name: Some("Line Material".into()),
            mutable: ServerMaterialStateUpdatable {
                pbr_info: Some(ServerPBRInfo {
                    base_color: [1.0, 1.0, 1.0, 1.0],
                    base_color_texture: Some(TextureRef {
                        texture: hsv_texture.clone(),
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

        // Load texture and material for animated flow lines
        let flow_texture = make_chevron_texture(&mut state_lock);

        // Build a material for line flow
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
                    base_color: [0.75, 0.75, 1.0, 0.9],
                    metallic: Some(0.0),
                    roughness: Some(1.0),
                    ..Default::default()
                }),
                use_alpha: Some(true),
                ..Default::default()
            },
        });

        // Create empty instanced geometry containers
        let bus = make_bus_element(&mut state_lock, line_mat.clone());
        let line = make_line_element(&mut state_lock, line_mat.clone());
        let line_flow = make_line_flow_element(&mut state_lock, line_flow_mat);
        let transformer = make_transformer_element(&mut state_lock, line_mat);
        let generator = make_generator_element(&mut state_lock, hsv_texture);
        let hazard = make_hazard_element(&mut state_lock, hazard_mat);

        let ts_len = system.lines.len();

        // determine bounding box
        let mut bounds_min = glm::DVec2::new(1E9, 1E9);
        let mut bounds_max = glm::DVec2::new(-1E9, -1E9);

        // Compute the bounding box of the power system based on all line endpoints
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

        // Optionally add floorplan and rulers to the scene
        let base_map = make_basemap(&mut state_lock, &system, &domain);

        let ruler = make_ruler(&mut state_lock, &domain, Voltage);
        let ruler_ll = make_ruler(&mut state_lock, &domain, LineLoad);

        let (probe_signal_tx, probe_signal_rx) = tokio::sync::mpsc::unbounded_channel::<bool>();

        let summary_item = SummaryItem::new(&system, &domain, &mut state_lock);

        // Construct shared GridState instance
        let ret = Arc::new(Mutex::new(GridState {
            state: state.clone(),
            system: Arc::new(system),
            time_step: (ts_len / 2).clamp(0, ts_len),
            time_step_direction: 0,
            max_time_step: ts_len,
            previous_show_line_load: false,
            show_line_load: false,
            bus,
            line,
            line_flow,
            transformer,
            generator,
            domain,
            hazard,
            _base_map: base_map,
            ruler,
            line_load_ruler: ruler_ll,
            summary: summary_item,
            move_func: None,
            activate_func: None,
            probes: Default::default(),
            active_timer: None,
            send_back: None,
            probe_move_request_signal: probe_signal_tx,
        }));

        // Spawn time-step advancement watcher
        {
            let (tx, rx) = tokio::sync::mpsc::channel(16);

            tokio::spawn(crate::methods::advance_watcher(ret.clone(), rx));

            let mut lock = ret.lock().unwrap();

            lock.send_back = Some(tx);
        }

        // Spawn probe update service (handles probe movement + chart generation)
        {
            tokio::spawn(crate::methods::probe_service(ret.clone(), probe_signal_rx));
        }

        ret
    }

    /// Registers method handlers with the server after initial setup.
    ///
    /// Attaches user-interactable methods like time controls and probe creation.
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

        let create_probe = state_lock
            .methods
            .new_owned_component(create_create_probe(app_state.clone()));

        let create_activate = state_lock
            .methods
            .new_owned_component(create_activate(app_state.clone()));

        let create_line_load = state_lock
            .methods
            .new_owned_component(create_toggle_line_load(app_state.clone()));

        // Register methods to be advertised by the server
        state_lock.update_document(ServerDocumentUpdate {
            methods_list: Some(vec![
                comp_set_time,
                comp_step_time,
                comp_adv_time,
                create_probe,
                create_line_load,
            ]),
            signals_list: None,
        });

        // Cache references to shared interaction methods
        let move_func = state_lock
            .methods
            .new_owned_component(create_set_position(app_state.clone()));

        {
            let mut app_lock = app_state.lock().unwrap();
            app_lock.move_func = Some(move_func);
            app_lock.activate_func = Some(create_activate);

            let time_frac = app_lock.time_frac();

            app_lock.summary.set_time_normalized(time_frac);
        }
    }

    /// Returns the current normalized time step (0.0 - 1.0).
    pub fn time_frac(&self) -> f32 {
        self.time_step as f32 / self.max_time_step as f32
    }
}

/// Recomputes all instance data and line visuals based on the current time step.
///
/// Clears all geometry buffers, processes per-phase instances, and updates visibility.
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

    // Toggle ruler visibility based on active data mode
    if gstate.show_line_load != gstate.previous_show_line_load {
        gstate.previous_show_line_load = gstate.show_line_load;

        let set_invisible = ServerEntityStateUpdatable {
            visible: Some(false),
            ..Default::default()
        };

        let set_visible = ServerEntityStateUpdatable {
            visible: Some(true),
            ..Default::default()
        };

        if gstate.show_line_load {
            set_invisible.patch(&gstate.ruler);
            set_visible.patch(&gstate.line_load_ruler);
        } else {
            set_visible.patch(&gstate.ruler);
            set_invisible.patch(&gstate.line_load_ruler);
        }
    }

    // ===

    const BAND_RED: f32 = 0.0;
    const BAND_GREEN: f32 = 0.33;
    const BAND_BLUE: f32 = 0.66;

    // Recompute bus indicators (for phase A)
    recompute_buses(
        line_ts,
        |s| LineGetterResult {
            volt_start: s.voltage.sa,
            volt_end: s.voltage.ea,
            watt: s.real_power.sa.abs(),
            vars: s.reactive_power.sa.abs(),
            line_load: s.line_load.a,
        },
        &gstate.domain,
        PHASE_OFFSET * 0.0,
        BAND_RED,
        &mut gstate.bus.buffer,
        gstate.show_line_load,
    );

    // Recompute main line visuals for all three phases

    // Phase A
    recompute_lines(
        line_ts,
        |s| LineGetterResult {
            volt_start: s.voltage.sa,
            volt_end: s.voltage.ea,
            watt: s.real_power.sa,
            vars: s.reactive_power.sa,
            line_load: s.line_load.a,
        },
        &gstate.domain,
        PHASE_OFFSET * 0.0,
        BAND_RED,
        &mut gstate.line.buffer,
        &mut gstate.hazard.buffer,
        gstate.show_line_load,
    );

    // Phase B
    recompute_lines(
        line_ts,
        |s| LineGetterResult {
            volt_start: s.voltage.sb,
            volt_end: s.voltage.eb,
            watt: s.real_power.sb,
            vars: s.reactive_power.sb,
            line_load: s.line_load.b,
        },
        &gstate.domain,
        PHASE_OFFSET * 1.0,
        BAND_GREEN,
        &mut gstate.line.buffer,
        &mut gstate.hazard.buffer,
        gstate.show_line_load,
    );

    // Phase C
    recompute_lines(
        line_ts,
        |s| LineGetterResult {
            volt_start: s.voltage.sc,
            volt_end: s.voltage.ec,
            watt: s.real_power.sc,
            vars: s.reactive_power.ec,
            line_load: s.line_load.c,
        },
        &gstate.domain,
        PHASE_OFFSET * 2.0,
        BAND_BLUE,
        &mut gstate.line.buffer,
        &mut gstate.hazard.buffer,
        gstate.show_line_load,
    );

    // Generate low-lying ground-level connections (topological view)

    recompute_gound_lines(line_ts, &gstate.domain, &mut gstate.line.buffer);

    // Recompute flowing animation indicators for phase A

    recompute_line_flows(
        line_ts,
        |s| LineGetterResult {
            volt_start: s.voltage.sa,
            volt_end: s.voltage.ea,
            watt: s.real_power.sa.abs(),
            vars: s.reactive_power.sa.abs(),
            line_load: s.line_load.a,
        },
        &gstate.domain,
        PHASE_OFFSET * 0.0,
        //BAND_RED,
        &mut gstate.line_flow.buffer,
        gstate.show_line_load,
    );

    // Recompute transformer visuals for phases A/B/C

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

    // Recompute generator visuals (single-phase)

    recompute_gens(
        gen_ts,
        |s| GeneratorGetterResult {
            voltage: s.voltage.a,
            angle: s.angle.a,
            real: s.real,
            react: s.react,
            ty: s.ty,
        },
        &gstate.domain,
        PHASE_OFFSET * 0.0,
        &mut gstate.generator.buffer,
        gstate.show_line_load,
    );

    // Upload all instance buffers to the GPU and patch renderables

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

/// Uploads instance buffer data to the GPU and applies it to the target entity.
///
/// Wraps the buffer in a view and patches the entity’s instance data.
fn update_buffers(lock: &mut ServerState, element: &InstancedItem) {
    // Allocate a GPU buffer for the new instance data
    let line_buffer = lock
        .buffers
        .new_component(BufferState::new_from_bytes(element.buffer.clone()));

    // Create a view of the entire buffer for instance binding
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
