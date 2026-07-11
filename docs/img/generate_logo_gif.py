#!/usr/bin/env python3
"""Generates docs/img/logo.gif: a wave-reveal animation of the wolfie welcome
banner, parsed live from src/repl.rs so it can never drift from the real
WELCOME_BANNER/WELCOME_GRADIENT the CLI prints.

The reveal model (grid of cells, each with a dense/final character state,
timed by a diagonal sine-perturbed wavefront) is written to be trivially
portable to a future Rust terminal renderer -- see docs/superpowers if one
ever exists for that follow-up.
"""

import math
import re
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont, ImageFilter

REPO_ROOT = Path(__file__).resolve().parents[2]
REPL_RS = REPO_ROOT / "src" / "repl.rs"
FONT_PATH = "/home/tonya/.local/share/fonts/JetBrainsMonoNerd/JetBrainsMonoNerdFontMono-Regular.ttf"
OUT_PATH = Path(__file__).resolve().parent / "logo.gif"

FONT_SIZE = 20
CELL_W = 12
CELL_H = 24
PADDING = 28
DENSE_CHAR = "#"

NUM_FRAMES = 34
FRAME_MS = 40
WAVE_ANGLE_DEG = 32

# Wobble is the sum of a big lazy swell and a smaller choppy ripple riding on
# top of it, so the wavefront reads as a natural wave rather than a pure sine.
SWELL_AMPLITUDE = 4.2
SWELL_CYCLES = 1.4
RIPPLE_AMPLITUDE = 1.4
RIPPLE_CYCLES = 4.3
MAX_WOBBLE = SWELL_AMPLITUDE + RIPPLE_AMPLITUDE

SEAM_CHAR = "/"
SEAM_WIDTH = 1.6

# Idle animation played after the wave settles: a thin glint of SEAM_CHAR
# sweeps back and forth across the "Wolfie" word-art rows (border and the
# "Wolfram Friendly..." subtitle stay put), purely by swapping which
# character is drawn at each fixed cell -- no sub-cell offsets or color
# blending, so the same effect could be reproduced by a real tty repainting
# character cells.
ENABLE_IDLE_ANIMATION = False
IDLE_FRAMES_PER_CYCLE = 26
IDLE_FRAME_MS = 55
IDLE_CYCLES = 3
GLINT_HALF_WIDTH = 1.4
GLINT_WOBBLE_SCALE = 0.5
SETTLE_HOLD_MS = 2200


def parse_banner_and_gradient():
    src = REPL_RS.read_text()

    banner_match = re.search(r'const WELCOME_BANNER: &str = r#"\n(.*?)\n"#;', src, re.DOTALL)
    lines = banner_match.group(1).split("\n")

    gradient_match = re.search(
        r"const WELCOME_GRADIENT: \[nu_ansi_term::Rgb; 6\] = \[(.*?)\];", src, re.DOTALL
    )
    stops = [
        tuple(int(v) for v in m.groups())
        for m in re.finditer(r"Rgb::new\((\d+),\s*(\d+),\s*(\d+)\)", gradient_match.group(1))
    ]
    return lines, stops


def gradient_color(position, width, stops):
    span = max(width - 1, 1)
    scaled = position / span * (len(stops) - 1)
    lower = min(int(math.floor(scaled)), len(stops) - 2)
    upper = lower + 1
    mix = max(0.0, min(1.0, scaled - lower))
    r = stops[lower][0] + (stops[upper][0] - stops[lower][0]) * mix
    g = stops[lower][1] + (stops[upper][1] - stops[lower][1]) * mix
    b = stops[lower][2] + (stops[upper][2] - stops[lower][2]) * mix
    return (round(r), round(g), round(b))


def build_cells(lines, stops):
    """Returns list of (row, col, final_char, color, is_space) for every cell
    in the banner's bounding rectangle, including whitespace -- the dense
    start state fills the whole rectangle, not just the drawn glyphs."""
    cells = []
    line_count = len(lines)
    cols = max(len(line) for line in lines)
    for row, line in enumerate(lines):
        line_width = max(len(line), 1)
        gradient_width = line_width + line_count * 3
        for col in range(cols):
            ch = line[col] if col < len(line) else " "
            color = gradient_color(col + row * 10, gradient_width, stops)
            cells.append((row, col, ch, color, ch.isspace()))
    return cells


def wave_positions(cells):
    """Diagonal wavefront position u (sweep axis) and v (along-front axis, for
    the sine perturbation that makes the edge look like a natural wave).

    Coordinates are expressed in column-width units (row scaled by
    CELL_H/CELL_W) so WAVE_ANGLE_DEG maps to the actual on-screen angle --
    cells are much taller than they are wide, so working in raw row/col
    indices would visually skew the angle steeper than requested.
    """
    theta = math.radians(WAVE_ANGLE_DEG)
    cos_t, sin_t = math.cos(theta), math.sin(theta)
    row_scale = CELL_H / CELL_W
    positions = {}
    for row, col, *_rest in cells:
        x, y = col, row * row_scale
        u = x * cos_t + y * sin_t
        v = x * sin_t - y * cos_t
        positions[(row, col)] = (u, v)
    return positions


