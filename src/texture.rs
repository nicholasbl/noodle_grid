use colabrodo_common::components::{BufferState, BufferViewState, ImageSource};
use colabrodo_server::{server::ServerState, server_messages::*};

pub fn texture_from_bytes(state: &mut ServerState, bytes: &[u8], name: &str) -> TextureReference {
    assert!(!bytes.is_empty());

    let line_image_buffer = state
        .buffers
        .new_component(BufferState::new_from_bytes(bytes.into()));

    let line_image_view = state
        .buffer_views
        .new_component(BufferViewState::new_from_whole_buffer(line_image_buffer));

    let line_image = state.images.new_component(ServerImageState {
        name: Some(format!("{name} Image")),
        source: ImageSource::new_buffer(line_image_view),
    });

    state.textures.new_component(ServerTextureState {
        name: Some(format!("{name} Texture")),
        image: line_image,
        sampler: None,
    })
}

const HSV_TEXTURE_BYTES: &[u8; 89263] = include_bytes!("../assets/hsv.png");

pub fn make_hsv_texture(state: &mut ServerState) -> TextureReference {
    texture_from_bytes(state, HSV_TEXTURE_BYTES, "HSV")
}

const CHEV_TEXTURE_BYTES: &[u8; 12756] = include_bytes!("../assets/chevron_left.png");

pub fn make_chevron_texture(state: &mut ServerState) -> TextureReference {
    texture_from_bytes(state, CHEV_TEXTURE_BYTES, "Line Flow")
}

const RULER_TEXTURE_BYTES: &[u8; 430225] = include_bytes!("../assets/ruler.png");
const RULER_LL_TEXTURE_BYTES: &[u8; 444139] = include_bytes!("../assets/ruler_line_load.png");

pub fn make_ruler_texture(state: &mut ServerState) -> TextureReference {
    texture_from_bytes(state, RULER_TEXTURE_BYTES, "Ruler")
}

pub fn make_ruler_ll_texture(state: &mut ServerState) -> TextureReference {
    texture_from_bytes(state, RULER_LL_TEXTURE_BYTES, "Ruler (LL)")
}
