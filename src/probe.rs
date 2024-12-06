use colabrodo_common::components::*;
use colabrodo_server::{server::*, server_messages::*};
use nalgebra::distance;
use nalgebra_glm::{self as glm, vec3, Mat4, Vec2};
use nalgebra_glm::{vec2, Vec3};

use crate::chart::*;
use crate::geometry::{make_plane, make_sphere};
use crate::texture::texture_from_bytes;
use crate::GridState;

pub struct Probe {
    pub entity: EntityReference,
    pub world_pos: Vec3,
    pub dirty: bool,

    pub handle: Option<EntityReference>,

    pub chart: Option<EntityReference>,
    pub line_i: usize,
}

impl Probe {
    pub fn new(entity: EntityReference) -> Self {
        Self {
            entity,
            world_pos: glm::vec3(0.0, 0.0, 0.0),
            dirty: true,
            handle: None,
            chart: None,
            line_i: usize::MAX,
        }
    }

    pub fn install_handle(&mut self, gs: &mut GridState, state: &mut ServerState) {
        let geometry = make_sphere(state, glm::vec3(0.0, 0.0, 1.0), 0.05);

        let placement: [f32; 16] = {
            let spot: Vec3 = self.world_pos;
            let tf = glm::translation(&(spot + glm::vec3(0.25, 1.0, 0.0)));
            tf.as_slice().try_into().unwrap()
        };

        self.handle = Some(state.entities.new_component(ServerEntityState {
            name: Some("Transformers".to_string()),
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

    fn install_chart(&mut self, gs: &mut GridState, state: &mut ServerState) {
        println!("Generating chart for {}", self.line_i);

        let chart_timer = std::time::Instant::now();

        let chart_image = generate_chart_for(self.line_i, &gs.system);

        let tex = texture_from_bytes(state, &chart_image, "Voltage for Line");

        let chart_elapsed = chart_timer.elapsed();

        println!("Took: {}", chart_elapsed.as_millis());

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
            let transform = glm::scale(&transform, &glm::vec3(0.4, 1.0, 0.3));
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
            name: Some("Chart Entity".to_string()),
            mutable: ServerEntityStateUpdatable {
                parent: Some(self.handle.clone().unwrap()),
                transform: Some(placement),
                representation: Some(ServerEntityRepresentation::new_render(
                    ServerRenderRepresentation {
                        mesh: geometry,
                        instances: None,
                    },
                )),
                ..Default::default()
            },
        });

        self.chart = Some(entity);
    }

    pub fn update(&mut self, gs: &mut GridState, state: &mut ServerState) {
        self.dirty = false;

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

        self.install_chart(gs, state);
    }
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

pub fn update_probes(gs: &mut GridState, state: &mut ServerState) {
    let mut probes = std::mem::take(&mut gs.probes);

    for item in &mut probes {
        if !item.dirty {
            continue;
        }

        item.update(gs, state);
    }

    gs.probes = std::mem::take(&mut probes);
}
