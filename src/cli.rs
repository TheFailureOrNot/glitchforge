use anyhow::{anyhow, Context, Result};
use clap::{Parser, ValueEnum};
use image::{
    codecs::gif::{GifEncoder, Repeat},
    Delay, Frame,
};
use rand::Rng;
use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::{
    config, presets,
    render::{self, ExportFormat, RenderSettings, Subsampling},
};

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum CliFormat {
    Png,
    Jpeg,
    Webp,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum CliSubsampling {
    #[value(name = "4:4:4")]
    S444,
    #[value(name = "4:2:2")]
    S422,
    #[value(name = "4:2:0")]
    S420,
}
impl From<CliFormat> for ExportFormat {
    fn from(value: CliFormat) -> Self {
        match value {
            CliFormat::Png => Self::Png,
            CliFormat::Jpeg => Self::Jpeg,
            CliFormat::Webp => Self::Webp,
        }
    }
}

#[derive(Debug, Parser)]
#[command(
    name = "glitchforge",
    version,
    about = "GLITCHFORGE — deterministic visual simulation of digital image artifacts"
)]
pub struct Args {
    /// Image file or folder. Omit to launch the desktop application.
    pub input: Option<PathBuf>,
    /// Output file (single input) or directory (folder input).
    #[arg(short, long)]
    pub output: Option<PathBuf>,
    /// Realistic effect preset.
    #[arg(short, long, default_value = "edge", value_parser = ["subtle", "edge", "jpeg", "buffer", "stride", "datamosh", "analog", "chromatic", "heavy"])]
    pub preset: String,
    /// Optional artistic layer preset.
    #[arg(long, value_parser = ["crimson_dream", "signal_burst", "analog_vision", "glitch_cinema"])]
    pub artistic_preset: Option<String>,
    /// Realistic effect master strength.
    #[arg(short, long, default_value_t = 1.0)]
    pub intensity: f32,
    /// Artistic effect master strength.
    #[arg(long, default_value_t = 1.0)]
    pub artistic_intensity: f32,
    /// Artistic layer blend, in the range 0–1.
    #[arg(long, default_value_t = 1.0)]
    pub artistic_mix: f32,
    /// Realistic layer blend, in the range 0–1.
    #[arg(long, default_value_t = 1.0)]
    pub mix: f32,
    /// Repeatable random seed.
    #[arg(short, long)]
    pub seed: Option<u64>,
    /// Comma-separated realistic effect IDs to keep.
    #[arg(long)]
    pub only: Option<String>,
    /// Comma-separated realistic effect IDs to omit.
    #[arg(long)]
    pub skip: Option<String>,
    /// Number of deterministic variations to render.
    #[arg(long, default_value_t = 1)]
    pub frames: usize,
    /// Also write an animated GIF when rendering multiple frames.
    #[arg(long)]
    pub gif: bool,
    /// Animation frame rate.
    #[arg(long, default_value_t = 12)]
    pub fps: u32,
    /// JPEG / WebP quality.
    #[arg(long, default_value_t = 95)]
    pub quality: u8,
    /// Output encoder. By default inferred from the destination extension.
    #[arg(long, value_enum)]
    pub format: Option<CliFormat>,
    /// JPEG chroma subsampling.
    #[arg(long, value_enum)]
    pub subsampling: Option<CliSubsampling>,
    /// List available presets and effects.
    #[arg(long)]
    pub list: bool,
    /// Load a saved GlitchForge JSON configuration.
    #[arg(long)]
    pub config: Option<PathBuf>,
    /// Launch the desktop application.
    #[arg(long)]
    pub gui: bool,
}

pub fn list_catalogue() {
    println!("GLITCHFORGE — Rust edition\n");
    println!("Realistic presets:");
    for preset in presets::realistic_presets() {
        println!("  {:12} {}", preset.id, preset.description);
    }
    println!("\nArtistic presets:");
    for preset in presets::artistic_presets() {
        println!("  {:16} {}", preset.id, preset.description);
    }
    println!("\nRealistic effects:");
    for effect in presets::realistic_effects() {
        println!("  {:24} {}", effect.id, effect.hint);
    }
    println!("\nArtistic effects:");
    for effect in presets::artistic_effects() {
        println!("  {:24} {}", effect.id, effect.hint);
    }
}

