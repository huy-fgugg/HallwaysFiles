#!/usr/bin/env python3
"""Builds the HALLWAYS trailer from recorded game frames.

Inputs:
  frames/   frame_00000.png ... and segments.txt from `HALLWAYS_TRAILER=frames`
  audio/    WAVs from `cargo run --example trailer_audio -- audio`
Output:
  an H.264 MP4 at 24 fps with title cards, fades and the game's own music.

Usage: make_trailer.py <frames> <audio> <font.ttf> <font-bold.ttf> <out.mp4>
"""
import shutil
import subprocess
import sys
import tempfile
import wave
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw, ImageEnhance, ImageFont

FPS = 24
W, H = 1280, 720
RATE = 22050

frames_dir, audio_dir, font_regular, font_bold, out_path = map(Path, sys.argv[1:6])

segments = {}
for line in (frames_dir / "segments.txt").read_text().split("\n"):
    if line.strip():
        name, a, b = line.split()
        segments[name] = (int(a), int(b))

# (kind, payload): clips refer to segments; cards are (title, subtitle, seconds);
# "montage" is a list of segments cut together fast.
TIMELINE = [
    ("studio", ("CREATED BY DONUT STUDIOS!", "", 3.6)),
    ("card", ("THE LIFT ONLY GOES DOWN.", "", 2.0)),
    ("clip", "arrival"),
    ("clip", "guest_wing"),
    ("clip", "zigzag"),
    ("card", ("100 DOORS.", "NONE OF THEM THE SAME.", 2.0)),
    ("clip", "archive"),
    ("card", ("FOLLOW THE LIGHT.", "", 1.3)),
    ("clip", "reading_hall"),
    ("clip", "gardens"),
    ("card", ("SOME DOORS LIE.", "", 1.4)),
    ("clip", "fake_doors"),
    ("clip", "dark"),
    ("clip", "gallery"),
    ("card", ("SOMETHING IS LISTENING.", "", 1.6)),
    ("clip", "listener"),
    ("clip", "surge"),
    ("card", ("AFTER DOOR 40...", "", 1.5)),
    ("clip", "hollow"),
    ("run", ("RUN.", "", 0.9)),
    ("clip", "chase"),
    ("clip", "slam"),
    ("card", ("NO TWO CHASES ARE THE SAME.", "HANDS IN THE WALLS  -  FIRE  -  FALLING CEILINGS", 2.2)),
    ("card", ("MULTIPLAYER SUPPORTED", "CROSSPLAY  -  WINDOWS  -  LINUX  -  ANDROID", 2.4)),
    ("clip", "squad"),
    ("montage", ["chase", "surge", "gallery", "reading_hall", "listener", "fake_doors", "slam"]),
    ("clip", "ending"),
    ("final", ("HALLWAYS", "AVAILABLE NOW", 4.8)),
]
MONTAGE_FRAMES = 10

big = ImageFont.truetype(str(font_bold), 64)
huge = ImageFont.truetype(str(font_bold), 110)
small = ImageFont.truetype(str(font_regular), 28)
tag = ImageFont.truetype(str(font_regular), 22)
RED = (224, 90, 78)
TEAL = (127, 209, 217)
TEXT = (232, 230, 227)


def centered(draw, y, text, font, fill):
    w = draw.textlength(text, font=font)
    draw.text(((W - w) / 2, y), text, font=font, fill=fill)


def flicker(t, seed):
    """Bulb-like flicker for the first ~0.35 s of a card (deterministic)."""
    if t > 0.35:
        return 1.0
    k = int(t * FPS)
    return [0.0, 1.0, 0.15, 0.9, 0.0, 1.0, 0.6, 1.0, 1.0][(k + seed) % 9]


