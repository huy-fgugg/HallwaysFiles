//! Writes the game's built-in (original) music and sound effects as WAV
//! files for the trailer: `cargo run --example trailer_audio -- <folder>`.

use hallways::audio::{encode_wav, synth, Sfx, SAMPLE_RATE};
use hallways::music;

fn main() {
    let dir = std::env::args().nth(1).unwrap_or_else(|| "trailer_audio".into());
    std::fs::create_dir_all(&dir).expect("create folder");
    let music_rate = SAMPLE_RATE;
    let tracks: [(&str, Vec<f32>); 4] = [
        ("menu", music::synth_menu_theme()),
        ("chase", music::synth_chase_theme()),
        ("ending", music::synth_ending_cue()),
        ("victory", music::synth_victory_theme()),
    ];
    for (name, samples) in tracks {
        std::fs::write(format!("{dir}/{name}.wav"), encode_wav(&samples, music_rate)).unwrap();
    }
    for (name, s) in [("stinger", Sfx::Stinger), ("rumble", Sfx::Rumble), ("whoosh", Sfx::Whoosh), ("roar", Sfx::SurgeRoar)] {
        std::fs::write(format!("{dir}/{name}.wav"), encode_wav(&synth(s), SAMPLE_RATE)).unwrap();
    }
    println!("wrote trailer audio to {dir}");
}
