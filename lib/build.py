#!/usr/bin/python3
"""Omarchy Kyber cursor theme.

Builds two Xcursor themes in one run:
  Omarchy-Kyber          idle: dark faceted spearhead + hilt, accent edge light
  Omarchy-Kyber-Ignited  held: edge becomes a beam, crossguard beams appear

The pointer is a solid dark arrowhead (the classic mouse-pointer silhouette)
with a short cylindrical hilt behind it. Color lives only in the luminous
parts — the edge strip, the hilt strip, the crossguard beams — which are
also rendered on their own, Gaussian-blurred, and composited underneath as
real bloom rather than faked with thick strokes.
"""
import argparse, io, json, math, multiprocessing, os, re, struct, tomllib
from concurrent.futures import ProcessPoolExecutor
from pathlib import Path
from PIL import Image, ImageFilter
import cairosvg

HERE = Path(__file__).resolve().parent
THEME = 'Omarchy-Kyber'
SIZES = (24, 32, 48, 64)
# Bitmaps are rendered this much larger than their nominal Xcursor size slot,
# so the detail stays readable without touching the system cursor size.
SCALE = 1.6
VIEW = (3.0, 0.0, 48.0, 48.0)          # viewBox actually rendered: x, y, w, h
# None = ignite in the Omarchy accent. Set to e.g. '#ff2a2a' for a fixed red.
IGNITE_COLOR = None
NO_COLOR = '#ff4d5a'                   # not-allowed stays red whatever the accent

# ---------------------------------------------------------------- geometry
T = (8.0, 5.0)                         # pointer tip = hotspot
ANG = math.radians(53.4)               # pointer axis, tip -> pommel
U = (math.cos(ANG), math.sin(ANG))
N = (U[1], -U[0])                      # perpendicular, towards the upper-right corner
DEG = math.degrees(ANG)
HEAD_LEN, HEAD_HALF = 25.0, 10.0

def add(a, b, k=1.0):
    return (a[0] + b[0] * k, a[1] + b[1] * k)

def P(p):
    return f'{p[0]:.2f},{p[1]:.2f}'

M = add(T, U, HEAD_LEN)                # centre of the head's back face
L = add(M, N, -HEAD_HALF)              # lower-left corner: the lit edge ends here
R = add(M, N, HEAD_HALF)
MI = add(T, U, HEAD_LEN * 0.80)        # ridge end, a little inside the back face
SWEEP = math.radians(18)               # crossguard beams sweep back towards the pommel
BEAM_R = add((N[0] * math.cos(SWEEP), N[1] * math.cos(SWEEP)), U, math.sin(SWEEP))
BEAM_L = add((-N[0] * math.cos(SWEEP), -N[1] * math.cos(SWEEP)), U, math.sin(SWEEP))
EMIT = add(M, U, 1.7)
EMIT_R, EMIT_L = add(EMIT, N, 5.6), add(EMIT, N, -5.6)
BEAM_LEN = 8.5
STUB = 2.8                             # emitter stub length; beams start at its tip
BADGE = (41.0, 29.0)                   # badge centre, clear of the hilt and beams
CENTER = (VIEW[0] + VIEW[2] / 2, VIEW[1] + VIEW[3] / 2)

# ------------------------------------------------------------------ colour
def _rgb(h):
    return [int(h[i:i + 2], 16) for i in (1, 3, 5)]

def lighten(h, k):
    return '#%02x%02x%02x' % tuple(round(c + (255 - c) * k) for c in _rgb(h))

def darken(h, k):
    return '#%02x%02x%02x' % tuple(round(c * (1 - k)) for c in _rgb(h))

def luminance(h):
    """Relative luminance (WCAG): how bright a colour looks, 0..1."""
    def lin(c):
        c /= 255
        return c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4
    r, g, b = (lin(c) for c in _rgb(h))
    return 0.2126 * r + 0.7152 * g + 0.0722 * b

# Accents darker than this get their glow lifted so every theme glows about
# equally; a mid green sits right around it and is left untouched.
# Haze strength now that blurs no longer carry a dark fringe (it used to dim the haze).
DARK_K = 0.40
GLOW_LUMA_TARGET = 0.40

def current_accent():
    for p in (Path.home() / '.local/state/omarchy/current/theme/colors.toml',
              Path.home() / '.config/omarchy/current/theme/colors.toml'):
        if p.is_file():
            try:
                v = tomllib.loads(p.read_text()).get('accent', '')
                if re.fullmatch(r'#[0-9a-fA-F]{6}', v):
                    return v.lower()
            except Exception:
                pass
    return '#e8ecf2'

# ------------------------------------------------------------- primitives
def line(a, b, color, width, extra=''):
    return (f'<line x1="{a[0]:.2f}" y1="{a[1]:.2f}" x2="{b[0]:.2f}" y2="{b[1]:.2f}" '
            f'stroke="{color}" stroke-width="{width}" stroke-linecap="round"{extra}/>')

def edge_line(inset, start, end):
    """Segment parallel to the tip->L edge, pulled `inset` units into the head."""
    d = (L[0] - T[0], L[1] - T[1]); ln = math.hypot(*d); d = (d[0] / ln, d[1] / ln)
    inward = (-d[1], d[0])
    if inward[0] * (M[0] - T[0]) + inward[1] * (M[1] - T[1]) < 0:
        inward = (-inward[0], -inward[1])
    return add(add(T, d, start * ln), inward, inset), add(add(T, d, end * ln), inward, inset)

