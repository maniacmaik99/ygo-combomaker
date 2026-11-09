#!/usr/bin/env python3
"""Generates the YGO Combomaker app icon (original artwork).

Motif: moonlit night. A large glowing golden crescent moon on a deep
violet-to-navy night sky (rounded square), a few small sparkling stars and
a plain, slightly tilted playing-card outline overlapping the moon.
Only moon, stars, sky and a simple card outline - no figures or symbols.
Colours match the app theme (violet accent, gold highlight).

Usage:  python3 assets/make_icon.py
Writes: assets/icon.png (256), assets/icon-512.png (512), assets/icon.ico
Requires: Pillow (pip install pillow)
"""

import os

from PIL import Image, ImageChops, ImageDraw, ImageFilter

HERE = os.path.dirname(os.path.abspath(__file__))

BASE = 512  # logical design size
SS = 4  # supersampling factor (draw at 4x, then downscale)
S = BASE * SS

SKY_TOP = (62, 40, 140)  # deep violet
SKY_BOTTOM = (10, 12, 38)  # navy
GOLD = (242, 184, 75)
GOLD_LIGHT = (255, 224, 150)
CARD_LINE = (186, 168, 255)  # light violet (theme accent, brightened)
CARD_FILL = (20, 18, 52)
STAR = (255, 240, 205)

RADIUS = 104  # corner radius of the rounded square
MARGIN = 16


def px(v):
    """Logical units (0..512) -> supersampled pixels."""
    return int(round(v * SS))


def new_layer():
    return Image.new("RGBA", (S, S), (0, 0, 0, 0))


def square_mask():
    m = Image.new("L", (S, S), 0)
    box = (px(MARGIN), px(MARGIN), S - px(MARGIN), S - px(MARGIN))
    ImageDraw.Draw(m).rounded_rectangle(box, radius=px(RADIUS), fill=255)
    return m


def clip(layer, mask):
    layer.putalpha(ImageChops.multiply(layer.getchannel("A"), mask))
    return layer


def sky(mask):
    grad = Image.new("RGBA", (1, S))
    for y in range(S):
        t = y / (S - 1)
        t = t * t * (3 - 2 * t)  # smoothstep for a softer transition
        c = tuple(int(SKY_TOP[i] + (SKY_BOTTOM[i] - SKY_TOP[i]) * t) for i in range(3))
        grad.putpixel((0, y), c + (255,))
    grad = grad.resize((S, S))
    img = new_layer()
    img.paste(grad, (0, 0), mask)
    return img


def moon_mask(cx, cy, r, cut_dx, cut_dy, cut_r):
    """Crescent = disc minus an offset disc."""
    m = Image.new("L", (S, S), 0)
    d = ImageDraw.Draw(m)
    d.ellipse((px(cx - r), px(cy - r), px(cx + r), px(cy + r)), fill=255)
    ox, oy = cx + cut_dx, cy + cut_dy
    d.ellipse((px(ox - cut_r), px(oy - cut_r), px(ox + cut_r), px(oy + cut_r)), fill=0)
    return m


def solid(color, mask):
    layer = Image.new("RGBA", (S, S), color + (255,))
    layer.putalpha(mask)
    return layer


def sparkle(d, cx, cy, size, color):
    """Four-pointed star (two thin diamonds)."""
    s, w = px(size), px(size * 0.22)
    x, y = px(cx), px(cy)
    d.polygon([(x, y - s), (x + w, y), (x, y + s), (x - w, y)], fill=color)
    d.polygon([(x - s, y), (x, y - w), (x + s, y), (x, y + w)], fill=color)


def card_outline():
    layer = new_layer()
    d = ImageDraw.Draw(layer)
    w, h = px(172), px(244)
    x0, y0 = (S - w) // 2, (S - h) // 2
    d.rounded_rectangle((x0, y0, x0 + w, y0 + h), radius=px(24),
                        fill=CARD_FILL + (150,), outline=CARD_LINE + (255,), width=px(18))
    layer = layer.rotate(-14, resample=Image.BICUBIC, center=(S / 2, S / 2))
    return ImageChops.offset(layer, px(78), px(62))


def build():
    mask = square_mask()
    img = sky(mask)

    # moon: large crescent, upper left
    mcx, mcy, mr = 212, 210, 140
    crescent = moon_mask(mcx, mcy, mr, 74, -46, 122)

    # soft wide halo following the crescent shape
    halo = solid(GOLD, crescent)
    halo.putalpha(halo.getchannel("A").point(lambda v: v * 110 // 255))
    halo = halo.filter(ImageFilter.GaussianBlur(px(36)))
    img = Image.alpha_composite(img, clip(halo, mask))

    glow = solid(GOLD, crescent).filter(ImageFilter.GaussianBlur(px(14)))
    img = Image.alpha_composite(img, clip(glow, mask))
    img = Image.alpha_composite(img, solid(GOLD, crescent))

    # lighter inner rim on the lit edge for a bit of depth
    inner = moon_mask(mcx - 10, mcy + 8, mr - 26, 84, -54, 124)
    rim = solid(GOLD_LIGHT, ImageChops.subtract(crescent, inner))
    rim = rim.filter(ImageFilter.GaussianBlur(px(3)))
    img = Image.alpha_composite(img, clip(rim, crescent))

    # stars
    stars = new_layer()
    sd = ImageDraw.Draw(stars)
    sparkle(sd, 392, 112, 30, STAR + (255,))
    sparkle(sd, 436, 214, 15, STAR + (230,))
    sparkle(sd, 108, 420, 18, STAR + (220,))
    r = 6
    sd.ellipse((px(330 - r), px(70 - r), px(330 + r), px(70 + r)), fill=STAR + (200,))
    star_glow = stars.filter(ImageFilter.GaussianBlur(px(8)))
    img = Image.alpha_composite(img, clip(star_glow, mask))
    img = Image.alpha_composite(img, clip(stars, mask))

    # card outline overlapping the moon (with a soft drop shadow)
    card = card_outline()
    shadow = new_layer()
    shadow.putalpha(card.getchannel("A").point(lambda v: v * 150 // 255))
    shadow = ImageChops.offset(shadow.filter(ImageFilter.GaussianBlur(px(14))), 0, px(10))
    img = Image.alpha_composite(img, clip(shadow, mask))
    img = Image.alpha_composite(img, clip(card, mask))

    # thin violet rim around the square
    edge = new_layer()
    box = (px(MARGIN), px(MARGIN), S - px(MARGIN), S - px(MARGIN))
    ImageDraw.Draw(edge).rounded_rectangle(
        box, radius=px(RADIUS), outline=(124, 92, 255, 110), width=px(4)
    )
    return Image.alpha_composite(img, edge)


def main():
    big = build()
    big.resize((512, 512), Image.LANCZOS).save(os.path.join(HERE, "icon-512.png"))
    out256 = big.resize((256, 256), Image.LANCZOS)
    out256.save(os.path.join(HERE, "icon.png"))
    sizes = [16, 32, 48, 64, 128, 256]
    out256.save(os.path.join(HERE, "icon.ico"), sizes=[(s, s) for s in sizes])
    print("wrote icon.png, icon-512.png, icon.ico")


if __name__ == "__main__":
    main()
