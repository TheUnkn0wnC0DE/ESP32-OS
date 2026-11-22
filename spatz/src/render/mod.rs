//! Software-Rendering: Framebuffer, Rasterizer, Pipeline, Meshes, Text.

pub mod camera;
pub mod clip;
pub mod font;
pub mod framebuffer;
pub mod mesh;
pub mod overlay;
pub mod pipeline;
pub mod raster;
pub mod shapes;

pub use camera::Camera;
pub use framebuffer::{DEPTH_SCALE, Framebuffer};
pub use mesh::{Mesh, MeshId, MeshStore};
pub use pipeline::{
    DrawItem, DrawList, DrawMode, DrawParams, Environment, Material, RenderStats, Renderer, Sprite, bake_lighting,
    light_faces,
};
