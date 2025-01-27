use colabrodo_common::components::BufferState;
use colabrodo_server::{server::*, server_messages::*};

use nalgebra_glm::{self as glm, vec2, vec3, Mat4};

use crate::{domain::Domain, geometry::make_plane, PowerSystem};

pub fn make_basemap(
    state: &mut ServerState,
    system: &PowerSystem,
    domain: &Domain,
) -> Option<EntityReference> {
    let Some(fp) = system.floor_plan.as_ref() else {
        return None;
    };
    let ll = vec2(domain.lerp_x(fp.ll_x as f32), domain.lerp_y(fp.ll_y as f32));
    let ur = vec2(domain.lerp_x(fp.ur_x as f32), domain.lerp_y(fp.ur_y as f32));

    // center is?
    let center = (ll + ur) / 2.0;
    let scale = (ll - ur).abs();

    let transform = glm::scale(
        &glm::translate(&Mat4::identity(), &vec3(center.x, 0.0, center.y)),
        &vec3(scale.x, 1.0, scale.y),
    );

    if fp.data.is_empty() {
        log::warn!("No basemap data!");
        return None;
    }

    let buf = state
        .buffers
        .new_component(BufferState::new_from_bytes(fp.data.clone()));

    let view = state
        .buffer_views
        .new_component(ServerBufferViewState::new_from_whole_buffer(buf));

    let image = state.images.new_component(ServerImageState {
        name: Some("Basemap Image".into()),
        source: ServerImageStateSource::new_buffer(view),
    });

    let tex = state.textures.new_component(ServerTextureState {
        name: Some("Basemap Texture".into()),
        image,
        sampler: None,
    });

    let mat = state.materials.new_component(ServerMaterialState {
        name: Some("Basemap Material".into()),
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
            ..Default::default()
        },
    });

    let geom = make_plane(state, transform, mat);

    Some(state.entities.new_component(ServerEntityState {
        name: Some("Basemap".into()),
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
    }))
}
