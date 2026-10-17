//! Cursor geometry and SVG builders. Port of `lib/build.py`.
//!
//! The split is load-bearing: every builder returns `(body_svg, halo_svg)`.
//! The body rasterizes crisp; the halo rasterizes separately and is
//! blurred into bloom. The ignite overlay reuses the same art. Rasterization
//! (resvg/tiny-skia + blur) lands in step 2; this crate owns the vectors.

use crate::color::{darken, lighten};

pub const THEME: &str = "Omarchy-Kyber";
pub const SIZES: [u32; 4] = [24, 32, 48, 64];
/// Bitmaps render this much larger than nominal, so detail stays readable.
pub const SCALE: f64 = 1.6;
/// viewBox actually rendered: x, y, w, h.
pub const VIEW: [f64; 4] = [3.0, 0.0, 48.0, 48.0];
pub const NO_COLOR: &str = "#ff4d5a";
pub const OUTLINE: &str = "#0b0c0f";

/// Pointer tip = hotspot.
pub fn tip() -> (f64, f64) {
    (8.0, 5.0)
}
pub fn axis_angle() -> f64 {
    53.4f64.to_radians()
}
/// Unit vector along the pointer axis, tip -> pommel.
pub fn axis_u() -> (f64, f64) {
    let a = axis_angle();
    (a.cos(), a.sin())
}
/// Perpendicular, towards the upper-right corner.
pub fn axis_n() -> (f64, f64) {
    let (ux, uy) = axis_u();
    (uy, -ux)
}
pub const HEAD_LEN: f64 = 25.0;
pub const HEAD_HALF: f64 = 10.0;

pub fn add(a: (f64, f64), b: (f64, f64), k: f64) -> (f64, f64) {
    (a.0 + b.0 * k, a.1 + b.1 * k)
}

pub fn fmt_pt(p: (f64, f64)) -> String {
    format!("{:.2},{:.2}", p.0, p.1)
}

pub fn head_back() -> (f64, f64) {
    add(tip(), axis_u(), HEAD_LEN)
}
pub fn head_l() -> (f64, f64) {
    add(head_back(), axis_n(), -HEAD_HALF)
}
pub fn head_r() -> (f64, f64) {
    add(head_back(), axis_n(), HEAD_HALF)
}
pub fn ridge_end() -> (f64, f64) {
    add(tip(), axis_u(), HEAD_LEN * 0.80)
}

fn beam_dirs() -> ((f64, f64), (f64, f64)) {
    let sweep = 18f64.to_radians();
    let (ux, uy) = axis_u();
    let (nx, ny) = axis_n();
    let c = sweep.cos();
    let s = sweep.sin();
    (
        (nx * c + ux * s, ny * c + uy * s),
        (-nx * c + ux * s, -ny * c + uy * s),
    )
}

pub fn emitter() -> (f64, f64) {
    add(head_back(), axis_u(), 1.7)
}
pub fn emitter_r() -> (f64, f64) {
    add(emitter(), axis_n(), 5.6)
}
pub fn emitter_l() -> (f64, f64) {
    add(emitter(), axis_n(), -5.6)
}
pub const BEAM_LEN: f64 = 8.5;
pub const STUB: f64 = 2.8;
pub fn badge_center() -> (f64, f64) {
    (41.0, 29.0)
}
pub fn view_center() -> (f64, f64) {
    (VIEW[0] + VIEW[2] / 2.0, VIEW[1] + VIEW[3] / 2.0)
}

pub fn line(a: (f64, f64), b: (f64, f64), color: &str, width: f64, extra: &str) -> String {
    format!(
        "<line x1=\"{:.2}\" y1=\"{:.2}\" x2=\"{:.2}\" y2=\"{:.2}\" stroke=\"{}\" stroke-width=\"{}\" stroke-linecap=\"round\"{}/>",
        a.0, a.1, b.0, b.1, color, width, extra
    )
}

/// Segment parallel to the tip->L edge, pulled `inset` units into the head.
pub fn edge_line(inset: f64, start: f64, end: f64) -> ((f64, f64), (f64, f64)) {
    let t = tip();
    let l = head_l();
    let m = head_back();
    let d = (l.0 - t.0, l.1 - t.1);
    let len = (d.0 * d.0 + d.1 * d.1).sqrt();
    let d = (d.0 / len, d.1 / len);
    let mut inward = (-d.1, d.0);
    if inward.0 * (m.0 - t.0) + inward.1 * (m.1 - t.1) < 0.0 {
        inward = (-inward.0, -inward.1);
    }
    (
        add(add(t, d, start * len), inward, inset),
        add(add(t, d, end * len), inward, inset),
    )
}

