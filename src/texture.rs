use colabrodo_common::components::{BufferState, BufferViewState, ImageSource};
use colabrodo_server::{server::ServerState, server_messages::*};

pub const HSV_TEXTURE_BYTES: &[u8; 89263] = include_bytes!("../assets/hsv.png");

pub fn make_hsv_texture(state: &mut ServerState) -> TextureReference {
    let line_image_buffer = state
        .buffers
        .new_component(BufferState::new_from_bytes(HSV_TEXTURE_BYTES.into()));

    let line_image_view = state
        .buffer_views
        .new_component(BufferViewState::new_from_whole_buffer(line_image_buffer));

    let line_image = state.images.new_component(ServerImageState {
        name: Some("Line Image".into()),
        source: ImageSource::new_buffer(line_image_view),
    });

    state.textures.new_component(ServerTextureState {
        name: Some("Line Texture".into()),
        image: line_image,
        sampler: None,
    })
}
