#!/usr/bin/env python3
"""SketchCraft's mark: an engraved beaver with a carpenter's pencil in its
teeth, on a violet tile. Drawn from code so every size comes from one
source:

    python3 brand/beaver.py            # writes brand/*.svg

Shapes are smooth closed curves (Catmull-Rom through hand-placed points, in
a 1000 x 1000 tile). The engraving is fur strokes traced along a flow field
(back from the nose and down the neck), denser and heavier where the light
doesn't reach, with cross-hatching in the deepest shadow. Plain Python, no
dependencies; seeded, so the output never changes unless the code does.
"""

import math
import os
import random

# ---- palette (brand/DESIGN.md) ---------------------------------------------------
VIOLET = "#6f4df0"
VIOLET_DEEP = "#5b39dc"
VIOLET_LITE = "#bfb2ff"
INK = "#15131c"
PAPER = "#f8f3e6"
PENCIL = "#f2c230"
PENCIL_DARK = "#c99a12"
TOOTH = "#e8892b"  # beavers' incisors are orange: iron in the enamel
TOOTH_DARK = "#b85f12"
GRAPHITE = "#4b4658"

OUT = os.path.dirname(os.path.abspath(__file__))


# ---- geometry --------------------------------------------------------------------
def catmull(points, steps=14):
    """A smooth closed curve through `points`."""
    out = []
    n = len(points)
    for i in range(n):
        p0, p1, p2, p3 = points[(i - 1) % n], points[i], points[(i + 1) % n], points[(i + 2) % n]
        for s in range(steps):
            t = s / steps
            t2, t3 = t * t, t * t * t
            out.append(tuple(0.5 * ((2 * p1[k]) + (-p0[k] + p2[k]) * t + (2 * p0[k] - 5 * p1[k] + 4 * p2[k] - p3[k]) * t2 + (-p0[k] + 3 * p1[k] - 3 * p2[k] + p3[k]) * t3) for k in (0, 1)))
    return out


def inside(poly, x, y):
    c = False
    j = len(poly) - 1
    for i in range(len(poly)):
        xi, yi = poly[i]
        xj, yj = poly[j]
        if (yi > y) != (yj > y) and x < (xj - xi) * (y - yi) / (yj - yi + 1e-12) + xi:
            c = not c
        j = i
    return c


def path_d(pts, close=True):
    d = "M" + " L".join(f"{x:.1f} {y:.1f}" for x, y in pts)
    return d + ("Z" if close else "")


def ellipse_pts(cx, cy, rx, ry, rot=0.0, n=40):
    c, s = math.cos(rot), math.sin(rot)
    return [(cx + rx * math.cos(2 * math.pi * k / n) * c - ry * math.sin(2 * math.pi * k / n) * s, cy + rx * math.cos(2 * math.pi * k / n) * s + ry * math.sin(2 * math.pi * k / n) * c) for k in range(n)]


# ---- the beaver (facing left, three-quarter) ------------------------------------
# a broad, flat-topped head, blunt muzzle, tiny ear, thick neck into the body
HEAD = catmull([
    (150, 352),  # muzzle front, under the nose
    (168, 290),  # top of the nose
    (226, 238),  # bridge
    (318, 196),  # brow
    (430, 178),  # flat crown
    (548, 190),
    (650, 240),  # back of the head
    (724, 330),
    (770, 450),
    (808, 600),
    (850, 760),
    (880, 1010),  # down off the tile
    (236, 1010),
    (214, 870),  # chest: chunky, no neck to speak of
    (196, 750),
    (184, 660),  # fat throat
    (164, 590),  # chin
    (138, 520),
    (126, 450),  # lower muzzle
    (126, 400),
])
# small and set low into the head (a big round ear reads as bear or mouse)
EAR = catmull([(560, 214), (568, 178), (600, 160), (636, 170), (646, 204), (618, 226)], 10)
EAR_IN = catmull([(578, 206), (584, 184), (604, 174), (626, 182), (630, 204), (610, 214)], 10)
NOSE = catmull([(130, 330), (150, 282), (204, 268), (246, 290), (248, 332), (212, 362), (160, 366)], 10)
CHEEK = catmull([(176, 372), (240, 350), (320, 368), (356, 430), (330, 486), (250, 498), (182, 470)], 10)
EYE = ellipse_pts(330, 290, 21, 17, -0.15)
# two broad orange incisors, in front of the pencil
TEETH = [
    [(166, 452), (220, 446), (228, 552), (176, 560)],
    [(220, 446), (272, 442), (278, 544), (228, 552)],
]
# the pencil: across the mouth, point down-left, eraser up-right behind the head
P0, P1, PW = (34, 596), (640, 400), 54  # point end, eraser end, width

