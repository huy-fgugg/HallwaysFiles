# HALLWAYS

First-person procedural horror-survival for Windows, built with **Rust + Bevy 0.20**.
Open numbered doors, search drawers, read the cues, and use the right counter: hide, look away, stop moving, or run.

> Working title. Check trademark and Steam store availability before any commercial release.

## Download the Windows build

Every push builds a ready-to-run ZIP with GitHub Actions:

1. Open the repository's **Actions** tab, then **Build Windows EXE**, then the latest green run.
2. Download the **hallways-windows** artifact and unzip it anywhere.
3. Double-click **`hallways.exe`**.

**The `assets` folder must stay next to `hallways.exe`.** The game loads its models, textures, fonts, cutscenes and music from it.

To build manually, go to **Actions**, then **Build Windows EXE**, then **Run workflow**.

## Run from source

Requirements: the [Rust stable toolchain](https://rustup.rs). On Linux you also need `libasound2-dev libudev-dev libwayland-dev libxkbcommon-dev`.

```bash
cargo run --release
```

The first build takes 5–15 minutes (Bevy is large); later builds take seconds.

Run the tests:

```bash
cargo test --lib
```

## Controls

| Action | Keyboard / mouse | Controller |
|---|---|---|
| Move | WASD | Left stick |
| Look | Mouse | Right stick |
| Sprint | Shift | L3 / LT |
| Crouch | Ctrl or C | B / R3 |
| Interact / hide / leave hiding | E | X |
| Flashlight on/off | F | D-pad up |
| Select item slot | 1 / 2 / 3, mouse wheel | LT / RT, D-pad left/right |
| Use selected item | Q or right mouse | Y |
| Start the lift (host) | Enter | A |
| Pause | Esc | Start |
| Menus | Arrows/WASD, Enter/Space, Esc | D-pad, A, B |
| Skip cutscene | Hold Space | Hold A |

## Your own music and cutscenes

- `assets/music/`: drop `menu.ogg`, `elevator.ogg`, `ambience.ogg` and `chase.ogg` here. Loops must be **20 s to 5 min**. See `assets/music/README.md`.
- `assets/cutscenes/<name>/`: each cutscene has its own folder with an editable `cutscene.ron` timeline and a `music.ogg` slot. Cutscene music must be **at least the cutscene's length (14 s intro, 16 s Hollow) and at most 60 s**. See `assets/cutscenes/README.md`.
- Credit any music you add in `assets/music/CREDITS.txt`; the in-game Credits screen shows it. Only add music you have permission to use.

Empty slots fall back to the built-in original score.

## Tuning

`assets/data/entities.ron` (spawn chances, warning times, damage) and `assets/data/items.ron` (item rarity, battery, healing) are plain text. Edit them and restart the game. A broken edit falls back to the built-in values.

## Settings and saves

Settings are saved to `%APPDATA%\Hallways\settings.ron` on Windows (or `~/.config/hallways` on Linux). Delete the file to reset. A corrupt file is ignored and the defaults are used.

## Visual QA capture

The game can photograph a scripted tour of the level, which is handy for checking visuals without playing:

```bash
HALLWAYS_CAPTURE=shots cargo run --release
```

This saves PNGs to `shots/` and exits.

## Project layout

```
src/
  main.rs            window + plugin setup (no console window in release)
  lib.rs             HallwaysPlugin: wires every system together
  state.rs           AppState / Overlay / computed Screen state machine
  player.rs          first-person controller
  interaction.rs     doors, drawers, hiding spots, hide timer
  hollow.rs          The Hollow entity: emergence, chase AI, catch
  surge.rs           The Surge and The Undertow (rushing entities)
  encounters.rs      The Whisperer, The Gallery, The Hush
  items.rs, icons.rs inventory, flashlight, item effects, pixel-art icons
  lobby.rs           elevator lobby (players / host force-start)
  floor/             100-door generator (plan.rs) and room builder (build.rs)
  data.rs            loads assets/data/*.ron tuning
  cutscene.rs        data-driven cutscene engine (RON timelines)
  music.rs           music director, drop-in slots, original procedural score
  audio.rs           procedural sound effects
  world/             level builder, materials, plaque textures, elevator
  ui/                theme-driven menus, HUD, settings, credits, death screen
  physics.rs, rng.rs collision helpers and the deterministic seeded RNG
assets/              fonts, CC0 models and textures, cutscenes, music slots
```

See `GAME_DESIGN.md` for the design and `TASKS.md` for progress.
