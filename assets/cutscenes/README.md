# Cutscenes

One folder per cutscene. Each folder has:

| File | What it is |
|------|------------|
| `cutscene.ron` | The timeline: camera shots, subtitles, light changes, fades, sound cues. Edit it with any text editor. If you break it, the game falls back to the built-in copy and logs a warning. |
| (music) | **Easiest:** drop any audio file into `assets/music/cutscene_intro/` or `assets/music/cutscene_hollow/`. A `music.ogg` placed in this folder also still works. |

| Folder | When it plays | Length |
|--------|---------------|--------|
| `01_intro/` | The elevator arrival, the first time you start a run each session | **14 s** |
| `02_the_hollow/` | When you walk deep into the Room 001 corridor | **16 s** |

## Music length for a cutscene

- **Minimum:** the cutscene's length (14 s for `01_intro`, 16 s for `02_the_hollow`). Shorter files are allowed but leave silence at the end.
- **Maximum:** 60 s. Anything after the cutscene ends fades out over 1.5 s, and the next track (ambience or chase) fades in.
- **Best:** exactly the cutscene length plus a 1–2 s tail.
- Format: OGG Vorbis (best), MP3 or WAV; 44.1 or 48 kHz; stereo or mono.
- For `01_intro`, the bell ding is at **7.0 s** and the gate opens at **7.6 s**.
- If you change the music for `02_the_hollow`, put its big hit at about **11.3 s**, or edit the `Hollow(Ignite)` event time in `cutscene.ron` to match your music.

Only add music you have the rights to use, and put its credit in `assets/music/CREDITS.txt`.