pub fn run(args: Args) -> Result<()> {
    if args.list {
        list_catalogue();
        return Ok(());
    }
    let Some(input) = args.input.as_deref() else {
        return Err(anyhow!("No input file was provided."));
    };
    if !input.exists() {
        return Err(anyhow!("Input not found: {}", input.display()));
    }
    let from_config = args.config.is_some();
    let mut settings = if let Some(path) = args.config.as_deref() {
        config::load_config(path)?.settings
    } else {
        let mut defaults = RenderSettings::default();
        defaults.use_realistic_preset(&args.preset);
        defaults
    };
    if let Some(name) = args.artistic_preset.as_deref() {
        settings.use_artistic_preset(Some(name));
    }
    if !from_config || (args.intensity - 1.0).abs() > f32::EPSILON {
        settings.intensity = args.intensity;
    }
    if !from_config || (args.artistic_intensity - 1.0).abs() > f32::EPSILON {
        settings.artistic_intensity = args.artistic_intensity;
    }
    if !from_config || (args.mix - 1.0).abs() > f32::EPSILON {
        settings.mix = args.mix;
    }
    if !from_config || (args.artistic_mix - 1.0).abs() > f32::EPSILON {
        settings.artistic_mix = args.artistic_mix;
    }
    settings.seed = args.seed.unwrap_or_else(|| {
        if from_config {
            settings.seed
        } else {
            random_seed()
        }
    });
    if !from_config || args.quality != 95 {
        settings.quality = args.quality.clamp(1, 100);
    }
    if let Some(format) = args.format {
        settings.format = format.into();
    }
    if let Some(sampling) = args.subsampling {
        settings.subsampling = match sampling {
            CliSubsampling::S444 => Subsampling::S444,
            CliSubsampling::S422 => Subsampling::S422,
            CliSubsampling::S420 => Subsampling::S420,
        };
    }

    if let Some(only) = args.only.as_deref() {
        let names: Vec<&str> = only
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect();
        validate_effect_names(&names)?;
        for effect in presets::realistic_effects() {
            let current = settings.strengths.get(effect.id).copied().unwrap_or(0.0);
            let strength = if names.contains(&effect.id) {
                current
            } else {
                0.0
            };
            settings.strengths.insert(effect.id.to_owned(), strength);
        }
    }
    if let Some(skip) = args.skip.as_deref() {
        let names: Vec<&str> = skip
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect();
        validate_effect_names(&names)?;
        for name in names {
            settings.strengths.insert(name.to_owned(), 0.0);
        }
    }
    settings.validate();
    let frames = args.frames.clamp(1, 256);
    let fps = args.fps.clamp(1, 60);

    if input.is_dir() {
        let output_dir = args
            .output
            .clone()
            .unwrap_or_else(|| input.join("glitchforge"));
        fs::create_dir_all(&output_dir)
            .with_context(|| format!("Could not create {}", output_dir.display()))?;
        let mut files = Vec::new();
        for entry in
            fs::read_dir(input).with_context(|| format!("Could not read {}", input.display()))?
        {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() && is_supported_image(&path) {
                files.push(path);
            }
        }
        files.sort();
        if files.is_empty() {
            return Err(anyhow!("No supported images found in {}", input.display()));
        }
        for path in files {
            let destination = output_dir.join(format!(
                "{}_glitch.{}",
                file_stem(&path),
                preferred_extension(&path, args.format)
            ));
            process_file(&path, &destination, &settings, frames, args.gif, fps)?;
        }
    } else {
        let mut destination = args.output.clone().unwrap_or_else(|| {
            let extension = preferred_extension(input, args.format);
            input.with_file_name(format!("{}_glitch.{}", file_stem(input), extension))
        });
        if args.format.is_some() || destination.extension().is_none() {
            destination.set_extension(preferred_extension(input, args.format));
        }
        process_file(input, &destination, &settings, frames, args.gif, fps)?;
    }
    Ok(())
}

fn validate_effect_names(names: &[&str]) -> Result<()> {
    for name in names {
        if !presets::realistic_effects().iter().any(|e| e.id == *name) {
            return Err(anyhow!(
                "Unknown realistic effect `{name}`. Use `glitchforge --list` to see valid IDs."
            ));
        }
    }
    Ok(())
}
fn random_seed() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64
        ^ rand::thread_rng().gen::<u64>()
}
fn file_stem(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("image")
        .to_owned()
}
fn is_supported_image(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase()
            .as_str(),
        "png" | "jpg" | "jpeg" | "bmp" | "webp" | "tif" | "tiff" | "gif"
    )
}
fn preferred_extension(path: &Path, format: Option<CliFormat>) -> String {
    if let Some(f) = format {
        return match f {
            CliFormat::Png => "png",
            CliFormat::Jpeg => "jpg",
            CliFormat::Webp => "webp",
        }
        .to_owned();
    }
    match path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("png")
        .to_ascii_lowercase()
        .as_str()
    {
        "jpg" | "jpeg" => "jpg".into(),
        "webp" => "webp".into(),
        "bmp" => "bmp".into(),
        "tif" | "tiff" => "tiff".into(),
        "gif" => "gif".into(),
        _ => "png".into(),
    }
}

