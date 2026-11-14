//! Mobile entry point. Android: GameActivity loads this library and calls
//! `main`. iPhone/iPad: the app's tiny C `main` (packaging/ios/main.m) calls
//! `main_rs` below.

use bevy::prelude::*;

#[bevy_main]
fn main() {
    hallways::app::run();
}

/// iPhone/iPad: called from the C `main` in packaging/ios/main.m.
#[cfg(target_os = "ios")]
#[unsafe(no_mangle)]
pub extern "C" fn main_rs() {
    main();
}