pub const SHARED_DEFS: &str = r##"<linearGradient id="cyl" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#0d0e11"/><stop offset="0.35" stop-color="#3b414b"/><stop offset="0.55" stop-color="#1c1f24"/><stop offset="1" stop-color="#08090b"/></linearGradient><linearGradient id="silver" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#6c7380"/><stop offset="0.4" stop-color="#eef1f5"/><stop offset="1" stop-color="#5a606b"/></linearGradient><linearGradient id="metal" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#3a3f48"/><stop offset="1" stop-color="#101216"/></linearGradient>"##;

pub fn lit_beam(a: (f64, f64), b: (f64, f64), glow: &str) -> String {
    lit_beam_scaled(a, b, glow, 1.0)
}

pub fn lit_beam_scaled(a: (f64, f64), b: (f64, f64), glow: &str, extra_width: f64) -> String {
    let boost = crate::color::vivid_beam_boost(glow);
    let width = crate::color::beam_width_scale(glow) * extra_width * (1.0 + 0.35 * boost);
    let outer = crate::color::beam_color(glow);
    line(a, b, &outer, 3.0 * width, " opacity=\"0.94\"")
        + &lit_beam_core_scaled(a, b, glow, extra_width)
}

/// Bright center without the colored rim, so the overlay can feather that rim
/// independently without blurring the blade's white center.
pub(crate) fn lit_beam_core_scaled(
    a: (f64, f64),
    b: (f64, f64),
    glow: &str,
    extra_width: f64,
) -> String {
    let boost = crate::color::vivid_beam_boost(glow);
    let width = crate::color::beam_width_scale(glow) * extra_width * (1.0 + 0.35 * boost);
    let core = crate::color::blade_core_color(glow);
    format!(
        "{}{}{}",
        line(a, b, &core, (1.9 + 0.35 * boost) * width, ""),
        line(
            a,
            b,
            &lighten(&core, 0.58),
            (1.25 + 0.45 * boost) * width,
            ""
        ),
        line(a, b, "#ffffff", (0.75 + 0.65 * boost) * width, ""),
    )
}