LIGHT = (-0.75, -0.66)  # from the top left


def tone(x, y):
    """0 lit .. 1 deep shadow, over the head and neck."""
    t = -0.05 + 0.42 * (x - 160) / 700 + 0.5 * (y - 220) / 800
    # rounder: darker toward the silhouette's back edge
    edge = max(0.0, (x - (560 + (y - 200) * 0.35)) / 260)
    t += 0.35 * edge
    # the neck under the jaw sits in shadow
    if y > 560 and x < 520:
        t += 0.25 * min(1.0, (y - 560) / 120)
    # the cheek pad and brow catch light
    if inside(CHEEK, x, y):
        t -= 0.25
    if (x - 330) ** 2 / 160 ** 2 + (y - 240) ** 2 / 70 ** 2 < 1:
        t -= 0.18
    return max(0.0, min(1.0, t))


def flow(x, y):
    """Fur direction: back from the nose over the head, down the neck."""
    nx, ny = x - 170, y - 330
    a = math.atan2(ny, nx)
    # further back, the fur turns down toward the body
    turn = max(0.0, min(1.0, (x - 380) / 420)) * 0.95 + max(0.0, min(1.0, (y - 520) / 300)) * 0.6
    a = a * (1 - turn) + (math.pi / 2 - 0.25) * turn
    return math.cos(a), math.sin(a)


def fur(rng, poly, spacing=7.0, scale=1.0):
    """Engraved fur strokes: each starts on a jittered grid, is kept or not
    by the tone there, and follows the flow, swelling in the middle."""
    strokes = []
    y = 150.0
    while y < 1010:
        x = 100.0 + (rng.random() * spacing)
        while x < 900:
            jx, jy = x + rng.uniform(-3, 3), y + rng.uniform(-3, 3)
            if inside(poly, jx, jy) and not inside(EYE, jx, jy) and not inside(NOSE, jx, jy):
                t = tone(jx, jy)
                # keep fewer strokes where it's lit: the paper shows through
                if rng.random() < 0.06 + 0.94 * t ** 1.5:
                    length = (18 + 30 * t) * rng.uniform(0.75, 1.25)
                    pts = [(jx, jy)]
                    px, py = jx, jy
                    for _ in range(3):
                        fx, fy = flow(px, py)
                        px, py = px + fx * length / 3, py + fy * length / 3
                        if not inside(poly, px, py):
                            break
                        pts.append((px, py))
                    if len(pts) > 2:
                        strokes.append((pts, (0.6 + 1.9 * t) * scale))
            x += spacing * rng.uniform(0.85, 1.15)
        y += spacing * 0.9
    return strokes


def cross_hatch(poly, angle, gap, t_min, width):
    """Straight parallel lines over the parts darker than `t_min`."""
    out = []
    ca, sa = math.cos(angle), math.sin(angle)
    for k in range(-80, 160):
        off = k * gap
        seg = []
        for s in range(0, 1500, 10):
            x = 100 + off * -sa + (s - 300) * ca
            y = 150 + off * ca + (s - 300) * sa
            ok = 0 <= x <= 1000 and 0 <= y <= 1010 and inside(poly, x, y) and tone(x, y) > t_min and not inside(CHEEK, x, y)
            if ok:
                seg.append((x, y))
            elif len(seg) > 2:
                out.append((seg, width))
                seg = []
            else:
                seg = []
        if len(seg) > 2:
            out.append((seg, width))
    return out


def polyline(pts, w, color=INK):
    d = "M" + " ".join(f"{x:.0f} {y:.0f}" for x, y in pts)
    return f'<path d="{d}" fill="none" stroke="{color}" stroke-width="{w:.1f}" stroke-linecap="round" stroke-linejoin="round"/>'


