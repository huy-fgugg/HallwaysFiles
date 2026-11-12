# HALLWAYS: Game Design

## Pillars
1. **Learnable fear.** Every threat has a telegraphed cue and a specific counter. Every death should teach a rule, not feel random. The death screen always states the cause and the lesson.
2. **Readable atmosphere.** Warm, worn interiors lit by real lights, with fog and contact shadows. Horror comes from light turning against you, not from darkness hiding everything.
3. **Replayable runs.** Seeded generation means a seed reproduces the same layout, loot and entity schedule, so runs can be shared and bugs reproduced.
4. **Suspense, not gore.** Failure is shown with shake, sound and a fade to black. A spooky-intensity slider, a loud-sound limiter and a reduce-flashing mode are always available.

## Core loop
Elevator arrival, then rooms in numbered order: open the door, search drawers for Lumens and items, react to cues, reach the final door, earn persistent currency, unlock items and modifiers, repeat.

## Current vertical slice (Phase 1 plus cutscenes)
| Element | Status |
|---|---|
| Arrival lift (brass cage elevator, animated dial and lattice gate) and the 14 s intro cutscene | Done |
| Room 000 (sitting room) and Room 001 (corridor), textured and trimmed, with lamps and sconces | Done |
| Panelled numbered doors, built from about 40 parts with engraved brass plaques | Done |
| Drawers (7, animated glTF parts) with seeded Lumens loot | Done |
| Wardrobe hiding: animated doors, peek crack, 10 s limit with a warning at 7 s | Done |
| The Hollow: emergence cutscene, red-light cue, chase, search, sink-away; death screen with a lesson | Done |
| Animated 3D main menu, credits, settings, pause | Done |

## Entities (original roster)
These are original designs. The counters follow genre-standard rules (hide, look, stop, verify), but names, looks and behaviours are HALLWAYS' own.

| Entity | Cue | Counter | Fail | Status |
|---|---|---|---|---|
| **The Hollow**: stooped figure of black tatters with a cracked porcelain mask and amber eye-slits; rises from a pool of ink | Lights stutter, then turn red; low rumble; ink on the floor | Break line of sight and **hide in a wardrobe**. It searches outside for 3 s, then sinks away | Caught if it reaches you while you're in the open | Done |
| **The Surge**: a roaring knot of crackling white-blue static shards with trailing streaks | Lights strobe (slow dim with Reduce Flashing), a roar builds over 3.2–5 s (shorter deeper in) | **Hide in a wardrobe or locker** until the lights settle. Surge rooms always have one | Caught in the open | Done |
| **The Undertow**: like The Surge, but it passes back and forth 2–5 times | Same as The Surge but lower-pitched, with a "double-back" sound | Stay hidden until the green flicker stops. The hide timer pauses while it's out there | Leaving too early | Done |
| **The Whisperer**: a pale shape in dark rooms | Whispering from a random direction in a dark room | **Look straight at it** within the window (3 s) | 30 damage | Done |
| **The Gallery**: a cluster of painted eyes that open on the walls | The room dims to violet, with a heartbeat | **Look away** and leave; 9 vitality per second while you look | Staring | Done |
| **The Hush**: a pressure wave | Screen vignette and "STOP" prompt, all sound muffled | **Stop moving** until it lifts (3 s) | 35 damage | Done |
| **Mimic Doors** | A second door whose plaque number is wrong (skips, repeats or swaps digits) | **Check the numbers**; open the right one | 40 damage plus a scare; 3 hits is death | Done |

Every entity's cue window is at least 2 s at default difficulty, and scales with floor depth.

## Floor generation (Phase 2, done)
- **Graph:** a linear spine of 100 numbered rooms with occasional side closets and dead-end branches. Seeded with `GameRng::fork(seed, room_index)` so each room is independent and reproducible.
- **Areas** (original themes, changing every 20–30 rooms with a transition room):
  1. *The Guest Wing*: wallpaper, wainscoting, wardrobes, warm sconces (current art).
  2. *The Archive*: towering shelves, reading lamps, ladders; dark-room heavy.
  3. *The Service Tunnels*: pipes, puddles, flickering industrial cages; more lockers to hide in.
  4. *The Quarry Lift*: rough-cut stone, timber props, mine-cart rails; long chase corridors.
  5. *The Atrium*: the final area, a vast collapsing hall leading to Door 100.
- **Room types:** Loot, Puzzle (key, lever or plaque clue), Entity, Chase, Safe/Shop, all weighted with cooldowns so a type doesn't repeat back-to-back.
- **Lighting states:** Bright, Dim, Dark, Blackout. Dark rooms raise Whisperer odds.

## Items (Phase 3, done)
| Item | Effect | Stack |
|---|---|---|
| Flashlight | F toggles a torch; about 150 s of battery; another flashlight recharges it | 1 |
| Lockpick | Opens a key-locked exit door (E on the door) | 3 |
| Vitamins | 35% faster for 8 s | 3 |
| Bandage | +35 vitality | 3 |
| Skeleton Key | Opens any key-locked door | 1 |

3 slots. Picking something up with a full inventory replaces the selected slot, so carrying choices matter. All values live in `assets/data/items.ron`.

## Elevator lobby
Every fresh run starts in the lift with its own looping music. The lobby waits until every expected player has joined, then counts down 15 s; the host can force-start at any time. Retrying after death skips the lobby.

## Progression (Phase 5)
Lumens collected in a run are banked when it ends (death or Door 100), multiplied by any active modifiers: Lights Low +25%, Restless +35%, Fragile +30%. The bank buys up to 3 starting items for the next run (Flashlight 60, Lockpick 45, Vitamins 30, Bandage 35, Skeleton Key 90). Purchases are refundable before the run starts. There are no randomised rewards and no real-money purchases.

## Room features
- **Number signs:** every door has a back-lit number sign above its frame plus the engraved plaque.
- **Choice walls (rare):** from Room 005, about 6% of rooms (11% after Room 020) line up one to three look-alike doors beside the real exit. Only the real door shows the next number. Opening a fake costs 25 vitality.
- **Raised exits:** some rooms climb a staircase to a landing with the exit door set high in the wall; the next room begins on a landing with stairs back down.
- **Key lofts:** some key rooms hide the key in a chest on a loft up a staircase along a side wall, lit by its own lamp.

## Cutscenes
Data-driven RON timelines in `assets/cutscenes/<id>/cutscene.ron`: camera shots (absolute or player-eye), subtitles, fades, light moods, sounds and entity/elevator commands. Each folder has a music slot. Skippable by holding Space; skipping applies all state changes (gate opens, entity risen) so the world stays consistent.

## Multiplayer (online rooms)
Up to 4 players. The host creates a room and gets a 5-letter code (no 0/O or 1/I); others join with it. Everyone generates the same floor from the host's seed. The lift lobby waits until the room is full (then counts down 15 s) or the host starts. Shared: positions, poses, held items, flashlight beams, gestures and opened doors. Each player meets entities on their own machine. If the host leaves, the next player becomes host. The relay server holds no game logic and stores nothing.

## Accessibility
Subtitles on by default, including in cutscenes. Spooky intensity scales shake, cue loudness and distortion. The loud-sound limiter caps stingers. Reduce Flashing replaces strobes with slow dims and disables head-bob and title flicker. Full keyboard/controller menu navigation with visible focus.
