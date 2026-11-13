//! Android entry point: GameActivity loads this library and calls `main`.

use bevy::prelude::*;

#[bevy_main]
fn main() {
    hallways::app::run();
}
