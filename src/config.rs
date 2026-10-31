use anyhow::{anyhow, Context, Result};
use serde_json::{json, Map, Value};
use std::{collections::BTreeMap, path::Path};

use crate::{
    presets,
    render::{ExportFormat, Grade, RenderSettings, Subsampling},
};

#[derive(Clone, Debug)]
pub struct ConfigBundle {
    pub settings: RenderSettings,
    pub ui_preset: String,
    pub ui_artistic_preset: String,
}

impl Default for ConfigBundle {
    fn default() -> Self {
        Self {
            settings: RenderSettings::default(),
            ui_preset: "edge".into(),
            ui_artistic_preset: "None".into(),
        }
    }
}

fn number(value: Option<&Value>, fallback: f32) -> f32 {
    value
        .and_then(Value::as_f64)
        .map(|v| v as f32)
        .filter(|v| v.is_finite())
        .unwrap_or(fallback)
}
fn integer(value: Option<&Value>, fallback: u64) -> u64 {
    value
        .and_then(Value::as_u64)
        .or_else(|| value.and_then(Value::as_i64).map(|v| v.max(0) as u64))
        .unwrap_or(fallback)
}
fn text<'a>(value: Option<&'a Value>, fallback: &'a str) -> &'a str {
    value.and_then(Value::as_str).unwrap_or(fallback)
}
fn object_map(value: Option<&Value>) -> BTreeMap<String, f32> {
    let mut out = BTreeMap::new();
    if let Some(object) = value.and_then(Value::as_object) {
        for (key, value) in object {
            out.insert(key.clone(), number(Some(value), 0.0));
        }
    }
    out
}
fn subsampling_from_text(value: &str) -> Subsampling {
    match value {
        "4:4:4" => Subsampling::S444,
        "4:2:2" => Subsampling::S422,
        _ => Subsampling::S420,
    }
}
fn format_from_text(value: &str) -> ExportFormat {
    match value.to_ascii_uppercase().as_str() {
        "JPEG" | "JPG" => ExportFormat::Jpeg,
        "WEBP" => ExportFormat::Webp,
        _ => ExportFormat::Png,
    }
}

pub fn load_config(path: &Path) -> Result<ConfigBundle> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("Could not read configuration {}", path.display()))?;
    let payload: Value =
        serde_json::from_str(&raw).context("The configuration file is not valid JSON")?;
    let app = payload.get("application").and_then(Value::as_str);
    if app.is_some() && app != Some("GLITCHFORGE") {
        return Err(anyhow!("This file is not a GlitchForge configuration."));
    }
    let settings = payload.get("settings").unwrap_or(&payload);
    let mut out = RenderSettings::default();
    out.preset = text(settings.get("preset"), "edge").to_owned();
    out.intensity = number(settings.get("intensity"), 1.0);
    out.seed = integer(settings.get("seed"), 1);
    out.mix = number(settings.get("mix"), 1.0);
    out.artistic_preset = settings
        .get("artistic_preset")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    out.artistic_intensity = number(settings.get("artistic_intensity"), 1.0);
    out.artistic_mix = number(settings.get("artistic_mix"), 1.0);
    out.quality = integer(settings.get("quality"), 95).clamp(1, 100) as u8;
    out.format = format_from_text(text(settings.get("format"), "PNG"));
    out.subsampling = subsampling_from_text(text(settings.get("subsampling"), "4:2:0"));

    if let Some(grade) = settings.get("grade") {
        out.grade = Grade {
            exposure: number(grade.get("exposure"), 0.0),
            contrast: number(grade.get("contrast"), 1.0),
            saturation: number(grade.get("saturation"), 1.0),
            gamma: number(grade.get("gamma"), 1.0),
        };
    }
    out.strengths = object_map(settings.get("strengths"));
    out.artistic_strengths = object_map(settings.get("artistic_strengths"));

    // Python prototype config v2 stores enabled/strength pairs beside settings.
    let realistic_controls = payload
        .get("realistic_effect_controls")
        .or_else(|| payload.get("effect_controls"));
    if let Some(controls) = realistic_controls.and_then(Value::as_object) {
        for (id, value) in controls {
            let enabled = value
                .get("enabled")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            let strength = number(
                value.get("strength"),
                *out.strengths.get(id).unwrap_or(&0.0),
            );
            out.strengths
                .insert(id.clone(), if enabled { strength } else { 0.0 });
        }
    }
    if let Some(controls) = payload
        .get("artistic_effect_controls")
        .and_then(Value::as_object)
    {
        for (id, value) in controls {
            let enabled = value
                .get("enabled")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            let strength = number(
                value.get("strength"),
                *out.artistic_strengths.get(id).unwrap_or(&0.0),
            );
            out.artistic_strengths
                .insert(id.clone(), if enabled { strength } else { 0.0 });
        }
    }

    // Config files saved by the Python UI can name the currently selected UI preset
    // separately from the resolved `preset` used by the render engine.
    let ui_preset = settings
        .get("ui_preset")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| out.preset.clone());
    let ui_artistic_preset = settings
        .get("ui_artistic_preset")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| {
            settings
                .get("artistic_preset")
                .and_then(Value::as_str)
                .unwrap_or("None")
                .to_owned()
        });
    if !presets::all_realistic_preset_ids().contains(&ui_preset.as_str()) && ui_preset != "Custom" {
        out.preset = "edge".into();
    }
    out.validate();
    Ok(ConfigBundle {
        settings: out,
        ui_preset,
        ui_artistic_preset,
    })
}

