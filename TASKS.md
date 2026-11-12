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
- [ ] The Undertow, The Whisperer, The Gallery, The Hush, as data-driven RON configs
- [ ] Inventory and items (flashlight, lockpick, vitamins, bandage, skeleton key)
- [ ] Deterministic entity-schedule tests per seed

## Phase 4: UI polish
- [ ] Item icons, vitality damage feedback, run summary with next unlock

## Phase 5: Progression
- [ ] Persistent Lumens, shop, modifiers, seed input in lobby, versioned saves

## Phase 6: Release QA
- [ ] Packaged-build QA checklist on Windows, GitHub Release

## Multiplayer (separate phase)
- [ ] Networking layer, relay/matchmaking service for room codes, synced doors and entities

## Known issues / not verified
- Visual checks were done with software rendering (Mesa lavapipe) on Linux; real-GPU performance on Windows is unmeasured.
- The CI smoke test cannot render (no GPU on GitHub runners); it only confirms the exe and assets package correctly.
- Dark rooms have no flashlight yet (Phase 3); exit sconces keep them navigable.