fn process_file(
    input: &Path,
    destination: &Path,
    base: &RenderSettings,
    frame_count: usize,
    gif: bool,
    fps: u32,
) -> Result<()> {
    let source_real = fs::canonicalize(input)
        .with_context(|| format!("Could not resolve input {}", input.display()))?;
    if fs::canonicalize(destination)
        .ok()
        .is_some_and(|destination_real| destination_real == source_real)
    {
        return Err(anyhow!(
            "Refusing to overwrite the source image: {}",
            input.display()
        ));
    }
    let source = render::load_image(input)?;
    let destination_ext = destination
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("png")
        .to_ascii_lowercase();
    if !matches!(
        destination_ext.as_str(),
        "png" | "jpg" | "jpeg" | "webp" | "bmp" | "tif" | "tiff" | "gif"
    ) {
        return Err(anyhow!("Unsupported output extension `.{destination_ext}`. Use PNG, JPEG, WebP, BMP, TIFF or GIF."));
    }
    let mut settings = base.clone();
    if !matches!(destination_ext.as_str(), "png" | "jpg" | "jpeg" | "webp") {
        settings.format = ExportFormat::Png;
    } else if destination_ext == "jpg" || destination_ext == "jpeg" {
        settings.format = ExportFormat::Jpeg;
    } else if destination_ext == "webp" {
        settings.format = ExportFormat::Webp;
    } else {
        settings.format = ExportFormat::Png;
    }

    // Stream GIF frames directly to a temporary file instead of retaining every
    // full-resolution variation in RAM. This keeps 40 MP × many frames bounded.
    let mut gif_temp: Option<(tempfile::NamedTempFile, PathBuf)> = None;
    let mut gif_encoder = None;
    let gif_delay = Delay::from_numer_denom_ms(1000, fps.max(1));
    if gif && frame_count > 1 {
        let gif_path = destination.with_extension("gif");
        let parent = gif_path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent)?;
        let temporary = tempfile::NamedTempFile::new_in(parent)
            .context("Could not create temporary animation file")?;
        let writer = temporary
            .reopen()
            .context("Could not open temporary animation stream")?;
        let mut encoder = GifEncoder::new(writer);
        encoder
            .set_repeat(Repeat::Infinite)
            .context("Could not configure GIF looping")?;
        gif_temp = Some((temporary, gif_path));
        gif_encoder = Some(encoder);
    }

    for index in 0..frame_count {
        settings.seed = base.seed.wrapping_add(index as u64);
        let rendered = render::render(&source, &settings).with_context(|| {
            format!(
                "Render failed for {} (frame {})",
                input.display(),
                index + 1
            )
        })?;
        let encoded = render::encode_image(
            &rendered,
            settings.format,
            settings.quality,
            settings.subsampling,
        )
        .with_context(|| {
            format!(
                "Encoding failed for {} (frame {})",
                input.display(),
                index + 1
            )
        })?;
        let path = if frame_count <= 1 {
            destination.to_path_buf()
        } else {
            destination.with_file_name(format!(
                "{}_{}.{}",
                file_stem(destination),
                format!("{index:03}"),
                destination
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("png")
            ))
        };
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            fs::create_dir_all(parent)?;
        }
        // For legacy CLI output extensions (BMP/TIFF/GIF), encode a valid image
        // with the selected container rather than writing mislabeled PNG bytes.
        if matches!(destination_ext.as_str(), "bmp" | "tif" | "tiff" | "gif") {
            let format = match destination_ext.as_str() {
                "bmp" => image::ImageFormat::Bmp,
                "tif" | "tiff" => image::ImageFormat::Tiff,
                _ => image::ImageFormat::Gif,
            };
            let mut cursor = std::io::Cursor::new(Vec::new());
            image::DynamicImage::ImageRgb8(rendered.clone()).write_to(&mut cursor, format)?;
            render::atomic_write(&path, cursor.get_ref())?;
        } else {
            render::atomic_write(&path, &encoded)
                .with_context(|| format!("Could not save {}", path.display()))?;
        }
        if let Some(encoder) = gif_encoder.as_mut() {
            let rgba = image::DynamicImage::ImageRgb8(rendered.clone()).to_rgba8();
            encoder
                .encode_frame(Frame::from_parts(rgba, 0, 0, gif_delay.clone()))
                .with_context(|| format!("Could not append animation frame {}", index + 1))?;
        }
        println!(
            "[ok] {} -> {} (seed {})",
            input.display(),
            path.display(),
            settings.seed
        );
    }
    drop(gif_encoder);
    if let Some((temporary, gif_path)) = gif_temp {
        temporary
            .persist(&gif_path)
            .map_err(|error| error.error)
            .with_context(|| format!("Could not commit animation {}", gif_path.display()))?;
        println!("[ok] animation -> {}", gif_path.display());
    }
    Ok(())
}
