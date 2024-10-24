use colabrodo_server::{server::*, server_messages::*};

use nalgebra_glm as glm;

use crate::{domain::Domain, geometry::make_cube};

pub fn make_hazard_planes(
    state: &mut ServerState,
    domain: &Domain,
) -> (EntityReference, EntityReference) {
    // set up hazard planes
    let lower_hazard_coord = glm::vec3(0.0, domain.voltage_to_height(0.95), 0.0);
    let upper_hazard_coord = glm::vec3(0.0, domain.voltage_to_height(1.05), 0.0);

    let hazard_mat = state.materials.new_component(ServerMaterialState {
        name: None,
        mutable: ServerMaterialStateUpdatable {
            pbr_info: Some(ServerPBRInfo {
                base_color: [1.0, 1.0, 1.0, 0.25],
                metallic: Some(0.0),
                roughness: Some(1.0),
                ..Default::default()
            }),
            use_alpha: Some(true),
            ..Default::default()
        },
    });

    let hazard_geom = make_cube(state, glm::scaling(&glm::vec3(2.0, 0.001, 2.0)), hazard_mat);

    let lower_hazard_entity = state.entities.new_component(ServerEntityState {
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

    let upper_hazard_entity = state.entities.new_component(ServerEntityState {
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

    (lower_hazard_entity, upper_hazard_entity)
}
