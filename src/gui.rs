use eframe::egui::{self, Color32, RichText, TextureHandle, TextureOptions, Vec2};
use image::RgbImage;
use rand::Rng;
use std::{
    path::PathBuf,
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::{Duration, Instant},
};

use crate::{
    config::{self, ConfigBundle},
    presets,
    render::{self, ExportFormat, RenderSettings, Subsampling},
};

const BG: Color32 = Color32::from_rgb(13, 16, 20);
const PANEL: Color32 = Color32::from_rgb(22, 27, 34);
const PANEL_ALT: Color32 = Color32::from_rgb(29, 36, 45);
const PREVIEW_BG: Color32 = Color32::from_rgb(9, 12, 16);
const BORDER: Color32 = Color32::from_rgb(48, 59, 72);
const FG: Color32 = Color32::from_rgb(231, 237, 244);
const MUTED: Color32 = Color32::from_rgb(143, 157, 173);
const ACCENT: Color32 = Color32::from_rgb(130, 239, 163);
const ACCENT_DARK: Color32 = Color32::from_rgb(35, 91, 62);

#[derive(Clone, Copy, PartialEq, Eq)]
enum MainTab {
    Project,
    Effects,
    Grade,
    Export,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum EffectLayer {
    Realistic,
    Artistic,
}

struct RenderMessage {
    revision: u64,
    elapsed: Duration,
    result: anyhow::Result<render::RenderedOutput>,
}

pub struct GlitchForgeApp {
    source: Option<RgbImage>,
    source_path: Option<PathBuf>,
    source_texture: Option<TextureHandle>,
    output_texture: Option<TextureHandle>,
    rendered: Option<RgbImage>,
    encoded: Option<Vec<u8>>,
    settings: RenderSettings,
    ui_preset: String,
    ui_artistic_preset: String,
    tab: MainTab,
    effect_layer: EffectLayer,
    status: String,
    last_error: Option<String>,
    sender: Sender<RenderMessage>,
    receiver: Receiver<RenderMessage>,
    revision: u64,
    rendered_revision: Option<u64>,
    dirty_at: Option<Instant>,
    rendering: bool,
    show_help: bool,
    show_about: bool,
    last_render_duration: Option<Duration>,
}

impl GlitchForgeApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut visuals = egui::Visuals::dark();
        visuals.dark_mode = true;
        visuals.panel_fill = BG;
        visuals.window_fill = PANEL;
        visuals.extreme_bg_color = PREVIEW_BG;
        visuals.faint_bg_color = PANEL_ALT;
        visuals.override_text_color = Some(FG);
        visuals.selection.bg_fill = ACCENT_DARK;
        visuals.selection.stroke = egui::Stroke::new(1.0_f32, ACCENT);
        visuals.widgets.noninteractive.bg_fill = PANEL;
        visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0_f32, MUTED);
        visuals.widgets.inactive.bg_fill = PANEL_ALT;
        visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0_f32, FG);
        visuals.widgets.hovered.bg_fill = Color32::from_rgb(40, 51, 62);
        visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0_f32, ACCENT);
        visuals.widgets.active.bg_fill = ACCENT_DARK;
        visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0_f32, ACCENT);
        cc.egui_ctx.set_visuals(visuals);
        cc.egui_ctx.style_mut(|style| {
            style.spacing.item_spacing = Vec2::new(8.0, 8.0);
            style.spacing.button_padding = Vec2::new(12.0, 7.0);
            style.visuals.window_rounding = egui::Rounding::same(10.0);
            style.visuals.menu_rounding = egui::Rounding::same(8.0);
            style.visuals.widgets.noninteractive.rounding = egui::Rounding::same(8.0);
            style.visuals.widgets.inactive.rounding = egui::Rounding::same(7.0);
            style.visuals.widgets.hovered.rounding = egui::Rounding::same(7.0);
            style.visuals.widgets.active.rounding = egui::Rounding::same(7.0);
        });
        let (sender, receiver) = mpsc::channel();
        Self {
            source: None,
            source_path: None,
            source_texture: None,
            output_texture: None,
            rendered: None,
            encoded: None,
            settings: RenderSettings::default(),
            ui_preset: "edge".into(),
            ui_artistic_preset: "None".into(),
            tab: MainTab::Project,
            effect_layer: EffectLayer::Realistic,
            status: "Drop an image here or open a file to start.".into(),
            last_error: None,
            sender,
            receiver,
            revision: 0,
            rendered_revision: None,
            dirty_at: None,
            rendering: false,
            show_help: false,
            show_about: false,
            last_render_duration: None,
        }
    }

    fn mark_dirty(&mut self) {
        if self.source.is_none() {
            return;
        }
        self.revision = self.revision.wrapping_add(1);
        self.dirty_at = Some(Instant::now());
        self.rendered_revision = None;
        self.status = "Settings updated · preparing preview…".into();
    }

    fn start_render(&mut self) {
        let Some(source) = self.source.clone() else {
            return;
        };
        self.dirty_at = None;
        let revision = self.revision;
        let settings = self.settings.clone();
        let sender = self.sender.clone();
        self.rendering = true;
        self.status = "Rendering full-resolution image…".into();
        let spawn_result = thread::Builder::new()
            .name("glitchforge-render".into())
            .spawn(move || {
                let start = Instant::now();
                let result = render::render_encoded(&source, &settings);
                let elapsed = start.elapsed();
                let _ = sender.send(RenderMessage {
                    revision,
                    elapsed,
                    result,
                });
            });
        if let Err(error) = spawn_result {
            self.rendering = false;
            self.last_error = Some(format!("Could not start render worker: {error}"));
            self.status = "Render worker could not start · see Help".into();
        }
    }

    fn poll_render(&mut self, ctx: &egui::Context) {
        while let Ok(message) = self.receiver.try_recv() {
            self.rendering = false;
            if message.revision != self.revision {
                continue;
            }
            match message.result {
                Ok(output) => {
                    let preview = output.preview.clone();
                    self.output_texture = Some(ctx.load_texture(
                        "glitchforge-output",
                        rgb_to_color_image(&preview),
                        TextureOptions::LINEAR,
                    ));
                    self.rendered = Some(output.rendered);
                    self.encoded = Some(output.encoded);
                    self.rendered_revision = Some(message.revision);
                    self.last_render_duration = Some(message.elapsed);
                    if let (Some(source), Some(encoded)) = (&self.source, &self.encoded) {
                        self.status = format!(
                            "Preview ready  ·  {} × {}  ·  {}  ·  {} bytes  ·  {:.2}s",
                            source.width(),
                            source.height(),
                            self.settings.format.label(),
                            encoded.len(),
                            message.elapsed.as_secs_f32()
                        );
                    }
                    self.last_error = None;
                }
                Err(error) => {
                    self.last_error = Some(format!("{error:#}"));
                    self.status = "Render failed · open Help for troubleshooting".into();
                }
            }
        }
    }

    fn open_image(&mut self, ctx: &egui::Context) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter(
                "Image files",
                &["png", "jpg", "jpeg", "webp", "bmp", "tif", "tiff", "gif"],
            )
            .pick_file()
        {
            self.load_path(path, ctx);
        }
    }

    fn load_path(&mut self, path: PathBuf, ctx: &egui::Context) {
        match render::load_image(&path) {
            Ok(image) => {
                let name = path
                    .file_name()
                    .and_then(|v| v.to_str())
                    .unwrap_or("image")
                    .to_owned();
                self.source_texture = Some(ctx.load_texture(
                    "glitchforge-source",
                    rgb_to_color_image(&image),
                    TextureOptions::LINEAR,
                ));
                self.source = Some(image.clone());
                self.source_path = Some(path);
                self.rendered = None;
                self.encoded = None;
                self.output_texture = None;
                self.last_render_duration = None;
                self.status = format!("Loaded {name}  ·  {} × {}", image.width(), image.height());
                self.last_error = None;
                self.mark_dirty();
            }
            Err(error) => {
                self.last_error = Some(format!("{error:#}"));
                self.status = "Could not load image · see error details".into();
            }
        }
    }

    fn save_config_dialog(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("GlitchForge config", &["json"])
            .set_file_name("glitchforge_config.json")
            .save_file()
        else {
            return;
        };
        let bundle = ConfigBundle {
            settings: self.settings.clone(),
            ui_preset: self.ui_preset.clone(),
            ui_artistic_preset: self.ui_artistic_preset.clone(),
        };
        match config::save_config(&path, &bundle) {
            Ok(()) => {
                self.status = format!(
                    "Configuration saved · {}",
                    path.file_name()
                        .and_then(|v| v.to_str())
                        .unwrap_or("config.json")
                )
            }
            Err(error) => {
                self.last_error = Some(format!("{error:#}"));
                self.status = "Could not save configuration".into();
            }
        }
    }

    fn load_config_dialog(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("GlitchForge config", &["json"])
            .pick_file()
        else {
            return;
        };
        match config::load_config(&path) {
            Ok(bundle) => {
                self.settings = bundle.settings;
                self.ui_preset = bundle.ui_preset;
                self.ui_artistic_preset = bundle.ui_artistic_preset;
                self.mark_dirty();
                self.status = format!(
                    "Loaded configuration · {}",
                    path.file_name()
                        .and_then(|v| v.to_str())
                        .unwrap_or("config.json")
                );
                self.last_error = None;
            }
            Err(error) => {
                self.last_error = Some(format!("{error:#}"));
                self.status = "Could not load configuration".into();
            }
        }
    }

    fn export_dialog(&mut self, ctx: &egui::Context) {
        let Some(source) = self.source.clone() else {
            self.status = "Open an image before exporting.".into();
            return;
        };
        let suggested = self
            .source_path
            .as_ref()
            .map(|path| {
                format!(
                    "{}_glitchforge.{}",
                    path.file_stem().and_then(|v| v.to_str()).unwrap_or("image"),
                    self.settings.format.extension()
                )
            })
            .unwrap_or_else(|| format!("glitchforge_export.{}", self.settings.format.extension()));
        let Some(path) = rfd::FileDialog::new()
            .add_filter(
                self.settings.format.label(),
                &[self.settings.format.extension()],
            )
            .set_file_name(&suggested)
            .save_file()
        else {
            return;
        };
        let mut path = path;
        path.set_extension(self.settings.format.extension());
        let bytes = if self.rendered_revision == Some(self.revision) {
            self.encoded.clone()
        } else {
            self.status = "Rendering full-resolution export…".into();
            match render::render_encoded(&source, &self.settings) {
                Ok(result) => {
                    let preview = result.preview.clone();
                    self.output_texture = Some(ctx.load_texture(
                        "glitchforge-output",
                        rgb_to_color_image(&preview),
                        TextureOptions::LINEAR,
                    ));
                    self.rendered = Some(result.rendered);
                    self.encoded = Some(result.encoded.clone());
                    self.rendered_revision = Some(self.revision);
                    Some(result.encoded)
                }
                Err(error) => {
                    self.last_error = Some(format!("{error:#}"));
                    None
                }
            }
        };
        let Some(bytes) = bytes else {
            return;
        };
        if let Some(source_path) = &self.source_path {
            let same_file = std::fs::canonicalize(source_path)
                .ok()
                .zip(std::fs::canonicalize(&path).ok())
                .is_some_and(|(source, destination)| source == destination);
            if same_file {
                self.last_error = Some(
                    "The export destination is the source image. Choose a different filename."
                        .into(),
                );
                self.status = "Export blocked · source images are never overwritten".into();
                return;
            }
        }
        match render::atomic_write(&path, &bytes) {
            Ok(()) => {
                self.status = format!(
                    "Exported {}  ·  {} bytes",
                    path.file_name().and_then(|v| v.to_str()).unwrap_or("image"),
                    bytes.len()
                )
            }
            Err(error) => {
                self.last_error = Some(format!("Could not save export: {error}"));
                self.status = "Export failed".into();
            }
        }
    }

    fn reset(&mut self) {
        self.settings = RenderSettings::default();
        self.ui_preset = "edge".into();
        self.ui_artistic_preset = "None".into();
        self.mark_dirty();
        self.status = "Settings reset to defaults.".into();
    }

    fn randomize_seed(&mut self) {
        self.settings.seed = rand::thread_rng().gen::<u64>();
        self.mark_dirty();
    }

    fn draw_topbar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("glitchforge_topbar")
            .frame(
                egui::Frame::none()
                    .fill(PANEL)
                    .inner_margin(egui::Margin::symmetric(18.0, 12.0)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new("GLITCHFORGE")
                                    .size(20.0)
                                    .strong()
                                    .color(ACCENT),
                            );
                            ui.add_space(8.0);
                            ui.label(
                                RichText::new("IMAGE ARTIFACT LABORATORY")
                                    .size(9.0)
                                    .strong()
                                    .color(MUTED),
                            );
                        });
                        ui.label(
                            RichText::new(
                                "Deterministic signal damage · artistic image processing",
                            )
                            .size(10.0)
                            .color(MUTED),
                        );
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add_enabled(
                                self.source.is_some(),
                                egui::Button::new(
                                    RichText::new("Export full resolution")
                                        .strong()
                                        .color(Color32::from_rgb(7, 27, 15)),
                                )
                                .fill(ACCENT),
                            )
                            .clicked()
                        {
                            self.export_dialog(ctx);
                        }
                        if ui.button("? Help").clicked() {
                            self.show_help = true;
                        }
                        if ui.button("About").clicked() {
                            self.show_about = true;
                        }
                        if ui.button("Reset").clicked() {
                            self.reset();
                        }
                        if ui.button("Re-roll seed").clicked() {
                            self.randomize_seed();
                        }
                        if ui.button("Save config").clicked() {
                            self.save_config_dialog();
                        }
                        if ui.button("Load config").clicked() {
                            self.load_config_dialog();
                        }
                        if ui.button("Open image").clicked() {
                            self.open_image(ctx);
                        }
                    });
                });
            });
    }

    fn draw_sidebar(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("glitchforge_controls")
            .resizable(true)
            .default_width(360.0)
            .width_range(300.0..=460.0)
            .frame(
                egui::Frame::none()
                    .fill(PANEL)
                    .inner_margin(egui::Margin::same(12.0)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    for (tab, label) in [
                        (MainTab::Project, "PROJECT"),
                        (MainTab::Effects, "EFFECTS"),
                        (MainTab::Grade, "GRADE"),
                        (MainTab::Export, "OUTPUT"),
                    ] {
                        let selected = self.tab == tab;
                        let button = egui::Button::new(
                            RichText::new(label).size(10.0).strong().color(if selected {
                                ACCENT
                            } else {
                                MUTED
                            }),
                        )
                        .fill(if selected { ACCENT_DARK } else { PANEL })
                        .stroke(egui::Stroke::new(
                            1.0_f32,
                            if selected { ACCENT_DARK } else { PANEL },
                        ));
                        if ui.add(button).clicked() {
                            self.tab = tab;
                        }
                    }
                });
                ui.separator();
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| match self.tab {
                        MainTab::Project => self.show_project_controls(ui),
                        MainTab::Effects => self.show_effect_controls(ui),
                        MainTab::Grade => self.show_grade_controls(ui),
                        MainTab::Export => self.show_export_controls(ui),
                    });
            });
    }

    fn show_project_controls(&mut self, ui: &mut egui::Ui) {
        section_title(
            ui,
            "PROJECT SETUP",
            "Choose a starting profile, then fine-tune the layers.",
        );
        ui.label(
            RichText::new("REALISTIC PROFILE")
                .size(9.0)
                .strong()
                .color(MUTED),
        );
        let mut preset_changed = false;
        egui::ComboBox::from_id_salt("realistic_preset")
            .selected_text(preset_label(&self.ui_preset))
            .width(ui.available_width())
            .show_ui(ui, |ui| {
                for preset in presets::realistic_presets() {
                    preset_changed |= ui
                        .selectable_value(
                            &mut self.ui_preset,
                            preset.id.to_owned(),
                            format!("{}  ·  {}", preset.label, preset.description),
                        )
                        .changed();
                }
                preset_changed |= ui
                    .selectable_value(&mut self.ui_preset, "Custom".into(), "Custom profile")
                    .changed();
            });
        if preset_changed && self.ui_preset != "Custom" {
            self.settings.use_realistic_preset(&self.ui_preset);
            self.mark_dirty();
        }
        ui.add_space(8.0);
        let mut changed = false;
        ui.horizontal(|ui| {
            ui.label("Realistic intensity");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(mono(format!("{:0.2}", self.settings.intensity)));
            });
        });
        changed |= ui
            .add(egui::Slider::new(&mut self.settings.intensity, 0.0..=2.5).show_value(false))
            .changed();
        ui.horizontal(|ui| {
            ui.label("Realistic blend");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(mono(format!("{:0.0}%", self.settings.mix * 100.0)));
            });
        });
        changed |= ui
            .add(egui::Slider::new(&mut self.settings.mix, 0.0..=1.0).show_value(false))
            .changed();
        ui.add_space(10.0);
        section_title(
            ui,
            "ARTISTIC LAYER",
            "Stylisation effects compose over the realistic result.",
        );
        ui.label(
            RichText::new("ARTISTIC PROFILE")
                .size(9.0)
                .strong()
                .color(MUTED),
        );
        let mut art_changed = false;
        egui::ComboBox::from_id_salt("artistic_preset")
            .selected_text(artistic_label(&self.ui_artistic_preset))
            .width(ui.available_width())
            .show_ui(ui, |ui| {
                art_changed |= ui
                    .selectable_value(
                        &mut self.ui_artistic_preset,
                        "None".into(),
                        "None · realistic layer only",
                    )
                    .changed();
                for preset in presets::artistic_presets() {
                    art_changed |= ui
                        .selectable_value(
                            &mut self.ui_artistic_preset,
                            preset.id.to_owned(),
                            format!("{}  ·  {}", preset.label, preset.description),
                        )
                        .changed();
                }
                art_changed |= ui
                    .selectable_value(
                        &mut self.ui_artistic_preset,
                        "Custom".into(),
                        "Custom profile",
                    )
                    .changed();
            });
        if art_changed && self.ui_artistic_preset != "Custom" {
            self.settings
                .use_artistic_preset(if self.ui_artistic_preset == "None" {
                    None
                } else {
                    Some(&self.ui_artistic_preset)
                });
            self.mark_dirty();
        }
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label("Artistic intensity");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(mono(format!("{:0.2}", self.settings.artistic_intensity)));
            });
        });
        changed |= ui
            .add(
                egui::Slider::new(&mut self.settings.artistic_intensity, 0.0..=2.5)
                    .show_value(false),
            )
            .changed();
        ui.horizontal(|ui| {
            ui.label("Artistic blend");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(mono(format!("{:0.0}%", self.settings.artistic_mix * 100.0)));
            });
        });
        changed |= ui
            .add(egui::Slider::new(&mut self.settings.artistic_mix, 0.0..=1.0).show_value(false))
            .changed();
        if changed {
            self.mark_dirty();
        }
        ui.add_space(12.0);
        section_title(
            ui,
            "REPRODUCIBILITY",
            "Same image + settings + seed = same render.",
        );
        ui.horizontal(|ui| {
            ui.label("Seed");
            let response = ui.add(
                egui::DragValue::new(&mut self.settings.seed)
                    .speed(1.0)
                    .range(0..=u64::MAX),
            );
            if response.changed() {
                self.mark_dirty();
            }
            if ui.small_button("Randomize").clicked() {
                self.randomize_seed();
            }
        });
        ui.add_space(8.0);
        ui.label(RichText::new("QUICK START").size(9.0).strong().color(MUTED));
        ui.horizontal_wrapped(|ui| {
            for (name, preset) in [
                ("Subtle", "subtle"),
                ("Edge", "edge"),
                ("Datamosh", "datamosh"),
                ("Heavy", "heavy"),
            ] {
                if ui.small_button(name).clicked() {
                    self.ui_preset = preset.into();
                    self.settings.use_realistic_preset(preset);
                    self.mark_dirty();
                }
            }
        });
    }

    fn show_effect_controls(&mut self, ui: &mut egui::Ui) {
        section_title(
            ui,
            "EFFECT STACK",
            "Enable individual effects and adjust their contribution.",
        );
        ui.horizontal(|ui| {
            if ui
                .selectable_label(
                    self.effect_layer == EffectLayer::Realistic,
                    RichText::new("REALISTIC · 19").strong(),
                )
                .clicked()
            {
                self.effect_layer = EffectLayer::Realistic;
            }
            if ui
                .selectable_label(
                    self.effect_layer == EffectLayer::Artistic,
                    RichText::new("ARTISTIC · 8").strong(),
                )
                .clicked()
            {
                self.effect_layer = EffectLayer::Artistic;
            }
        });
        ui.separator();
        let is_art = self.effect_layer == EffectLayer::Artistic;
        let effects = if is_art {
            presets::artistic_effects()
        } else {
            presets::realistic_effects()
        };
        let mut changed = false;
        for effect in effects {
            let map = if is_art {
                &mut self.settings.artistic_strengths
            } else {
                &mut self.settings.strengths
            };
            let mut value = map.get(effect.id).copied().unwrap_or(0.0);
            let mut enabled = value > 0.0;
            egui::Frame::none()
                .fill(PANEL_ALT)
                .rounding(egui::Rounding::same(7.0))
                .inner_margin(egui::Margin::symmetric(9.0, 7.0))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        changed |= ui
                            .checkbox(&mut enabled, RichText::new(effect.label).strong())
                            .changed();
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(mono(format!("{value:0.2}")));
                        });
                    });
                    ui.label(RichText::new(effect.hint).size(9.0).color(MUTED));
                    changed |= ui
                        .add(egui::Slider::new(&mut value, 0.0..=2.0).show_value(false))
                        .changed();
                });
            if enabled {
                map.insert(effect.id.to_owned(), value.max(0.01));
            } else {
                map.insert(effect.id.to_owned(), 0.0);
            }
            ui.add_space(5.0);
        }
        if changed {
            if is_art {
                self.ui_artistic_preset = "Custom".into();
            } else {
                self.ui_preset = "Custom".into();
            }
            self.mark_dirty();
        }
    }

    fn show_grade_controls(&mut self, ui: &mut egui::Ui) {
        section_title(
            ui,
            "FINAL IMAGE ADJUSTMENTS",
            "Applied after both effect layers have been composited.",
        );
        let mut changed = false;
        changed |= slider_row(
            ui,
            "Exposure",
            &mut self.settings.grade.exposure,
            -2.0..=2.0,
            "{:+0.2} EV",
        );
        changed |= slider_row(
            ui,
            "Contrast",
            &mut self.settings.grade.contrast,
            0.0..=2.0,
            "{:0.2}",
        );
        changed |= slider_row(
            ui,
            "Saturation",
            &mut self.settings.grade.saturation,
            0.0..=2.0,
            "{:0.2}",
        );
        changed |= slider_row(
            ui,
            "Gamma",
            &mut self.settings.grade.gamma,
            0.5..=2.0,
            "{:0.2}",
        );
        if ui.button("Reset colour grade").clicked() {
            self.settings.grade = Default::default();
            changed = true;
        }
        if changed {
            self.mark_dirty();
        }
    }

    fn show_export_controls(&mut self, ui: &mut egui::Ui) {
        section_title(
            ui,
            "OUTPUT ENCODING",
            "The preview is decoded from the same bytes that export writes.",
        );
        let mut changed = false;
        ui.label(RichText::new("FORMAT").size(9.0).strong().color(MUTED));
        egui::ComboBox::from_id_salt("format_select")
            .selected_text(self.settings.format.label())
            .width(ui.available_width())
            .show_ui(ui, |ui| {
                changed |= ui
                    .selectable_value(
                        &mut self.settings.format,
                        ExportFormat::Png,
                        "PNG · lossless",
                    )
                    .changed();
                changed |= ui
                    .selectable_value(
                        &mut self.settings.format,
                        ExportFormat::Jpeg,
                        "JPEG · lossy",
                    )
                    .changed();
                changed |= ui
                    .selectable_value(
                        &mut self.settings.format,
                        ExportFormat::Webp,
                        "WebP · lossy",
                    )
                    .changed();
            });
        ui.add_space(7.0);
        ui.horizontal(|ui| {
            ui.label("Quality");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(mono(format!("{} / 100", self.settings.quality)));
            });
        });
        changed |= ui
            .add(egui::Slider::new(&mut self.settings.quality, 1..=100).show_value(false))
            .changed();
        ui.add_space(7.0);
        ui.label(
            RichText::new("JPEG CHROMA SUBSAMPLING")
                .size(9.0)
                .strong()
                .color(MUTED),
        );
        egui::ComboBox::from_id_salt("subsampling_select")
            .selected_text(self.settings.subsampling.label())
            .width(ui.available_width())
            .show_ui(ui, |ui| {
                changed |= ui
                    .selectable_value(
                        &mut self.settings.subsampling,
                        Subsampling::S444,
                        "4:4:4 · highest colour detail",
                    )
                    .changed();
                changed |= ui
                    .selectable_value(
                        &mut self.settings.subsampling,
                        Subsampling::S422,
                        "4:2:2 · balanced",
                    )
                    .changed();
                changed |= ui
                    .selectable_value(
                        &mut self.settings.subsampling,
                        Subsampling::S420,
                        "4:2:0 · compact",
                    )
                    .changed();
            });
        ui.add_space(12.0);
        egui::Frame::none().fill(PANEL_ALT).rounding(egui::Rounding::same(8.0)).inner_margin(egui::Margin::same(10.0)).show(ui, |ui| {
            ui.label(RichText::new("EXPORT BEHAVIOUR").strong().color(ACCENT));
            ui.label(RichText::new("• Full-resolution output, not the scaled preview.\n• Original source is never overwritten.\n• PNG preserves the processed RGB pixels.\n• JPEG / WebP are previewed after encoding.").size(10.0).color(MUTED));
        });
        if changed {
            self.mark_dirty();
        }
    }

    fn draw_preview(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(
                egui::Frame::none()
                    .fill(BG)
                    .inner_margin(egui::Margin::same(14.0)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("LIVE COMPARISON")
                            .size(13.0)
                            .strong()
                            .color(FG),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            RichText::new("FULL-RES RENDER  →  OUTPUT-CODEC PREVIEW")
                                .size(9.0)
                                .strong()
                                .color(MUTED),
                        );
                    });
                });
                ui.add_space(10.0);
                let source_meta = self
                    .source
                    .as_ref()
                    .map(|img| format!("{} × {}", img.width(), img.height()))
                    .unwrap_or_else(|| "No source".into());
                let output_meta = self
                    .source
                    .as_ref()
                    .map(|img| {
                        format!(
                            "{} × {}  ·  {}",
                            img.width(),
                            img.height(),
                            self.settings.format.label()
                        )
                    })
                    .unwrap_or_else(|| "Waiting for source".into());
                ui.columns(2, |columns| {
                    preview_card(
                        &mut columns[0],
                        "SOURCE",
                        &source_meta,
                        self.source_texture.as_ref(),
                        "Drop an image here\nor click Open image",
                    );
                    preview_card(
                        &mut columns[1],
                        "OUTPUT",
                        &output_meta,
                        self.output_texture.as_ref(),
                        if self.rendering {
                            "Rendering preview…"
                        } else {
                            "Output preview will appear here"
                        },
                    );
                });
                if self.source.is_none() {
                    ui.add_space(10.0);
                    egui::Frame::none()
                        .fill(PANEL)
                        .rounding(egui::Rounding::same(10.0))
                        .inner_margin(egui::Margin::same(18.0))
                        .show(ui, |ui| {
                            ui.vertical_centered(|ui| {
                                ui.label(
                                    RichText::new("START WITH AN IMAGE")
                                        .size(14.0)
                                        .strong()
                                        .color(ACCENT),
                                );
                                ui.add_space(4.0);
                                ui.label(
                                    RichText::new("PNG · JPEG · WebP · BMP · TIFF · GIF")
                                        .size(10.0)
                                        .color(MUTED),
                                );
                                ui.add_space(8.0);
                                if ui
                                    .button(RichText::new("Browse image files").strong())
                                    .clicked()
                                {
                                    self.open_image(ctx);
                                }
                            });
                        });
                }
            });
    }

    fn draw_footer(&self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("glitchforge_footer")
            .frame(
                egui::Frame::none()
                    .fill(Color32::from_rgb(9, 12, 16))
                    .inner_margin(egui::Margin::symmetric(14.0, 7.0)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if self.rendering {
                        ui.spinner();
                    }
                    let status_color = if self.last_error.is_some() {
                        Color32::from_rgb(255, 132, 132)
                    } else {
                        MUTED
                    };
                    ui.label(RichText::new(&self.status).size(10.0).color(status_color));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if let Some(duration) = self.last_render_duration {
                            ui.label(
                                RichText::new(format!("RENDER {:.2}s", duration.as_secs_f32()))
                                    .size(8.0)
                                    .strong()
                                    .color(MUTED),
                            );
                        }
                        ui.label(
                            RichText::new("RUST NATIVE  ·  DETERMINISTIC  ·  NON-DESTRUCTIVE")
                                .size(8.0)
                                .strong()
                                .color(Color32::from_rgb(93, 109, 127)),
                        );
                    });
                });
            });
    }

    fn draw_help(&mut self, ctx: &egui::Context) {
        if self.show_help {
            egui::Window::new("GlitchForge · Help & support").open(&mut self.show_help).default_width(520.0).resizable(true).show(ctx, |ui| {
                ui.label(RichText::new("GETTING STARTED").strong().color(ACCENT));
                ui.label("Open or drop an image, choose a profile, then tune effects in the Effects tab. Source files are never modified.");
                ui.add_space(8.0);
                ui.label(RichText::new("RENDERING & REPRODUCIBILITY").strong().color(ACCENT));
                ui.label("Every effect uses a stable, independent random stream. With the same source image, settings, and seed, GlitchForge produces the same Rust render. Changing one effect does not re-seed the other effects.");
                ui.label("The output preview is decoded from the selected export encoding. Export always writes full-resolution bytes.");
                ui.add_space(8.0);
                ui.label(RichText::new("SUPPORTED INPUTS").strong().color(ACCENT));
                ui.label("PNG, JPEG, WebP, BMP, TIFF and GIF (first frame). Output formats: PNG, JPEG and WebP.");
                ui.add_space(8.0);
                ui.label(RichText::new("KEYBOARD & WORKFLOW").strong().color(ACCENT));
                ui.label("Use Open image, Load config, Save config, Re-roll seed and Export full resolution in the top toolbar. Drag a file into the preview workspace to open it.");
                ui.add_space(8.0);
                ui.label(RichText::new("TROUBLESHOOTING").strong().color(ACCENT));
                ui.label("If an image fails to load, check that it opens in another image viewer and is not truncated. Very large images may exceed the built-in 40-megapixel safety limit. For a useful bug report, include OS, app version, input format and dimensions, steps to reproduce, the error text, and whether the issue is deterministic.");
                if let Some(error) = &self.last_error {
                    ui.add_space(6.0);
                    ui.label(RichText::new("LATEST ERROR").strong().color(Color32::from_rgb(255, 140, 140)));
                    egui::ScrollArea::vertical().max_height(100.0).show(ui, |ui| {
                        ui.label(RichText::new(error).monospace().size(9.0).color(MUTED));
                    });
                    if ui.button("Copy error details").clicked() { ui.ctx().copy_text(error.clone()); }
                }
                ui.add_space(8.0);
                ui.label(RichText::new("More detail: see README.md and SUPPORT.md in the project folder. No image data or telemetry is transmitted.").size(10.0).color(MUTED));
            });
        }
        if self.show_about {
            egui::Window::new("About GlitchForge")
                .open(&mut self.show_about)
                .show(ctx, |ui| {
                    ui.label(
                        RichText::new("GLITCHFORGE")
                            .size(22.0)
                            .strong()
                            .color(ACCENT),
                    );
                    ui.label("A deterministic image-artifact and stylisation laboratory.");
                    ui.label("Rust desktop edition · Version 0.1.0");
                    ui.label("No network connection or account is required for image processing.");
                });
        }
    }
}

