"""Shared pieces for the Drop Pod icon concepts: the macOS frame, the spark and the streak."""
import math

SIZE = 1024
BODY_X, BODY_Y, BODY_W, RADIUS = 100, 100, 824, 185  # macOS icon grid: 824 body, 100 margins


def f(v):
    return f"{v:.1f}".rstrip("0").rstrip(".")


def pts(points):
    return " ".join(f"{f(x)},{f(y)}" for x, y in points)


def blur(fid, sd):
    """Gaussian blur over the whole canvas, so thin shapes don't clip their glow."""
    return (f'<filter id="{fid}" filterUnits="userSpaceOnUse" x="0" y="0" width="{SIZE}" height="{SIZE}">'
            f'<feGaussianBlur stdDeviation="{sd}"/></filter>')


def document(defs, background, artwork):
    """Wrap artwork in the rounded-square body with a soft drop shadow. `background` is a fill value."""
    body = f'x="{BODY_X}" y="{BODY_Y}" width="{BODY_W}" height="{BODY_W}" rx="{RADIUS}"'
    return f'''<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {SIZE} {SIZE}" width="{SIZE}" height="{SIZE}">
<defs>
<clipPath id="body"><rect {body}/></clipPath>
<filter id="shadow" x="-20%" y="-20%" width="140%" height="140%">
<feDropShadow dx="0" dy="12" stdDeviation="14" flood-color="#000" flood-opacity="0.32"/></filter>
<linearGradient id="edge" x1="0" y1="0" x2="0" y2="1">
<stop offset="0" stop-color="#fff" stop-opacity="0.22"/><stop offset="0.5" stop-color="#fff" stop-opacity="0.04"/>
<stop offset="1" stop-color="#fff" stop-opacity="0.10"/></linearGradient>
{defs}
</defs>
<rect {body} fill="{background}" filter="url(#shadow)"/>
<g clip-path="url(#body)">
{artwork}
</g>
<rect x="{BODY_X + 1}" y="{BODY_Y + 1}" width="{BODY_W - 2}" height="{BODY_W - 2}" rx="{RADIUS - 1}" fill="none" stroke="url(#edge)" stroke-width="2"/>
</svg>
'''


def cubic(p0, p1, p2, p3, n=64):
    out = []
    for i in range(n + 1):
        t = i / n
        a, b, c, d = (1 - t) ** 3, 3 * (1 - t) ** 2 * t, 3 * (1 - t) * t * t, t ** 3
        out.append((a * p0[0] + b * p1[0] + c * p2[0] + d * p3[0], a * p0[1] + b * p1[1] + c * p2[1] + d * p3[1]))
    return out


def tapered(path, width, cap=24):
    """Polygon around `path` (tail first, head last); width(t) gives the half-width; round cap at the head."""
    left, right = [], []
    n = len(path) - 1
    for i, (x, y) in enumerate(path):
        ax, ay = path[max(i - 1, 0)]
        bx, by = path[min(i + 1, n)]
        dx, dy = bx - ax, by - ay
        ln = math.hypot(dx, dy) or 1
        nx, ny = -dy / ln, dx / ln
        w = width(i / n)
        left.append((x + nx * w, y + ny * w))
        right.append((x - nx * w, y - ny * w))
    # Semicircle cap around the head, from the left edge round to the right edge.
    hx, hy = path[-1]
    px, py = path[-2]
    ln = math.hypot(hx - px, hy - py)
    tx, ty = (hx - px) / ln, (hy - py) / ln
    nx, ny = -ty, tx
    w = width(1.0)
    head = [(hx + (nx * math.cos(a) + tx * math.sin(a)) * w, hy + (ny * math.cos(a) + ty * math.sin(a)) * w)
            for a in (math.pi * k / cap for k in range(1, cap))]
    return f'<polygon points="{pts(left + head + right[::-1])}"'


def streak_gradient(gid, tail, head, stops):
    """Linear gradient running from the streak's tail to its head. stops: (offset, colour, opacity)."""
    s = "".join(f'<stop offset="{o}" stop-color="{c}" stop-opacity="{a}"/>' for o, c, a in stops)
    return (f'<linearGradient id="{gid}" gradientUnits="userSpaceOnUse" x1="{f(tail[0])}" y1="{f(tail[1])}" '
            f'x2="{f(head[0])}" y2="{f(head[1])}">{s}</linearGradient>')


HOT = [(0, "#ff2a3a", 0), (0.35, "#ff4a2a", 0.55), (0.7, "#ff8a1e", 0.95), (0.9, "#ffd15a", 1), (1, "#fff6dc", 1)]
CORE = [(0, "#fff", 0), (0.6, "#ffe2a0", 0), (0.85, "#fff3d6", 0.85), (1, "#fff", 1)]


def streak(sid, path, half_width, glow=1.0, taper=1.3, soften=0):
    """A re-entry streak: wide blurred plasma, the hot body, and a thin white core. Returns (defs, svg).
    Lower `taper` keeps the streak fuller along its length; `glow` scales the plasma;
    `soften` blurs the body and core edges by that many px so the layers melt together."""
    tail, head = path[0], path[-1]
    defs = (streak_gradient(f"{sid}-hot", tail, head, HOT) + streak_gradient(f"{sid}-core", tail, head, CORE)
            + blur(f"{sid}-blur", 22 * glow))
    body = lambda t: half_width * t ** taper
    core = lambda t: half_width * 0.5 * t ** 2.2
    plasma = lambda t: half_width * (1 + 1.3 * glow) * t ** (taper * 0.85)
    soft = ""
    if soften:
        defs += blur(f"{sid}-edge", soften)
        soft = f' filter="url(#{sid}-edge)"'
    svg = (f'{tapered(path, plasma)} fill="url(#{sid}-hot)" opacity="{0.9 * glow}" filter="url(#{sid}-blur)"/>\n'
           f'{tapered(path, body)} fill="url(#{sid}-hot)"{soft}/>\n'
           f'{tapered(path, core)} fill="url(#{sid}-core)"{soft}/>')
    return defs, svg


def spark(sid, cx, cy, core=30, glow=150, flare=300, flare_w=7, tint="#ffc861"):
    """The white-hot star: soft glow, thin cross flare, hard white core. Returns (defs, svg)."""
    defs = (f'<radialGradient id="{sid}-glow">'
            f'<stop offset="0" stop-color="#fff" stop-opacity="1"/>'
            f'<stop offset="0.18" stop-color="#fff7e2" stop-opacity="0.95"/>'
            f'<stop offset="0.42" stop-color="{tint}" stop-opacity="0.45"/>'
            f'<stop offset="1" stop-color="{tint}" stop-opacity="0"/></radialGradient>'
            + blur(f"{sid}-soft", 5))
    h = [(cx - flare, cy), (cx, cy - flare_w), (cx + flare, cy), (cx, cy + flare_w)]
    v = [(cx, cy - flare), (cx + flare_w, cy), (cx, cy + flare), (cx - flare_w, cy)]
    flares = f'<polygon points="{pts(h)}"/><polygon points="{pts(v)}"/>'
    svg = (f'<circle cx="{f(cx)}" cy="{f(cy)}" r="{f(glow)}" fill="url(#{sid}-glow)"/>\n'
           f'<g fill="{tint}" opacity="0.9" filter="url(#{sid}-soft)">{flares}</g>\n'
           f'<g fill="#fff">{flares}</g>\n'
           f'<circle cx="{f(cx)}" cy="{f(cy)}" r="{f(core)}" fill="#fff"/>')
    return defs, svg


def write(path, svg):
    with open(path, "w") as fh:
        fh.write(svg)
