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
| **The Surge**: a roaring wall of static and wind that tears down the hall | Lights strobe (slow pulse with Reduce Flashing), distant roar growing over 4–6 s, subtitle "[A roar builds in the walls]" | **Hide in a closet or wardrobe** before it passes | Caught in the open | Phase 3 |
| **The Undertow**: like The Surge, but it passes back and forth 2–5 times | Same as The Surge but lower-pitched, with a "double-back" sound | Stay hidden until the lights stop flickering; leave between passes if your hide timer is low | Leaving too early | Phase 3 |
| **The Whisperer**: a pale shape in dark rooms | Whispering from a random direction in a dark room | **Look straight at it** within 2.5 s | Ignored too long | Phase 3 |
| **The Gallery**: a cluster of painted eyes that open on the walls | The room dims to violet, with a heartbeat | **Look away**; vitality drains while you look at it | Staring | Phase 3 |
| **The Hush**: a pressure wave | Screen vignette and "STOP" prompt, all sound muffled | **Stop moving** until the prompt clears (2–4 s) | Moving | Phase 3 |
| **Mimic Doors** | A door whose plaque number is wrong (skips or repeats) | **Check the numbers**; open the right one | Opening the mimic causes damage and a jump scare | Phase 2 |

Every entity's cue window is at least 2 s at default difficulty, and scales with floor depth.

## Floor generation (Phase 2, target 100 doors)
- **Graph:** a linear spine of 100 numbered rooms with occasional side closets and dead-end branches. Seeded with `GameRng::fork(seed, room_index)` so each room is independent and reproducible.
- **Areas** (original themes, changing every 20–30 rooms with a transition room):
  1. *The Guest Wing*: wallpaper, wainscoting, wardrobes, warm sconces (current art).
  2. *The Archive*: towering shelves, reading lamps, ladders; dark-room heavy.
  3. *The Service Tunnels*: pipes, puddles, flickering industrial cages; more lockers to hide in.
  4. *The Quarry Lift*: rough-cut stone, timber props, mine-cart rails; long chase corridors.
  5. *The Atrium*: the final area, a vast collapsing hall leading to Door 100.
- **Room types:** Loot, Puzzle (key, lever or plaque clue), Entity, Chase, Safe/Shop, all weighted with cooldowns so a type doesn't repeat back-to-back.
- **Lighting states:** Bright, Dim, Dark, Blackout. Dark rooms raise Whisperer odds.

## Items (Phase 3)
Flashlight, Lockpick, Vitamins (speed), Bandage (vitality), Skeleton Key. 3 inventory slots, data-driven RON definitions.

## Progression (Phase 5)
Lumens are earned per room cleared, per item found and for surviving encounters. They buy pre-run items and unlock modifiers ("Blackout rooms only", "More entities"). Save files are versioned, with corrupted-save recovery like the settings file.

## Cutscenes
Data-driven RON timelines in `assets/cutscenes/<id>/cutscene.ron`: camera shots (absolute or player-eye), subtitles, fades, light moods, sounds and entity/elevator commands. Each folder has a music slot. Skippable by holding Space; skipping applies all state changes (gate opens, entity risen) so the world stays consistent.

## Multiplayer (planned, separate phase)
Co-op for up to 4 players with **room codes**. Plan: client-hosted sessions with a small relay/matchmaking service that maps 6-character codes to hosts (needs a hosted server). Deterministic seeds keep the level identical on every client; the host is authoritative for entity spawns and door state. This is deliberately deferred until the single-player loop is complete.

## Accessibility
Subtitles on by default, including in cutscenes. Spooky intensity scales shake, cue loudness and distortion. The loud-sound limiter caps stingers. Reduce Flashing replaces strobes with slow dims and disables head-bob and title flicker. Full keyboard/controller menu navigation with visible focus.
