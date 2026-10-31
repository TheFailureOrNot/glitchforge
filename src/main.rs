use anyhow::Result;
use clap::Parser;

fn main() -> Result<()> {
    let args = glitchforge::cli::Args::parse();
    if args.list {
        return glitchforge::cli::run(args);
    }
    if args.gui || args.input.is_none() {
        let options = eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([1480.0, 920.0])
                .with_min_inner_size([1060.0, 700.0])
                .with_title("GLITCHFORGE — Image Artifact Laboratory"),
            ..Default::default()
        };
        return eframe::run_native(
            "GLITCHFORGE — Image Artifact Laboratory",
            options,
            Box::new(|cc| Ok(Box::new(glitchforge::gui::GlitchForgeApp::new(cc)))),
        )
        .map_err(|e| anyhow::anyhow!("Could not launch desktop application: {e}"));
    }
    glitchforge::cli::run(args)
}
