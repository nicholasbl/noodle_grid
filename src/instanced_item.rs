use colabrodo_server::{server::*, server_messages::*};

use nalgebra_glm as glm;

use crate::geometry::{make_bus, make_cube, make_cyl, make_sphere};

/// Collects relevant info for instancing geometry
pub struct InstancedItem {
    pub entity: EntityReference,
    pub geometry: GeometryReference,
    pub buffer: Vec<u8>,
}

pub fn make_bus_element(state: &mut ServerState, material: MaterialReference) -> InstancedItem {
    // Create geometry for the buses
    let geometry = make_bus(state, glm::identity(), material);

    // Create an entity to render the buses
    let entity = state.entities.new_component(ServerEntityState {
        name: Some("Buses".to_string()),
        mutable: ServerEntityStateUpdatable {
            parent: None,
            transform: None,
            representation: Some(ServerEntityRepresentation::new_render(
                ServerRenderRepresentation {
                    mesh: geometry.clone(),
                    instances: None,
                },
            )),
            ..Default::default()
        },
    });

    InstancedItem {
        entity,
        geometry,
        buffer: vec![],
    }
}

pub fn make_line_element(state: &mut ServerState, material: MaterialReference) -> InstancedItem {
    // Create geometry for the lines
    let geometry = make_cube(state, glm::identity(), material);

    // Create an entity to render the lines
    let entity = state.entities.new_component(ServerEntityState {
        name: Some("Lines".to_string()),
        mutable: ServerEntityStateUpdatable {
            parent: None,
            transform: None,
            representation: Some(ServerEntityRepresentation::new_render(
                ServerRenderRepresentation {
                    mesh: geometry.clone(),
                    instances: None,
                },
            )),
            ..Default::default()
        },
    });

    InstancedItem {
        entity,
        geometry,
        buffer: vec![],
    }
}

pub fn make_line_flow_element(
    state: &mut ServerState,
    material: MaterialReference,
) -> InstancedItem {
    // Create geometry for the lines

    const TEX_CUBE: &str = include_str!("../assets/tex_cube.obj");

    let contents = std::io::BufReader::new(std::io::Cursor::new(TEX_CUBE));

    let (cube_ent, cube_geom) =
        crate::import_obj::import_file(contents, state, None, Some(material))
            .unwrap()
            .into_iter()
            .next()
            .unwrap();

    InstancedItem {
        entity: cube_ent,
        geometry: cube_geom,
        buffer: vec![],
    }
}

pub fn make_transformer_element(
    state: &mut ServerState,
    material: MaterialReference,
) -> InstancedItem {
    // Create geometry for the tfs
    let geometry = make_cyl(state, glm::identity(), material);

    // Create an entity to render the tfs
    let entity = state.entities.new_component(ServerEntityState {
        name: Some("Transformers".to_string()),
        mutable: ServerEntityStateUpdatable {
            parent: None,
            transform: None,
            representation: Some(ServerEntityRepresentation::new_render(
                ServerRenderRepresentation {
                    mesh: geometry.clone(),
                    instances: None,
                },
            )),
            ..Default::default()
        },
    });

    InstancedItem {
        entity,
        geometry,
        buffer: vec![],
    }
}

pub fn make_generator_element(state: &mut ServerState) -> InstancedItem {
    // Create geometry for the generators
    let geometry = make_sphere(state, glm::Vec3::new(1.0, 1.0, 0.0), 1.0);

    // Create an entity to render the gens
    let entity = state.entities.new_component(ServerEntityState {
        name: Some("Generator".to_string()),
        mutable: ServerEntityStateUpdatable {
            parent: None,
            transform: None,
            representation: Some(ServerEntityRepresentation::new_render(
                ServerRenderRepresentation {
                    mesh: geometry.clone(),
                    instances: None,
                },
            )),
            ..Default::default()
        },
    });

    InstancedItem {
        entity,
        geometry,
        buffer: vec![],
    }
}

pub fn make_hazard_element(state: &mut ServerState, material: MaterialReference) -> InstancedItem {
    let contents = include_str!("../assets/rounded_rect.obj");

    let contents = std::io::BufReader::new(std::io::Cursor::new(contents));

    let (entity, geometry) = crate::import_obj::import_file(contents, state, None, Some(material))
        .unwrap()
        .into_iter()
        .next()
        .unwrap();

    InstancedItem {
        entity,
        geometry,
        buffer: vec![],
    }
}
