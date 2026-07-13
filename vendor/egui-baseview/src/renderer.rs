#[cfg(feature = "opengl")]
mod opengl;
#[cfg(feature = "opengl")]
pub use opengl::renderer::{GraphicsConfig, Renderer};

// NOTE: The upstream `wgpu` renderer was removed from this vendored copy. It was never
// used by this project and has not been ported to baseview 0.2.