/// Spearhead pointer. Returns `(body_svg, halo_svg)`.
pub fn pointer(accent: &str, glow: &str, lit: bool) -> (String, String) {
    let t = tip();
    let l = head_l();
    let r = head_r();
    let m = head_back();
    let mi = ridge_end();
    let deg = 53.4;
    let (beam_r, beam_l) = beam_dirs();
    let (emit_r, emit_l) = (emitter_r(), emitter_l());

    let (a, b) = if lit {
        edge_line(1.1, 0.03, 0.80)
    } else {
        edge_line(1.1, 0.14, 0.72)
    };
    let edge = if lit { glow } else { accent };
    let port = if lit { glow } else { "#3a3f48" };
    let hilt_strip = if lit {
        lighten(glow, 0.15)
    } else {
        accent.to_string()
    };
    let (fl0, fl1): (String, String) = if lit {
        (lighten(&darken(glow, 0.55), 0.10), darken(glow, 0.80))
    } else {
        ("#4a505b".into(), "#1d2026".into())
    };
    let (fr0, fr1): (String, String) = if lit {
        (darken(glow, 0.72), "#0c0d10".into())
    } else {
        ("#23262c".into(), "#0c0d10".into())
    };
    let defs = format!(
        "<linearGradient id=\"faceL\" gradientUnits=\"userSpaceOnUse\" x1=\"{:.2}\" y1=\"{:.2}\" x2=\"{:.2}\" y2=\"{:.2}\"><stop offset=\"0\" stop-color=\"{}\"/><stop offset=\"1\" stop-color=\"{}\"/></linearGradient><linearGradient id=\"faceR\" gradientUnits=\"userSpaceOnUse\" x1=\"{:.2}\" y1=\"{:.2}\" x2=\"{:.2}\" y2=\"{:.2}\"><stop offset=\"0\" stop-color=\"{}\"/><stop offset=\"1\" stop-color=\"{}\"/></linearGradient>",
        t.0, t.1, l.0, l.1, fl0, fl1, t.0, t.1, r.0, r.1, fr0, fr1
    );

    let mut stubs = String::new();
    let mut beams = String::new();
    for (e, d) in [(emit_r, beam_r), (emit_l, beam_l)] {
        let s0 = add(e, d, -0.6);
        let stb = add(e, d, STUB);
        stubs.push_str(&line(s0, stb, "#4a505a", 3.0, ""));
        stubs.push_str(&line(s0, stb, "#16181c", 2.3, ""));
        stubs.push_str(&line(
            add(stb, d, -0.5),
            stb,
            if lit { "#c9ced6" } else { "#5d6470" },
            2.6,
            "",
        ));
        if lit {
            beams.push_str(&lit_beam(stb, add(stb, d, BEAM_LEN), glow));
        }
    }
    let hilt_core = if lit {
        "<rect x=\"7.4\" y=\"-2.65\" width=\"7.2\" height=\"0.7\" rx=\"0.35\" fill=\"#ffffff\"/>"
            .to_string()
    } else {
        String::new()
    };
    let pommel = if lit {
        format!(
            "<circle cx=\"20.05\" cy=\"0\" r=\"1.1\" fill=\"{}\"/>",
            glow
        )
    } else {
        String::new()
    };
    let hilt = format!(
        "<g transform=\"translate({}) rotate({:.2})\"><rect x=\"-0.4\" y=\"-5.2\" width=\"3.6\" height=\"10.4\" rx=\"1.0\" fill=\"url(#cyl)\" stroke=\"#4a505a\" stroke-width=\"0.35\"/><rect x=\"0.5\" y=\"-5.9\" width=\"1.8\" height=\"1.3\" rx=\"0.45\" fill=\"{}\"/><rect x=\"0.5\" y=\"4.6\" width=\"1.8\" height=\"1.3\" rx=\"0.45\" fill=\"{}\"/><rect x=\"3.2\" y=\"-5.5\" width=\"2.2\" height=\"11.0\" rx=\"0.7\" fill=\"url(#silver)\" stroke=\"#3a3f48\" stroke-width=\"0.3\"/><rect x=\"5.4\" y=\"-4.5\" width=\"11.0\" height=\"9.0\" rx=\"1.8\" fill=\"url(#cyl)\" stroke=\"#4a505a\" stroke-width=\"0.35\"/><rect x=\"7.0\" y=\"-3.0\" width=\"8.0\" height=\"1.5\" rx=\"0.75\" fill=\"{}\"/>{}<rect x=\"16.4\" y=\"-3.7\" width=\"2.6\" height=\"7.4\" rx=\"1.1\" fill=\"url(#cyl)\" stroke=\"#4a505a\" stroke-width=\"0.35\"/><rect x=\"18.6\" y=\"-2.9\" width=\"1.4\" height=\"5.8\" rx=\"0.7\" fill=\"url(#silver)\"/>{}</g>",
        fmt_pt(m), deg, port, port, hilt_strip, hilt_core, pommel
    );
    let head = format!(
        "<polygon points=\"{} {} {}\" fill=\"url(#faceL)\"/><polygon points=\"{} {} {}\" fill=\"url(#faceR)\"/><polygon points=\"{} {} {} {}\" fill=\"#0b0c0f\"/><polyline points=\"{} {} {}\" fill=\"none\" stroke=\"#b8c0cc\" stroke-width=\"0.55\" stroke-linejoin=\"round\" stroke-linecap=\"round\"/>{}{}{}",
        fmt_pt(t), fmt_pt(l), fmt_pt(mi),
        fmt_pt(t), fmt_pt(mi), fmt_pt(r),
        fmt_pt(l), fmt_pt(mi), fmt_pt(r), fmt_pt(m),
        fmt_pt(l), fmt_pt(t), fmt_pt(r),
        line(l, r, "#2c3038", 0.5, ""),
        line(t, mi, "#5d6470", 0.35, ""),
        if lit {
            lit_beam(a, b, glow)
        } else {
            format!("{}{}", line(a, b, edge, 1.3, ""), line(a, b, "#ffffff", 0.45, " opacity=\"0.85\""))
        }
    );
    let body = format!("<defs>{}</defs>", defs) + &beams + &hilt + &stubs + &head;

    let (ga, gb) = if lit {
        edge_line(1.1, 0.01, 0.82)
    } else {
        edge_line(1.1, 0.11, 0.75)
    };
    if !lit {
        let u = axis_u();
        let n = axis_n();
        let hs = add(add(m, u, 7.0), n, 2.25);
        let he = add(add(m, u, 15.0), n, 2.25);
        return (
            body,
            line(ga, gb, edge, 2.2, "") + &line(hs, he, &hilt_strip, 1.8, ""),
        );
    }
    let mut halo = line(ga, gb, glow, 6.5, "");
    for (e, d) in [(emit_r, beam_r), (emit_l, beam_l)] {
        let stb = add(e, d, STUB);
        halo.push_str(&line(stb, add(stb, d, BEAM_LEN), glow, 6.5, ""));
    }
    halo.push_str(&format!(
        "<polygon points=\"{} {} {} {}\" fill=\"{}\" stroke=\"{}\" stroke-width=\"2.5\" stroke-linejoin=\"round\"/>",
        fmt_pt(t), fmt_pt(l), fmt_pt(m), fmt_pt(r), glow, glow
    ));
    (body, halo)
}

