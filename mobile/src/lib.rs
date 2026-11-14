//! Mobile entry point. Android: GameActivity loads this library and calls
//! `main`. iPhone/iPad: the app's tiny C `main` (packaging/ios/main.m) calls
//! the `main_rs` function that `#[bevy_main]` exports.

use bevy::prelude::*;

#[bevy_main]
fn main() {
    hallways::app::run();
}
