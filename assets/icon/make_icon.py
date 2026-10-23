"""Drop Pod's icon: a white-hot spark trailing a re-entry streak arcs down toward a planet's glowing blue limb.

Variant A1b ("A1 polished") of the icon rounds, the one picked: writes droppod-icon.svg next to this file.
`../../scripts/make_icon.sh` renders it to droppod-icon-1024.png and Droppod.icns. Original geometry drawn in code:
no fonts, game art or traced shapes.
"""
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "tools"))
from iconkit import cubic, document, spark, streak, write  # noqa: E402

STARS = [(300, 590, 3), (760, 240, 3.5), (850, 400, 2.5), (640, 170, 2), (210, 400, 2)]

# A1 polished: softer streak edges, a tighter glow, the spark's light caught on the limb below it.
A1B = dict(planet=(790, 1250, 720), path=((60, 130), (300, 165), (470, 280), (560, 455)),
           width=44, flare=195, flash=False, dawn=False,
           soften=3.5, plasma=0.85, core=34, glow=145, cast=0.4)


def scene(planet, path, width, flare, flash, dawn, stars=STARS,
          soften=0, plasma=1.0, core=32, glow=165, cast=0):
    cx, cy, r = planet
    trail = cubic(*path)
    hx, hy = trail[-1]
    s_defs, s_svg = streak("trail", trail, width, glow=plasma, soften=soften)
    p_defs, p_svg = spark("star", hx, hy, core=core, glow=glow, flare=flare, flare_w=7)

    # Where the trajectory meets the limb (straight down from the spark), for the atmosphere flash.
    ly = cy - (r * r - (hx - cx) ** 2) ** 0.5
    rim_hi = "#c8eeff"
    # Dawn breaks where the limb leaves the right edge of the body.
    dx = 960
    dy = cy - (r * r - (dx - cx) ** 2) ** 0.5
    defs = f'''
<linearGradient id="sky" x1="0" y1="0" x2="0.4" y2="1">
<stop offset="0" stop-color="#060a1c"/><stop offset="0.6" stop-color="#0e1640"/>
<stop offset="1" stop-color="{"#2a2a6e" if dawn else "#1a2a6a"}"/></linearGradient>
<radialGradient id="planet" cx="{cx}" cy="{cy}" r="{r}" gradientUnits="userSpaceOnUse">
<stop offset="0.80" stop-color="#060b22"/><stop offset="0.95" stop-color="#122459"/>
<stop offset="0.982" stop-color="#2f6fd0"/><stop offset="0.995" stop-color="#7cc8ff"/>
<stop offset="1" stop-color="{rim_hi}"/></radialGradient>
<radialGradient id="haze" cx="{cx}" cy="{cy}" r="{r + 110}" gradientUnits="userSpaceOnUse">
<stop offset="0.86" stop-color="#3d8bff" stop-opacity="0"/><stop offset="{r / (r + 110):.3f}" stop-color="#58a6ff" stop-opacity="0.55"/>
<stop offset="1" stop-color="#4a9bff" stop-opacity="0"/></radialGradient>
<radialGradient id="dawn" cx="{dx}" cy="{dy}" r="560" gradientUnits="userSpaceOnUse">
<stop offset="0" stop-color="#fff0c8" stop-opacity="1"/><stop offset="0.3" stop-color="#ffaa5a" stop-opacity="0.85"/>
<stop offset="0.7" stop-color="#ff7a4a" stop-opacity="0.25"/><stop offset="1" stop-color="#ff6a3a" stop-opacity="0"/></radialGradient>
<radialGradient id="flash" cx="{hx}" cy="{ly}" r="230" gradientUnits="userSpaceOnUse">
<stop offset="0" stop-color="#fff4dc" stop-opacity="1"/><stop offset="0.35" stop-color="#ffb066" stop-opacity="0.75"/>
<stop offset="1" stop-color="#ff7a3a" stop-opacity="0"/></radialGradient>
<filter id="limb-blur" filterUnits="userSpaceOnUse" x="0" y="0" width="1024" height="1024"><feGaussianBlur stdDeviation="16"/></filter>
{s_defs}{p_defs}
'''
    star_svg = "".join(f'<circle cx="{x}" cy="{y}" r="{rr}" fill="#cfe0ff" opacity="0.7"/>' for x, y, rr in stars)
    rim = f'<circle cx="{cx}" cy="{cy}" r="{r - 3}" fill="none" stroke="{rim_hi}" stroke-width="6" opacity="0.8"/>'

    def limb_light(paint):
        """Warm light running along the limb only: a blurred band of air plus a hot rim line, faded by `paint`."""
        return (f'<circle cx="{cx}" cy="{cy}" r="{r + 14}" fill="none" stroke="url(#{paint})" stroke-width="56" '
                f'filter="url(#limb-blur)"/>'
                f'<circle cx="{cx}" cy="{cy}" r="{r - 3}" fill="none" stroke="url(#{paint})" stroke-width="8"/>')

    dawn_svg = limb_light("dawn") if dawn else ""
    flash_svg = limb_light("flash") if flash else ""
    if cast:  # a faint version of the flash: the spark's light caught on the air below it
        flash_svg = f'<g opacity="{cast}">{limb_light("flash")}</g>'
    art = f'''
<rect x="0" y="0" width="1024" height="1024" fill="url(#sky)"/>
{star_svg}
<circle cx="{cx}" cy="{cy}" r="{r + 110}" fill="url(#haze)"/>
<circle cx="{cx}" cy="{cy}" r="{r}" fill="url(#planet)"/>
{rim}
{dawn_svg}
{flash_svg}
{s_svg}
{p_svg}
'''
    return document(defs, "#0b1236", art)


if __name__ == "__main__":
    write(os.path.join(HERE, "droppod-icon.svg"), scene(**A1B))
