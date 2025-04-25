use std::collections::HashMap;

use colabrodo_common::components::*;
use colabrodo_common::nooid::EntityID;
use colabrodo_server::{server::*, server_messages::*};
use nalgebra::distance;
use nalgebra_glm::{self as glm, vec3, Mat4, Vec2};
use nalgebra_glm::{vec2, Vec3};

use crate::geometry::{make_plane, make_sphere};
use crate::state::GridStatePtr;
use crate::texture::texture_from_bytes;
use crate::GridState;
use crate::{chart::*, ruler::make_obj};

/// Represents a movable probe in the visualization space.
///
/// Probes can attach to nearby lines, generate charts, and be interactively manipulated.
pub struct Probe {
    pub entity: EntityReference,
    pub world_pos: Vec3,
    pub dirty: Option<Vec3>, // The user has asked to move this probe to this position

    pub handle: Option<EntityReference>,

    pub chart: Option<EntityReference>,
    pub chart_delete: Option<EntityReference>,
    pub line_i: usize,
}

impl Probe {
    /// Creates a new probe with default uninitialized state.
    pub fn new(entity: EntityReference) -> Self {
        Self {
            entity,
            world_pos: glm::vec3(0.0, 0.0, 0.0),
            dirty: None,
            //pending_chart: None,
            handle: None,
            chart: None,
            chart_delete: None,
            line_i: usize::MAX,
        }
    }

    /// Installs a draggable handle entity for this probe.
    ///
    /// Allows users to reposition the associated chart interactively.
    pub fn install_handle(&mut self, gs: &mut GridState, state: &mut ServerState) {
        let geometry = make_sphere(state, glm::vec3(0.0, 0.0, 1.0), 0.05);

        // Position the handle slightly above and offset from the probe's world position
        let placement: [f32; 16] = {
            let spot: Vec3 = self.world_pos;
            let tf = glm::translation(&(spot + glm::vec3(0.25, 1.0, 0.0)));
            tf.as_slice().try_into().unwrap()
        };

        self.handle = Some(state.entities.new_component(ServerEntityState {
            name: Some("Chart Handle".to_string()),
            mutable: ServerEntityStateUpdatable {
                parent: None,
                transform: Some(placement),
                representation: Some(ServerEntityRepresentation::new_render(
                    ServerRenderRepresentation {
                        mesh: geometry,
                        instances: None,
                    },
                )),
                methods_list: Some(vec![gs.move_func.clone().unwrap()]),
                ..Default::default()
            },
        }));
    }

    /// Finds the closest line segment to the probe in 2D space.
    ///
    /// Returns both the index and closest point on the line.
    fn get_closest_line(&self, gs: &mut GridState) -> Option<(usize, Vec2)> {
        let lines = gs.system.lines.get(gs.time_step)?;

        let domain = &gs.domain;

        let p = self.world_pos.xz();

        // Track minimum distance and closest line index
        let mut min_distance = f32::INFINITY;
        let mut index: usize = usize::MAX;
        let mut closest_point = vec2(0.0, 0.0);

        for (l_i, l) in lines.iter().enumerate() {
            let a = glm::vec2(
                domain.lerp_x(l.loc.sx as f32),
                domain.lerp_y(l.loc.sy as f32),
            );

            let b = glm::vec2(
                domain.lerp_x(l.loc.ex as f32),
                domain.lerp_y(l.loc.ey as f32),
            );

            let ap = p - a;
            let ab = b - a;

            let t = ap.dot(&ab) / ab.dot(&ab);
            let c = a + t * ab;

            // Update closest if this segment is nearer
            let this_distance = if t < 0.0 {
                distance(&p.into(), &a.into())
            } else if t > 1.0 {
                distance(&p.into(), &b.into())
            } else {
                distance(&p.into(), &c.into())
            };

            if this_distance < min_distance {
                min_distance = this_distance;
                index = l_i;
                closest_point = c;
            }
        }

        if index == usize::MAX {
            None
        } else {
            Some((index, closest_point))
        }
    }

    /// Installs a floating chart billboard above the probe.
    ///
    /// Generates geometry, texture, and parent-child relationships.
    fn install_chart(&mut self, gs: &mut GridState, state: &mut ServerState, new_image: Vec<u8>) {
        log::debug!("Generating chart for {}", self.line_i);

        let chart_gen_timer = std::time::Instant::now();
        let tex = texture_from_bytes(state, &new_image, "Voltage for Line");
        log::debug!("Tex: {}", chart_gen_timer.elapsed().as_millis());

        // Create material using the chart texture
        let chart_mat = state.materials.new_component(ServerMaterialState {
            name: Some("Chart Material".into()),
            mutable: ServerMaterialStateUpdatable {
                pbr_info: Some(ServerPBRInfo {
                    base_color: [1.0, 1.0, 1.0, 1.0],
                    base_color_texture: Some(TextureRef {
                        texture: tex,
                        transform: None,
                        texture_coord_slot: None,
                    }),
                    metallic: Some(0.0),
                    roughness: Some(1.0),
                    ..Default::default()
                }),
                ..Default::default()
            },
        });

        // Create a plane rotated to face the user, scaled for aspect ratio
        let geometry = {
            let transform = glm::rotate_x(&Mat4::identity(), 90.0f32.to_radians());
            let transform = glm::scale(&transform, &glm::vec3(0.5, 1.0, 0.4));
            make_plane(state, transform, chart_mat)
        };

        let placement: [f32; 16] = {
            let tf = glm::translation(&glm::vec3(0.0, 0.25, 0.0));
            tf.as_slice().try_into().unwrap()
        };

        // Ensure we have a handle to attach the chart to
        if self.handle.is_none() {
            self.install_handle(gs, state);
        }

        let entity = state.entities.new_component(ServerEntityState {
            name: Some(format!("Chart for {}", self.line_i)),
            mutable: ServerEntityStateUpdatable {
                parent: Some(self.handle.clone().unwrap()),
                transform: Some(placement),
                representation: Some(ServerEntityRepresentation::new_render(
                    ServerRenderRepresentation {
                        mesh: geometry,
                        instances: None,
                    },
                )),
                billboard: Some(true),
                ..Default::default()
            },
        });

        self.chart = Some(entity);

        // now install the delete button
        self.install_delete_buttion(gs, state);
    }

