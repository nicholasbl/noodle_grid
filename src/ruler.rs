use colabrodo_server::{server::*, server_messages::*};

use nalgebra_glm::{self as glm, vec3, Mat4};

use crate::{domain::Domain, geometry::make_plane, texture::*};

#[derive(Debug, PartialEq)]
pub enum RulerType {
    Voltage,
    LineLoad,
}

pub fn make_ruler(state: &mut ServerState, domain: &Domain, ty: RulerType) -> EntityReference {
    let tex = match ty {
        RulerType::Voltage => make_ruler_texture(state),
        RulerType::LineLoad => make_ruler_ll_texture(state),
    };

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
            visible: Some(ty == RulerType::Voltage),
            ..Default::default()
        },
    })
}

#[allow(dead_code)]
pub struct VerticalAxisSelector {
    indicator: EntityReference,
    voltage: EntityReference,
    line_load: EntityReference,
}

impl VerticalAxisSelector {
    pub fn new(state: &mut ServerState) -> Self {
        let indicator_source = include_str!("../assets/selection_indicator.obj");

        let indicator = make_obj(
            state,
            "Indicator",
            [0.5, 0.5, 0.5, 1.0],
            glm::vec3(0.5, 0.5, 0.5),
            glm::vec3(0.5, 0.0, 0.0),
            None,
            indicator_source,
        );

        let voltage_source = include_str!("../assets/selection_indicator.obj");

        let voltage = make_obj(
            state,
            "Indicator",
            [0.5, 0.5, 0.5, 1.0],
            glm::vec3(0.5, 0.5, 0.5),
            glm::vec3(0.5, 0.0, 0.0),
            None,
            voltage_source,
        );

        let line_load_source = include_str!("../assets/selection_indicator.obj");

        let line_load = make_obj(
            state,
            "Indicator",
            [0.5, 0.5, 0.5, 1.0],
            glm::vec3(0.5, 0.5, 0.5),
            glm::vec3(0.5, 0.0, 0.0),
            None,
            line_load_source,
        );

        Self {
            indicator,
            voltage,
            line_load,
        }
    }
}

pub fn make_obj(
    state: &mut ServerState,
    name: &str,
    color: [f32; 4],
    scale: glm::Vec3,
    offset: glm::Vec3,
    parent: Option<EntityReference>,
    content: &str,
) -> EntityReference {
    let contents = std::io::BufReader::new(std::io::Cursor::new(content));

    let material = state.materials.new_component(ServerMaterialState {
        name: Some(format!("{name} Mat")),
        mutable: ServerMaterialStateUpdatable {
            pbr_info: Some(ServerPBRInfo {
                base_color: color,
                metallic: Some(0.0),
                roughness: Some(1.0),
                ..Default::default()
            }),
            ..Default::default()
        },
    });

    let tf = glm::translation(&offset);
    let scale = glm::scale(&tf, &scale);

    let (entity, _) =
        crate::import_obj::import_file(contents, state, Some(scale), parent, Some(material))
            .unwrap()
            .into_iter()
            .next()
            .unwrap();

    entity
}
