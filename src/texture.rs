use colabrodo_common::components::{BufferState, BufferViewState, ImageSource};
use colabrodo_server::{server::ServerState, server_messages::*};

/// Creates a texture from raw image bytes and registers it in the server state.
///
/// This function:
/// - Creates a `BufferState` from raw bytes
/// - Wraps it in a `BufferViewState`
/// - Wraps it in a `ServerImageState`
/// - Creates a `ServerTextureState` pointing to the image
///
/// Returns a `TextureReference` to the new texture.
///
/// # Panics
/// Panics if `bytes` is empty.
pub fn texture_from_bytes(state: &mut ServerState, bytes: &[u8], name: &str) -> TextureReference {
    assert!(!bytes.is_empty());

    // Create buffer component from raw bytes
    let line_image_buffer = state
        .buffers
        .new_component(BufferState::new_from_bytes(bytes.into()));

    // Create view into entire buffer
    let line_image_view = state
        .buffer_views
        .new_component(BufferViewState::new_from_whole_buffer(line_image_buffer));

    // Create image from buffer view
    let line_image = state.images.new_component(ServerImageState {
        name: Some(format!("{name} Image")),
        source: ImageSource::new_buffer(line_image_view),
    });

    // Create texture from image
    state.textures.new_component(ServerTextureState {
        name: Some(format!("{name} Texture")),
        image: line_image,
        sampler: None,
    })
}

// Embeds a static HSV gradient image into the binary at compile time.
const HSV_TEXTURE_BYTES: &[u8; 89263] = include_bytes!("../assets/hsv.png");

/// Creates and registers a pre-defined HSV gradient texture.
pub fn make_hsv_texture(state: &mut ServerState) -> TextureReference {
    texture_from_bytes(state, HSV_TEXTURE_BYTES, "HSV")
}

// Embeds a static chevron (arrow) image into the binary at compile time.
const CHEV_TEXTURE_BYTES: &[u8; 12756] = include_bytes!("../assets/chevron_left.png");

/// Creates and registers a pre-defined "Line Flow" chevron texture.
pub fn make_chevron_texture(state: &mut ServerState) -> TextureReference {
    texture_from_bytes(state, CHEV_TEXTURE_BYTES, "Line Flow")
}

// Embeds static ruler images into the binary at compile time.
const RULER_TEXTURE_BYTES: &[u8; 430225] = include_bytes!("../assets/ruler.png");
const RULER_LL_TEXTURE_BYTES: &[u8; 444139] = include_bytes!("../assets/ruler_line_load.png");

/// Creates and registers a pre-defined ruler texture.
pub fn make_ruler_texture(state: &mut ServerState) -> TextureReference {
    texture_from_bytes(state, RULER_TEXTURE_BYTES, "Ruler")
}

/// Creates and registers a pre-defined ruler texture for line load visualization.
pub fn make_ruler_ll_texture(state: &mut ServerState) -> TextureReference {
    texture_from_bytes(state, RULER_LL_TEXTURE_BYTES, "Ruler (LL)")
}