def whiskers(rng):
    out = []
    for k in range(5):
        x0, y0 = 196 + rng.uniform(-4, 4), 380 + k * 14
        a = math.pi + (k - 3) * 0.11 + rng.uniform(-0.04, 0.04)
        L = 120 + rng.uniform(-20, 30)
        pts = [(x0 + math.cos(a) * L * s / 8 + 0.0, y0 + math.sin(a) * L * s / 8 + (s / 8) ** 2 * 18) for s in range(9)]
        out.append(polyline(pts, 2.0))
    # follicles on the whisker pad
    for k in range(16):
        x = 190 + (k % 4) * 17 + rng.uniform(-3, 3)
        y = 376 + (k // 4) * 14 + rng.uniform(-3, 3)
        out.append(f'<circle cx="{x:.1f}" cy="{y:.1f}" r="2.6" fill="{INK}"/>')
    return out


def pencil():
    (x0, y0), (x1, y1) = P0, P1
    L = math.hypot(x1 - x0, y1 - y0)
    ux, uy = (x1 - x0) / L, (y1 - y0) / L
    nx, ny = -uy, ux
    h = PW / 2

    def at(s, o):
        return (x0 + ux * s + nx * o, y0 + uy * s + ny * o)

    tip, wood, body_end, band_end = 0.0, 92.0, L - 64, L - 30
    parts = []
    # graphite point and sharpened wood
    parts.append(f'<path d="{path_d([at(tip, 0), at(34, -h * 0.36), at(34, h * 0.36)])}" fill="{GRAPHITE}" stroke="{INK}" stroke-width="5" stroke-linejoin="round"/>')
    parts.append(f'<path d="{path_d([at(34, -h * 0.36), at(wood, -h), at(wood, h), at(34, h * 0.36)])}" fill="#f1dcb4" stroke="{INK}" stroke-width="5" stroke-linejoin="round"/>')
    # scalloped cut where paint meets wood
    sc = [at(wood, -h)] + [at(wood + (10 if k % 2 else 0), -h + k * PW / 6) for k in range(1, 6)] + [at(wood, h)]
    # the yellow hexagonal body: three facets
    parts.append(f'<path d="{path_d([at(wood, -h), at(body_end, -h), at(body_end, h), at(wood, h)])}" fill="{PENCIL}" stroke="{INK}" stroke-width="5" stroke-linejoin="round"/>')
    parts.append(f'<path d="{path_d([at(wood, h * 0.32), at(body_end, h * 0.32), at(body_end, h), at(wood, h)])}" fill="{PENCIL_DARK}"/>')
    for o in (-h * 0.32, h * 0.32):
        parts.append(polyline([at(wood + 6, o), at(body_end, o)], 2.6))
    parts.append(polyline(sc, 3.2))
    # metal band and eraser
    parts.append(f'<path d="{path_d([at(body_end, -h), at(band_end, -h), at(band_end, h), at(body_end, h)])}" fill="#d8d3c6" stroke="{INK}" stroke-width="5" stroke-linejoin="round"/>')
    for s in (body_end + 10, body_end + 22):
        parts.append(polyline([at(s, -h), at(s, h)], 2.4))
    parts.append(f'<path d="{path_d([at(band_end, -h), at(L - 6, -h * 0.9), at(L, -h * 0.5), at(L, h * 0.5), at(L - 6, h * 0.9), at(band_end, h)])}" fill="{VIOLET_LITE}" stroke="{INK}" stroke-width="5" stroke-linejoin="round"/>')
    # wood grain on the sharpened part
    for k in (-0.5, 0.0, 0.5):
        parts.append(polyline([at(44, h * k * 0.6), at(wood - 6, h * k)], 1.6))
    return "".join(parts)


def beaver(seed=7, detail=1.0):
    """The engraved beaver, in a 1000 x 1000 tile (no background)."""
    rng = random.Random(seed)
    g = []
    # paper-white body
    g.append(f'<path d="{path_d(HEAD)}" fill="{PAPER}"/>')
    # ear behind the head
    g.append(f'<path d="{path_d(EAR)}" fill="{PAPER}" stroke="{INK}" stroke-width="7" stroke-linejoin="round"/>')
    g.append(f'<path d="{path_d(EAR_IN)}" fill="{INK}"/>')
    # engraving
    for pts, w in fur(rng, HEAD, spacing=8.5 / detail):
        g.append(polyline(pts, w))
    for pts, w in cross_hatch(HEAD, 0.62, 12 / detail, 0.8, 1.2) + cross_hatch(HEAD, -0.35, 14 / detail, 0.93, 1.1):
        g.append(polyline(pts, w))
    # outline: heavier on the shadow side
    g.append(f'<path d="{path_d(HEAD)}" fill="none" stroke="{INK}" stroke-width="9" stroke-linejoin="round"/>')
    # fur tufts breaking the outline along the back
    for k in range(46):
        i = int(len(HEAD) * (0.22 + 0.42 * k / 46))
        x, y = HEAD[i]
        fx, fy = flow(x, y)
        L = rng.uniform(10, 22)
        g.append(polyline([(x - fx * 4, y - fy * 4), (x + fx * L + 6, y + fy * L)], 1.8))
    # cheek pad: light, with a soft contour on its lower side
    g.append(polyline(CHEEK[int(len(CHEEK) * 0.35):int(len(CHEEK) * 0.95)], 3.2))
    # brow ridge and eye
    g.append(polyline([(282, 262), (318, 248), (362, 252), (384, 266)], 4.0))
    g.append(f'<path d="{path_d(EYE)}" fill="{INK}"/>')
    g.append(f'<circle cx="323" cy="284" r="5.5" fill="{PAPER}"/>')
    # nose: big, black, glossy
    g.append(f'<path d="{path_d(NOSE)}" fill="{INK}"/>')
    g.append(polyline([(168, 300), (194, 288), (222, 292)], 5.0, PAPER))
    g.append(f'<path d="M188 340c6 6 18 8 28 4" fill="none" stroke="{GRAPHITE}" stroke-width="4" stroke-linecap="round"/>')
    # mouth line toward the pencil
    g.append(polyline([(160, 448), (222, 442), (290, 438), (336, 426)], 4.5))
    g.extend(whiskers(rng))
    # pencil, then the teeth over it
    g.append(pencil())
    for t in TEETH:
        g.append(f'<path d="{path_d(t)}" fill="{TOOTH}" stroke="{INK}" stroke-width="6" stroke-linejoin="round"/>')
        (ax, ay), (bx, by), (cx, cy), (dx, dy) = t
        # enamel shine and the tooth's darker root
        g.append(polyline([(ax + 10, ay + 14), (dx + 8, dy - 14)], 3.0, "#ffd29a"))
        g.append(f'<path d="{path_d([(ax, ay), (bx, by), (bx + 1, by + 16), (ax + 1, ay + 16)])}" fill="{TOOTH_DARK}"/>')
    return "".join(g)


def svg(body, w, h, extra_defs=""):
    return f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w} {h}" width="{w}" height="{h}"><defs>{extra_defs}</defs>{body}</svg>\n'


