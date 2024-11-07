use colabrodo_common::components::BufferState;
use colabrodo_server::{server::*, server_messages::*};

use nalgebra_glm::{self as glm, vec2, vec3, Mat4};

use crate::{domain::Domain, geometry::make_plane, texture::make_ruler_texture, PowerSystem};

pub fn make_ruler(
    state: &mut ServerState,
    system: &PowerSystem,
    domain: &Domain,
) -> EntityReference {
    let tex = make_ruler_texture(state);

    let mat = state.materials.new_component(ServerMaterialState {
        name: Some("Ruler Material".into()),
        mutable: ServerMaterialStateUpdatable {
            pbr_info: Some(ServerPBRInfo {
                base_color: [1.0, 1.0, 1.0, 1.0],
                metallic: Some(0.0),
                roughness: Some(0.25),
                base_color_texture: Some(ServerTextureRef {
                    texture: tex,
                    transform: None,
                    texture_coord_slot: None,
                }),
                ..Default::default()
            }),
            use_alpha: Some(true),
            ..Default::default()
        },
    });

    let transform = glm::rotate_x(&Mat4::identity(), 90.0f32.to_radians());
    let transform = glm::scale(&transform, &vec3(0.5625, 1.0, 1.5));
    let transform = glm::translate(
        &transform,
        &vec3(0.0, domain.lerp_y(domain.data_y.y as f32), -0.5),
    );

    let geom = make_plane(state, transform, mat);

    state.entities.new_component(ServerEntityState {
        name: Some("Ruler".into()),
        mutable: ServerEntityStateUpdatable {
            //transform: Some(transform.as_slice().try_into().unwrap()),
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