SHARED_DEFS = '''
  <linearGradient id="cyl" x1="0" y1="0" x2="0" y2="1">
    <stop offset="0" stop-color="#0d0e11"/><stop offset="0.35" stop-color="#3b414b"/>
    <stop offset="0.55" stop-color="#1c1f24"/><stop offset="1" stop-color="#08090b"/></linearGradient>
  <linearGradient id="silver" x1="0" y1="0" x2="0" y2="1">
    <stop offset="0" stop-color="#6c7380"/><stop offset="0.4" stop-color="#eef1f5"/><stop offset="1" stop-color="#5a606b"/></linearGradient>
  <linearGradient id="metal" x1="0" y1="0" x2="1" y2="1">
    <stop offset="0" stop-color="#3a3f48"/><stop offset="1" stop-color="#101216"/></linearGradient>'''

def lit_beam(a, b, glow):
    # white-hot core, then a pale tint, then the full colour, so the beam
    # grades into the bloom instead of ending at a hard coloured edge
    return (line(a, b, glow, 2.4, ' opacity="0.9"') + line(a, b, lighten(glow, 0.45), 1.5)
            + line(a, b, lighten(glow, 0.8), 1.0) + line(a, b, '#ffffff', 0.55))

# ----------------------------------------------------------------- pointer
def pointer(accent, glow, lit):
    """Returns (body_svg, glow_svg) for the spearhead pointer."""
    a, b = edge_line(1.1, 0.03, 0.80) if lit else edge_line(1.1, 0.14, 0.72)
    edge = glow if lit else accent
    port = glow if lit else '#3a3f48'
    hilt_strip = lighten(glow, 0.15) if lit else accent
    # Ignited: the blade's light spills onto the faces, strongest beside the lit edge.
    fl0, fl1 = (lighten(darken(glow, 0.55), 0.10), darken(glow, 0.80)) if lit else ('#4a505b', '#1d2026')
    fr0, fr1 = (darken(glow, 0.72), '#0c0d10') if lit else ('#23262c', '#0c0d10')
    defs = f'''
  <linearGradient id="faceL" gradientUnits="userSpaceOnUse" x1="{T[0]:.2f}" y1="{T[1]:.2f}" x2="{L[0]:.2f}" y2="{L[1]:.2f}">
    <stop offset="0" stop-color="{fl0}"/><stop offset="1" stop-color="{fl1}"/></linearGradient>
  <linearGradient id="faceR" gradientUnits="userSpaceOnUse" x1="{T[0]:.2f}" y1="{T[1]:.2f}" x2="{R[0]:.2f}" y2="{R[1]:.2f}">
    <stop offset="0" stop-color="{fr0}"/><stop offset="1" stop-color="{fr1}"/></linearGradient>'''
    # Crossguard emitter stubs: always present hardware, so the side beams
    # come out of something instead of out of nowhere.
    stubs, beams = '', ''
    for e, d in ((EMIT_R, BEAM_R), (EMIT_L, BEAM_L)):
        s0, tip = add(e, d, -0.6), add(e, d, STUB)
        stubs += (line(s0, tip, '#4a505a', 3.0) + line(s0, tip, '#16181c', 2.3)
                  + line(add(tip, d, -0.5), tip, '#c9ced6' if lit else '#5d6470', 2.6))
        if lit:
            end = add(tip, d, BEAM_LEN)
            beams += lit_beam(tip, end, glow)
    hilt = f'''<g transform="translate({P(M)}) rotate({DEG:.2f})">
  <rect x="-0.4" y="-5.2" width="3.6" height="10.4" rx="1.0" fill="url(#cyl)" stroke="#4a505a" stroke-width="0.35"/>
  <rect x="0.5" y="-5.9" width="1.8" height="1.3" rx="0.45" fill="{port}"/>
  <rect x="0.5" y="4.6" width="1.8" height="1.3" rx="0.45" fill="{port}"/>
  <rect x="3.2" y="-5.5" width="2.2" height="11.0" rx="0.7" fill="url(#silver)" stroke="#3a3f48" stroke-width="0.3"/>
  <rect x="5.4" y="-4.5" width="11.0" height="9.0" rx="1.8" fill="url(#cyl)" stroke="#4a505a" stroke-width="0.35"/>
  <rect x="7.0" y="-3.0" width="8.0" height="1.5" rx="0.75" fill="{hilt_strip}"/>
  {'<rect x="7.4" y="-2.65" width="7.2" height="0.7" rx="0.35" fill="#ffffff"/>' if lit else ''}
  <rect x="16.4" y="-3.7" width="2.6" height="7.4" rx="1.1" fill="url(#cyl)" stroke="#4a505a" stroke-width="0.35"/>
  <rect x="18.6" y="-2.9" width="1.4" height="5.8" rx="0.7" fill="url(#silver)"/>
  {f'<circle cx="20.05" cy="0" r="1.1" fill="{glow}"/>' if lit else ''}
</g>'''
    head = f'''
  <polygon points="{P(T)} {P(L)} {P(MI)}" fill="url(#faceL)"/>
  <polygon points="{P(T)} {P(MI)} {P(R)}" fill="url(#faceR)"/>
  <polygon points="{P(L)} {P(MI)} {P(R)} {P(M)}" fill="#0b0c0f"/>
  <polyline points="{P(L)} {P(T)} {P(R)}" fill="none" stroke="#b8c0cc" stroke-width="0.55" stroke-linejoin="round" stroke-linecap="round"/>
  {line(L, R, '#2c3038', 0.5)}
  {line(T, MI, '#5d6470', 0.35)}
  {lit_beam(a, b, glow) if lit else line(a, b, edge, 1.3) + line(a, b, '#ffffff', 0.45, ' opacity="0.85"')}'''
    body = f'<defs>{defs}</defs>' + beams + hilt + stubs + head

    ga, gb = edge_line(1.1, 0.01, 0.82) if lit else edge_line(1.1, 0.11, 0.75)
    if not lit:
        hs, he = add(add(M, U, 7.0), N, 2.25), add(add(M, U, 15.0), N, 2.25)
        return body, line(ga, gb, edge, 2.2) + line(hs, he, hilt_strip, 1.8)
    # Ignited: the spearhead and the crossguard beams radiate; the handle
    # does not — it reads as the dark thing holding the light.
    halo = line(ga, gb, glow, 6.5)
    for e, d in ((EMIT_R, BEAM_R), (EMIT_L, BEAM_L)):
        tip = add(e, d, STUB)
        halo += line(tip, add(tip, d, BEAM_LEN), glow, 6.5)
    halo += f'<polygon points="{P(T)} {P(L)} {P(M)} {P(R)}" fill="{glow}" stroke="{glow}" stroke-width="2.5" stroke-linejoin="round"/>'
    return body, halo

