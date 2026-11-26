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

## 1.0.6
- [x] After a crash, the next launch shows the error on screen (phones hide crash.log) so it can be reported
- [x] Android falls back to OpenGL ES when the Vulkan driver is missing or broken
- [x] Android plays the same drop-in music as PC/Linux (read from inside the APK)
- [x] Chase: a guiding light flies ahead along the escape route; rooms 43 and 45 end in a real and a fake door; ink runs down the walls in the three rooms before Door 040
- [x] Chase lighting is held for the whole chase; a Surge/Undertow from an earlier room is called off when it starts
- [x] Remade models: player avatar (jacket, backpack, face, boots), The Hollow (hood, detailed mask, ribs, clawed hands), The Listener, The Whisperer
- [x] Fix: narrow Atrium halls were nearly blocked by columns
- [x] Three more full-floor autopilot runs; the last one clean from the lift to Door 100

## 1.2.0
- [x] Peer-to-peer rooms: players connect straight to the host's computer (WebRTC); the room server only introduces players and relays for anyone whose network blocks direct links. Phones always use the server and never host
- [x] Host leaves mid-run: every game pauses on RECONNECTING ("Choosing new host...") while the room moves to the best remaining computer, then carries on
- [x] Only a computer that can handle it hosts: each game reports its frame rate, CPU cores and edition; phones, the Low-End Edition and slow computers are never picked automatically. A room started on a phone hands over to a good computer when one joins
- [x] No computer good enough left: "NO NEW HOST FOUND :(" with a vote listing everyone's names; the computer with the most votes hosts (phones can't be voted in). Only phones left: "No new host found :(" and back to the menu
- [x] A host whose game vanished is noticed in 20 s instead of 60
- [x] The stamina bar no longer pushes the vitality bar up into the voice/mic badges

## 1.1.8
- [x] A full-length mirror in the lift lobby (back wall, left of the doorway): check your skin before the descent (not in the Low-End Edition)

## 1.1.7
- [x] Sprinting is now a short burst: walking is a little quicker, sprinting only a bit faster, and a stamina bar (bottom-left) drains in about 5 seconds and refills after a short rest; run it dry and it turns red until you've caught your breath. During a Hollow chase adrenaline keeps it full
- [x] Rooms are longer (about a third deeper, halls up to 30 m), and zigzag rooms turn up twice as often and twist back and forth up to six times
- [x] Fix the Listener turning up room after room: at most 3 per floor, always at least 8 rooms apart

## 1.1.6
- [x] Fix random disconnects: the free server host put the server to sleep after 15 minutes without new web requests, even mid-game; the game now pokes it every 4 minutes while you're in a room
- [x] If the server restarts anyway, the room isn't lost: the first player back recreates it with the same code and floor, and everyone else rejoins it automatically
- [x] Hosting a big hand-built map no longer gets the host's connection closed (the server's message size limit was too small)

## 1.1.5
- [x] Text chat: press T, type, Enter to send (Esc to close); messages show bottom-left
- [x] Mute your microphone with M (saved); a MIC MUTED badge shows while you're in a room
- [x] A speaker icon lights up over players who are talking, and they nod and gesture as they talk
- [x] Settings: set each player in your room's voice volume, or mute them (muting also hides their chat); saved by name
- [x] New items: Battery (common: recharges the flashlight), Med Kit (uncommon: +70 vitality), Lucky Charm (very rare: saves you from one death); rare finds say so
- [x] More Lumens in drawers, and now and then a pouch of 15-30
- [x] Two players with the same name can't be in one room
- [x] Dropped connections: the game notices within 25 seconds, says so at the top of the screen and rejoins the room automatically (even mid-run); being removed from a room says why
- [x] Admin panel: bring a player to you, mute their chat, give Lumens, kick (room host), announcements to the room; with the server's admin key: list every room, announce to all, close rooms, remove players
- [x] The start lift takes everyone down when it leaves; the ending lift only takes the players who reached it (the others keep playing and can still escape)
- [x] Fix: crouching sank the legs into the floor
- [x] Fix: pressed against a mirror, your reflection's head was cut open

## 1.1.4: Low-End Edition (downloads from 1.1.3 on)
- [x] Every release has Low-End Edition downloads for weak PCs and phones (Windows zip, Linux tar.gz, Android APK that installs as its own "HALLWAYS Low-End" app): textures shrunk to 256 px (62 MB of assets down to 12 MB), Low graphics; from 1.1.4 also a 1280x720 window, 2 rooms loaded ahead instead of 3 and no mirrors
- [x] Fix: phones (and the Low-End Edition) really start on Low graphics the first time (the setting said Low but Medium was used)

