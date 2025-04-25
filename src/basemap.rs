use colabrodo_common::components::BufferState;
use colabrodo_server::{server::*, server_messages::*};

use nalgebra_glm::{self as glm, vec2, vec3, Mat4};

use crate::{domain::Domain, geometry::make_plane, PowerSystem};
/// Creates a textured basemap plane from the system's floorplan image.
///
/// This constructs a plane aligned with the floorplan's lower-left and upper-right
/// coordinates, using a texture generated from the embedded image data.
pub fn make_basemap(
    state: &mut ServerState,
    system: &PowerSystem,
    domain: &Domain,
) -> Option<EntityReference> {
    let fp = system.floor_plan.as_ref()?; // Return None if no floorplan exists

    // Convert floorplan world coordinates to normalized domain space
    let ll = vec2(domain.lerp_x(fp.ll_x as f32), domain.lerp_y(fp.ll_y as f32));
    let ur = vec2(domain.lerp_x(fp.ur_x as f32), domain.lerp_y(fp.ur_y as f32));

    // Compute center and scale for transform
    let center = (ll + ur) / 2.0;
    let scale = (ll - ur).abs();

    // Build transform: translate to center, then scale the plane
    let transform = glm::scale(
        &glm::translate(&Mat4::identity(), &vec3(center.x, 0.0, center.y)),
        &vec3(scale.x, 1.0, scale.y),
    );

    // If no image data exists, skip rendering
    if fp.data.is_empty() {
        log::warn!("No basemap data!");
        return None;
    }

    // Create a buffer from the image bytes
    let buf = state
        .buffers
        .new_component(BufferState::new_from_bytes(fp.data.clone()));

    // Create a full buffer view
    let view = state
        .buffer_views
        .new_component(ServerBufferViewState::new_from_whole_buffer(buf));

    // Create image from view
    let image = state.images.new_component(ServerImageState {
        name: Some("Basemap Image".into()),
        source: ServerImageStateSource::new_buffer(view),
    });

    // Create texture referencing the image
    let tex = state.textures.new_component(ServerTextureState {
        name: Some("Basemap Texture".into()),
        image,
        sampler: None,
    });

    // Create a material that uses the texture
    let mat = state.materials.new_component(ServerMaterialState {
        name: Some("Basemap Material".into()),
        mutable: ServerMaterialStateUpdatable {
            pbr_info: Some(ServerPBRInfo {
                base_color: [1.0, 1.0, 1.0, 1.0], // White base color (fully unlit)
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

    // Create a plane geometry using the transform and material
    let geom = make_plane(state, transform, mat);

    // Register the geometry as a renderable entity in the scene
    Some(state.entities.new_component(ServerEntityState {
        name: Some("Basemap".into()),
        mutable: ServerEntityStateUpdatable {
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