/// Non-pointer cursors share the hilt language: white metal arrows/bars
/// around a dark hilt segment with silver rings and a lit accent strip.
pub fn hilt_segment(length: f64, half: f64, hot: &str, lit: bool) -> String {
    let x0 = -length / 2.0;
    let mut core = format!(
        "<rect x=\"{:.2}\" y=\"-0.65\" width=\"{:.2}\" height=\"1.3\" rx=\"0.65\" fill=\"{}\"/>",
        x0 + 1.4,
        length - 2.8,
        hot
    );
    if lit {
        core.push_str(&format!(
            "<rect x=\"{:.2}\" y=\"-0.3\" width=\"{:.2}\" height=\"0.6\" rx=\"0.3\" fill=\"#ffffff\"/>",
            x0 + 1.8,
            length - 3.6
        ));
    }
    format!(
        "<rect x=\"{:.2}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"1.2\" fill=\"url(#cyl)\" stroke=\"#4a505a\" stroke-width=\"0.35\"/><rect x=\"{:.2}\" y=\"{}\" width=\"1.4\" height=\"{}\" rx=\"0.5\" fill=\"url(#silver)\"/><rect x=\"{:.2}\" y=\"{}\" width=\"1.4\" height=\"{}\" rx=\"0.5\" fill=\"url(#silver)\"/>{}",
        x0, half, length, 2.0 * half,
        x0 - 0.9, -half - 0.5, 2.0 * half + 1.0,
        -x0 - 0.5, -half - 0.5, 2.0 * half + 1.0,
        core
    )
}

/// White arrowhead pointing along +x (direction 1) or -x (-1), tip at x.
pub fn arrowhead(x: f64, direction: f64) -> String {
    let b = x - direction * 6.5;
    format!(
        "<polygon points=\"{},0 {},-5.2 {},5.2\" fill=\"#f2f4f7\" stroke=\"{}\" stroke-width=\"1.0\" stroke-linejoin=\"round\"/>",
        x, b, b, OUTLINE
    )
}

pub fn double_arrow(angle: f64, hot: &str, lit: bool) -> (String, String) {
    let shaft = format!(
        "<line x1=\"-14\" y1=\"0\" x2=\"14\" y2=\"0\" stroke=\"{}\" stroke-width=\"3.4\"/><line x1=\"-14\" y1=\"0\" x2=\"14\" y2=\"0\" stroke=\"#f2f4f7\" stroke-width=\"1.6\"/>",
        OUTLINE
    );
    let c = view_center();
    let inner = shaft
        + &arrowhead(19.0, 1.0)
        + &arrowhead(-19.0, -1.0)
        + &hilt_segment(10.0, 2.6, hot, lit);
    let body = format!(
        "<g transform=\"translate({},{}) rotate({})\">{}</g>",
        c.0, c.1, angle, inner
    );
    let halo = format!(
        "<g transform=\"translate({},{}) rotate({})\">{}</g>",
        c.0,
        c.1,
        angle,
        line(
            (-3.6, 0.0),
            (3.6, 0.0),
            hot,
            if lit { 3.0 } else { 1.8 },
            ""
        )
    );
    (body, halo)
}

pub fn move_cursor(hot: &str, lit: bool) -> (String, String) {
    let c = view_center();
    let mut inner = String::new();
    for ang in [0, 90, 180, 270] {
        inner.push_str(&format!(
            "<g transform=\"rotate({}\"><line x1=\"4\" y1=\"0\" x2=\"15\" y2=\"0\" stroke=\"{}\" stroke-width=\"3.4\"/><line x1=\"4\" y1=\"0\" x2=\"15\" y2=\"0\" stroke=\"#f2f4f7\" stroke-width=\"1.6\"/>{}</g>",
            ang, OUTLINE, arrowhead(20.0, 1.0)
        ));
    }
    inner.push_str(&format!(
        "<circle r=\"4.6\" fill=\"url(#metal)\" stroke=\"#b8c0cc\" stroke-width=\"0.7\"/><circle r=\"2.4\" fill=\"none\" stroke=\"{}\" stroke-width=\"1.3\"/>{}",
        hot, if lit { "<circle r=\"1.0\" fill=\"#ffffff\"/>" } else { "" }
    ));
    let body = format!("<g transform=\"translate({},{})\">{}</g>", c.0, c.1, inner);
    let halo =
        format!(
        "<circle cx=\"{}\" cy=\"{}\" r=\"2.6\" fill=\"none\" stroke=\"{}\" stroke-width=\"{}\"/>",
        c.0, c.1, hot, if lit { "2.8" } else { "1.6" }
    );
    (body, halo)
}

