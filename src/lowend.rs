//! Low-End Edition: the build for weak PCs (the `-lowend` downloads).
//!
//! It's the same game, packaged with smaller textures (see
//! `packaging/lowend/shrink.py`) and an `assets/LOWEND` marker file. With the
//! marker (or `--lowend` / `HALLWAYS_LOWEND=1`) the game also:
//! * starts on Low graphics (until you pick something else in Settings),
//! * opens a 1280x720 window,
//! * keeps 2 rooms loaded ahead instead of 3,
//! * leaves out mirrors (each one is an extra render pass).

use std::sync::OnceLock;

/// Name of the marker file in `assets/`.
pub const MARKER: &str = "LOWEND";

/// Whether this is the Low-End Edition.
pub fn enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| {
        std::env::args().skip(1).any(|a| a.eq_ignore_ascii_case("--lowend"))
            || std::env::var_os("HALLWAYS_LOWEND").is_some_and(|v| v != "0")
            || crate::music::asset_root().join(MARKER).is_file()
    })
}

/// Rooms kept loaded ahead of you.
pub fn rooms_ahead(normal: u32) -> u32 {
    if enabled() { normal.min(2) } else { normal }
}