def card_frame(title, sub, alpha, final=False, t=0.0, red=False):
    img = Image.new("RGB", (W, H), (0, 0, 0))
    d = ImageDraw.Draw(img)
    a = max(0.0, min(1.0, alpha)) * (1.0 if final else flicker(t, len(title)))
    if red:
        # One huge red word that punches in slightly larger, then settles.
        size = int(200 * (1.12 - 0.12 * min(1.0, t / 0.25)))
        f = ImageFont.truetype(str(font_bold), size)
        col = tuple(int(c * a) for c in RED)
        w = d.textlength(title, font=f)
        d.text(((W - w) / 2, (H - size) / 2 - 20), title, font=f, fill=col)
        return img
    col = tuple(int(c * a) for c in TEXT)
    if final:
        # Title with a slow red underline sweep, then the call to action.
        centered(d, 230, title, huge, col)
        line_w = int(520 * min(1.0, t / 0.8))
        d.rectangle([(W - line_w) // 2, 365, (W + line_w) // 2, 370], fill=tuple(int(c * a) for c in RED))
        k = max(0.0, min(1.0, (t - 0.6) / 0.6)) * a
        centered(d, 400, sub, big, tuple(int(c * k) for c in TEAL))
        k2 = max(0.0, min(1.0, (t - 1.2) / 0.6)) * a
        centered(d, 490, "MULTIPLAYER SUPPORTED  -  CROSSPLAY", tag, tuple(int(c * k2) for c in TEXT))
        centered(d, 525, "WINDOWS  -  LINUX  -  ANDROID", tag, tuple(int(c * k2 * 0.7) for c in TEXT))
    else:
        y = 300 if not sub else 270
        centered(d, y, title, big, col)
        if sub:
            centered(d, y + 95, sub, small, tuple(int(c * a) for c in TEAL))
    return img


LOGO = Image.open(Path(__file__).resolve().parents[2] / "assets" / "branding" / "donut_studios.png").convert("RGBA")
PINK = (255, 115, 217)


def studio_frame(alpha, t):
    """Opening card: CREATED BY DONUT STUDIOS! over the studio logo, with a slow push-in."""
    img = Image.new("RGB", (W, H), (8, 5, 12))
    a = max(0.0, min(1.0, alpha))
    h = int(500 * (1.0 + 0.04 * t / 3.6))
    w = int(LOGO.width * h / LOGO.height)
    logo = LOGO.resize((w, h), Image.LANCZOS)
    if a < 1.0:
        logo.putalpha(logo.getchannel("A").point(lambda v: int(v * a)))
    img.paste(logo, ((W - w) // 2, 170 + (500 - h) // 2), logo)
    d = ImageDraw.Draw(img)
    left, right = "CREATED BY ", "DONUT STUDIOS!"
    wl, wr = d.textlength(left, font=big), d.textlength(right, font=big)
    x = (W - wl - wr) / 2
    d.text((x, 70), left, font=big, fill=tuple(int(c * a) for c in TEXT))
    d.text((x + wl, 70), right, font=big, fill=tuple(int(c * a) for c in PINK))
    return img


def fade(i, n, edge=6):
    """0..1 brightness for frame i of n with `edge`-frame fades at both ends."""
    return min(1.0, (i + 1) / edge, (n - i) / edge)


def read_wav(name):
    with wave.open(str(audio_dir / f"{name}.wav")) as w:
        data = np.frombuffer(w.readframes(w.getnframes()), dtype=np.int16).astype(np.float32) / 32768.0
    return data


def place(track, clip, start_s, gain=1.0, length_s=None, loop=False):
    s0 = int(start_s * RATE)
    if length_s is not None:
        n = int(length_s * RATE)
        if loop:
            reps = int(np.ceil(n / len(clip)))
            clip = np.tile(clip, reps)
        clip = clip[:n].copy()
        f = min(len(clip), int(0.4 * RATE))
        clip[:f] *= np.linspace(0, 1, f)
        clip[-f:] *= np.linspace(1, 0, f)
    e = min(len(track), s0 + len(clip))
    track[s0:e] += clip[: e - s0] * gain


with tempfile.TemporaryDirectory() as tmp:
    out_frames = Path(tmp) / "f"
    out_frames.mkdir()
    n_out = 0
    marks = []  # (kind, name, start seconds, seconds)

    def emit(img):
        global n_out
        img.save(out_frames / f"{n_out:05d}.png")
        n_out += 1

    for kind, payload in TIMELINE:
        start = n_out / FPS
        if kind == "clip":
            if payload not in segments:
                print(f"missing segment {payload}, skipping")
                continue
            a, b = segments[payload]
            n = b - a + 1
            for i in range(n):
                p = frames_dir / f"frame_{a + i:05d}.png"
                if not p.exists():
                    continue
                img = Image.open(p).convert("RGB").resize((W, H))
                k = fade(i, n)
                if k < 1.0:
                    img = ImageEnhance.Brightness(img).enhance(k)
                emit(img)
        elif kind == "montage":
            # Fast cuts, each starting on a white flash.
            for name in payload:
                if name not in segments:
                    continue
                a, b = segments[name]
                mid = (a + b) // 2
                for i in range(MONTAGE_FRAMES):
                    p = frames_dir / f"frame_{min(b, mid + i):05d}.png"
                    if not p.exists():
                        continue
                    img = Image.open(p).convert("RGB").resize((W, H))
                    if i == 0:
                        img = ImageEnhance.Brightness(img).enhance(2.6)
                    elif i == 1:
                        img = ImageEnhance.Brightness(img).enhance(1.5)
                    emit(img)
                marks.append(("cut", name, n_out / FPS - MONTAGE_FRAMES / FPS, MONTAGE_FRAMES / FPS))
            marks.append(("montage", "montage", start, n_out / FPS - start))
            continue
        elif kind == "studio":
            title, sub, secs = payload
            n = int(secs * FPS)
            for i in range(n):
                emit(studio_frame(fade(i, n, 12), i / FPS))
        else:
            title, sub, secs = payload
            n = int(secs * FPS)
            for i in range(n):
                t = i / FPS
                emit(card_frame(title, sub, fade(i, n, 8) if kind != "run" else min(1.0, (n - i) / 4), final=(kind == "final"), t=t, red=(kind == "run")))
        marks.append((kind, payload if kind == "clip" else payload[0], start, n_out / FPS - start))

    total = n_out / FPS
    print(f"{n_out} frames, {total:.1f} s")

    # ---- Soundtrack: the menu theme while you explore, a low rumble bed
    # once something is listening, the chase theme from "RUN." through the
    # montage, the ending cue's crash, then a stinger and the victory theme.
    mix = np.zeros(int((total + 1.0) * RATE), dtype=np.float32)
    menu, chase, ending, victory = (read_wav(n) for n in ("menu", "chase", "ending", "victory"))
    whoosh, stinger, rumble, roar = read_wav("whoosh"), read_wav("stinger"), read_wav("rumble"), read_wav("roar")

    def at(name, kind=None, default=0.0):
        return next((s for k, n, s, _ in marks if n == name and (kind is None or k == kind)), default)

    listen_at = at("SOMETHING IS LISTENING.", default=total * 0.45)
    run_at = at("RUN.", default=total * 0.6)
    end_clip = next(((s, d) for k, n, s, d in marks if n == "ending"), (total - 9, 5))
    final_at = at("HALLWAYS", "final", total - 4.8)
    studio_end = next((s + d for k, n, s, d in marks if k == "studio"), 0.0)
    place(mix, stinger, 0.3, 0.45)
    place(mix, menu, studio_end, 0.55, length_s=listen_at + 0.4 - studio_end, loop=True)
    place(mix, rumble, listen_at, 0.45, length_s=run_at - listen_at, loop=True)
    place(mix, chase, run_at, 0.65, length_s=end_clip[0] - run_at + 0.3, loop=True)
    place(mix, stinger, run_at, 0.9)
    # Ending cue from 12.3 s of the cutscene so its crash lands on the cut to black.
    cue = ending[int(12.3 * RATE):]
    place(mix, cue, end_clip[0], 0.9, length_s=min(len(cue) / RATE, final_at - end_clip[0] + 0.2))
    place(mix, stinger, final_at, 0.8)
    place(mix, victory, final_at + 0.4, 0.5, length_s=total - final_at - 0.2)
    for k, n, s, d in marks:
        if k == "card":
            place(mix, whoosh, max(0.0, s - 0.15), 0.55)
        if n == "hollow" and k == "clip":
            place(mix, rumble, s, 0.5)
        if n == "surge" and k == "clip":
            place(mix, roar, s + 1.1, 0.8)
        if n == "slam" and k == "clip":
            place(mix, stinger, s + 0.8, 0.9)
        if k == "cut":
            place(mix, whoosh[: int(0.35 * RATE)], s, 0.7)
    peak = np.max(np.abs(mix)) or 1.0
    mix = np.clip(mix / peak * 0.9, -1, 1)
    wav_path = Path(tmp) / "mix.wav"
    with wave.open(str(wav_path), "w") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(RATE)
        w.writeframes((mix * 32767).astype(np.int16).tobytes())

    subprocess.run(
        [
            "ffmpeg", "-y", "-loglevel", "error",
            "-framerate", str(FPS), "-i", str(out_frames / "%05d.png"),
            "-i", str(wav_path),
            "-c:v", "libx264", "-preset", "medium", "-crf", "20", "-pix_fmt", "yuv420p",
            "-c:a", "aac", "-b:a", "160k", "-ar", "44100",
            "-shortest", "-movflags", "+faststart", str(out_path),
        ],
        check=True,
    )
    print(f"wrote {out_path}")
