use std::collections::HashMap;

use colabrodo_common::components::*;
use colabrodo_common::nooid::EntityID;
use colabrodo_server::{
    server::{self, *},
    server_messages::*,
};
use nalgebra::distance;
use nalgebra_glm::{self as glm, vec3, Mat4, Vec2};
use nalgebra_glm::{vec2, Vec3};

use crate::chart::*;
use crate::geometry::{make_plane, make_sphere};
use crate::state::GridStatePtr;
use crate::texture::texture_from_bytes;
use crate::GridState;

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

    /// Creates an object that the user can grab to reposition the chart
    pub fn install_handle(&mut self, gs: &mut GridState, state: &mut ServerState) {
        let geometry = make_sphere(state, glm::vec3(0.0, 0.0, 1.0), 0.05);

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

    /// Given a list of lines, we want to find the closest one, in 2D space
    fn get_closest_line(&self, gs: &mut GridState) -> Option<(usize, Vec2)> {
        let lines = gs.system.lines.get(gs.time_step)?;

        let domain = &gs.domain;

        let p = self.world_pos.xz();

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

    /// Generate a chart, and build the graphics object for the chart display
    fn install_chart(&mut self, gs: &mut GridState, state: &mut ServerState, new_image: Vec<u8>) {
        log::debug!("Generating chart for {}", self.line_i);

        let chart_gen_timer = std::time::Instant::now();
        let tex = texture_from_bytes(state, &new_image, "Voltage for Line");
        log::debug!("Tex: {}", chart_gen_timer.elapsed().as_millis());

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

        let geometry = {
            let transform = glm::rotate_x(&Mat4::identity(), 90.0f32.to_radians());
            let transform = glm::scale(&transform, &glm::vec3(0.5, 1.0, 0.4));
            make_plane(state, transform, chart_mat)
        };

        let placement: [f32; 16] = {
            let tf = glm::translation(&glm::vec3(0.0, 0.25, 0.0));
            tf.as_slice().try_into().unwrap()
        };

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

    fn install_delete_buttion(&mut self, gs: &mut GridState, state: &mut ServerState) {
        let geometry = make_sphere(state, glm::vec3(1.0, 0.1, 0.1), 0.05);

        let placement: [f32; 16] = {
            let tf = glm::translation(&glm::vec3(0.25, 0.25, 0.0));
            tf.as_slice().try_into().unwrap()
        };

        let entity = state.entities.new_component(ServerEntityState {
            name: Some("Delete Button".to_string()),
            mutable: ServerEntityStateUpdatable {
                parent: Some(self.chart.clone().unwrap()),
                transform: Some(placement),
                representation: Some(ServerEntityRepresentation::new_render(
                    ServerRenderRepresentation {
                        mesh: geometry,
                        instances: None,
                    },
                )),
                methods_list: Some(vec![gs.activate_func.clone().unwrap()]),
                ..Default::default()
            },
        });

        self.chart_delete = Some(entity);
    }

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

        if self.line_i == closest_line_index {
            return;
        }

        self.line_i = closest_line_index;
    }

    pub fn check_click(&self, entity: &EntityReference) -> Option<ClickResult> {
        todo!()
    }
}

pub enum ClickResult {
    Delete,
}

fn move_entity(entity: &EntityReference, pos: Vec3) {
    let tf = glm::translation(&pos);
    let tf: [f32; 16] = tf.as_slice().try_into().unwrap();

    let update = ServerEntityStateUpdatable {
        transform: Some(tf),
        ..Default::default()
    };

    update.patch(entity);
}

pub fn update_probes(gs: GridStatePtr) {
    let chart_timer = std::time::Instant::now();
    // we need to do this in stages to avoid blocking others from using the state. First step is to see if any probes are dirty. If they are, we want to start generating new chart images for them

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