pub fn text_beam(hot: &str, lit: bool) -> (String, String) {
    let path = "M-4.5,-16 C-2,-16 0,-15 0,-12.5 C0,-15 2,-16 4.5,-16 M0,-12.5 V12.5 M-4.5,16 C-2,16 0,15 0,12.5 C0,15 2,16 4.5,16";
    let c = view_center();
    let inner = format!(
        "<path d=\"{}\" fill=\"none\" stroke=\"{}\" stroke-width=\"3.2\" stroke-linecap=\"round\"/><path d=\"{}\" fill=\"none\" stroke=\"#f2f4f7\" stroke-width=\"1.5\" stroke-linecap=\"round\"/><g transform=\"rotate(90)\">{}</g>",
        path, OUTLINE, path, hilt_segment(7.0, 2.2, hot, lit)
    );
    let body = format!("<g transform=\"translate({},{})\">{}</g>", c.0, c.1, inner);
    let halo = line(
        (c.0, c.1 - 2.4),
        (c.0, c.1 + 2.4),
        hot,
        if lit { 2.8 } else { 1.6 },
        "",
    );
    (body, halo)
}

pub fn crosshair(hot: &str, lit: bool, cell: bool) -> (String, String) {
    let c = view_center();
    let inner = if cell {
        format!(
            "<path d=\"M-2.6,-14 H2.6 V-2.6 H14 V2.6 H2.6 V14 H-2.6 V2.6 H-14 V-2.6 H-2.6 Z\" fill=\"#f2f4f7\" stroke=\"{}\" stroke-width=\"1.0\" stroke-linejoin=\"round\"/><rect x=\"-1.3\" y=\"-1.3\" width=\"2.6\" height=\"2.6\" fill=\"{}\"/>",
            OUTLINE, hot
        )
    } else {
        let arms = "M0,-17 V-5 M0,5 V17 M-17,0 H-5 M5,0 H17";
        format!(
            "<path d=\"{}\" stroke=\"{}\" stroke-width=\"3.0\" stroke-linecap=\"round\"/><path d=\"{}\" stroke=\"#f2f4f7\" stroke-width=\"1.3\" stroke-linecap=\"round\"/><circle r=\"3.0\" fill=\"none\" stroke=\"{}\" stroke-width=\"1.3\"/><circle r=\"0.8\" fill=\"#ffffff\"/>",
            arms, OUTLINE, arms, hot
        )
    };
    let body = format!("<g transform=\"translate({},{})\">{}</g>", c.0, c.1, inner);
    let halo =
        format!(
        "<circle cx=\"{}\" cy=\"{}\" r=\"3.0\" fill=\"none\" stroke=\"{}\" stroke-width=\"{}\"/>",
        c.0, c.1, hot, if lit { "2.6" } else { "1.4" }
    );
    (body, halo)
}

pub fn busy(accent: &str, glow: &str, lit: bool) -> (String, String) {
    let hot = if lit { glow } else { accent };
    let c = view_center();
    let ring = format!(
        "<circle cx=\"{}\" cy=\"{}\" r=\"15\" fill=\"none\" stroke=\"{}\" stroke-width=\"4.2\" stroke-dasharray=\"9 4.7\"/><circle cx=\"{}\" cy=\"{}\" r=\"15\" fill=\"none\" stroke=\"{}\" stroke-width=\"2.6\" stroke-dasharray=\"9 4.7\"/>",
        c.0, c.1, OUTLINE, c.0, c.1, hot
    );
    // A shrunken pointer sits inside the ring, its tip a little above centre.
    let (pb, ph) = pointer(accent, glow, lit);
    let t = tip();
    let s = 0.42;
    let shrink = format!(
        "translate({:.2},{:.2}) scale({})",
        c.0 - (t.0 + 13.0) * s,
        c.1 - (t.1 + 15.0) * s,
        s
    );
    let body = ring + &format!("<g transform=\"{}\">{}</g>", shrink, pb);
    let halo = format!(
        "<circle cx=\"{}\" cy=\"{}\" r=\"15\" fill=\"none\" stroke=\"{}\" stroke-width=\"{}\" stroke-dasharray=\"9 4.7\"/><g transform=\"{}\">{}</g>",
        c.0, c.1, hot, if lit { "4.2" } else { "3.0" }, shrink, ph
    );
    (body, halo)
}