def make_background(width, height):
    base = Image.new("RGB", (width, height), (10, 8, 9))

    glow = Image.new("L", (width, height), 0)
    gdraw = ImageDraw.Draw(glow)
    cx, cy, r = int(width * 0.72), int(height * 0.35), int(max(width, height) * 0.55)
    gdraw.ellipse((cx - r, cy - r, cx + r, cy + r), fill=140)
    glow = glow.filter(ImageFilter.GaussianBlur(radius=r * 0.45))

    warm = Image.new("RGB", (width, height), (168, 58, 40))
    base = Image.composite(warm, base, glow.point(lambda p: int(p * 0.55)))
    base = base.filter(ImageFilter.GaussianBlur(radius=1))
    return base


def word_art_rows(lines):
    """Rows that make up the "Wolfie" word art itself, as opposed to the
    box-drawing border or the "Wolfram Friendly Interactive Shell" subtitle."""
    return {
        i
        for i, line in enumerate(lines)
        if 0 < i < len(lines) - 1
        and line.strip()
        and "Wolfram Friendly Interactive Shell" not in line
    }


def brighten(color, amount):
    return tuple(round(c + (255 - c) * amount) for c in color)


def wobble_at(v_norm):
    swell = SWELL_AMPLITUDE * math.sin(v_norm * SWELL_CYCLES * 2 * math.pi)
    ripple = RIPPLE_AMPLITUDE * math.sin(v_norm * RIPPLE_CYCLES * 2 * math.pi + 1.7)
    return swell + ripple


def render_frame(background, lines, cells, positions, threshold, u_min, u_max, v_min, v_span):
    img = background.copy()
    draw = ImageDraw.Draw(img)
    font = ImageFont.truetype(FONT_PATH, FONT_SIZE)

    for row, col, final_char, color, is_space in cells:
        u, v = positions[(row, col)]
        wobble = wobble_at((v - v_min) / v_span)
        edge = u + wobble
        distance = threshold - edge
        x = PADDING + col * CELL_W
        y = PADDING + row * CELL_H

        if distance >= SEAM_WIDTH:
            if not is_space:
                draw.text((x, y), final_char, font=font, fill=color)
        elif distance >= 0:
            draw.text((x, y), SEAM_CHAR, font=font, fill=brighten(color, 0.75))
        else:
            draw.text((x, y), DENSE_CHAR, font=font, fill=color)

    return img


def render_idle_frame(background, cells, positions, animated_rows, v_min, v_span, center):
    """Fully-revealed banner where a thin glint of SEAM_CHAR sweeps back and
    forth across the word-art rows -- a pure character swap at each cell's
    fixed grid position (same char, same color otherwise), so it's something
    a real tty could reproduce by just reprinting cells. Border and subtitle
    rows never change. center=None renders the plain settled frame."""
    img = background.copy()
    draw = ImageDraw.Draw(img)
    font = ImageFont.truetype(FONT_PATH, FONT_SIZE)

    for row, col, final_char, color, is_space in cells:
        if is_space:
            continue
        x = PADDING + col * CELL_W
        y = PADDING + row * CELL_H
        ch = final_char
        if center is not None and row in animated_rows:
            u, v = positions[(row, col)]
            wobble = wobble_at((v - v_min) / v_span) * GLINT_WOBBLE_SCALE
            if abs((u + wobble) - center) <= GLINT_HALF_WIDTH:
                ch = SEAM_CHAR
        draw.text((x, y), ch, font=font, fill=color)

    return img


def main():
    lines, stops = parse_banner_and_gradient()
    cells = build_cells(lines, stops)
    positions = wave_positions(cells)

    cols = max(len(line) for line in lines)
    rows = len(lines)
    width = PADDING * 2 + cols * CELL_W
    height = PADDING * 2 + rows * CELL_H

    background = make_background(width, height)

    us = [positions[(r, c)][0] for r, c, *_rest in cells]
    vs = [positions[(r, c)][1] for r, c, *_rest in cells]
    u_min, u_max = min(us), max(us)
    v_min, v_max = min(vs), max(vs)
    v_span = max(v_max - v_min, 1)

    # Margin sized to the wobble amplitude so the wave still fully clears
    # every crest/trough at both ends of the sweep.
    margin = MAX_WOBBLE + SEAM_WIDTH + 0.5
    sweep_start = u_min - margin
    sweep_end = u_max + margin

    frames = []
    durations = []
    for i in range(NUM_FRAMES):
        frac = i / (NUM_FRAMES - 1)
        threshold = sweep_start + (sweep_end - sweep_start) * frac
        frames.append(
            render_frame(background, lines, cells, positions, threshold, u_min, u_max, v_min, v_span)
        )
        durations.append(FRAME_MS)

    animated_rows = word_art_rows(lines)
    if ENABLE_IDLE_ANIMATION:
        u_mid = (u_min + u_max) / 2
        u_amp = (u_max - u_min) / 2 * 0.9
        total_idle_frames = IDLE_FRAMES_PER_CYCLE * IDLE_CYCLES
        for i in range(total_idle_frames):
            center = u_mid + u_amp * math.sin(2 * math.pi * i / IDLE_FRAMES_PER_CYCLE)
            frames.append(
                render_idle_frame(background, cells, positions, animated_rows, v_min, v_span, center)
            )
            durations.append(IDLE_FRAME_MS)

    frames.append(
        render_idle_frame(background, cells, positions, animated_rows, v_min, v_span, None)
    )
    durations.append(SETTLE_HOLD_MS)

    frames[0].save(
        OUT_PATH,
        save_all=True,
        append_images=frames[1:],
        duration=durations,
        disposal=2,
        optimize=False,
    )
    print(f"wrote {OUT_PATH} ({width}x{height}, {len(frames)} frames)")


if __name__ == "__main__":
    main()
