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
- [x] Room server (`server/`, axum WebSockets) with 5-letter codes, host promotion, version check
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

## Ending
- [x] Ending cutscene: characters (you + online teammates) walk through the final door into a detailed exit lift, the gate closes, the lift rises, claws tear through the roof, everyone panics, the cable snaps, cut to black, title card
- [x] Victory screen: animated gold VICTORY title, original victory theme; drop-in folders music/victory and music/cutscene_ending
- [x] Android APK built by the Release workflow

## 1.0.4
- [x] Door 40 chase: stepping through Door 041 turns the lights red and The Hollow rises behind you; it follows you through rooms 41-46 (faster than walking, slower than sprinting), doors on the way open as you reach them, and it sinks away at Room 047 (+15 Lumens). Chase rooms have no locks, fake doors or other entities
- [x] Every entity (Surge, Undertow, Whisperer, Gallery, Hush, Listener) now appears in every full run; the Listener can also haunt the Archive
- [x] Custom Floors start straight in the first room of the chosen floor (no lift, Room 000 or opening chase); rooms still stream in a few at a time
- [x] Custom Floors screen: Play Solo is focused on open, small buttons keep their size
- [x] Fix: The Whisperer could appear in your face (and flee at once) when your back was to a wall
- [x] Fix: subtitles overlapped the battery bar and hide meter
- [x] Fix: the opening Hollow is removed once it sinks; each Hollow lights its own eyes
- [x] Fix: Play screen focus order (name box first on a new profile; Solo Run and Custom Floors no longer share a slot)
- [x] Playtested on Linux with simulated keyboard/mouse: menus, custom floor, every entity, the Door 40 chase, death and retry
- [x] Chase rooms (41-46) are big halls with crate barricades; the openings alternate sides so you have to weave. The Hollow rises right behind Door 041 and follows the same route
- [x] Door 047 slams shut and locks once every living player is through it
- [x] Zigzag rooms: partition walls with openings at alternating ends
- [x] The Reading Hall (through Door 050): a large dim archive with rows of double-sided shelves; Door 051 is locked and the key is in one of seven drawers
- [x] Guiding light: after 35 s in a locked room (55 s in the Reading Hall) without the key, a pale wisp appears and floats to the drawer that hides it
- [x] Movement is resolved in short sub-steps so slow frames can't carry you through thin walls
- [ ] iPhone/iPad version (cancelled for now; launcher files kept in `packaging/ios/`)
- [x] Render setup notes and `assets/server.txt` removed (the server address is built in)
- [x] QA switches: HALLWAYS_WARP=<room|entity|Zigzag|ReadingHall>, HALLWAYS_BOT=1 (autopilot along room routes), HALLWAYS_GUIDE_SECS

## 1.0.5
- [x] Menus fit any screen: the UI scale shrinks until the open menu fits (phones in landscape), and grows back when there is room
- [x] Fix: wide or tall rooms showed the empty sky through gaps beside the previous room's wall
- [x] New trailer shots (chase halls, door slam, Reading Hall and guiding light, zigzag room, the Surge) and editing
- [x] QA switch HALLWAYS_WINDOW=667x311@3 opens a phone-shaped window
- [x] The old corridor hide-and-seek chase is removed; The Hollow's chase is the Door 40 run (Door 002 starts open)
- [x] Fix: getting stuck halfway up stairs (step-up height now clears two stair rises)
- [x] Fix: Atrium columns and garden rock piles could block stairs, landings or the path
- [x] Full-floor QA autopilot (HALLWAYS_BOT=1): five complete runs from the lift to Door 100, the last with no stuck spots and no crashes

## Phase 6: Release QA (waiting for the owner's final touches)
- [x] Windows zip and Linux .deb built by the Release workflow and published as a GitHub Release (v1.0.0)
- [x] .deb installed and launched in a clean Linux container (menu renders, assets found)
- [ ] Packaged-build QA checklist on real hardware

## Known issues / not verified
- Visual checks were done with software rendering (Mesa lavapipe) on Linux; real-GPU performance on Windows is unmeasured.
- The CI smoke test cannot render (no GPU on GitHub runners); it only confirms the exe and assets package correctly.
- Spot lights don't render on the Linux software renderer used for QA, so the flashlight also casts a point-light pool that works everywhere; the spot beam is unverified on real GPUs.
