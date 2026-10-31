use anyhow::{anyhow, Context, Result};
use image::{DynamicImage, ImageBuffer, ImageFormat, ImageReader, Rgb, RgbImage};
use jpeg_encoder::{ColorType as JpegColorType, Encoder as JpegEncoder, SamplingFactor};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::File,
    io::{BufReader, Cursor, Write},
    path::Path,
};

use crate::{effects, presets};

pub const MAX_IMAGE_PIXELS: u64 = 40_000_000;
pub const MAX_IMAGE_DIMENSION: u32 = 30_000;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "UPPERCASE")]
pub enum ExportFormat {
    Png,
    Jpeg,
    Webp,
}

impl Default for ExportFormat {
    fn default() -> Self {
        Self::Png
    }
}

impl ExportFormat {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
            Self::Webp => "webp",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Png => "PNG",
            Self::Jpeg => "JPEG",
            Self::Webp => "WebP",
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Subsampling {
    S444,
    S422,
    S420,
}

impl Default for Subsampling {
    fn default() -> Self {
        Self::S420
    }
}
impl Subsampling {
    pub fn label(self) -> &'static str {
        match self {
            Self::S444 => "4:4:4",
            Self::S422 => "4:2:2",
            Self::S420 => "4:2:0",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Grade {
    pub exposure: f32,
    pub contrast: f32,
    pub saturation: f32,
    pub gamma: f32,
}
impl Default for Grade {
    fn default() -> Self {
        Self {
            exposure: 0.0,
            contrast: 1.0,
            saturation: 1.0,
            gamma: 1.0,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct RenderSettings {
    pub preset: String,
    pub intensity: f32,
    pub seed: u64,
    pub strengths: BTreeMap<String, f32>,
    pub mix: f32,
    pub artistic_preset: Option<String>,
    pub artistic_intensity: f32,
    pub artistic_strengths: BTreeMap<String, f32>,
    pub artistic_mix: f32,
    pub grade: Grade,
    pub format: ExportFormat,
    pub quality: u8,
    pub subsampling: Subsampling,
}

impl Default for RenderSettings {
    fn default() -> Self {
        Self {
            preset: "edge".into(),
            intensity: 1.0,
            seed: 1,
            strengths: presets::realistic_preset("edge"),
            mix: 1.0,
            artistic_preset: None,
            artistic_intensity: 1.0,
            artistic_strengths: BTreeMap::new(),
            artistic_mix: 1.0,
            grade: Grade::default(),
            format: ExportFormat::Png,
            quality: 95,
            subsampling: Subsampling::S420,
        }
    }
}

impl RenderSettings {
    pub fn use_realistic_preset(&mut self, name: &str) {
        self.preset = name.to_owned();
        self.strengths = presets::realistic_preset(name);
    }
    pub fn use_artistic_preset(&mut self, name: Option<&str>) {
        self.artistic_preset = name.map(ToOwned::to_owned);
        self.artistic_strengths = name.map(presets::artistic_preset).unwrap_or_default();
    }
    pub fn validate(&mut self) {
        self.intensity = finite_clamp(self.intensity, 0.0, 4.0, 1.0);
        self.mix = finite_clamp(self.mix, 0.0, 1.0, 1.0);
        self.artistic_intensity = finite_clamp(self.artistic_intensity, 0.0, 4.0, 1.0);
        self.artistic_mix = finite_clamp(self.artistic_mix, 0.0, 1.0, 1.0);
        self.quality = self.quality.clamp(1, 100);
        self.grade.exposure = finite_clamp(self.grade.exposure, -3.0, 3.0, 0.0);
        self.grade.contrast = finite_clamp(self.grade.contrast, 0.0, 3.0, 1.0);
        self.grade.saturation = finite_clamp(self.grade.saturation, 0.0, 3.0, 1.0);
        self.grade.gamma = finite_clamp(self.grade.gamma, 0.25, 3.0, 1.0);
        for value in self.strengths.values_mut() {
            *value = finite_clamp(*value, 0.0, 2.0, 0.0);
        }
        for value in self.artistic_strengths.values_mut() {
            *value = finite_clamp(*value, 0.0, 2.0, 0.0);
        }
    }
}

fn finite_clamp(v: f32, lo: f32, hi: f32, fallback: f32) -> f32 {
    if v.is_finite() {
        v.clamp(lo, hi)
    } else {
        fallback
    }
}

#[derive(Clone)]
pub struct RenderedOutput {
    /// RGB image before output encoding.
    pub rendered: RgbImage,
    /// The exact bytes written on export.
    pub encoded: Vec<u8>,
    /// RGB image decoded back from `encoded`; this is what the UI previews.
    pub preview: RgbImage,
}

pub fn load_image(path: &Path) -> Result<RgbImage> {
    let reader = ImageReader::open(path)
        .with_context(|| format!("Could not open {}", path.display()))?
        .with_guessed_format()
        .context("Could not determine the image format")?;
    let (width, height) = reader
        .into_dimensions()
        .context("Could not read image dimensions")?;
    validate_dimensions(width, height)?;
    let image =
        image::open(path).with_context(|| format!("Could not decode {}", path.display()))?;
    let mut rgb = image.to_rgb8();
    validate_dimensions(rgb.width(), rgb.height())?;

    // Match the Python prototype's EXIF-aware loading when an orientation tag exists.
    // Metadata parsing is best-effort; malformed optional EXIF must not block a valid image.
    if let Ok(file) = File::open(path) {
        let mut reader = BufReader::new(file);
        if let Ok(exif) = exif::Reader::new().read_from_container(&mut reader) {
            if let Some(field) = exif.get_field(exif::Tag::Orientation, exif::In::PRIMARY) {
                if let Some(orientation) = field.value.get_uint(0) {
                    rgb = apply_exif_orientation(rgb, orientation);
                }
            }
        }
    }
    validate_dimensions(rgb.width(), rgb.height())?;
    Ok(rgb)
}

fn apply_exif_orientation(image: RgbImage, orientation: u32) -> RgbImage {
    match orientation {
        2 => image::imageops::flip_horizontal(&image),
        3 => image::imageops::rotate180(&image),
        4 => image::imageops::flip_vertical(&image),
        5 => image::imageops::flip_horizontal(&image::imageops::rotate90(&image)),
        6 => image::imageops::rotate90(&image),
        7 => image::imageops::flip_vertical(&image::imageops::rotate90(&image)),
        8 => image::imageops::rotate270(&image),
        _ => image,
    }
}

pub fn validate_dimensions(width: u32, height: u32) -> Result<()> {
    if width == 0 || height == 0 {
        return Err(anyhow!("The image has zero dimensions."));
    }
    if width > MAX_IMAGE_DIMENSION || height > MAX_IMAGE_DIMENSION {
        return Err(anyhow!(
            "Image dimensions exceed the {} px safety limit.",
            MAX_IMAGE_DIMENSION
        ));
    }
    let pixels = u64::from(width) * u64::from(height);
    if pixels > MAX_IMAGE_PIXELS {
        return Err(anyhow!(
            "Image contains {pixels} pixels; the current limit is {MAX_IMAGE_PIXELS}."
        ));
    }
    Ok(())
}

pub fn render(source: &RgbImage, settings: &RenderSettings) -> Result<RgbImage> {
    validate_dimensions(source.width(), source.height())?;
    let mut cfg = settings.clone();
    cfg.validate();
    let original = effects::Raster::from_rgb(source);
    let mut pixels = original.clone();

    for (index, effect) in presets::realistic_effects().iter().enumerate() {
        let base_strength = cfg.strengths.get(effect.id).copied().unwrap_or(0.0);
        let strength = (base_strength * cfg.intensity).clamp(0.0, 3.0);
        if strength <= 0.005 {
            continue;
        }
        let effect_seed = cfg.seed ^ ((index as u64 + 1).wrapping_mul(0x9E37_79B1));
        pixels = effects::apply_realistic(effect.id, &pixels, strength, effect_seed)?;
    }
    if cfg.mix < 1.0 {
        pixels = effects::blend(&original, &pixels, cfg.mix);
    }

    let artistic_base = pixels.clone();
    for (index, effect) in presets::artistic_effects().iter().enumerate() {
        let base_strength = cfg
            .artistic_strengths
            .get(effect.id)
            .copied()
            .unwrap_or(0.0);
        let strength = (base_strength * cfg.artistic_intensity).clamp(0.0, 3.0);
        if strength <= 0.005 {
            continue;
        }
        let effect_seed = cfg.seed ^ 0xA71C_57D3 ^ ((index as u64 + 1).wrapping_mul(0x85EB_CA77));
        pixels = effects::apply_artistic(effect.id, &pixels, strength, effect_seed)?;
    }
    if cfg.artistic_mix < 1.0 && cfg.artistic_strengths.values().any(|v| *v > 0.005) {
        pixels = effects::blend(&artistic_base, &pixels, cfg.artistic_mix);
    }

    let mut output = pixels.into_rgb();
    apply_grade(&mut output, &cfg.grade);
    Ok(output)
}

pub fn render_encoded(source: &RgbImage, settings: &RenderSettings) -> Result<RenderedOutput> {
    let rendered = render(source, settings)?;
    let encoded = encode_image(
        &rendered,
        settings.format,
        settings.quality,
        settings.subsampling,
    )?;
    let preview = decode_image(&encoded)?;
    Ok(RenderedOutput {
        rendered,
        encoded,
        preview,
    })
}

pub fn apply_grade(image: &mut RgbImage, grade: &Grade) {
    let exposure = finite_clamp(grade.exposure, -3.0, 3.0, 0.0);
    let contrast = finite_clamp(grade.contrast, 0.0, 3.0, 1.0);
    let saturation = finite_clamp(grade.saturation, 0.0, 3.0, 1.0);
    let gamma = finite_clamp(grade.gamma, 0.25, 3.0, 1.0);
    let exposure_gain = 2.0_f32.powf(exposure);
    for px in image.pixels_mut() {
        let mut rgb = [
            px[0] as f32 / 255.0,
            px[1] as f32 / 255.0,
            px[2] as f32 / 255.0,
        ];
        for c in 0..3 {
            rgb[c] = ((rgb[c] * exposure_gain - 0.5) * contrast + 0.5).clamp(0.0, 1.0);
        }
        let luma = rgb[0] * 0.2126 + rgb[1] * 0.7152 + rgb[2] * 0.0722;
        for c in 0..3 {
            rgb[c] = (luma + (rgb[c] - luma) * saturation).clamp(0.0, 1.0);
            rgb[c] = rgb[c].powf(1.0 / gamma).clamp(0.0, 1.0);
            px[c] = (rgb[c] * 255.0).round() as u8;
        }
    }
}

pub fn encode_image(
    image: &RgbImage,
    format: ExportFormat,
    quality: u8,
    subsampling: Subsampling,
) -> Result<Vec<u8>> {
    let quality = quality.clamp(1, 100);
    match format {
        ExportFormat::Png => {
            let mut cursor = Cursor::new(Vec::new());
            DynamicImage::ImageRgb8(image.clone())
                .write_to(&mut cursor, ImageFormat::Png)
                .context("PNG encoding failed")?;
            Ok(cursor.into_inner())
        }
        ExportFormat::Jpeg => {
            let mut buffer = Vec::new();
            let mut encoder = JpegEncoder::new(&mut buffer, quality);
            let factor = match subsampling {
                Subsampling::S444 => SamplingFactor::F_1_1,
                Subsampling::S422 => SamplingFactor::F_2_1,
                Subsampling::S420 => SamplingFactor::F_2_2,
            };
            encoder.set_sampling_factor(factor);
            encoder
                .encode(
                    image.as_raw(),
                    image.width() as u16,
                    image.height() as u16,
                    JpegColorType::Rgb,
                )
                .context("JPEG encoding failed")?;
            Ok(buffer)
        }
        ExportFormat::Webp => {
            let encoded = webp::Encoder::from_rgb(image.as_raw(), image.width(), image.height())
                .encode(quality as f32);
            Ok(encoded.to_vec())
        }
    }
}

pub fn decode_image(bytes: &[u8]) -> Result<RgbImage> {
    let image = image::load_from_memory(bytes).context("Could not decode the encoded preview")?;
    Ok(image.to_rgb8())
}

pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)
        .with_context(|| format!("Could not create {}", parent.display()))?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)
        .context("Could not create a temporary export file")?;
    temporary
        .write_all(bytes)
        .context("Could not write temporary export data")?;
    temporary
        .flush()
        .context("Could not flush temporary export data")?;
    temporary
        .as_file()
        .sync_all()
        .context("Could not sync temporary export data")?;
    temporary
        .persist(path)
        .map_err(|error| error.error)
        .with_context(|| format!("Could not commit export to {}", path.display()))?;
    Ok(())
}

pub fn save_rgb_image(image: &RgbImage, path: &Path, settings: &RenderSettings) -> Result<()> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let legacy_format = match ext.as_str() {
        "bmp" => Some(ImageFormat::Bmp),
        "tif" | "tiff" => Some(ImageFormat::Tiff),
        "gif" => Some(ImageFormat::Gif),
        _ => None,
    };
    let bytes = if let Some(format) = legacy_format {
        let mut cursor = Cursor::new(Vec::new());
        DynamicImage::ImageRgb8(image.clone())
            .write_to(&mut cursor, format)
            .with_context(|| format!("Could not encode {} image", ext.to_ascii_uppercase()))?;
        cursor.into_inner()
    } else {
        let format = match ext.as_str() {
            "jpg" | "jpeg" => ExportFormat::Jpeg,
            "webp" => ExportFormat::Webp,
            "png" => ExportFormat::Png,
            _ => settings.format,
        };
        encode_image(image, format, settings.quality, settings.subsampling)?
    };
    atomic_write(path, &bytes)?;
    Ok(())
}

pub fn rgb_from_raw(width: u32, height: u32, data: Vec<u8>) -> Result<RgbImage> {
    ImageBuffer::<Rgb<u8>, Vec<u8>>::from_raw(width, height, data)
        .ok_or_else(|| anyhow!("Invalid RGB buffer dimensions"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_seed_produces_same_image() {
        let source = RgbImage::from_fn(96, 64, |x, y| {
            Rgb([(x % 255) as u8, (y % 255) as u8, ((x + y) % 255) as u8])
        });
        let settings = RenderSettings::default();
        let first = render(&source, &settings).unwrap();
        let second = render(&source, &settings).unwrap();
        assert_eq!(first.as_raw(), second.as_raw());
    }

    #[test]
    fn output_preview_is_decoded_from_export_bytes() {
        let source = RgbImage::from_pixel(32, 24, Rgb([80, 120, 170]));
        let mut settings = RenderSettings::default();
        settings.format = ExportFormat::Png;
        let result = render_encoded(&source, &settings).unwrap();
        assert_eq!(result.preview, decode_image(&result.encoded).unwrap());
    }

    #[test]
    fn rejects_excessive_dimensions() {
        assert!(validate_dimensions(50_001, 20).is_err());
        assert!(validate_dimensions(10_000, 10_001).is_err());
    }

    #[test]
    fn applies_exif_rotation_orientation() {
        let image = RgbImage::from_fn(2, 3, |x, y| Rgb([(x + 1) as u8, (y + 1) as u8, 0]));
        let rotated = apply_exif_orientation(image.clone(), 6);
        assert_eq!((rotated.width(), rotated.height()), (3, 2));
        assert_eq!(*rotated.get_pixel(2, 0), *image.get_pixel(0, 0));
    }

    #[test]
    fn atomic_write_replaces_existing_destination() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("output.bin");
        std::fs::write(&path, b"old").unwrap();
        atomic_write(&path, b"new data").unwrap();
        assert_eq!(std::fs::read(path).unwrap(), b"new data");
    }
}
