"""Rebuild every icon from the design sources:  python3 _design/icons.py

Needs Sketch, Xcode 26+ and `brew install imagemagick librsvg oxipng`.

unplugg.sketch  → logo.png + logo.icns (macOS 10.15–15, refit to Apple's squircle grid;
                  the .sketch is kept out of git, so only its owner can re-run this step)
                  + logo.ico           (Windows)
unplugg.icon    → Assets.car           (macOS 26+, Liquid Glass light/dark/clear/tinted)
menubar*.svg    → menubar*.pdf         (status item: idle + armed templates, red alarm)
                  + a copy in assets   (Windows and Linux tray)
plus the Telegram bot avatar and GitHub social preview, written next to this file.
"""
import json
import struct
import shutil
import subprocess
import tempfile
import zipfile
from pathlib import Path

DESIGN = Path(__file__).resolve().parent
ASSETS = DESIGN.parent / "assets"
SKETCHTOOL = "/Applications/Sketch.app/Contents/MacOS/sketchtool"

# The Sketch plate is 400pt at (200,100). Apple's grid wants an 824px body on a
# 1024 canvas with a 22.5% smooth corner, so export at 2x of 824/400 and crop.
SCALE = 4.12
CROP = "2048x2048+624+212"
SQUIRCLE = 0.225


def run(*args):
    subprocess.run([str(a) for a in args], check=True, capture_output=True)


def export(tmp, keep):
    """Render one patched copy of the logo. keep: plate | gloss | mask."""
    with zipfile.ZipFile(DESIGN / "unplugg.sketch") as src:
        page_name = next(n for n in src.namelist() if n.startswith("pages/"))
        page = json.loads(src.read(page_name))
        board = page["layers"][0]
        for fill in board["style"]["fills"]:
            fill["isEnabled"] = False
        for layer in board["layers"]:
            is_group = layer["_class"] == "group"
            # loose top-level shapes are the gloss; "Rectangle 1" is the backdrop
            layer["isVisible"] = layer["name"] != "Rectangle 1" and (keep == "gloss") != is_group
            for child in layer.get("layers", []):
                if child["name"].startswith("Rectangle 2"):
                    child["pointRadiusBehaviour"] = 2  # smooth corners
                    for point in child["points"]:
                        point["cornerRadius"] = child["frame"]["width"] * SQUIRCLE
                if keep == "mask":
                    child["isVisible"] = child["name"] == "Rectangle 2"
                    child["style"]["shadows"] = []
        patched = tmp / f"{keep}.sketch"
        with zipfile.ZipFile(patched, "w") as out:
            for n in src.namelist():
                out.writestr(n, json.dumps(page) if n == page_name else src.read(n))
    run(SKETCHTOOL, "export", "artboards", patched, "--formats=png", f"--scales={SCALE}",
        f"--output={tmp}", "--overwriting=YES")
    return next(tmp.glob("Artboard 1@*.png")).rename(tmp / f"{keep}.png")


# iconutil would recompress the optimised PNGs, so pack the icns by hand: a header,
# then each PNG behind its 4-letter type and length
ICNS = [("icp4", "16x16"), ("ic11", "16x16@2x"), ("icp5", "32x32"), ("ic12", "32x32@2x"),
        ("ic07", "128x128"), ("ic13", "128x128@2x"), ("ic08", "256x256"), ("ic14", "256x256@2x"),
        ("ic09", "512x512"), ("ic10", "512x512@2x")]


def pack_icns(iconset, out):
    body = b""
    for kind, name in ICNS:
        png = (iconset / f"icon_{name}.png").read_bytes()
        body += kind.encode() + struct.pack(">I", 8 + len(png)) + png
    out.write_bytes(b"icns" + struct.pack(">I", 8 + len(body)) + body)


def png_256(ico, tmp):
    """magick stores the 256 px size as a 270 KB bitmap; Windows reads PNG there too."""
    png = tmp / "256.png"
    run("magick", f"{ico}[0]", png)
    run("oxipng", "-q", "-o", "max", "--strip", "safe", png)
    data = ico.read_bytes()
    count = struct.unpack_from("<H", data, 4)[0]
    entries = [list(struct.unpack_from("<BBBBHHII", data, 6 + 16 * i)) for i in range(count)]
    images = [data[e[7]:e[7] + e[6]] for e in entries]
    images[0] = png.read_bytes()  # the first entry is the 256 px one
    offset, table = 6 + 16 * count, b""
    for entry, image in zip(entries, images):
        entry[6], entry[7] = len(image), offset
        table += struct.pack("<BBBBHHII", *entry)
        offset += len(image)
    ico.write_bytes(data[:6] + table + b"".join(images))


def main():
    with tempfile.TemporaryDirectory() as t:
        tmp = Path(t)
        plate, gloss, mask = (export(tmp, k) for k in ("plate", "gloss", "mask"))
        master = tmp / "master.png"
        run("magick", gloss, mask, "-compose", "DstIn", "-composite", gloss)
        # the design's soft shadow outgrows Apple's margin; fade it before the bottom edge
        run("magick", plate, gloss, "-compose", "over", "-composite", "-crop", CROP, "+repage",
            "(", "-size", "2048x1908", "xc:white", "-size", "2048x140", "gradient:white-none",
            "-append", ")", "-compose", "DstIn", "-composite", master)

        iconset = tmp / "logo.iconset"
        iconset.mkdir()
        for pt in (16, 32, 128, 256, 512):
            for scale, suffix in ((1, ""), (2, "@2x")):
                px = pt * scale
                run("magick", master, "-filter", "Lanczos", "-resize", f"{px}x{px}", "-strip",
                    iconset / f"icon_{pt}x{pt}{suffix}.png")
        run("oxipng", "-q", "-o", "max", "--strip", "safe", *iconset.glob("*.png"))  # lossless
        pack_icns(iconset, ASSETS / "logo.icns")
        shutil.copy(iconset / "icon_512x512@2x.png", ASSETS / "logo.png")
        run("magick", ASSETS / "logo.png", "-define", "icon:auto-resize=256,64,48,32,24,16",
            ASSETS / "logo.ico")
        png_256(ASSETS / "logo.ico", tmp)

        run("xcrun", "actool", DESIGN / "unplugg.icon", "--compile", tmp, "--app-icon", "unplugg",
            "--platform", "macosx", "--target-device", "mac",
            "--minimum-deployment-target", "26.0", "--output-partial-info-plist", "/dev/null")
        shutil.copy(tmp / "Assets.car", ASSETS)

        for name in ("menubar", "menubar_armed", "menubar_alarm"):
            run("rsvg-convert", "-f", "pdf", DESIGN / f"{name}.svg", "-o", ASSETS / f"{name}.pdf")
            shutil.copy(DESIGN / f"{name}.svg", ASSETS)  # the Windows and Linux tray tints these

        # Telegram crops bot photos to a circle, so use the plate face edge to edge.
        run("magick", master, "-crop", "1648x1648+200+200", "+repage", "-background", "#e4e5ea",
            "-alpha", "remove", "-resize", "640x640", DESIGN / "telegram-avatar.png")
        card = tmp / "card.png"
        run("rsvg-convert", DESIGN / "social-preview.svg", "-o", card)
        run("magick", card, "(", ASSETS / "logo.png", "-resize", "520x520", ")",
            "-geometry", "+60+60", "-composite", DESIGN / "social-preview.png")


if __name__ == "__main__":
    main()