    /// Adds a delete button next to the chart, allowing user to remove the probe.
    fn install_delete_buttion(&mut self, gs: &mut GridState, state: &mut ServerState) {
        let del_obj = make_obj(
            state,
            "Delete Button",
            [1.0, 0.2, 0.2, 1.0],
            glm::vec3(0.025, 0.025, 0.025),
            glm::vec3(0.25, 0.25, 0.0),
            self.chart.clone(),
            include_str!("../assets/close.obj"),
        );

        // Patch delete button with an activation method (click-to-delete)
        let patch = ServerEntityStateUpdatable {
            methods_list: Some(vec![gs.activate_func.clone().unwrap()]),
            ..Default::default()
        };

        patch.patch(&del_obj);

        self.chart_delete = Some(del_obj);
    }

    /// Updates the probe's world position and reattaches it to the closest line.
    ///
    /// If the attached line changes, resets internal reference.
    pub fn update(&mut self, gs: &mut GridState) {
        log::debug!("Updating probe {:?}", self.dirty);
        // new position
        self.world_pos = self.dirty.unwrap();
        self.dirty = None;

        // find the closest line (for now)

        let Some((closest_line_index, closest_point)) = self.get_closest_line(gs) else {
            // make sure it is at least seated to the ground
            move_entity(&self.entity, self.world_pos);
            return;
        };

        // use closest point to move our probe over

        move_entity(&self.entity, vec3(closest_point.x, 0.0f32, closest_point.y));

        // If already attached to correct line, no change needed
        if self.line_i == closest_line_index {
            return;
        }

        self.line_i = closest_line_index;
    }

    /// Checks if a clicked entity corresponds to this probe's delete button.
    ///
    /// Returns a `ClickResult` if matched.
    pub fn check_click(&self, entity: &EntityReference) -> Option<ClickResult> {
        if entity.id()
            == self
                .chart_delete
                .as_ref()
                .map(|f| f.id())
                .unwrap_or_default()
        {
            return Some(ClickResult::Delete);
        }

        None
    }
}

/// Possible outcomes of clicking on probe-related UI elements.
pub enum ClickResult {
    Delete,
}

/// Moves an entity to a new 3D world position.
///
/// Updates its transform immediately.
fn move_entity(entity: &EntityReference, pos: Vec3) {
    let tf = glm::translation(&pos);
    let tf: [f32; 16] = tf.as_slice().try_into().unwrap();

    let update = ServerEntityStateUpdatable {
        transform: Some(tf),
        ..Default::default()
    };

    update.patch(entity);
}

/// Updates all probes, generating charts and repositioning entities if needed.
///
/// Splits work into two stages to avoid blocking other operations.
pub fn update_probes(gs: GridStatePtr) {
    let chart_timer = std::time::Instant::now();
    // we need to do this in stages to avoid blocking others from using the state. First step is to see if any probes are dirty. If they are, we want to start generating new chart images for them

    // Stage 1: Mark dirty probes and schedule chart generation
    let mut image_to_generate = HashMap::<EntityID, (usize, Vec<u8>)>::default();

    let power_system = {
        // acquire locks
        let mut gs = gs.lock().unwrap();

        let mut probes = std::mem::take(&mut gs.probes);

        for item in &mut probes {
            if item.dirty.is_none() {
                continue;
            }

            item.update(&mut gs);

            image_to_generate.insert(item.entity.id(), (item.line_i, vec![]));
        }

        // put probes back
        gs.probes = std::mem::take(&mut probes);

        gs.system.clone()
    };

    // Stage 2: Generate charts for updated probes
    for item in image_to_generate.values_mut() {
        // now generate lines
        // let chart_gen_timer = std::time::Instant::now();
        let chart_image = generate_chart_for(item.0, &power_system);
        item.1 = chart_image;
        // println!("Gen: {}", chart_gen_timer.elapsed().as_millis());
    }

    {
        let mut gs = gs.lock().unwrap();
        let state_ptr = gs.state.clone();
        let mut state = state_ptr.lock().unwrap();

        let mut probes = std::mem::take(&mut gs.probes);

        // Stage 3: Install new charts into probes after generation
        for item in &mut probes {
            let Some((_, content)) = image_to_generate.remove(&item.entity.id()) else {
                continue;
            };

            item.install_chart(&mut gs, &mut state, content);
        }

        // put probes back
        gs.probes = std::mem::take(&mut probes);
    }

    let since = chart_timer.elapsed();
    println!("Took: {}", since.as_millis());
}