# ------------------------------------------------------------------ badges
def disc(r=6.2, fill='#121418', rim='#b8c0cc'):
    return f'<circle cx="{BADGE[0]}" cy="{BADGE[1]}" r="{r}" fill="{fill}" stroke="{rim}" stroke-width="0.8"/>'

def at_badge(inner):
    return f'<g transform="translate({BADGE[0]},{BADGE[1]})">{inner}</g>'

def badge(kind, accent, glow, lit):
    hot = glow if lit else accent
    if kind == 'link':
        chain = ('<g transform="rotate(-45)" fill="none" stroke-width="2.6" stroke-linecap="round">'
                 '<rect x="-6.6" y="-2.4" width="7.6" height="4.8" rx="2.4" stroke="#121418"/>'
                 '<rect x="-1.0" y="-2.4" width="7.6" height="4.8" rx="2.4" stroke="#121418"/>'
                 '<rect x="-6.6" y="-2.4" width="7.6" height="4.8" rx="2.4" stroke="url(#silver)" stroke-width="1.4"/>'
                 '<rect x="-1.0" y="-2.4" width="7.6" height="4.8" rx="2.4" stroke="#eef1f5" stroke-width="1.4"/></g>')
        return at_badge(chain), ''
    if kind == 'help':
        q = ('<path d="M-2.4,-2.4 C-2.4,-5.0 2.6,-5.0 2.6,-2.2 C2.6,-0.2 0.1,0.0 0.1,1.8" fill="none" '
             'stroke="#ffffff" stroke-width="1.5" stroke-linecap="round"/><circle cx="0.1" cy="3.8" r="0.95" fill="#ffffff"/>')
        return disc() + at_badge(q), ''
    if kind == 'not-allowed':
        mark = (f'<circle r="5.0" fill="none" stroke="{NO_COLOR}" stroke-width="1.8"/>'
                f'<line x1="-3.4" y1="-3.4" x2="3.4" y2="3.4" stroke="{NO_COLOR}" stroke-width="1.8" stroke-linecap="round"/>')
        halo = (f'<circle cx="{BADGE[0]}" cy="{BADGE[1]}" r="5.0" fill="none" stroke="{NO_COLOR}" stroke-width="2.5"/>')
        return disc(fill='#121418', rim='#5d6470') + at_badge(mark), halo
    if kind in ('zoom-in', 'zoom-out', 'copy'):
        bar = '<line x1="-3.4" y1="0" x2="3.4" y2="0" stroke="#ffffff" stroke-width="1.6" stroke-linecap="round"/>'
        if kind != 'zoom-out':
            bar += '<line x1="0" y1="-3.4" x2="0" y2="3.4" stroke="#ffffff" stroke-width="1.6" stroke-linecap="round"/>'
        shell = disc() if kind != 'copy' else (
            f'<rect x="{BADGE[0] - 5.6}" y="{BADGE[1] - 5.6}" width="11.2" height="11.2" rx="2.4" '
            'fill="#121418" stroke="#b8c0cc" stroke-width="0.8"/>')
        return shell + at_badge(bar), ''
    if kind == 'alias':
        arrow = ('<path d="M-3.2,2.8 C-3.2,-1.4 -0.6,-2.6 2.6,-2.6" fill="none" stroke="#ffffff" stroke-width="1.5" stroke-linecap="round"/>'
                 '<path d="M0.6,-4.8 L3.4,-2.6 L0.6,-0.4" fill="none" stroke="#ffffff" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>')
        return disc() + at_badge(arrow), ''
    if kind == 'progress':
        glass = ('<path d="M-3.6,-5.6 H3.6 M-3.6,5.6 H3.6" stroke="url(#silver)" stroke-width="1.6" stroke-linecap="round"/>'
                 '<path d="M-2.8,-5.0 C-2.8,-1.2 2.8,1.2 2.8,5.0 M2.8,-5.0 C2.8,-1.2 -2.8,1.2 -2.8,5.0" fill="none" stroke="#d6dbe3" stroke-width="1.0"/>'
                 f'<path d="M-1.6,3.6 L1.6,3.6 L0,1.2 Z" fill="{hot}"/><path d="M-1.4,-3.4 L1.4,-3.4 L0,-1.6 Z" fill="{hot}"/>')
        return at_badge(glass), f'<circle cx="{BADGE[0]}" cy="{BADGE[1]}" r="2.4" fill="{hot}"/>'
    if kind == 'context':
        card = (f'<rect x="{BADGE[0] - 5.0}" y="{BADGE[1] - 6.2}" width="10" height="12.4" rx="1.6" fill="#121418" stroke="#b8c0cc" stroke-width="0.8"/>'
                + at_badge('<path d="M-2.8,-2.8 H2.8 M-2.8,0 H2.8 M-2.8,2.8 H1.2" stroke="#eef1f5" stroke-width="1.1" stroke-linecap="round"/>'))
        return card, ''
    return '', ''

