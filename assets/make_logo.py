#!/usr/bin/env python3
"""Hammer + lightning-bolt logo for blitznir.

Exact geometry, symmetric by construction. Writes hammer.svg and hammer.png
(512 px, white on black) next to this file; make-glow.sh makes the tray's
glow-*.png from hammer.png. icon-1024.png, Blitznir.icns and tray-*.png have
no generator in this repo: they are committed as drawn.

    pip install cairosvg pillow numpy
    python3 make_logo.py

Every dimension below is a named constant."""
import io, math, pathlib, cairosvg, numpy as np
from PIL import Image

# --- head --------------------------------------------------------------------
W, H, D = 797, 657, 92            # half-width, height, notch depth
A, B, C, M = 448, 353, 189, 165   # cap inner, notch outer, notch inner, middle half
CB = 268                          # bottom notch inner edge: wider, so the wedge
                                  # clears the shaft on both sides
top = [(-W,0), (-A,0), (-B,D), (-C,D), (-M,0), (M,0), (C,D), (B,D), (A,0), (W,0)]
bot = [(-W,0), (-A,0), (-B,D), (-CB,D), (-M,0), (M,0), (CB,D), (B,D), (A,0), (W,0)]
head = "M " + " L ".join(f"{x},{y}" for x, y in top)
head += " L " + " L ".join(f"{x},{H-y}" for x, y in reversed(bot)) + " Z"

# --- bolt --------------------------------------------------------------------
OV = 40                           # overlap into the head, kills the AA seam
BA = 0                            # degrees the shaft swings off the head
bolt = (f"M {-M},{H-OV} L {M},{H-OV} L {M},{H} "
        f"L 235,{H+800} L -30,{H+800} "
        f"L 45,{H+1540} "
        f"L -215,{H+640} L 10,{H+640} L {-M},{H} Z")

shapes = f'<path d="{head}"/><g transform="rotate({BA} 0 {H})"><path d="{bolt}"/></g>'
variants = {"hammer": f'<g fill="#fff">{shapes}</g>'}

VB = (-W-30, -30, 2*W+60, H+1560)
S2 = math.hypot(VB[2], VB[3])
OUT = pathlib.Path(__file__).parent
MARGIN = 0.82                     # fraction of the frame the mark occupies

def build(body, view):
    return (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="{view}">'
            f'<rect x="-99999" y="-99999" width="199999" height="199999" fill="#000"/>'
            f'<g transform="rotate(-25 {S2/2:.0f} {S2/2:.0f})">'
            f'<g transform="translate({(S2-VB[2])/2-VB[0]:.0f},{(S2-VB[3])/2-VB[1]:.0f})">'
            f'{body}</g></g></svg>')

for name, body in variants.items():
    # measure the rotated mark, then set a viewBox that centres it
    probe = build(body, f"0 0 {S2:.0f} {S2:.0f}")
    png = cairosvg.svg2png(bytestring=probe.encode(),
                           output_width=1200, output_height=1200)
    a = np.asarray(Image.open(io.BytesIO(png)).convert("L"))
    ys, xs = np.where(a > 128)
    k = S2 / 1200.0                                   # px -> user units
    bx0, bx1, by0, by1 = xs.min()*k, xs.max()*k, ys.min()*k, ys.max()*k
    side = max(bx1-bx0, by1-by0) / MARGIN
    vx = (bx0+bx1)/2 - side/2
    vy = (by0+by1)/2 - side/2
    svg = build(body, f"{vx:.1f} {vy:.1f} {side:.1f} {side:.1f}")
    (OUT / f"{name}.svg").write_text(svg)
    cairosvg.svg2png(url=str(OUT / f"{name}.svg"), write_to=str(OUT / f"{name}.png"),
                     output_width=512, output_height=512)
    print(f"  wrote {name}.svg + {name}.png")
