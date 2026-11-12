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

## Phase 2: Procedural floor (next)
- [ ] Seeded room-graph generator: 100 doors, 5 original areas
- [ ] Room types with weighted cooldowns; lighting states
- [ ] Mimic (fake-number) doors
- [ ] Run timer and summary screen

## Phase 3: Entities and items
- [ ] The Surge (rushing entity: strobing lights, hide in closets), The Undertow, The Whisperer, The Gallery, The Hush, as data-driven RON configs
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
- The floor currently ends at the locked Door 002 (Phase 2 generates the rest).