# ------------------------------------------------------------------ glyphs
# Non-pointer cursors share the hilt language: white metal arrows/bars around
# a dark hilt segment with silver rings and a lit accent strip.
OUTLINE = '#0b0c0f'

def hilt_segment(length, half, hot, lit):
    x0 = -length / 2
    core = f'<rect x="{x0 + 1.4:.2f}" y="-0.65" width="{length - 2.8:.2f}" height="1.3" rx="0.65" fill="{hot}"/>'
    if lit:
        core += f'<rect x="{x0 + 1.8:.2f}" y="-0.3" width="{length - 3.6:.2f}" height="0.6" rx="0.3" fill="#ffffff"/>'
    return (f'<rect x="{x0:.2f}" y="{-half}" width="{length}" height="{2 * half}" rx="1.2" fill="url(#cyl)" stroke="#4a505a" stroke-width="0.35"/>'
            f'<rect x="{x0 - 0.9:.2f}" y="{-half - 0.5}" width="1.4" height="{2 * half + 1}" rx="0.5" fill="url(#silver)"/>'
            f'<rect x="{-x0 - 0.5:.2f}" y="{-half - 0.5}" width="1.4" height="{2 * half + 1}" rx="0.5" fill="url(#silver)"/>'
            + core)

def arrowhead(x, direction):
    """White arrowhead pointing along +x (direction 1) or -x (-1), tip at x."""
    b = x - direction * 6.5
    return f'<polygon points="{x},0 {b},-5.2 {b},5.2" fill="#f2f4f7" stroke="{OUTLINE}" stroke-width="1.0" stroke-linejoin="round"/>'

def double_arrow(angle, hot, lit):
    shaft = (f'<line x1="-14" y1="0" x2="14" y2="0" stroke="{OUTLINE}" stroke-width="3.4"/>'
             '<line x1="-14" y1="0" x2="14" y2="0" stroke="#f2f4f7" stroke-width="1.6"/>')
    inner = shaft + arrowhead(19, 1) + arrowhead(-19, -1) + hilt_segment(10, 2.6, hot, lit)
    body = f'<g transform="translate({CENTER[0]},{CENTER[1]}) rotate({angle})">{inner}</g>'
    halo = f'<g transform="translate({CENTER[0]},{CENTER[1]}) rotate({angle})">{line((-3.6, 0), (3.6, 0), hot, 3.0 if lit else 1.8)}</g>'
    return body, halo

def move(hot, lit):
    inner = ''
    for ang in (0, 90, 180, 270):
        inner += (f'<g transform="rotate({ang})"><line x1="4" y1="0" x2="15" y2="0" stroke="{OUTLINE}" stroke-width="3.4"/>'
                  f'<line x1="4" y1="0" x2="15" y2="0" stroke="#f2f4f7" stroke-width="1.6"/>{arrowhead(20, 1)}</g>')
    inner += (f'<circle r="4.6" fill="url(#metal)" stroke="#b8c0cc" stroke-width="0.7"/>'
              f'<circle r="2.4" fill="none" stroke="{hot}" stroke-width="1.3"/>'
              + ('<circle r="1.0" fill="#ffffff"/>' if lit else ''))
    body = f'<g transform="translate({CENTER[0]},{CENTER[1]})">{inner}</g>'
    halo = f'<circle cx="{CENTER[0]}" cy="{CENTER[1]}" r="2.6" fill="none" stroke="{hot}" stroke-width="{2.8 if lit else 1.6}"/>'
    return body, halo

def text_beam(hot, lit):
    path = 'M-4.5,-16 C-2,-16 0,-15 0,-12.5 C0,-15 2,-16 4.5,-16 M0,-12.5 V12.5 M-4.5,16 C-2,16 0,15 0,12.5 C0,15 2,16 4.5,16'
    inner = (f'<path d="{path}" fill="none" stroke="{OUTLINE}" stroke-width="3.2" stroke-linecap="round"/>'
             f'<path d="{path}" fill="none" stroke="#f2f4f7" stroke-width="1.5" stroke-linecap="round"/>'
             f'<g transform="rotate(90)">{hilt_segment(7, 2.2, hot, lit)}</g>')
    body = f'<g transform="translate({CENTER[0]},{CENTER[1]})">{inner}</g>'
    halo = line((CENTER[0], CENTER[1] - 2.4), (CENTER[0], CENTER[1] + 2.4), hot, 2.8 if lit else 1.6)
    return body, halo

