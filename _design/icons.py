"""Rebuild every icon from the design sources:  python3 _design/icons.py

Needs Sketch, Xcode 26+ and `brew install imagemagick librsvg`.

unplugg.sketch  → logo.png + logo.icns (macOS 10.15–15, refit to Apple's squircle grid;
                  the .sketch is kept out of git, so only its owner can re-run this step)
unplugg.icon    → Assets.car           (macOS 26+, Liquid Glass light/dark/clear/tinted)
menubar*.svg    → menubar*.pdf         (template images for the status item)
plus the Telegram bot avatar and GitHub social preview, written next to this file.
"""
import json
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
        run("iconutil", "-c", "icns", iconset, "-o", ASSETS / "logo.icns")
        shutil.copy(iconset / "icon_512x512@2x.png", ASSETS / "logo.png")

        run("xcrun", "actool", DESIGN / "unplugg.icon", "--compile", tmp, "--app-icon", "unplugg",
            "--platform", "macosx", "--target-device", "mac",
            "--minimum-deployment-target", "26.0", "--output-partial-info-plist", "/dev/null")
        shutil.copy(tmp / "Assets.car", ASSETS)

        for name in ("menubar", "menubar_armed"):
            run("rsvg-convert", "-f", "pdf", DESIGN / f"{name}.svg", "-o", ASSETS / f"{name}.pdf")

        # Telegram crops bot photos to a circle, so use the plate face edge to edge.
        run("magick", master, "-crop", "1648x1648+200+200", "+repage", "-background", "#e4e5ea",
            "-alpha", "remove", "-resize", "640x640", DESIGN / "telegram-avatar.png")
        card = tmp / "card.png"
        run("rsvg-convert", DESIGN / "social-preview.svg", "-o", card)
        run("magick", card, "(", ASSETS / "logo.png", "-resize", "520x520", ")",
            "-geometry", "+60+60", "-composite", DESIGN / "social-preview.png")


if __name__ == "__main__":
    main()