## 1.1.3
- [x] The Mode Creator is gone (with its Cutscene Maker and custom monster); the Map Builder is back under Extras
- [x] Player-made floors and maps still say so on screen

## 1.1.2
- [x] Mode Creator redesigned: a brown and cream screen with tabs (Floor, Areas, Monsters, Your Monster, Rooms, Lights, Player, Supplies, Cutscenes, Help), a row per setting with a selector box, ON/OFF pills and folding groups; nothing overlaps on PC or phone
- [x] Every setting a mode can have: length, start door, room size, structures, locked doors, Hollow chases on/off, area order, each area, each monster's frequency, lights, dark rooms, fake doors, health, walk speed, sprinting, hiding time, revives (extra lives), starting items, supplies, Lumens earned and an item pool (switch items off)
- [x] Your Monster: make your own rushing monster (name, colour, slowest and fastest speed, passes, damage, warning time, spawn chance, first room)
- [x] Room Rules: how often key lofts, stairs, zigzag walls, pillar halls and streams appear, and which room the Reading Hall is at (or none)
- [x] Cutscene Maker: build your own cutscenes shot by shot (camera move, effect, time, caption), played at the start of your mode or when players walk through a chosen door; Preview plays it straight away
- [x] Share codes carry every new setting (cutscenes are saved with the mode file)
- [x] Chases are scarier: a flash, then the light turns blood-red and ripples; eyes open in the walls and follow you; fallen beams with toppled bookshelves and spilled books to crouch under; burning chandeliers to weave around; a red glow bleeds under the door that slams behind you
- [x] Player-made modes and maps say so on screen the whole run
- [x] Working mirrors in some hotel rooms (beside the entry door): you see the room and yourself in them, moving and crouching as you do
- [x] Fix: looking back from Room 002 showed the night sky around Door 001 (the wall around the start corridor was missing)
- [x] Fix: the wood panelling ran straight across the doorway you came in through
- [x] Fix: your first-person hand vanished after Retry
- [x] Credits for the new Poly Haven models (bookshelf, books, chandelier) and the dark wood texture

## 1.1.1
- [x] Scenery now uses models and textures from free online libraries instead of shapes built in code: low-poly trees (Low Poly Forest Pack), a garden shed (Medieval Shack) and windmills (Old Windmill) from OpenGameArt; Victorian street lamps, benches, a gothic statue, boulders, stumps, fallen trunks, hedges, potted plants, grandfather clocks, metal drums, crates and buckets from Poly Haven; an ambientCG lawn and Poly Haven gravel and mossy stone
- [x] Nothing overlaps: every garden piece claims its footprint, tree crowns stay inside the walls, off the path and clear of lamps, and wall furniture keeps clear of them all
- [x] Proper standing street lamps with glowing lanterns, spaced further apart
- [x] Planned layouts instead of random scatter: tree avenues along the garden walls and one corner cluster (with a stump, boulder or fallen trunk), sheds in corners, windmills in a free corner facing the garden, pairs of benches facing the path, atrium benches in pairs along the carpet, plants flanking the exit door and in the far corners, drums stored together in a corner
- [x] The chases end in a hand hall: huge black hands reach out of both walls in rolling waves, right up to a glowing lane down the middle; leave the lane while they reach and they drag you into the wall. The hands in the earlier chase rooms are big clawed arms too, and hit harder
- [x] Credits list every model and texture source (and the player characters)

## 1.1.0
- [x] New player characters: chunky voxel heroes (Knight, Wizard, Cleric, Ranger; "Voxel Chibi Adventurers" free pack by zaddy99999, CC0) with walking, crouching, reaching, panicking and falling animations
- [x] Customise screen (main menu button, and a Skin table with a floating figure in the boarding hall): pick your character, a crown and a backpack; drag the 3D preview to turn it; your look shows online; your first-person arms match your character
- [x] Voice chat online: push to talk (V, or TALK on phones) or open mic; voices fade with distance; your character's mouth moves when you talk
- [x] Teammates are solid (you bump into them) and their bodies never poke through walls; characters are sized to fit through doors
- [x] Two players fit in each wardrobe, and rooms get enough hiding spots for the whole party
- [x] Map Builder: build a floor room by room (area, size, type, lights, monster, structure), see it from above, play-test or host it, and save/export `.hallwaysmap` files; drop one on the game window (or Import from Downloads) on another computer to play or edit it
- [x] Scenery: garden sheds, turning windmills (and tall ones beyond the walls), benches, planters, floor lamps, barrels and floor clutter in every area
- [x] Every mode has a proper opening: custom floors and maps get their own start cutscene, retries get a "Try Again" cutscene
- [x] Death animation: you fall, the screen washes red, then the death screen
- [x] The Rusher (Surge) fills the whole room: only hiding saves you
- [x] Fix: the door in the ending cutscene opens
- [x] Lighter fog so big rooms don't look like missing walls
- [x] Back to up to 4 players online