def tile(radius, art, bg=VIOLET):
    """The mark: violet rounded tile, faint drafting grid, the beaver."""
    grid = "".join(f'<path d="M{k} 0V1000M0 {k}H1000" stroke="#ffffff" stroke-opacity=".07" stroke-width="2"/>' for k in range(100, 1000, 100))
    return (
        f'<clipPath id="tile"><rect width="1000" height="1000" rx="{radius}"/></clipPath>'
        f'<g clip-path="url(#tile)"><rect width="1000" height="1000" fill="{bg}"/>{grid}'
        f'<circle cx="760" cy="250" r="360" fill="{VIOLET_DEEP}" opacity=".35"/>{art}</g>'
    )


def favicon():
    """A flat cut for 16-32 px: broad head, black nose up front, orange
    teeth, the pencil steeply across (flat, it would read as a bill)."""
    return svg(
        f'<rect width="32" height="32" rx="7" fill="{VIOLET}"/>'
        # head and chunky body, broad and flat-topped
        f'<path d="M4.2 14.2C4 10.2 7.6 7.2 13.6 6.4C20.6 5.6 26.4 8.6 28.2 15L30.5 32H8.2L8.2 26.6C6 25.2 4.6 22.6 4.4 19Z" fill="{PAPER}"/>'
        # small ear set low, eye high
        f'<circle cx="21.6" cy="7.6" r="1.7" fill="{INK}"/>'
        f'<circle cx="12.4" cy="11" r="1.2" fill="{INK}"/>'
        # big black nose at the very front
        f'<path d="M2.6 12.6C2.6 10.4 4.6 9.4 6.8 9.8C8.6 10.2 8.8 12.6 7.6 13.8C6 15.2 2.6 15 2.6 12.6Z" fill="{INK}"/>'
        # pencil: steep, point down by the chin, body up behind the cheek
        f'<path d="M5.5 27.6L22.5 14.4L24.6 17L7.8 30Z" fill="{PENCIL}" stroke="{INK}" stroke-width=".9" stroke-linejoin="round"/>'
        f'<path d="M3.4 30.6L5.5 27.6L7.8 30Z" fill="#f1dcb4" stroke="{INK}" stroke-width=".7" stroke-linejoin="round"/>'
        # two orange incisors from under the nose, over the pencil
        f'<path d="M4.8 16H10.4L10.6 22.4H5.4Z" fill="{TOOTH}" stroke="{INK}" stroke-width="1" stroke-linejoin="round"/>'
        f'<path d="M7.7 16.1V22.3" stroke="{INK}" stroke-width=".9"/>',
        32,
        32,
    )


