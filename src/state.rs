use std::{
    f32::consts::PI,
    ops::{Add, Div, Mul, Sub},
    sync::{Arc, Mutex},
};

use colabrodo_common::components::BufferState;
use colabrodo_server::{server::*, server_bufferbuilder::*, server_messages::*};
use nalgebra::{vector, Matrix3, Quaternion, Rotation3, UnitQuaternion, UnitVector3, Vector3};

use crate::PowerSystem;

use nalgebra_glm as glm;

#[inline]
fn lerp<T>(x: T, x0: T, x1: T, y0: T, y1: T) -> T
where
    T: Sub<Output = T> + Add<Output = T> + Div<Output = T> + Mul<Output = T> + Copy,
{
    y0 + (x - x0) * ((y1 - y0) / (x1 - x0))
}

#[inline]
fn clamped_lerp<T>(x: T, x0: T, x1: T, y0: T, y1: T) -> T
where
    T: Sub<Output = T>
        + Add<Output = T>
        + Div<Output = T>
        + Mul<Output = T>
        + Copy
        + Sized
        + PartialOrd,
{
    num_traits::clamp(lerp(x, x0, x1, y0, y1), y0, y1)
}

const VOLT_MIN: f32 = 108.0;
const VOLT_MAX: f32 = 132.0;

const TUBE_MIN: f32 = 0.001;
const TUBE_MAX: f32 = 0.015;

#[inline]
fn voltage_to_height(v: f32) -> f32 {
    clamped_lerp(v, VOLT_MIN, VOLT_MAX, 0.0, 1.5)
}

#[inline]
fn real_power_to_width(v: f32) -> f32 {
    clamped_lerp(v, 0.0, 704.0, TUBE_MIN, TUBE_MAX)
}

#[inline]
fn reactive_power_to_width(v: f32) -> f32 {
    clamped_lerp(v, 0.0, 704.0, TUBE_MIN, TUBE_MAX)
}

#[inline]
fn lerp_x(v: f32) -> f32 {
    lerp(v, 18401.9, 18694.2, -1.0, 1.0)
}

#[inline]
fn lerp_y(v: f32) -> f32 {
    lerp(v, -10117.4, -9818.12, -1.0, 1.0)
}

//type Vec2 = Vector2<f32>;
type Vec3 = Vector3<f32>;
//type Vec4 = Vector4<f32>;
type Mat3 = Matrix3<f32>;
//type Mat4 = Matrix4<f32>;
type Quat = Quaternion<f32>;

//const PHASE_OFFSET: Vec3 = Vec3::new(0.001, 0.0, -0.001);

pub struct GridState {
    state: ServerStatePtr,

    system: PowerSystem,
    time_step: usize,

    line_entity: EntityReference,
    line_geometry: GeometryReference,

    line_mat_buffer: Vec<u8>,
}

pub type GridStatePtr = Arc<Mutex<GridState>>;

impl GridState {
    pub fn new(state: ServerStatePtr, system: PowerSystem) -> GridStatePtr {
        let (cube, ent) = {
            let mut state_lock = state.lock().unwrap();

            // Create a cube
            let cube = make_cube(&mut state_lock);

            let comp = state_lock.entities.new_component(ServerEntityState {
                name: Some("Cube".to_string()),
                mutable: ServerEntityStateUpdatable {
                    parent: None,
                    transform: None,
                    representation: Some(ServerEntityRepresentation::new_render(
                        ServerRenderRepresentation {
                            mesh: cube.clone(),
                            instances: None,
                        },
                    )),
                    ..Default::default()
                },
            });

            (cube, comp)
        };

        Arc::new(Mutex::new(GridState {
            state,
            system,
            time_step: 0,
            line_geometry: cube,
            line_mat_buffer: vec![],
            line_entity: ent,
        }))
    }
}