def crosshair(hot, lit, cell=False):
    if cell:
        inner = ('<path d="M-2.6,-14 H2.6 V-2.6 H14 V2.6 H2.6 V14 H-2.6 V2.6 H-14 V-2.6 H-2.6 Z" '
                 f'fill="#f2f4f7" stroke="{OUTLINE}" stroke-width="1.0" stroke-linejoin="round"/>'
                 f'<rect x="-1.3" y="-1.3" width="2.6" height="2.6" fill="{hot}"/>')
    else:
        arms = 'M0,-17 V-5 M0,5 V17 M-17,0 H-5 M5,0 H17'
        inner = (f'<path d="{arms}" stroke="{OUTLINE}" stroke-width="3.0" stroke-linecap="round"/>'
                 f'<path d="{arms}" stroke="#f2f4f7" stroke-width="1.3" stroke-linecap="round"/>'
                 f'<circle r="3.0" fill="none" stroke="{hot}" stroke-width="1.3"/><circle r="0.8" fill="#ffffff"/>')
    body = f'<g transform="translate({CENTER[0]},{CENTER[1]})">{inner}</g>'
    halo = f'<circle cx="{CENTER[0]}" cy="{CENTER[1]}" r="3.0" fill="none" stroke="{hot}" stroke-width="{2.6 if lit else 1.4}"/>'
    return body, halo

def busy(accent, glow, lit):
    hot = glow if lit else accent
    ring = (f'<circle cx="{CENTER[0]}" cy="{CENTER[1]}" r="15" fill="none" stroke="{OUTLINE}" stroke-width="4.2" stroke-dasharray="9 4.7"/>'
            f'<circle cx="{CENTER[0]}" cy="{CENTER[1]}" r="15" fill="none" stroke="{hot}" stroke-width="2.6" stroke-dasharray="9 4.7"/>')
    # a shrunken pointer sits inside the ring, its tip a little above centre
    pb, ph = pointer(accent, glow, lit)
    s = 0.42
    shrink = f'translate({CENTER[0] - (T[0] + 13) * s:.2f},{CENTER[1] - (T[1] + 15) * s:.2f}) scale({s})'
    body = ring + f'<g transform="{shrink}">{pb}</g>'
    halo = (f'<circle cx="{CENTER[0]}" cy="{CENTER[1]}" r="15" fill="none" stroke="{hot}" stroke-width="{4.2 if lit else 3.0}" stroke-dasharray="9 4.7"/>'
            f'<g transform="{shrink}">{ph}</g>')
    return body, halo

GRAB = ('M16 27 V17 Q16 13 20 13 Q23 13 23 17 V11 Q23 7 27 7 Q31 7 31 11 V17 V14 Q31 10 35 10 Q39 10 39 14 V18 '
        'Q39 14 43 15 Q46 15 46 19 V32 Q46 44 32 44 H27 Q20 44 16 37 L10 29 Q8 25 11 23 Q14 21 16 27 Z')
GRABBING = ('M12 23 V17 Q12 13 16 13 Q19 13 19 17 V12 Q19 8 23 8 Q27 8 27 12 V16 V11 Q27 7 31 7 Q35 7 35 11 V16 '
            'Q35 12 39 12 Q43 12 43 16 V30 Q43 43 30 43 H23 Q12 43 12 32 Z')

def hand(closed, accent, glow, lit):
    path = GRABBING if closed else GRAB
    knuckles = ('M19 17 V24 M27 16 V24 M35 16 V24' if closed else 'M23 17 V25 M31 17 V25 M39 18 V25')
    body = (f'<path d="{path}" fill="url(#metal)" stroke="#b8c0cc" stroke-width="1.0" stroke-linejoin="round"/>'
            f'<path d="{knuckles}" stroke="#05060a" stroke-width="0.8" stroke-linecap="round"/>')
    halo = ''
    if closed:
        # gripping a lit saber: a beam runs behind the fist towards the corner
        beam_c = glow if lit else accent
        a, b = (6.0, 3.0), (24.0, 26.0)
        body = line(a, b, beam_c, 2.4) + line(a, b, '#ffffff', 1.0) + body
        halo = line(a, b, beam_c, 4.6)
    return body, halo