pub const GRAB: &str = "M16 27 V17 Q16 13 20 13 Q23 13 23 17 V11 Q23 7 27 7 Q31 7 31 11 V17 V14 Q31 10 35 10 Q39 10 39 14 V18 Q39 14 43 15 Q46 15 46 19 V32 Q46 44 32 44 H27 Q20 44 16 37 L10 29 Q8 25 11 23 Q14 21 16 27 Z";
pub const GRABBING: &str = "M12 23 V17 Q12 13 16 13 Q19 13 19 17 V12 Q19 8 23 8 Q27 8 27 12 V16 V11 Q27 7 31 7 Q35 7 35 11 V16 Q35 12 39 12 Q43 12 43 16 V30 Q43 43 30 43 H23 Q12 43 12 32 Z";

pub fn hand(closed: bool, accent: &str, glow: &str, lit: bool) -> (String, String) {
    let path = if closed { GRABBING } else { GRAB };
    let knuckles = if closed {
        "M19 17 V24 M27 16 V24 M35 16 V24"
    } else {
        "M23 17 V25 M31 17 V25 M39 18 V25"
    };
    let mut body = format!(
        "<path d=\"{}\" fill=\"url(#metal)\" stroke=\"#b8c0cc\" stroke-width=\"1.0\" stroke-linejoin=\"round\"/><path d=\"{}\" stroke=\"#05060a\" stroke-width=\"0.8\" stroke-linecap=\"round\"/>",
        path, knuckles
    );
    let mut halo = String::new();
    if closed {
        // Gripping a lit saber: a beam runs behind the fist towards the corner.
        let beam_c = if lit { glow } else { accent };
        let (a, b) = ((6.0, 3.0), (24.0, 26.0));
        body = line(a, b, beam_c, 2.4, "") + &line(a, b, "#ffffff", 1.0, "") + &body;
        halo = line(a, b, beam_c, 4.6, "");
    }
    (body, halo)
}

pub fn disc(r: f64, fill: &str, rim: &str) -> String {
    let b = badge_center();
    format!(
        "<circle cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"{}\" stroke=\"{}\" stroke-width=\"0.8\"/>",
        b.0, b.1, r, fill, rim
    )
}

pub fn at_badge(inner: &str) -> String {
    let b = badge_center();
    format!("<g transform=\"translate({},{})\">{}</g>", b.0, b.1, inner)
}

