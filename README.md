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
| Menus | Arrows/WASD, Enter/Space, Esc (letters type while a text box is selected) | D-pad, A, B |
| Skip cutscene | Hold Space | Hold A |

## Play screen: solo, online rooms, shop, modifiers

Main menu → **Play**:

- **Name** and **Seed** boxes: click one (or arrow to it) and type. An empty seed gives a new floor each run; type any word or number to replay the same floor.
- **Solo Run** starts alone.
- **Host Room** creates an online room and shows a 5-letter **room code** in the lift. Friends type that code into **Room Code** and press **Join Room** (up to 4 players). The lift music loops until all 4 are in (then a 15 s countdown) or the host presses **Enter**.
- **Shop**: Lumens you collect are banked when a run ends. Spend them on up to 3 items to start your next run with. **Refund Last** gives the Lumens back. No random rewards, no real money.
- **Modifiers**: Lights Low, Restless and Fragile make runs harder and pay more Lumens.

What's shared online: every player gets the same floor (the host's seed), sees teammates' characters walk, crouch, carry items, shine flashlights, reach and use items, and doors opened by anyone open for everyone. Entities run separately on each player's machine, so each player survives their own encounters.

## The online server (Render)

Online rooms need the small server in `server/` running somewhere. It's set up for [Render](https://render.com):

1. Sign in to Render with GitHub.
2. **New → Blueprint**, pick this repository, then **Apply**. Render reads `render.yaml`, builds `server/Dockerfile` and starts the service on the free plan.
3. When it's live, open `https://<service-name>.onrender.com/health`. It should say `ok`.
4. If your service name isn't `hallways-server`, put `wss://<service-name>.onrender.com/ws` in `assets/server.txt` (next to the game) before playing or packaging.

Free Render services fall asleep after about 15 minutes without players; the first **Host Room** after that can take up to a minute while it wakes up.
To test locally: `cd server && cargo run`, then start the game with `HALLWAYS_SERVER=ws://127.0.0.1:8080/ws`.

## Your own music and cutscenes

Every song has its own folder in `assets/music/`. Drop **one** audio file (`.ogg`, `.mp3` or `.wav`, any name) into a folder:

| Folder | Plays | Length |
|---|---|---|
| `menu/` | Main menu (loops) | 20 s – 5 min |
| `elevator/` | Lift lobby (loops until everyone joins or the host starts) | 20 s – 5 min |
| `ambience/` | Exploring (loops) | 20 s – 5 min |
| `chase/` | Being hunted (loops) | 20 s – 5 min |
| `cutscene_intro/` | Elevator arrival cutscene (once) | 14 s – 60 s |
| `cutscene_hollow/` | The Hollow cutscene (once) | 16 s – 60 s |

Main menu → **Credits** → "YOUR MUSIC" shows which file each slot found. Empty folders use the built-in original score.
Cutscene timelines are editable in `assets/cutscenes/<name>/cutscene.ron`. Credit any music you add in `assets/music/CREDITS.txt`; only add music you have permission to use.

## Tuning

`assets/data/entities.ron` (spawn chances, warning times, damage) and `assets/data/items.ron` (item rarity, battery, healing) are plain text. Edit them and restart the game. A broken edit falls back to the built-in values.

## Settings and saves

Settings and your profile (banked Lumens, shop items, modifiers, name) are saved to `%APPDATA%\Hallways\` on Windows (or `~/.config/hallways` on Linux) as `settings.ron` and `profile.ron`. Profiles are versioned and older saves upgrade automatically. A corrupt profile is kept as `profile.bad.ron` and a fresh one starts.

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
