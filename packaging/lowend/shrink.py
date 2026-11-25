#!/usr/bin/env python3
"""Makes an assets/ folder lighter for the HALLWAYS Low-End Edition.

usage: shrink.py ASSETS_DIR [MAX_SIZE]

* Every texture image under textures/ is scaled down so its longest side is
  at most MAX_SIZE pixels (default 256).
* Images embedded in .glb models under models/ get the same treatment (the
  binary chunk is rebuilt with the smaller images).
* Writes the LOWEND marker file, which the game reads to switch on its
  low-end tweaks (Low graphics by default, fewer rooms loaded, no mirrors).

Needs Pillow (pip install pillow).
"""
import io
import json
import os
import struct
import sys

from PIL import Image

MARKER_TEXT = "HALLWAYS Low-End Edition: smaller textures, Low graphics by default, fewer rooms loaded ahead, no mirrors.\n"


def shrink_image(data: bytes, max_size: int, fmt_hint: str) -> bytes:
    im = Image.open(io.BytesIO(data))
    if max(im.size) <= max_size:
        return data
    im.thumbnail((max_size, max_size), Image.LANCZOS)
    out = io.BytesIO()
    if fmt_hint == "png" or im.mode in ("RGBA", "LA", "P"):
        im.save(out, "PNG", optimize=True)
    else:
        im.convert("RGB").save(out, "JPEG", quality=85)
    return out.getvalue()


def shrink_file(path: str, max_size: int) -> int:
    with open(path, "rb") as f:
        data = f.read()
    fmt = "png" if path.lower().endswith(".png") else "jpg"
    new = shrink_image(data, max_size, fmt)
    if new is data or len(new) >= len(data):
        return 0
    with open(path, "wb") as f:
        f.write(new)
    return len(data) - len(new)


def pad4(b: bytes, fill: bytes = b"\0") -> bytes:
    return b + fill * ((4 - len(b) % 4) % 4)


def shrink_glb(path: str, max_size: int) -> int:
    with open(path, "rb") as f:
        data = f.read()
    magic, version, _length = struct.unpack_from("<III", data, 0)
    if magic != 0x46546C67 or version != 2:
        return 0
    off, json_chunk, bin_chunk = 12, None, b""
    while off < len(data):
        clen, ctype = struct.unpack_from("<II", data, off)
        body = data[off + 8 : off + 8 + clen]
        if ctype == 0x4E4F534A:
            json_chunk = json.loads(body.decode("utf-8"))
        elif ctype == 0x004E4942:
            bin_chunk = body
        off += 8 + clen
    if json_chunk is None or not bin_chunk:
        return 0
    views = json_chunk.get("bufferViews", [])
    image_views = {}
    for img in json_chunk.get("images", []):
        if "bufferView" in img:
            mime = img.get("mimeType", "image/jpeg")
            image_views[img["bufferView"]] = "png" if "png" in mime else "jpg"
    if not image_views:
        return 0
    # Rebuild the binary chunk view by view (4-byte aligned), shrinking images.
    new_bin = b""
    for i, v in enumerate(views):
        if v.get("buffer", 0) != 0:
            continue
        start = v.get("byteOffset", 0)
        chunk = bin_chunk[start : start + v["byteLength"]]
        if i in image_views:
            chunk = shrink_image(chunk, max_size, image_views[i])
        new_bin = pad4(new_bin)
        v["byteOffset"] = len(new_bin)
        v["byteLength"] = len(chunk)
        new_bin += chunk
    new_bin = pad4(new_bin)
    json_chunk["buffers"][0]["byteLength"] = len(new_bin)
    js = pad4(json.dumps(json_chunk, separators=(",", ":")).encode("utf-8"), b" ")
    out = struct.pack("<III", 0x46546C67, 2, 12 + 8 + len(js) + 8 + len(new_bin))
    out += struct.pack("<II", len(js), 0x4E4F534A) + js
    out += struct.pack("<II", len(new_bin), 0x004E4942) + new_bin
    if len(out) >= len(data):
        return 0
    with open(path, "wb") as f:
        f.write(out)
    return len(data) - len(out)


def main() -> None:
    assets = sys.argv[1]
    max_size = int(sys.argv[2]) if len(sys.argv) > 2 else 256
    saved = 0
    for sub, exts, fn in (("textures", (".jpg", ".jpeg", ".png"), shrink_file), ("models", (".glb",), shrink_glb), ("models", (".jpg", ".jpeg", ".png"), shrink_file)):
        for root, _dirs, files in os.walk(os.path.join(assets, sub)):
            for name in files:
                if name.lower().endswith(exts):
                    saved += fn(os.path.join(root, name), max_size)
    with open(os.path.join(assets, "LOWEND"), "w") as f:
        f.write(MARKER_TEXT)
    print(f"low-end assets: saved {saved / 1e6:.1f} MB (textures at most {max_size}px)")


if __name__ == "__main__":
    main()