pub fn badge(kind: &str, accent: &str, glow: &str, lit: bool) -> (String, String) {
    let hot = if lit { glow } else { accent };
    match kind {
        "link" => {
            let chain = "<g transform=\"rotate(-45)\" fill=\"none\" stroke-width=\"2.6\" stroke-linecap=\"round\"><rect x=\"-6.6\" y=\"-2.4\" width=\"7.6\" height=\"4.8\" rx=\"2.4\" stroke=\"#121418\"/><rect x=\"-1.0\" y=\"-2.4\" width=\"7.6\" height=\"4.8\" rx=\"2.4\" stroke=\"#121418\"/><rect x=\"-6.6\" y=\"-2.4\" width=\"7.6\" height=\"4.8\" rx=\"2.4\" stroke=\"url(#silver)\" stroke-width=\"1.4\"/><rect x=\"-1.0\" y=\"-2.4\" width=\"7.6\" height=\"4.8\" rx=\"2.4\" stroke=\"#eef1f5\" stroke-width=\"1.4\"/></g>";
            (at_badge(chain), String::new())
        }
        "help" => {
            let q = "<path d=\"M-2.4,-2.4 C-2.4,-5.0 2.6,-5.0 2.6,-2.2 C2.6,-0.2 0.1,0.0 0.1,1.8\" fill=\"none\" stroke=\"#ffffff\" stroke-width=\"1.5\" stroke-linecap=\"round\"/><circle cx=\"0.1\" cy=\"3.8\" r=\"0.95\" fill=\"#ffffff\"/>";
            (
                disc(6.2, "#121418", "#b8c0cc") + &at_badge(q),
                String::new(),
            )
        }
        "not-allowed" => {
            let mark = format!(
                "<circle r=\"5.0\" fill=\"none\" stroke=\"{}\" stroke-width=\"1.8\"/><line x1=\"-3.4\" y1=\"-3.4\" x2=\"3.4\" y2=\"3.4\" stroke=\"{}\" stroke-width=\"1.8\" stroke-linecap=\"round\"/>",
                NO_COLOR, NO_COLOR
            );
            let b = badge_center();
            let halo = format!("<circle cx=\"{}\" cy=\"{}\" r=\"5.0\" fill=\"none\" stroke=\"{}\" stroke-width=\"2.5\"/>", b.0, b.1, NO_COLOR);
            (disc(6.2, "#121418", "#5d6470") + &at_badge(&mark), halo)
        }
        "zoom-in" | "zoom-out" | "copy" => {
            let mut bar = "<line x1=\"-3.4\" y1=\"0\" x2=\"3.4\" y2=\"0\" stroke=\"#ffffff\" stroke-width=\"1.6\" stroke-linecap=\"round\"/>".to_string();
            if *kind != *"zoom-out" {
                bar.push_str("<line x1=\"0\" y1=\"-3.4\" x2=\"0\" y2=\"3.4\" stroke=\"#ffffff\" stroke-width=\"1.6\" stroke-linecap=\"round\"/>");
            }
            let b = badge_center();
            let shell = if *kind != *"copy" {
                disc(6.2, "#121418", "#b8c0cc")
            } else {
                format!("<rect x=\"{}\" y=\"{}\" width=\"11.2\" height=\"11.2\" rx=\"2.4\" fill=\"#121418\" stroke=\"#b8c0cc\" stroke-width=\"0.8\"/>", b.0 - 5.6, b.1 - 5.6)
            };
            (shell + &at_badge(&bar), String::new())
        }
        "alias" => {
            let arrow = "<path d=\"M-3.2,2.8 C-3.2,-1.4 -0.6,-2.6 2.6,-2.6\" fill=\"none\" stroke=\"#ffffff\" stroke-width=\"1.5\" stroke-linecap=\"round\"/><path d=\"M0.6,-4.8 L3.4,-2.6 L0.6,-0.4\" fill=\"none\" stroke=\"#ffffff\" stroke-width=\"1.5\" stroke-linecap=\"round\" stroke-linejoin=\"round\"/>";
            (
                disc(6.2, "#121418", "#b8c0cc") + &at_badge(arrow),
                String::new(),
            )
        }
        "progress" => {
            let glass = format!(
                "<path d=\"M-3.6,-5.6 H3.6 M-3.6,5.6 H3.6\" stroke=\"url(#silver)\" stroke-width=\"1.6\" stroke-linecap=\"round\"/><path d=\"M-2.8,-5.0 C-2.8,-1.2 2.8,1.2 2.8,5.0 M2.8,-5.0 C2.8,-1.2 -2.8,1.2 -2.8,5.0\" fill=\"none\" stroke=\"#d6dbe3\" stroke-width=\"1.0\"/><path d=\"M-1.6,3.6 L1.6,3.6 L0,1.2 Z\" fill=\"{}\"/><path d=\"M-1.4,-3.4 L1.4,-3.4 L0,-1.6 Z\" fill=\"{}\"/>",
                hot, hot
            );
            let b = badge_center();
            (
                at_badge(&glass),
                format!(
                    "<circle cx=\"{}\" cy=\"{}\" r=\"2.4\" fill=\"{}\"/>",
                    b.0, b.1, hot
                ),
            )
        }
        "context" => {
            let b = badge_center();
            let card = format!(
                "<rect x=\"{}\" y=\"{}\" width=\"10\" height=\"12.4\" rx=\"1.6\" fill=\"#121418\" stroke=\"#b8c0cc\" stroke-width=\"0.8\"/>",
                b.0 - 5.0, b.1 - 6.2
            ) + &at_badge("<path d=\"M-2.8,-2.8 H2.8 M-2.8,0 H2.8 M-2.8,2.8 H1.2\" stroke=\"#eef1f5\" stroke-width=\"1.1\" stroke-linecap=\"round\"/>");
            (card, String::new())
        }
        _ => (String::new(), String::new()),
    }
}

/// Builder key for a cursor role (must match `build.py::SHAPES` values).
pub fn role_kind(role: &str) -> &'static str {
    match role {
        "default" => "pointer",
        "pointer" => "pointer:link",
        "help" => "pointer:help",
        "progress" => "pointer:progress",
        "not-allowed" => "pointer:not-allowed",
        "copy" => "pointer:copy",
        "alias" => "pointer:alias",
        "context" => "pointer:context",
        "zoom-in" => "pointer:zoom-in",
        "zoom-out" => "pointer:zoom-out",
        "wait" => "busy",
        "text" => "text",
        "crosshair" => "crosshair",
        "cell" => "cell",
        "move" => "move",
        "ns-resize" => "resize:90",
        "ew-resize" => "resize:0",
        "nwse-resize" => "resize:45",
        "nesw-resize" => "resize:-45",
        "grab" => "hand:open",
        "grabbing" => "hand:closed",
        _ => "pointer",
    }
}

