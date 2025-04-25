use colabrodo_server::{server::*, server_messages::*};

use nalgebra_glm as glm;

use crate::geometry::{make_bus, make_cube, make_cyl};

/// Represents a template for instancing entities with geometry and per-instance data.
///
/// The `buffer` is used to store transform data (empty initially).
pub struct InstancedItem {
    pub entity: EntityReference,
    pub geometry: GeometryReference,
    pub buffer: Vec<u8>,
}

/// Creates an instanced bus element.
///
/// This generates a basic bus geometry and an entity ready for instancing.
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
                    instances: None, // Instances will be populated later
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

/// Creates an instanced line element.
///
/// This uses a cube geometry as the base for line visualization.
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

/// Creates an instanced line element with flow texture mapping.
///
/// Loads a pre-textured cube mesh from an embedded OBJ file.
pub fn make_line_flow_element(
    state: &mut ServerState,
    material: MaterialReference,
) -> InstancedItem {
    const TEX_CUBE: &str = include_str!("../assets/tex_cube.obj");

    let contents = std::io::BufReader::new(std::io::Cursor::new(TEX_CUBE));

    let (cube_ent, cube_geom) =
        crate::import_obj::import_file(contents, state, None, None, Some(material))
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

/// Creates an instanced transformer element.
///
/// Uses a cylinder primitive for the transformer body.
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

/// Creates an instanced generator element.
///
/// Loads a yellow generator model from an embedded OBJ file.
pub fn make_generator_element(state: &mut ServerState) -> InstancedItem {
    let contents = include_str!("../assets/generator.obj");

    let contents = std::io::BufReader::new(std::io::Cursor::new(contents));

    let material = state.materials.new_component(ServerMaterialState {
        name: None,
        mutable: ServerMaterialStateUpdatable {
            pbr_info: Some(ServerPBRInfo {
                base_color: [1.0, 1.0, 0.0, 1.0],
                metallic: Some(1.0),
                roughness: Some(0.25),
                ..Default::default()
            }),
            double_sided: Some(true),
            ..Default::default()
        },
    });

    let (entity, geometry) =
        crate::import_obj::import_file(contents, state, None, None, Some(material))
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

/// Creates an instanced hazard marker element.
///
/// Loads a rounded rectangle mesh from an embedded OBJ file.
pub fn make_hazard_element(state: &mut ServerState, material: MaterialReference) -> InstancedItem {
    let contents = include_str!("../assets/rounded_rect.obj");

    let contents = std::io::BufReader::new(std::io::Cursor::new(contents));

    let (entity, geometry) =
        crate::import_obj::import_file(contents, state, None, None, Some(material))
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
