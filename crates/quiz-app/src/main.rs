// No console window behind the GUI in Windows release builds.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod backend;
mod chrome;
mod i18n;
mod links;
mod multi;
mod screens;
mod settings;
mod theme;
mod themes;
mod widgets;

use eframe::egui;

fn main() -> anyhow::Result<()> {
    // winit warns about WSLg quirks (no XSETTINGS manager, 0mm display) that are harmless here.
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info,winit=error"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
    let active = startup_backend();
    let result = run(active);
    // Never leave the user without a window: retry once on X11 if Wayland failed to start.
    if result.is_err() && active.backend == backend::Backend::Wayland {
        tracing::warn!("Wayland failed to start, relaunching on X11");
        if backend::spawn_relaunch(Some(backend::Backend::X11)).is_ok() {
            return Ok(());
        }
    }
    result
}

fn startup_backend() -> backend::Active {
    let saved = settings::config_path()
        .map(|p| settings::load_from(&p).display_backend)
        .unwrap_or_default();
    let env = std::env::var(backend::ENV_VAR).ok();
    let wayland = std::env::var("WAYLAND_DISPLAY").ok();
    let active = backend::resolve(env.as_deref(), saved, wayland.as_deref());
    if active.fell_back {
        tracing::warn!("Wayland requested but WAYLAND_DISPLAY is not set, using X11");
    }
    active
}

#[cfg(target_os = "linux")]
fn apply_backend(options: &mut eframe::NativeOptions, backend: backend::Backend) {
    use winit::platform::wayland::EventLoopBuilderExtWayland;
    use winit::platform::x11::EventLoopBuilderExtX11;
    options.event_loop_builder = Some(Box::new(move |builder| match backend {
        backend::Backend::X11 => {
            builder.with_x11();
        }
        backend::Backend::Wayland => {
            builder.with_wayland();
        }
    }));
}

#[cfg(not(target_os = "linux"))]
fn apply_backend(_options: &mut eframe::NativeOptions, _backend: backend::Backend) {}

fn run(active: backend::Active) -> anyhow::Result<()> {
    let runtime = tokio::runtime::Runtime::new()?;
    let mut options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Quiz Rust")
            .with_inner_size([960.0, 720.0])
            .with_min_inner_size([560.0, 520.0]),
        ..Default::default()
    };
    apply_backend(&mut options, active.backend);
    eframe::run_native(
        "Quiz Rust",
        options,
        Box::new(move |cc| Ok(Box::new(app::QuizApp::new(&cc.egui_ctx, runtime, active)))),
    )
    .map_err(|e| anyhow::anyhow!("{e}"))
}
