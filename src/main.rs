// Hide the console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use bevy::{
    image::{ImageAddressMode, ImageSamplerDescriptor},
    prelude::*,
    window::{PresentMode, WindowResolution},
};
use hallways::HallwaysPlugin;

fn main() {
    let mut sampler = ImageSamplerDescriptor::linear();
    sampler.address_mode_u = ImageAddressMode::Repeat;
    sampler.address_mode_v = ImageAddressMode::Repeat;
    sampler.set_anisotropic_filter(16);

    App::new()
        .insert_resource(ClearColor(Color::srgb(0.02, 0.025, 0.035)))
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "HALLWAYS".into(),
                        resolution: WindowResolution::new(1600, 900),
                        present_mode: PresentMode::AutoVsync,
                        ..default()
                    }),
                    ..default()
                })
                .set(ImagePlugin { default_sampler: sampler }),
        )
        .add_plugins(HallwaysPlugin)
        .run();
}