def main():
    art = beaver()
    # the mark, square with rounded corners (README, web, docs)
    with open(os.path.join(OUT, "mark.svg"), "w") as f:
        f.write(svg(tile(200, art), 1000, 1000))
    # macOS app icon: 1024 canvas, 824 tile, 100 margin, soft shadow (macOS doesn't round icons)
    shadow = '<filter id="sh" x="-20%" y="-20%" width="140%" height="140%"><feGaussianBlur in="SourceAlpha" stdDeviation="14"/><feOffset dy="12"/><feComponentTransfer><feFuncA type="linear" slope=".32"/></feComponentTransfer><feMerge><feMergeNode/><feMergeNode in="SourceGraphic"/></feMerge></filter>'
    with open(os.path.join(OUT, "app-icon.svg"), "w") as f:
        f.write(svg(f'<g filter="url(#sh)"><g transform="translate(100 100) scale(.824)">{tile(225, art)}</g></g>', 1024, 1024, shadow))
    with open(os.path.join(OUT, "favicon.svg"), "w") as f:
        f.write(favicon())
    # the logo lockup: mark + wordmark (Bricolage Grotesque, else a sturdy system sans)
    word = f'<text x="1110" y="610" font-family="Bricolage Grotesque, Avenir Next, Helvetica Neue, Arial, sans-serif" font-weight="700" font-size="300" letter-spacing="-6" fill="{INK}">SketchCraft</text>'
    with open(os.path.join(OUT, "logo.svg"), "w") as f:
        f.write(svg(f'<g>{tile(200, art)}</g>{word}', 2900, 1000))
    # README banner: drafting paper, the mark, wordmark and tagline
    grid = "".join(f'<path d="M{k} 0V520" stroke="#d8cfb8" stroke-opacity=".55" stroke-width="1"/>' for k in range(40, 1800, 40)) + "".join(f'<path d="M0 {k}H1800" stroke="#d8cfb8" stroke-opacity=".55" stroke-width="1"/>' for k in range(40, 520, 40))
    font = "Bricolage Grotesque, Avenir Next, Helvetica Neue, Arial, sans-serif"
    banner = (
        f'<rect width="1800" height="520" fill="#f6f1e4"/>{grid}'
        f'<g transform="translate(90 70) scale(.38)">{tile(200, art)}</g>'
        f'<text x="560" y="250" font-family="{font}" font-weight="700" font-size="150" letter-spacing="-4" fill="{INK}">SketchCraft</text>'
        f'<rect x="566" y="290" width="150" height="10" rx="5" fill="{PENCIL}"/>'
        f'<text x="562" y="380" font-family="{font}" font-weight="500" font-size="62" fill="#4a2fc4">Draw an outline. Pull it up.</text>'
    )
    with open(os.path.join(OUT, "banner.svg"), "w") as f:
        f.write(svg(banner, 1800, 520))
    print("wrote mark.svg, app-icon.svg, favicon.svg, logo.svg, banner.svg in", OUT)


if __name__ == "__main__":
    main()
