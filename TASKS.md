# TASKS

## Phase 1: Foundation and playable test room
- [x] Cargo project, Bevy 0.20, release profile, no console window in release
- [x] Window, first-person controller (WASD, mouse/stick look, sprint, crouch), AABB collision
- [x] Raycast interaction with E prompt and wall occlusion
- [x] Test room: door, drawers, hiding spot (plus a corridor)
- [x] Main menu, pause, settings (sensitivity, volumes, spooky intensity, subtitles, reduce flashing, loud-sound limiter, fullscreen), all using `UiTheme`
- [x] Keyboard/controller focus navigation with visible focus states
- [x] GitHub Actions Windows build producing `hallways-windows.zip`
- [x] Unit tests: RNG determinism, collision, settings round-trip/corruption, menus, hide timer, cutscene files, waypoints, plaques, meshes
- [ ] Verify the Actions artifact on a real Windows PC by double-clicking the packaged exe (needs a human with Windows)

## Added on request
- [x] Detailed 3D art: CC0 PBR textures, CC0 furniture models, primitive-built panelled doors and elevator
- [x] Animated 3D main menu (drifting hallway camera, flickering title, sliding buttons)
- [x] Cutscene engine with per-cutscene folders and music slots
- [x] Elevator arrival intro cutscene
- [x] The Hollow emergence cutscene and chase
- [x] Original procedural score (menu, ambience, chase, two cutscene cues) and drop-in music folders
- [x] Credits screen
- [x] Death screen with cause and lesson, plus Retry

## Phase 2: Procedural floor
- [x] Seeded generator: rooms 2-99 ending at Door 100, in 5 original areas (Guest Wing, Archive, Service Tunnels, Quarry Lift, Atrium)
- [x] Room types (Transition, Loot, Hall, Dark, Key Puzzle, Final) with weights and cooldowns (no double halls, never 3 of a kind in a row)
- [x] Lighting states (Bright / Dim / Dark); guide sconces at every exit
- [x] Mimic doors with wrong numbers (40 damage, lesson on death)
- [x] Key puzzles: exit locked, key hidden in a drawer
- [x] Room streaming (1 behind, 3 ahead) with persistent doors, drawers, keys, mimics and surges
- [x] Area props built from primitives: lockers, tool chests, bookshelves with individual books, reading tables, crates, timber frames, rails, pipes, columns
- [x] The Surge (rushing entity): strobing lights and a building roar, then it tears through; hide in a wardrobe or locker
- [x] Door 002 unlocks after surviving The Hollow
- [x] Run summary screen after Door 100
- [x] Tests: determinism, chaining, door fit, areas, cooldowns, surge hiding, mimic numbers, z lookup, streaming window, surge path and timing

## Phase 3: Entities and items
- [x] Data-driven tuning: `assets/data/entities.ron`, `assets/data/items.ron` (with built-in fallbacks)
- [x] The Undertow (multi-pass rusher), The Whisperer (look at it), The Gallery (look away), The Hush (stand still)
- [x] One encounter per room, never in adjacent rooms; Whisperer only in dark rooms, Gallery and Hush only in lit ones
- [x] Hide timer pauses while a rushing entity is out there
- [x] 3-slot inventory with stacking and swap-on-full; Flashlight (battery), Lockpick, Vitamins, Bandage, Skeleton Key
- [x] Drawers roll items; every run's first drawer has a flashlight
- [x] Pixel-art item icons in the HUD; battery bar
- [x] Elevator lobby: lift music loops until everyone joins (then a countdown) or the host force-starts
- [x] Brighter rooms
- [x] Deterministic encounter-schedule tests per seed
- [x] Robustness: commands on streamed-out entities no longer crash the game

## Phase 3.5: Doors and rooms
- [x] Back-lit number sign above every door
- [x] Rare fake-door choice walls (wrong numbers; opening one costs 25 vitality)
- [x] Stair rooms: raised exits with landings, key lofts; walkable ground with step-up and falling
- [x] First-person arms: hold the selected item, sway, sprint pump, reach/use/flinch/swap gestures

## Phase 4: UI polish
- [x] Pixel item icons in the HUD
- [x] Red damage flash and low-vitality pulse (softer with Reduce Flashing)
- [x] Death and results screens show banked Lumens
- [x] Play screen with text boxes and keyboard/gamepad focus

## Phase 5: Progression
- [x] Persistent Lumens bank (versioned profile with migration and corrupt-file backup)
- [x] Pre-run shop (up to 3 items, refundable), no randomised rewards
- [x] Modifiers: Lights Low, Restless, Fragile (Lumens bonus)
- [x] Seed entry on the Play screen
- [x] Drop-in music folders: one folder per song, any file name, status in Credits

## Multiplayer
- [x] Room server (`server/`, axum WebSockets) with 5-letter codes, host promotion, version check; deploys to Render via `render.yaml`
- [x] Host / join by code, up to 4 players, shared seed, lift lobby counts and host start
- [x] Teammate avatars: walk cycle, crouch, head look, held items, flashlight beam, gestures, name tags
- [x] Doors opened by anyone open for everyone
- [ ] Shared entity encounters (each player currently runs their own)

## Requested extras
- [x] Name box on the Play screen (focused on first visit), shown over your head online
- [x] Overhand torch grip: fist wraps held items; arms drawn by their own camera so lights and walls don't affect them
- [x] Every run starts with a flashlight and a bandage
- [x] Admin panel (F8, code-locked): spawn items; kill, revive or jumpscare any player

## 1.0.1
- [x] Graphics quality setting (Low / Medium / High; Medium default); `--low` start-up switch and Low Graphics / Compatibility launchers
- [x] Heavy effects (SSAO, TAA, bloom) are only added at High/Medium, never on the first frame for Low
- [x] crash.log written to the settings folder on any crash
- [x] Credits screen: asset credits only; legal notices moved to assets/legal; README removed
- [x] Room number removed from the HUD

## Phase 6: Release QA (waiting for the owner's final touches)
- [x] Windows zip and Linux .deb built by the Release workflow and published as a GitHub Release (v1.0.0)
- [x] .deb installed and launched in a clean Linux container (menu renders, assets found)
- [ ] Packaged-build QA checklist on real hardware

## Known issues / not verified
- Visual checks were done with software rendering (Mesa lavapipe) on Linux; real-GPU performance on Windows is unmeasured.
- The CI smoke test cannot render (no GPU on GitHub runners); it only confirms the exe and assets package correctly.
- Spot lights don't render on the Linux software renderer used for QA, so the flashlight also casts a point-light pool that works everywhere; the spot beam is unverified on real GPUs.
- The Render deployment itself hasn't been run from here (no Render account access); the server was tested locally, including two game windows in one room.
- Online play over the internet (real latency, TLS to Render) is untested; local tests used a plain ws:// connection.