# --------------------------------------------------------------- the set
SHAPES = {
    # name: (builder, hotspot in viewBox units)
    'default':     ('pointer', T), 'pointer': ('pointer:link', T), 'help': ('pointer:help', T),
    'progress':    ('pointer:progress', T), 'not-allowed': ('pointer:not-allowed', T),
    'copy':        ('pointer:copy', T), 'alias': ('pointer:alias', T), 'context': ('pointer:context', T),
    'zoom-in':     ('pointer:zoom-in', T), 'zoom-out': ('pointer:zoom-out', T),
    'wait':        ('busy', CENTER), 'text': ('text', CENTER),
    'crosshair':   ('crosshair', CENTER), 'cell': ('cell', CENTER), 'move': ('move', CENTER),
    'ns-resize':   ('resize:90', CENTER), 'ew-resize': ('resize:0', CENTER),
    'nwse-resize': ('resize:45', CENTER), 'nesw-resize': ('resize:-45', CENTER),
    'grab':        ('hand:open', (30.0, 11.0)), 'grabbing': ('hand:closed', (27.0, 15.0)),
}
ALIASES = {
    'default': ['left_ptr', 'default', 'top_left_arrow', 'arrow'],
    'pointer': ['hand2', 'pointer', 'link', 'hand1'],
    'help': ['question_arrow', 'help', 'whats_this'],
    'progress': ['left_ptr_watch', 'progress', 'half-busy'],
    'not-allowed': ['crossed_circle', 'not-allowed', 'no-drop', 'forbidden'],
    'copy': ['copy', 'dnd-copy'], 'alias': ['alias', 'dnd-link'], 'context': ['context-menu'],
    'zoom-in': ['zoom-in'], 'zoom-out': ['zoom-out'],
    'wait': ['watch', 'wait'],
    'text': ['xterm', 'text', 'ibeam', 'vertical-text'],
    'crosshair': ['crosshair', 'cross', 'tcross'], 'cell': ['cell', 'plus'],
    'move': ['fleur', 'move', 'all-scroll', 'size_all', 'dnd-move'],
    'ns-resize': ['sb_v_double_arrow', 'ns-resize', 'size_ver', 'n-resize', 's-resize', 'top_side', 'bottom_side', 'row-resize', 'v_double_arrow'],
    'ew-resize': ['sb_h_double_arrow', 'ew-resize', 'size_hor', 'e-resize', 'w-resize', 'left_side', 'right_side', 'col-resize', 'h_double_arrow'],
    'nwse-resize': ['bd_double_arrow', 'nwse-resize', 'size_fdiag', 'nw-resize', 'se-resize', 'top_left_corner', 'bottom_right_corner'],
    'nesw-resize': ['fd_double_arrow', 'nesw-resize', 'size_bdiag', 'ne-resize', 'sw-resize', 'top_right_corner', 'bottom_left_corner'],
    'grab': ['openhand', 'grab'], 'grabbing': ['closedhand', 'grabbing', 'dnd-none'],
}

def build_shape(kind, accent, glow, lit):
    hot = glow if lit else accent
    if kind.startswith('pointer'):
        pb, ph = pointer(accent, glow, lit)
        if ':' in kind:
            bb, bh = badge(kind.split(':')[1], accent, glow, lit)
            return pb + bb, ph + bh
        return pb, ph
    if kind == 'busy':
        return busy(accent, glow, lit)
    if kind == 'text':
        return text_beam(hot, lit)
    if kind in ('crosshair', 'cell'):
        return crosshair(hot, lit, cell=(kind == 'cell'))
    if kind == 'move':
        return move(hot, lit)
    if kind.startswith('resize'):
        return double_arrow(float(kind.split(':')[1]), hot, lit)
    if kind.startswith('hand'):
        return hand(kind.endswith('closed'), accent, glow, lit)
    raise ValueError(kind)

def svg(inner, with_defs=True):
    x, y, w, h = VIEW
    defs = f'<defs>{SHARED_DEFS}</defs>' if with_defs else ''
    return f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="{x} {y} {w} {h}" width="{w}" height="{h}">{defs}{inner}</svg>'

def rasterize(kind, accent, glow, lit, px):
    body, halo = build_shape(kind, accent, glow, lit)
    crisp = Image.open(io.BytesIO(cairosvg.svg2png(bytestring=svg(body).encode(), output_width=px, output_height=px))).convert('RGBA')
    out = Image.new('RGBA', (px, px), (0, 0, 0, 0))
    if halo:
        g = Image.open(io.BytesIO(cairosvg.svg2png(bytestring=svg(halo, False).encode(), output_width=px, output_height=px))).convert('RGBA')
        def bloom(radius, strength):
            b = g.filter(ImageFilter.GaussianBlur(px * radius))
            b.putalpha(b.getchannel('A').point(lambda v: min(255, int(v * strength))))
            return b
        if lit:
            # wide outer haze, then a tight inner glow hugging the beams, so
            # the light falls off smoothly from core to haze
            out.alpha_composite(bloom(0.10, 2.4))
            out.alpha_composite(bloom(0.025, 1.6))
        else:
            out.alpha_composite(bloom(0.03, 0.8))
    out.alpha_composite(crisp)
    return out

# ----------------------------------------------------------------- overlay
# Swapping cursor themes on click can't work on Hyprland 0.56: the image on
# screen only changes when an app re-requests its cursor, which never
# happens mid-click. So the ignite listener instead draws this light-only
# image on a click-through overlay layer, lined up under the real cursor.
# It's drawn on a canvas larger than the cursor frame so the haze can spread
# past the cursor bitmap's edges.
OVERLAY_MARGIN = 14.0                  # extra viewBox units on every side
OVERLAY_PX_PER_UNIT = 6                # oversampled; the listener scales it down

