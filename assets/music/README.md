# Music drop-in folder

Put your own tracks here. Name them exactly like this (any of `.ogg`, `.mp3`, `.wav`):

| File name | Where it plays | Loops? |
|-----------|----------------|--------|
| `menu.ogg` | Main menu | yes |
| `elevator.ogg` | Inside the lift while players gather, until everyone has joined (then a 15 s countdown) or the host presses Enter | yes |
| `ambience.ogg` | Exploring the rooms | yes |
| `chase.ogg` | While an entity is hunting you | yes |

If a file is missing, the game plays its built-in original score instead.
Cutscene music goes in each cutscene's own folder: see `assets/cutscenes/README.md`.

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
