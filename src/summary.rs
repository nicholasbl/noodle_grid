use colabrodo_common::components::TextureRef;
use colabrodo_server::{server::*, server_messages::*};

use crate::domain::Domain;
use crate::dots::PowerSystem;
use crate::{
    geometry::{make_cyl, make_plane},
    texture::texture_from_bytes,
};

use nalgebra_glm::{self as glm, Mat4, Vec3};

#[allow(dead_code)]
pub struct SummaryItem {
    chart: EntityReference,
    indicator: EntityReference,
}

const PX_WIDTH: u32 = 1024;
const PX_HEIGHT: u32 = 768;

const CHART_SIZE: f32 = 0.5;
const ASPECT_W_H: f32 = (PX_WIDTH as f32) / (PX_HEIGHT as f32);

const SUMMARY_HEIGHT: f32 = CHART_SIZE;
const SUMMARY_WIDTH: f32 = CHART_SIZE * ASPECT_W_H;

impl SummaryItem {
    pub fn new(ps: &PowerSystem, domain: &Domain, state: &mut ServerState) -> Self {
        let chart = crate::chart::generate_time_chart(&ps, PX_WIDTH, PX_HEIGHT);

        //std::fs::write("temp.png", &chart).unwrap();

        let tex = texture_from_bytes(state, &chart, "Voltage for Line");

        // margins are 60 px on each side

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
            let transform = glm::scale(&transform, &glm::vec3(SUMMARY_WIDTH, 1.0, SUMMARY_HEIGHT));
            make_plane(state, transform, chart_mat)
        };

        let placement: [f32; 16] = {
            let spot: Vec3 = glm::vec3(domain.lerp_x(domain.x_bounds.x as f32), 0.5, -0.5);
            let tf = glm::translation(&spot);
            tf.as_slice().try_into().unwrap()
        };

        let chart = state.entities.new_component(ServerEntityState {
            name: Some("Time Chart".into()),
            mutable: ServerEntityStateUpdatable {
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

        let indicator = make_indicator(&chart, state);

        Self { chart, indicator }
    }

    pub fn set_time_normalized(&mut self, frac: f32) {
        // x is horizontal.

        // the chart is 0.5 wide.
        // 0.5 -> 1024 pixels
        // and we have to remove the margins
        // the margins are set at 60, but if you measure it, its actually 70
        const EFFECTIVE_WIDTH: f32 =
            ((PX_WIDTH as f32 - (70.0 * 2.0)) / PX_WIDTH as f32) * SUMMARY_WIDTH;

        let new_x = -EFFECTIVE_WIDTH / 2.0 + EFFECTIVE_WIDTH * frac;

        // Something changed, but it wasn't a probe. So we just accept it.
        let placement: [f32; 16] = {
            let spot: Vec3 = glm::vec3(new_x, 0.0, 0.0);
            let tf = glm::translation(&spot);
            tf.as_slice().try_into().unwrap()
        };

        let update = ServerEntityStateUpdatable {
            transform: Some(placement),
            ..Default::default()
        };

        update.patch(&self.indicator);
    }
}

fn make_indicator(parent: &EntityReference, state: &mut ServerState) -> EntityReference {
    let mat = state.materials.new_component(ServerMaterialState {
        name: Some("Indicator Mat".into()),
        mutable: ServerMaterialStateUpdatable {
            pbr_info: Some(ServerPBRInfo {
                base_color: [1.0, 0.01, 0.01, 1.0],
                metallic: Some(0.0),
                roughness: Some(1.0),
                ..Default::default()
            }),
            ..Default::default()
        },
    });

    let tf = glm::scaling(&glm::vec3(0.01, 0.4, 0.01));

    let geom = make_cyl(state, tf, mat);

    state.entities.new_component(ServerEntityState {
        name: Some("Indicator".into()),
        mutable: ServerEntityStateUpdatable {
            parent: Some(parent.clone()),
            representation: Some(ServerEntityRepresentation::new_render(
                ServerRenderRepresentation {
                    mesh: geom,
                    instances: None,
                },
            )),
            ..Default::default()
        },
    })
}
