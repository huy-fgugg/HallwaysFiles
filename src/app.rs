//! App construction shared by the desktop executable and the Android library.

use bevy::{
    image::{ImageAddressMode, ImageSamplerDescriptor},
    prelude::*,
    window::{PresentMode, WindowResolution},
};
use bevy::render::{
    settings::{RenderCreation, WgpuSettings, WgpuSettingsPriority},
    RenderPlugin,
};
use crate::settings::{config_dir, load_settings, Graphics};
use crate::HallwaysPlugin;

/// Writes panics to `crash.log` in the settings folder (the Windows build has
/// no console, so otherwise a crash leaves no trace).
fn install_crash_log() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let dir = config_dir();
        let _ = std::fs::create_dir_all(&dir);
        let when = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let text = format!(
            "HALLWAYS {} crashed (unix time {when})\n{info}\n\nBacktrace:\n{}\n\nTip: start with low graphics (\"HALLWAYS (low graphics)\" shortcut, or `hallways --low`).\n",
            env!("CARGO_PKG_VERSION"),
            std::backtrace::Backtrace::force_capture()
        );
        let _ = std::fs::write(dir.join("crash.log"), text);
        default_hook(info);
    }));
}

/// Builds and runs the whole game (desktop `main` and the Android library
/// both call this).
pub fn run() {
    install_crash_log();
    // Phones, tablets and the Low-End Edition start on Low; they can raise it in Settings.
    let start_low = cfg!(any(target_os = "android", target_os = "ios")) || crate::lowend::enabled();
    let saved = if start_low && !crate::settings::settings_file_exists() { Graphics::Low } else { load_settings().graphics };
    let graphics = Graphics::startup_override().unwrap_or(saved);

    let mut sampler = ImageSamplerDescriptor::linear();
    sampler.address_mode_u = ImageAddressMode::Repeat;
    sampler.address_mode_v = ImageAddressMode::Repeat;
    sampler.set_anisotropic_filter(match graphics {
        Graphics::Low => 1,
        Graphics::Medium => 4,
        Graphics::High => 16,
    });
    // Low asks the GPU only for widely supported limits (older/integrated GPUs).
    let wgpu = WgpuSettings {
        priority: if graphics == Graphics::Low { WgpuSettingsPriority::WebGPU } else { WgpuSettingsPriority::Functionality },
        ..default()
    };
    // Phones: fall back to OpenGL ES where the Vulkan driver is missing or broken.
    // (Elsewhere keep the default; `None` would switch rendering off.)
    let wgpu = if cfg!(target_os = "android") {
        WgpuSettings { backends: Some(bevy::render::settings::Backends::VULKAN | bevy::render::settings::Backends::GL), ..wgpu }
    } else {
        wgpu
    };

    App::new()
        // QA: HALLWAYS_VOID_DEBUG=1 paints the void magenta so missing walls stand out.
        .insert_resource(ClearColor(if std::env::var_os("HALLWAYS_VOID_DEBUG").is_some() { Color::srgb(1.0, 0.0, 1.0) } else { Color::srgb(0.02, 0.025, 0.035) }))
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "HALLWAYS".into(),
                        // Phones: true fullscreen in landscape.
                        mode: if cfg!(any(target_os = "android", target_os = "ios")) { bevy::window::WindowMode::BorderlessFullscreen(bevy::window::MonitorSelection::Primary) } else { bevy::window::WindowMode::Windowed },
                        resolution: qa_window().unwrap_or_else(|| if crate::lowend::enabled() { WindowResolution::new(1280, 720) } else { WindowResolution::new(1600, 900) }),
                        present_mode: PresentMode::AutoVsync,
                        ..default()
                    }),
                    ..default()
                })
                .set(ImagePlugin { default_sampler: sampler })
                .set(RenderPlugin { render_creation: RenderCreation::Automatic(Box::new(wgpu)), ..default() }),
        )
        .add_plugins(HallwaysPlugin)
        // A stray command on a just-despawned entity (e.g. a room streaming out
        // mid-load) is logged instead of crashing the player's game.
        .set_error_handler(bevy::ecs::error::warn)
        .run();
}

/// QA: `HALLWAYS_WINDOW=667x311@3` opens a window of that logical size and
/// pixel density (e.g. to check menus at phone proportions on a desktop).
fn qa_window() -> Option<WindowResolution> {
    let v = std::env::var("HALLWAYS_WINDOW").ok()?;
    let (size, scale) = v.split_once('@').map_or((v.as_str(), None), |(a, b)| (a, b.parse::<f32>().ok()));
    let (w, h) = size.split_once('x')?;
    let (w, h): (f32, f32) = (w.parse().ok()?, h.parse().ok()?);
    let k = scale.unwrap_or(1.0);
    let mut r = WindowResolution::new((w * k) as u32, (h * k) as u32);
    r.set_scale_factor_override(Some(k));
    Some(r)
}