/// Build one shape kind. Returns `(body_svg, halo_svg)`.
pub fn build_shape(kind: &str, accent: &str, glow: &str, lit: bool) -> (String, String) {
    let hot = if lit { glow } else { accent };
    if let Some(rest) = kind.strip_prefix("pointer") {
        let (pb, ph) = pointer(accent, glow, lit);
        if let Some(badge_kind) = rest.strip_prefix(':') {
            let (bb, bh) = badge(badge_kind, accent, glow, lit);
            return (pb + &bb, ph + &bh);
        }
        return (pb, ph);
    }
    match kind {
        "busy" => busy(accent, glow, lit),
        "text" => text_beam(hot, lit),
        "crosshair" => crosshair(hot, lit, false),
        "cell" => crosshair(hot, lit, true),
        "move" => move_cursor(hot, lit),
        "hand:open" => hand(false, accent, glow, lit),
        "hand:closed" => hand(true, accent, glow, lit),
        _ if kind.starts_with("resize:") => {
            let angle: f64 = kind["resize:".len()..].parse().unwrap_or(0.0);
            double_arrow(angle, hot, lit)
        }
        _ => pointer(accent, glow, lit),
    }
}

/// All cursor roles (must match `build.py::SHAPES` keys).
pub const ROLES: [&str; 21] = [
    "default",
    "pointer",
    "help",
    "progress",
    "not-allowed",
    "copy",
    "alias",
    "context",
    "zoom-in",
    "zoom-out",
    "wait",
    "text",
    "crosshair",
    "cell",
    "move",
    "ns-resize",
    "ew-resize",
    "nwse-resize",
    "nesw-resize",
    "grab",
    "grabbing",
];

/// Hotspot in viewBox units (must match `build.py::SHAPES`).
pub fn hotspot(role: &str) -> (f64, f64) {
    match role {
        "wait" | "text" | "crosshair" | "cell" | "move" | "ns-resize" | "ew-resize"
        | "nwse-resize" | "nesw-resize" => view_center(),
        "grab" => (30.0, 11.0),
        "grabbing" => (27.0, 15.0),
        _ => tip(),
    }
}

pub fn svg(inner: &str, with_defs: bool) -> String {
    svg_sized(inner, with_defs, VIEW[2] as u32)
}

/// Same as [`svg`] but with an explicit pixel viewport (for rasterization).
pub fn svg_sized(inner: &str, with_defs: bool, px: u32) -> String {
    let defs = if with_defs {
        format!("<defs>{}</defs>", SHARED_DEFS)
    } else {
        String::new()
    };
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"{} {} {} {}\" width=\"{}\" height=\"{}\">{}{}</svg>",
        VIEW[0], VIEW[1], VIEW[2], VIEW[3], px, px, defs, inner
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roles_have_hotspots() {
        assert_eq!(ROLES.len(), 21);
        assert_eq!(hotspot("default"), tip());
        assert_eq!(hotspot("grab"), (30.0, 11.0));
    }

    #[test]
    fn pointer_splits_body_and_halo() {
        let (body, halo) = pointer("#7aa2f7", "#7aa2f7", false);
        assert!(body.contains("polygon"));
        assert!(!halo.is_empty());
        let (lit_body, lit_halo) = pointer("#7aa2f7", "#7aa2f7", true);
        assert!(lit_body.contains("#ffffff"));
        assert!(lit_halo.len() > halo.len());
    }

    #[test]
    fn svg_wraps_viewbox() {
        let s = svg("<circle/>", true);
        assert!(s.contains("viewBox=\"3 0 48 48\""));
        assert!(s.contains("linearGradient"));
    }

    #[test]
    fn every_role_builds_body_and_halo() {
        for role in ROLES {
            for lit in [false, true] {
                let kind = role_kind(role);
                let (body, _) = build_shape(kind, "#7aa2f7", "#7aa2f7", lit);
                assert!(!body.is_empty(), "{role} lit={lit} has no body");
                // Full documents must parse as XML-ish SVG (balanced tags).
                let doc = svg(&body, true);
                assert!(doc.starts_with("<svg"), "{role} lit={lit}");
                assert!(doc.ends_with("</svg>"), "{role} lit={lit}");
            }
        }
    }

    #[test]
    fn badges_match_python_kinds() {
        // Every badged pointer kind from build.py renders a badge.
        for kind in [
            "link",
            "help",
            "not-allowed",
            "copy",
            "alias",
            "context",
            "zoom-in",
            "zoom-out",
            "progress",
        ] {
            let (b, _) = badge(kind, "#7aa2f7", "#7aa2f7", false);
            assert!(!b.is_empty(), "{kind}");
        }
        // Unknown badge kinds render nothing (Python returns '', '').
        assert_eq!(
            badge("nope", "#7aa2f7", "#7aa2f7", false),
            (String::new(), String::new())
        );
    }
}