impl eframe::App for GlitchForgeApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_render(ctx);
        if let Some(dirty_at) = self.dirty_at {
            if dirty_at.elapsed() >= Duration::from_millis(180) && !self.rendering {
                self.start_render();
            } else {
                ctx.request_repaint_after(Duration::from_millis(40));
            }
        }
        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        if let Some(file) = dropped.first() {
            if let Some(path) = file.path.clone() {
                self.load_path(path, ctx);
            }
        }
        let open_shortcut = ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::O));
        let export_shortcut = ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::E));
        let save_shortcut =
            ctx.input(|i| i.modifiers.ctrl && i.modifiers.shift && i.key_pressed(egui::Key::S));
        if open_shortcut {
            self.open_image(ctx);
        }
        if export_shortcut {
            self.export_dialog(ctx);
        }
        if save_shortcut {
            self.save_config_dialog();
        }
        self.draw_topbar(ctx);
        self.draw_sidebar(ctx);
        self.draw_preview(ctx);
        self.draw_footer(ctx);
        self.draw_help(ctx);
        if self.rendering {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }
}

fn rgb_to_color_image(image: &RgbImage) -> egui::ColorImage {
    // Keep GPU preview textures bounded. All processing and exports remain full-resolution.
    let thumb = image::imageops::thumbnail(image, 2048, 2048);
    egui::ColorImage::from_rgb(
        [thumb.width() as usize, thumb.height() as usize],
        thumb.as_raw(),
    )
}
fn mono(value: String) -> RichText {
    RichText::new(value).monospace().size(10.0).color(ACCENT)
}
fn section_title(ui: &mut egui::Ui, title: &str, description: &str) {
    ui.add_space(5.0);
    ui.label(RichText::new(title).size(10.0).strong().color(ACCENT));
    ui.label(RichText::new(description).size(9.0).color(MUTED));
    ui.add_space(6.0);
}
fn preset_label(preset: &str) -> &'static str {
    match preset {
        "subtle" => "Subtle",
        "edge" => "Edge",
        "jpeg" => "JPEG",
        "buffer" => "Buffer",
        "stride" => "Stride",
        "datamosh" => "Datamosh",
        "analog" => "Analog",
        "chromatic" => "Chromatic",
        "heavy" => "Heavy",
        _ => "Custom",
    }
}
fn artistic_label(preset: &str) -> &'static str {
    match preset {
        "None" => "None",
        "crimson_dream" => "Crimson Dream",
        "signal_burst" => "Signal Burst",
        "analog_vision" => "Analog Vision",
        "glitch_cinema" => "Glitch Cinema",
        _ => "Custom",
    }
}
fn slider_row(
    ui: &mut egui::Ui,
    name: &str,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    format: &str,
) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(name);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let text = match format {
                "{:+0.2} EV" => format!("{:+0.2} EV", *value),
                _ => format!("{:0.2}", *value),
            };
            ui.label(mono(text));
        });
    });
    changed |= ui
        .add(egui::Slider::new(value, range).show_value(false))
        .changed();
    changed
}
fn preview_card(
    ui: &mut egui::Ui,
    title: &str,
    meta: &str,
    texture: Option<&TextureHandle>,
    placeholder: &str,
) {
    egui::Frame::none()
        .fill(PANEL)
        .stroke(egui::Stroke::new(1.0_f32, BORDER))
        .rounding(egui::Rounding::same(10.0))
        .inner_margin(egui::Margin::same(9.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(title)
                        .size(10.0)
                        .strong()
                        .color(if title == "OUTPUT" { ACCENT } else { FG }),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new(meta).size(9.0).color(MUTED));
                });
            });
            ui.add_space(7.0);
            let available = ui.available_size().max(Vec2::new(80.0, 100.0));
            let (rect, painter) = ui.allocate_painter(available, egui::Sense::hover());
            painter.rect_filled(rect.rect, egui::Rounding::same(7.0), PREVIEW_BG);
            if let Some(texture) = texture {
                let image_size = texture.size_vec2();
                let scale = (rect.rect.width() / image_size.x)
                    .min(rect.rect.height() / image_size.y)
                    .max(0.001);
                let fitted = image_size * scale;
                let image_rect = egui::Rect::from_center_size(rect.rect.center(), fitted);
                painter.image(
                    texture.id(),
                    image_rect,
                    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    Color32::WHITE,
                );
            } else {
                painter.text(
                    rect.rect.center(),
                    egui::Align2::CENTER_CENTER,
                    placeholder,
                    egui::FontId::proportional(12.0),
                    MUTED,
                );
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn default_application_state_has_useful_defaults() {
        let settings = RenderSettings::default();
        assert_eq!(settings.preset, "edge");
        assert_eq!(settings.seed, 1);
    }
}