fn make_cube(server_state: &mut ServerState) -> GeometryReference {
    let verts = vec![
        VertexMinimal {
            position: [-0.5, -0.5, 0.5],
            normal: [-0.5774, -0.5774, 0.5774],
        },
        VertexMinimal {
            position: [0.5, -0.5, 0.5],
            normal: [0.5774, -0.5774, 0.5774],
        },
        VertexMinimal {
            position: [0.5, 0.5, 0.5],
            normal: [0.5774, 0.5774, 0.5774],
        },
        VertexMinimal {
            position: [-0.5, 0.5, 0.5],
            normal: [-0.5774, 0.5774, 0.5774],
        },
        VertexMinimal {
            position: [-0.5, -0.5, -0.5],
            normal: [-0.5774, -0.5774, -0.5774],
        },
        VertexMinimal {
            position: [0.5, -0.5, -0.5],
            normal: [0.5774, -0.5774, -0.5774],
        },
        VertexMinimal {
            position: [0.5, 0.5, -0.5],
            normal: [0.5774, 0.5774, -0.5774],
        },
        VertexMinimal {
            position: [-0.5, 0.5, -0.5],
            normal: [-0.5774, 0.5774, -0.5774],
        },
    ];

    let index_list = vec![
        // front
        [0, 1, 2],
        [2, 3, 0],
        // right
        [1, 5, 6],
        [6, 2, 1],
        // back
        [7, 6, 5],
        [5, 4, 7],
        // left
        [4, 0, 3],
        [3, 7, 4],
        // bottom
        [4, 5, 1],
        [1, 0, 4],
        // top
        [3, 2, 6],
        [6, 7, 3],
    ];
    let index_list = IndexType::Triangles(index_list.as_slice());

    let test_source = VertexSource {
        name: Some("Cube".to_string()),
        vertex: verts.as_slice(),
        index: index_list,
    };

    // Create a material to go along with this cube
    let material = server_state.materials.new_component(ServerMaterialState {
        name: None,
        mutable: ServerMaterialStateUpdatable {
            pbr_info: Some(ServerPBRInfo {
                base_color: [1.0, 1.0, 0.75, 1.0],
                metallic: Some(1.0),
                roughness: Some(0.1),
                ..Default::default()
            }),
            double_sided: Some(true),
            ..Default::default()
        },
    });

    let pack = test_source.pack_bytes().unwrap();

    // Return a new mesh with this geometry/material
    test_source
        .build_geometry(
            server_state,
            BufferRepresentation::Bytes(pack.bytes),
            material,
        )
        .unwrap()
}

// fn roll_free_rotation(direction: Vec3) -> Quat {
//     let rot = {
//         let m1 = 0.0;
//         let m2 = direction.z / direction.x;
//         ((m1 - m2) / (1.0 + m1 * m2)).abs().atan()
//     };

//     let tilt = {
//         let yp = Vec3::new(0.0, 1.0, 0.0);
//         (PI / 2.0) - direction.dot(&yp).acos()
//     };

//     let y_unit = UnitVector3::new_normalize(Vec3::new(0.0, 1.0, 0.0));
//     let z_unit = UnitVector3::new_normalize(Vec3::new(0.0, 0.0, 1.0));

//     UnitQuaternion::from_axis_angle(&z_unit, tilt).quaternion()
//         + UnitQuaternion::from_axis_angle(&y_unit, rot).quaternion()
// }

fn roll_free_rotation(direction: Vec3) -> Quat {
    let up = vector![0.0, 1.0, 0.0];

    let a = up.cross(&direction).normalize();
    let b = direction.cross(&a).normalize();

    let m = Mat3::new(
        a.x,
        b.x,
        direction.x,
        a.y,
        b.y,
        direction.y,
        a.z,
        b.z,
        direction.z,
    );

    glm::mat3_to_quat(&m)
}

impl GridState {
    fn recompute_lines(&mut self) {
        self.line_mat_buffer.clear();

        let ts = self.system.lines.get(self.time_step).unwrap();

        for state in ts {
            let p_a = Vec3::new(
                lerp_x(state.loc.sx as f32),
                voltage_to_height(state.voltage.sa),
                lerp_y(state.loc.sy as f32),
            );

            let p_b = Vec3::new(
                lerp_x(state.loc.ex as f32),
                voltage_to_height(state.voltage.ea),
                lerp_y(state.loc.ey as f32),
            );

            let v = p_b - p_a;

            // reverse?

            let rot = roll_free_rotation(v.normalize());

            let center = (p_a + p_b) / 2.0;

            let watt = real_power_to_width(state.real_power.sa);
            let vars = reactive_power_to_width(state.reactive_power.sa);
            let rot_vec = rot.as_vector();

            if p_a.y < 0.000001 || p_b.y < 0.000001 {
                continue;
            }

            let mat = [
                center.x,
                center.y,
                center.z,
                1.0, //
                1.0,
                1.0,
                1.0,
                1.0, //
                rot_vec.x,
                rot_vec.y,
                rot_vec.z,
                rot_vec.w, //
                vars,
                watt,
                v.magnitude(),
                1.0, //
            ];

            self.line_mat_buffer
                .extend_from_slice(bytemuck::cast_slice(&mat));
        }
    }

    pub fn recompute_all(&mut self) {
        self.recompute_lines();

        let mut lock = self.state.lock().unwrap();

        let line_buffer = lock
            .buffers
            .new_component(BufferState::new_from_bytes(self.line_mat_buffer.clone()));

        let view = lock
            .buffer_views
            .new_component(ServerBufferViewState::new_from_whole_buffer(line_buffer));

        let update = ServerEntityStateUpdatable {
            representation: Some(ServerEntityRepresentation::new_render(
                ServerRenderRepresentation {
                    mesh: self.line_geometry.clone(),
                    instances: Some(ServerGeometryInstance {
                        view,
                        stride: None,
                        bb: None,
                    }),
                },
            )),
            ..Default::default()
        };

        update.patch(&self.line_entity);
    }
}
