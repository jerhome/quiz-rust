mod app;
mod chrome;
mod i18n;
mod multi;
mod screens;
mod settings;
mod theme;
mod widgets;

use eframe::egui;

fn main() -> anyhow::Result<()> {
    // winit warns about WSLg quirks (no XSETTINGS manager, 0mm display) that are harmless here.
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info,winit=error"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
    let runtime = tokio::runtime::Runtime::new()?;
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Quiz Rust")
            .with_inner_size([960.0, 720.0])
            .with_min_inner_size([560.0, 520.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Quiz Rust",
        options,
        Box::new(|cc| Ok(Box::new(app::QuizApp::new(&cc.egui_ctx, runtime)))),
    )
    .map_err(|e| anyhow::anyhow!("{e}"))
}
