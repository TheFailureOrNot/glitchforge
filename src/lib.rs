//! GlitchForge's Rust-native image processing engine.
//!
//! The renderer works in RGB pixel space, uses a stable independent random stream
//! for each effect, then encodes and decodes the selected output format so the
//! preview shows the same pixels that will be written to disk.

pub mod cli;
pub mod config;
pub mod effects;
pub mod gui;
pub mod presets;
pub mod render;

pub use presets::{
    artistic_effects, artistic_presets, realistic_effects, realistic_presets, EffectInfo,
    PresetInfo,
};
pub use render::{
    decode_image, encode_image, load_image, render, render_encoded, ExportFormat, Grade,
    RenderSettings, RenderedOutput, Subsampling,
};