def write_overlay(glow, theme_dir):
    x, y, w, h = VIEW
    m = OVERLAY_MARGIN
    vb = (x - m, y - m, w + 2 * m, h + 2 * m)
    px = round(vb[2] * OVERLAY_PX_PER_UNIT)
    unit = OVERLAY_PX_PER_UNIT

    def render(inner):
        doc = (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="{vb[0]} {vb[1]} {vb[2]} {vb[3]}" '
               f'width="{vb[2]}" height="{vb[3]}">{inner}</svg>')
        return Image.open(io.BytesIO(cairosvg.svg2png(bytestring=doc.encode(), output_width=px, output_height=px))).convert('RGBA')

    # The overlay sits *under* the real cursor, so a beam inset inside the
    # spearhead would be hidden by the cursor's own head. Run it just outside
    # the edge instead, where it reads as the blade lighting up.
    # Dark accents (deep orange, red, purple) read as a weaker glow than
    # bright ones like green at the same strength, so lift the haze for them:
    # a stronger falloff, and a haze nudged towards white.
    luma = luminance(glow)
    lift = min(1.6, max(1.0, (GLOW_LUMA_TARGET / max(luma, 0.01)) ** 0.5))
    haze = lighten(glow, min(0.35, max(0.0, 1 - luma / GLOW_LUMA_TARGET) * 0.45))
    a, b = edge_line(-0.6, 0.03, 0.80)

    def glow_of(halo, col, crisp='', k=1.0):
        """Blurred halo (haze, glow, inner) with the crisp beam on top."""
        g = render(halo)
        out = Image.new('RGBA', (px, px), (0, 0, 0, 0))
        for radius_units, strength in ((9.5, 4.9), (2.8, 3.6), (1.0, 2.4)):
            # blur only the alpha: blurring RGBA mixes the transparent black into the
            # colour and leaves a dark fringe that turns black over a light background
            alpha = g.getchannel('A').filter(ImageFilter.GaussianBlur(radius_units * unit))
            b_ = Image.new('RGBA', (px, px), col)
            b_.putalpha(alpha.point(lambda v, s=strength * lift * 1.25 * k: min(255, int(v * s))))
            out.alpha_composite(b_)
        if crisp:
            out.alpha_composite(render(crisp))
        return out

    # Separate layers so the listener can animate them: the haze swells in,
    # each beam grows from its emitter ('from') to its end ('to').
    layers = []
    base = (f'<polygon points="{P(T)} {P(L)} {P(M)} {P(R)}" fill="{haze}" stroke="{haze}" stroke-width="4.0" stroke-linejoin="round"/>'
            # a softer spill under the handle, so the light reads as coming from the blade
            + line(add(M, U, 1.0), add(M, U, 19.5), haze, 11.0, ' opacity="0.50"'))
    glow_of(base, haze, k=DARK_K).save(theme_dir / 'overlay-base.png')
    layers.append({'image': 'overlay-base.png', 'kind': 'haze'})
    beams = [('overlay-edge.png', b, a)]   # the blade grows from the hilt end up to the tip
    for name, e, d in (('overlay-beam-r.png', EMIT_R, BEAM_R), ('overlay-beam-l.png', EMIT_L, BEAM_L)):
        tip = add(e, d, STUB)
        beams.append((name, tip, add(tip, d, BEAM_LEN)))
    for name, start, stop in beams:
        glow_of(line(start, stop, haze, 7.8), haze, lit_beam(start, stop, glow), k=DARK_K).save(theme_dir / name)
        layers.append({'image': name, 'kind': 'beam', 'from': [round(start[0], 3), round(start[1], 3)],
                       'to': [round(stop[0], 3), round(stop[1], 3)]})
    # Light-background set: over white the boosted haze saturates into one flat blob,
    # so use a gentler, lighter haze with a crisp deeper-coloured beam for definition.
    lk, lc, deep = 0.30, lighten(glow, 0.30), darken(glow, 0.30)
    light_layers = []
    lbase = (f'<polygon points="{P(T)} {P(L)} {P(M)} {P(R)}" fill="{lc}" stroke="{lc}" stroke-width="3.0" stroke-linejoin="round"/>'
             + line(add(M, U, 1.0), add(M, U, 19.5), lc, 9.0, ' opacity="0.50"'))
    glow_of(lbase, lc, k=lk * 0.6).save(theme_dir / 'overlay-light-base.png')
    light_layers.append({'image': 'overlay-light-base.png', 'kind': 'haze'})
    for (name, start, stop), meta_layer in zip(beams, layers[1:]):
        lname = name.replace('overlay-', 'overlay-light-')
        glow_of(line(start, stop, lc, 6.0), lc, line(start, stop, deep, 2.4), k=lk).save(theme_dir / lname)
        light_layers.append({**meta_layer, 'image': lname})
    meta = {'px_per_unit': unit, 'origin': [vb[0], vb[1]], 'hotspot': [T[0], T[1]],
            'view_w': VIEW[2], 'scale': SCALE, 'sizes': list(SIZES), 'layers': layers, 'light_layers': light_layers,
            # colours for the drag trail: dark backgrounds / light backgrounds (deeper, or it vanishes on a pale page)
            'trail_rgb': [[round(int(c[i:i + 2], 16) / 255, 3) for i in (1, 3, 5)] for c in (haze, darken(glow, 0.20))]}
    (theme_dir / 'overlay.json').write_text(json.dumps(meta) + '\n')
    stale = theme_dir / 'overlay.png'
    if stale.exists():
        stale.unlink()