## 1.0.9
- [x] No missing walls: the oldest loaded room gets a back wall with the closed door while the room behind it is unloaded, and a corridor stands behind Door 100 (checked with a magenta-void QA mode, including looking back through doors)
- [x] Much harder: monsters spawn far more often (Surge 32%, Undertow 22%, Gallery and Hush 25%, Whisperer 75%, Listener 65%) and hit harder with less time to react; faster Hollow; stronger chase hazards; less loot
- [x] Bigger, longer rooms; much bigger gardens
- [x] A second Hollow chase (rooms 076-081, Door 082 slams)
- [x] Retry starts a new random floor (unless you typed a seed)
- [x] The Listener is scarier (taller, ember cracks, gaping toothed jaw) and follows you through the door: it crouches, squeezes through and stands back up
- [x] Scarier Surge (screaming face in its storm); the Whisperer gropes and sways; the Hush is a shrouded figure that raises a finger to its face
- [x] New structures: pillar halls and garden streams with footbridges
- [x] Player models joined up (knees, elbows, ankles, hands) with feet on the floor; cutscene players use the same body and animations
- [x] Lift boarding: start in a boarding hall and walk in; everyone aboard = down in 5 s, otherwise a 20 s countdown leaves stragglers behind; up to 5 players online
- [x] Cutscenes change with the number of players; remade arrival
- [x] 14 achievements with pop-ups and an Achievements screen; pop-ups when players join or leave

## 1.0.8
- [x] "CREATED BY DONUT STUDIOS!" splash with the studio logo when the game starts (any key or tap skips it); Donut Studios in the credits
- [x] Room generation follows a door rule: every door sits at least m = W/2 + max(casing, open-leaf overshoot) + clearance from the side walls, rounding only ever moves doors inward, and each door's swing arc and the space in front of doors and fake doors are kept clear of furniture, columns, posts and rocks (checked on 1,500 floors in tests)
- [x] Levels come in a random order every run: the Guest Wing first, then the other five areas shuffled by the seed (type a seed on the Play screen to replay a floor)
- [x] The Door 40 chase is different every run: hands burst out of the walls, fire vents roar up and the ceiling collapses where you run, always leaving a free lane
- [x] Divider openings (barricades, shelves, zigzag walls) stay clear in every area
- [x] Three full-floor autopilot runs on random seeds (chase in the Grounds and the Atrium), click-through QA on PC and touch QA in a phone-sized window

## 1.0.7
- [x] Fix: menus could not be clicked during a run (an invisible cutscene fade layer was catching clicks)
- [x] Fix: admin panel: the code box is focused, Enter unlocks, the panel rebuilds in place and item buttons sit in tidy rows
- [x] Fix: Cutscenes menu plays all three cutscenes (The Hollow never started) and returns to the Cutscenes list, with the HUD hidden
- [x] Fix: main menu clicks missed after closing a submenu (the intro animation replayed)
- [x] Phone: quick taps always press menu buttons, even when the game runs at a low frame rate
- [x] Phone: the MENU (pause) and HOLD TO SKIP buttons stay on screen on short phones
- [x] Removed the DOORS soundtrack
- [x] Click-through QA on PC (every menu, settings, custom floors, pause, admin, death, retry, quit) and phone emulation (touch-only, phone-sized window: menus, joystick, look, every on-screen button, pause and admin), plus a full-floor autopilot run

## Phase 6: Release QA (waiting for the owner's final touches)
- [x] Windows zip and Linux .deb built by the Release workflow and published as a GitHub Release (v1.0.0)
- [x] .deb installed and launched in a clean Linux container (menu renders, assets found)
- [ ] Packaged-build QA checklist on real hardware

## Known issues / not verified
- Visual checks were done with software rendering (Mesa lavapipe) on Linux; real-GPU performance on Windows is unmeasured.
- The CI smoke test cannot render (no GPU on GitHub runners); it only confirms the exe and assets package correctly.
- Spot lights don't render on the Linux software renderer used for QA, so the flashlight also casts a point-light pool that works everywhere; the spot beam is unverified on real GPUs.