pub fn save_config(path: &Path, bundle: &ConfigBundle) -> Result<()> {
    let mut settings = bundle.settings.clone();
    settings.validate();
    let mut realistic_strengths = Map::new();
    for effect in presets::realistic_effects() {
        realistic_strengths.insert(
            effect.id.to_owned(),
            json!(settings.strengths.get(effect.id).copied().unwrap_or(0.0)),
        );
    }
    let mut artistic_strengths = Map::new();
    for effect in presets::artistic_effects() {
        artistic_strengths.insert(
            effect.id.to_owned(),
            json!(settings
                .artistic_strengths
                .get(effect.id)
                .copied()
                .unwrap_or(0.0)),
        );
    }
    let mut realistic_controls = Map::new();
    for effect in presets::realistic_effects() {
        let strength = settings.strengths.get(effect.id).copied().unwrap_or(0.0);
        realistic_controls.insert(
            effect.id.to_owned(),
            json!({ "enabled": strength > 0.0, "strength": strength.max(0.5) }),
        );
    }
    let mut artistic_controls = Map::new();
    for effect in presets::artistic_effects() {
        let strength = settings
            .artistic_strengths
            .get(effect.id)
            .copied()
            .unwrap_or(0.0);
        artistic_controls.insert(
            effect.id.to_owned(),
            json!({ "enabled": strength > 0.0, "strength": strength.max(0.5) }),
        );
    }
    let payload = json!({
        "application": "GLITCHFORGE",
        "config_version": 3,
        "settings": {
            "preset": settings.preset,
            "ui_preset": bundle.ui_preset,
            "intensity": settings.intensity,
            "seed": settings.seed,
            "strengths": realistic_strengths,
            "mix": settings.mix,
            "artistic_preset": settings.artistic_preset,
            "ui_artistic_preset": bundle.ui_artistic_preset,
            "artistic_intensity": settings.artistic_intensity,
            "artistic_strengths": artistic_strengths,
            "artistic_mix": settings.artistic_mix,
            "grade": {
                "exposure": settings.grade.exposure,
                "contrast": settings.grade.contrast,
                "saturation": settings.grade.saturation,
                "gamma": settings.grade.gamma
            },
            "format": settings.format.label(),
            "quality": settings.quality,
            "subsampling": settings.subsampling.label()
        },
        "effect_controls": realistic_controls,
        "artistic_effect_controls": artistic_controls
    });
    let bytes = serde_json::to_vec_pretty(&payload)?;
    crate::render::atomic_write(path, &bytes)
        .with_context(|| format!("Could not write configuration {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn round_trips_python_compatible_config() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fx.json");
        save_config(&path, &ConfigBundle::default()).unwrap();
        let loaded = load_config(&path).unwrap();
        assert_eq!(loaded.settings.seed, 1);
        assert_eq!(loaded.ui_preset, "edge");
    }

    #[test]
    fn imports_python_prototype_v2_effect_controls() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("python-v2.json");
        let fixture = r#"{
          "application": "GLITCHFORGE",
          "config_version": 2,
          "settings": {
            "preset": "edge",
            "ui_preset": "Custom",
            "intensity": 1.25,
            "seed": 42,
            "mix": 0.75,
            "strengths": {"edge_corruption": 1.0, "slice_shift": 0.35},
            "artistic_preset": null,
            "ui_artistic_preset": "None",
            "artistic_strengths": {},
            "artistic_mix": 1.0,
            "grade": {"exposure": 0.25, "contrast": 1.1, "saturation": 0.9, "gamma": 1.0},
            "format": "WEBP", "quality": 88, "subsampling": "4:2:0"
          },
          "effect_controls": {
            "edge_corruption": {"enabled": false, "strength": 1.0},
            "slice_shift": {"enabled": true, "strength": 0.6}
          },
          "artistic_effect_controls": {}
        }"#;
        std::fs::write(&path, fixture).unwrap();
        let loaded = load_config(&path).unwrap();
        assert_eq!(loaded.settings.seed, 42);
        assert_eq!(loaded.ui_preset, "Custom");
        assert_eq!(loaded.settings.strengths["edge_corruption"], 0.0);
        assert_eq!(loaded.settings.strengths["slice_shift"], 0.6);
        assert_eq!(loaded.settings.format, ExportFormat::Webp);
        assert!((loaded.settings.grade.exposure - 0.25).abs() < f32::EPSILON);
    }
}