# ----------------------------------------------------------------- xcursor
def xcursor(frames):
    # header, table of contents, then ARGB32 image chunks. Xcursor pixels are
    # premultiplied alpha; writing straight alpha fringes every soft edge.
    n = len(frames)
    blob = bytearray(struct.pack('<IIII', 0x72756358, 16, 0x10000, n))
    offset = 16 + n * 12
    toc, chunks = [], []
    for nominal, hx, hy, img in frames:
        w, h = img.size
        argb = b''.join(struct.pack('<I', (a << 24) | ((r * a // 255) << 16) | ((g * a // 255) << 8) | (b * a // 255))
                        for r, g, b, a in img.get_flattened_data())
        chunk = struct.pack('<IIIIIIIII', 36, 0xfffd0002, nominal, 1, w, h, hx, hy, 0) + argb
        toc.append(struct.pack('<III', 0xfffd0002, nominal, offset))
        chunks.append(chunk)
        offset += len(chunk)
    for t in toc:
        blob.extend(t)
    for c in chunks:
        blob.extend(c)
    return bytes(blob)

def _role_blob(lit, role, accent, glow):
    """One cursor role, all sizes, as xcursor bytes. Runs in a worker process."""
    kind, hot = SHAPES[role]
    frames = []
    for size in SIZES:
        px = round(size * SCALE)
        img = rasterize(kind, accent, glow, lit, px)
        hx = round((hot[0] - VIEW[0]) / VIEW[2] * px)
        hy = round((hot[1] - VIEW[1]) / VIEW[3] * px)
        frames.append((size, hx, hy, img))
    return role, xcursor(frames)

def _write_theme_files(name, label, blobs, out):
    theme = out / name
    (theme / 'cursors').mkdir(parents=True, exist_ok=True)
    (theme / 'index.theme').write_text(f'[Icon Theme]\nName={label}\nComment=Omarchy accent-adaptive Kyber cursor theme\nInherits=Adwaita\n')
    for role in SHAPES:
        names = ALIASES[role]
        (theme / 'cursors' / names[0]).write_bytes(blobs[role])
        for alias in names[1:]:
            dest = theme / 'cursors' / alias
            if dest.exists() or dest.is_symlink():
                dest.unlink()
            dest.symlink_to(names[0])

def write_theme(name, label, accent, glow, lit, out):
    """Sequential version (also the fallback when processes are unavailable)."""
    blobs = dict(_role_blob(lit, role, accent, glow) for role in SHAPES)
    _write_theme_files(name, label, blobs, out)

def _build_parallel(accent, glow, out, workers):
    # fork: the workers inherit this module as it is, nothing to re-import
    with ProcessPoolExecutor(max_workers=workers, mp_context=multiprocessing.get_context('fork')) as pool:
        ign = out / (THEME + '-Ignited')
        ign.mkdir(parents=True, exist_ok=True)
        overlay = pool.submit(write_overlay, glow, ign)      # runs alongside the shapes
        futs = {(lit, role): pool.submit(_role_blob, lit, role, accent, glow)
                for lit in (False, True) for role in SHAPES}
        for lit, name, label in ((False, THEME, 'Omarchy Kyber'), (True, THEME + '-Ignited', 'Omarchy Kyber Ignited')):
            blobs = dict(futs[(lit, role)].result() for role in SHAPES)
            _write_theme_files(name, label, blobs, out)
        overlay.result()

def build(accent, out):
    accent = accent.lower()
    if not re.fullmatch(r'#[0-9a-f]{6}', accent):
        raise ValueError('use an RGB hex value such as #7aa2f7')
    glow = IGNITE_COLOR or accent
    workers = min(os.cpu_count() or 1, 8)
    done = False
    if workers > 1:
        try:
            _build_parallel(accent, glow, out, workers)
            done = True
        except Exception:
            pass   # no fork, a worker died, ...: just do it one step at a time
    if not done:
        write_theme(THEME, 'Omarchy Kyber', accent, glow, False, out)
        write_theme(THEME + '-Ignited', 'Omarchy Kyber Ignited', accent, glow, True, out)
        write_overlay(glow, out / (THEME + '-Ignited'))
    print(f'Built {THEME} + {THEME}-Ignited ({len(SHAPES)} roles) with accent {accent}')

def preview(accent, path):
    glow = IGNITE_COLOR or accent
    cell, pad = 96, 10
    roles = list(SHAPES)
    cols = 7
    rows = -(-len(roles) // cols)
    sheet = Image.new('RGBA', (cols * (cell + pad) + pad, 2 * rows * (cell + pad) + pad + 20), (16, 18, 22, 255))
    for half, lit in enumerate((False, True)):
        for i, role in enumerate(roles):
            x = pad + (i % cols) * (cell + pad)
            y = pad + (half * rows + i // cols) * (cell + pad) + half * 20
            sheet.alpha_composite(rasterize(SHAPES[role][0], accent, glow, lit, cell), (x, y))
    sheet.save(path)
    print(path)

if __name__ == '__main__':
    ap = argparse.ArgumentParser()
    ap.add_argument('--accent')
    ap.add_argument('--output', type=Path, default=HERE / 'generated')
    ap.add_argument('--preview', type=Path, help='write a contact sheet of every role, idle and ignited, and exit')
    a = ap.parse_args()
    accent = a.accent or current_accent()
    if a.preview:
        preview(accent, a.preview)
    else:
        build(accent, a.output)
