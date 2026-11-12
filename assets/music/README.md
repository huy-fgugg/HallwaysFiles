# Music drop-in folder

**Easy setup: every song has its own folder. Drop one audio file into a folder; any file name works.**

| Folder | Where it plays | Loops? | Length |
|--------|----------------|--------|--------|
| `menu/` | Main menu | yes | 20 s – 5 min |
| `elevator/` | In the lift while players gather, until everyone has joined (then a 15 s countdown) or the host presses Enter | yes | 20 s – 5 min |
| `ambience/` | Exploring the rooms | yes | 20 s – 5 min |
| `chase/` | While something hunts you | yes | 20 s – 5 min |
| `cutscene_intro/` | The elevator arrival cutscene | no | 14 s – 60 s (bell at 7.0 s) |
| `cutscene_hollow/` | The Hollow cutscene | no | 16 s – 60 s (big hit at 11.3 s) |

- Supported: `.ogg` (best), `.mp3`, `.wav`.
- If a folder holds several songs, the first one alphabetically plays.
- Empty folder = the game's built-in original score.
- Check it worked: main menu → **Credits** → "YOUR MUSIC" lists the file each slot found.
- The older names (`music/menu.ogg`, `cutscenes/01_intro/music.ogg`, ...) still work.

## Length limits for looping tracks

- **Minimum: 20 seconds.** Shorter loops get repetitive fast.
- **Maximum: 5 minutes.** Longer files waste memory and make the download bigger.
- Cut the file so the end flows straight back into the start (a seamless loop). Don't add silence at the start or end.
- Format: OGG Vorbis (best), MP3 or WAV; 44.1 or 48 kHz. Keep each file under about 10 MB.

## Credits

Write one line per track in `CREDITS.txt`, for example:

```
Chase theme: "Track Name" by Artist Name (licence: CC BY 4.0)
```

The Credits screen in the main menu shows that file automatically.
Only use music you have permission to use. Crediting someone is not the same as having their permission.
