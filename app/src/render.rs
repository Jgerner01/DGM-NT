//! Drawing helpers: colours, the shaded height fill, contour lines with labels.

use egui::{Align2, Color32, FontId, Mesh, Painter, Pos2, Rect, Shape, Stroke};

use crate::contours::Contours;
use crate::tin::Surface;
use crate::view::View;

/// CADdy colour numbers used by dgm.sys (1 rot … 12 dunkelblau); 7 "weiß" is drawn black on paper.
pub fn caddy(c: i32) -> Color32 {
    match c {
        1 => Color32::from_rgb(210, 30, 30),
        2 => Color32::from_rgb(20, 150, 40),
        3 => Color32::from_rgb(200, 150, 0),
        4 => Color32::from_rgb(30, 70, 210),
        5 => Color32::from_rgb(190, 30, 190),
        6 => Color32::from_rgb(0, 160, 180),
        7 => Color32::from_rgb(25, 25, 25),
        8 => Color32::from_rgb(130, 130, 130),
        9 => Color32::from_rgb(130, 0, 0),
        10 => Color32::from_rgb(0, 100, 0),
        11 => Color32::from_rgb(150, 85, 25),
        12 => Color32::from_rgb(0, 0, 130),
        _ => Color32::from_rgb(60, 60, 60),
    }
}

/// Graphic colours from dgm.sys (lines 37–49).
pub struct Palette {
    pub point: Color32,
    pub point_nr: Color32,
    pub height: Color32,
    pub ring: Color32,
    pub contour: Color32,
    pub index: Color32,
    pub breakline: Color32,
    pub grid: Color32,
    pub tri: Color32,
    pub plan: Color32,
}

impl Default for Palette {
    fn default() -> Self {
        Palette {
            point: caddy(1),
            point_nr: caddy(1),
            height: caddy(6),
            ring: caddy(5),
            contour: Color32::from_rgb(125, 78, 30),
            index: Color32::from_rgb(90, 45, 10),
            breakline: caddy(11),
            grid: caddy(4),
            tri: Color32::from_rgba_unmultiplied(0, 150, 170, 150),
            plan: caddy(7),
        }
    }
}

/// Hypsometric tint: green lowlands, yellow, brown, light grey peaks.
pub fn height_color(t: f64) -> Color32 {
    let stops: [(f64, [f32; 3]); 6] = [
        (0.0, [72.0, 140.0, 90.0]),
        (0.22, [136.0, 182.0, 112.0]),
        (0.45, [222.0, 214.0, 150.0]),
        (0.68, [200.0, 160.0, 105.0]),
        (0.86, [160.0, 120.0, 95.0]),
        (1.0, [235.0, 232.0, 228.0]),
    ];
    let t = t.clamp(0.0, 1.0);
    for w in stops.windows(2) {
        let (a, ca) = w[0];
        let (b, cb) = w[1];
        if t <= b {
            let f = ((t - a) / (b - a)) as f32;
            return Color32::from_rgb(
                (ca[0] + (cb[0] - ca[0]) * f) as u8,
                (ca[1] + (cb[1] - ca[1]) * f) as u8,
                (ca[2] + (cb[2] - ca[2]) * f) as u8,
            );
        }
    }
    Color32::WHITE
}

/// Slope colours (Steigungen): flat green → steep red.
pub fn slope_color(tan: f64) -> Color32 {
    let t = (tan / 0.6).clamp(0.0, 1.0);
    let stops: [(f64, [f32; 3]); 4] = [(0.0, [90.0, 170.0, 90.0]), (0.33, [235.0, 220.0, 110.0]), (0.66, [235.0, 140.0, 60.0]), (1.0, [190.0, 40.0, 40.0])];
    for w in stops.windows(2) {
        let (a, ca) = w[0];
        let (b, cb) = w[1];
        if t <= b {
            let f = ((t - a) / (b - a)) as f32;
            return Color32::from_rgb((ca[0] + (cb[0] - ca[0]) * f) as u8, (ca[1] + (cb[1] - ca[1]) * f) as u8, (ca[2] + (cb[2] - ca[2]) * f) as u8);
        }
    }
    Color32::RED
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum FillMode {
    None,
    Height,
    Slope,
}

/// Pre-computed coloured mesh of the curved surface (built once, moved every frame).
pub struct FillCache {
    pos: Vec<[f64; 2]>,
    col: Vec<Color32>,
    idx: Vec<u32>,
    bbox: [f64; 4],
}

impl FillCache {
    pub fn build(s: &Surface, mode: FillMode, shade: bool, sub: usize, zr: (f64, f64)) -> Option<FillCache> {
        if mode == FillMode::None || s.tris.is_empty() {
            return None;
        }
        let n = sub.clamp(1, 6);
        let mut pos = Vec::new();
        let mut col = Vec::new();
        let mut idx: Vec<u32> = Vec::new();
        let mut bb = [f64::MAX, f64::MAX, f64::MIN, f64::MIN];
        let light = [-0.55f64, 0.55, 0.63];
        let ll = (light[0] * light[0] + light[1] * light[1] + light[2] * light[2]).sqrt();
        let span = (zr.1 - zr.0).max(1e-9);
        let mut grid = vec![vec![0u32; n + 1]; n + 1];
        let mut sub_xyz = vec![[0.0f64; 3]; (n + 1) * (n + 2) / 2];
        for (ti, t) in s.tris.iter().enumerate() {
            let (a, b, c) = (s.pos[t[0]], s.pos[t[1]], s.pos[t[2]]);
            let mut k = 0;
            for i in 0..=n {
                for j in 0..=(n - i) {
                    let l = n - i - j;
                    let (fl, fi, fj) = (l as f64 / n as f64, i as f64 / n as f64, j as f64 / n as f64);
                    sub_xyz[k] = [a[0] * fl + b[0] * fi + c[0] * fj, a[1] * fl + b[1] * fi + c[1] * fj, s.eval(ti, [fl, fi, fj])];
                    k += 1;
                }
            }
            // per-vertex normal from the planar triangle (+ curvature via numeric neighbours)
            let mut k = 0;
            for i in 0..=n {
                for j in 0..=(n - i) {
                    let p = sub_xyz[k];
                    let (fl, fi, fj) = ((n - i - j) as f64 / n as f64, i as f64 / n as f64, j as f64 / n as f64);
                    let color = if mode == FillMode::Height {
                        height_color((p[2] - zr.0) / span)
                    } else {
                        let g = triangle_grad(a, b, c);
                        slope_color(g[0].hypot(g[1]))
                    };
                    let color = if shade {
                        // smooth (Phong-like) shading: point gradients blended across the triangle,
                        // so no triangle facets show at any zoom
                        let w = [fl, fi, fj];
                        let mut g = [0.0, 0.0];
                        for (k, &vi) in t.iter().enumerate() {
                            g[0] += w[k] * s.vgrad[vi][0];
                            g[1] += w[k] * s.vgrad[vi][1];
                        }
                        let nrm = [-g[0], -g[1], 1.0];
                        let nl = (nrm[0] * nrm[0] + nrm[1] * nrm[1] + 1.0f64).sqrt();
                        let d = (nrm[0] * light[0] + nrm[1] * light[1] + nrm[2] * light[2]) / (nl * ll);
                        // flat ground keeps its colour; slopes facing the light get brighter, others darker
                        let flat = light[2] / ll;
                        let f = (1.0 + 1.6 * (d - flat)).clamp(0.5, 1.25) as f32;
                        Color32::from_rgb(
                            (color.r() as f32 * f).min(255.0) as u8,
                            (color.g() as f32 * f).min(255.0) as u8,
                            (color.b() as f32 * f).min(255.0) as u8,
                        )
                    } else {
                        color
                    };
                    grid[i][j] = pos.len() as u32;
                    pos.push([p[0] + s.ox, p[1] + s.oy]);
                    bb[0] = bb[0].min(p[0] + s.ox);
                    bb[1] = bb[1].min(p[1] + s.oy);
                    bb[2] = bb[2].max(p[0] + s.ox);
                    bb[3] = bb[3].max(p[1] + s.oy);
                    col.push(color);
                    k += 1;
                }
            }
            for i in 0..n {
                for j in 0..(n - i) {
                    idx.extend_from_slice(&[grid[i][j], grid[i + 1][j], grid[i][j + 1]]);
                    if i + j + 2 <= n {
                        idx.extend_from_slice(&[grid[i + 1][j], grid[i + 1][j + 1], grid[i][j + 1]]);
                    }
                }
            }
        }
        Some(FillCache { pos, col, idx, bbox: bb })
    }

    /// Farbnetz für die PDF-/DXF-Ausgabe: Ecken (Welt), Farben, Dreiecksindizes, Umgrenzung.
    pub fn mesh(&self) -> (&[[f64; 2]], &[Color32], &[u32], [f64; 4]) {
        (&self.pos, &self.col, &self.idx, self.bbox)
    }

    pub fn draw(&self, painter: &Painter, view: &View, rect: Rect, alpha: f32) {
        let vis = view.world_rect(rect);
        if self.bbox[2] < vis[0] || self.bbox[0] > vis[2] || self.bbox[3] < vis[1] || self.bbox[1] > vis[3] {
            return;
        }
        let mut mesh = Mesh::default();
        mesh.vertices.reserve(self.pos.len());
        for (p, c) in self.pos.iter().zip(&self.col) {
            mesh.colored_vertex(view.to_screen(rect, p[0], p[1]), c.gamma_multiply(alpha));
        }
        mesh.indices = self.idx.clone();
        painter.add(Shape::mesh(mesh));
    }
}

fn bary_shift(l: f64, i: f64, j: f64, di: f64, dj: f64) -> [f64; 3] {
    let (mut i2, mut j2) = (i + di, j + dj);
    let mut l2 = l - di - dj;
    if l2 < 0.0 {
        // step the other way when at the far edge
        i2 = i - di;
        j2 = j - dj;
        l2 = l + di + dj;
    }
    [l2, i2, j2]
}

fn bary_pos(a: [f64; 3], b: [f64; 3], c: [f64; 3], w: [f64; 3]) -> [f64; 2] {
    [a[0] * w[0] + b[0] * w[1] + c[0] * w[2], a[1] * w[0] + b[1] * w[1] + c[1] * w[2]]
}

/// true when the whole segment [sa, sb] lies inside one label gap
fn pieces_len_zero(gaps: &[(f64, f64)], sa: f64, sb: f64) -> bool {
    gaps.iter().any(|g| g.0 <= sa && g.1 >= sb)
}

pub fn triangle_grad(a: [f64; 3], b: [f64; 3], c: [f64; 3]) -> [f64; 2] {
    let det = (b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1]);
    if det.abs() < 1e-12 {
        return [0.0, 0.0];
    }
    let gx = ((b[2] - a[2]) * (c[1] - a[1]) - (c[2] - a[2]) * (b[1] - a[1])) / det;
    let gy = ((c[2] - a[2]) * (b[0] - a[0]) - (b[2] - a[2]) * (c[0] - a[0])) / det;
    [gx, gy]
}

pub struct ContourStyle {
    pub normal: Stroke,
    pub index: Stroke,
    pub label_color: Color32,
    pub font_px: f32,
    pub decimals: usize,
    pub show_labels: bool,
}

/// Draw contours: only lines in view, points closer than 0.6 px merged (level of detail),
/// constant crisp line widths at every zoom, index contours interrupted under their labels.
pub fn draw_contours(painter: &Painter, view: &View, rect: Rect, c: &Contours, ox: f64, oy: f64, st: &ContourStyle) -> usize {
    let vis = view.world_rect(rect);
    let (vx0, vy0, vx1, vy1) = (vis[0] - ox, vis[1] - oy, vis[2] - ox, vis[3] - oy);
    let px_per_m = view.ppm;
    // fade thin lines only when they would really crowd together (typical spacing on screen)
    let spacing_px = c.typical_spacing * px_per_m;
    let normal_alpha = ((spacing_px - 2.0) / 5.0).clamp(0.25, 1.0) as f32;
    let mut drawn = 0;
    let mut shapes: Vec<Shape> = Vec::new();
    let mut labels: Vec<Shape> = Vec::new();
    for l in &c.lines {
        if l.bbox[2] < vx0 || l.bbox[0] > vx1 || l.bbox[3] < vy0 || l.bbox[1] > vy1 {
            continue;
        }
        let stroke = if l.index { st.index } else { Stroke::new(st.normal.width, st.normal.color.gamma_multiply(normal_alpha)) };
        if stroke.color.a() == 0 {
            continue;
        }
        // label gaps (arc-length ranges) in metres
        let text = format!("{:.*}", st.decimals, l.z);
        let label_half_m = (text.len() as f64 * 0.28 + 0.35) * st.font_px as f64 / px_per_m;
        let show_labels = st.show_labels && l.index && label_half_m * 2.0 < (*l.len.last().unwrap_or(&0.0)) * 0.5;
        let gaps: Vec<(f64, f64)> = if show_labels { l.labels.iter().map(|lb| (lb.s - label_half_m, lb.s + label_half_m)).collect() } else { vec![] };
        let mut run: Vec<Pos2> = Vec::new();
        let n = l.pts.len();
        let total = n + if l.closed { 1 } else { 0 };
        let s_at = |k: usize| if k < n { l.len[k] } else { l.len[n - 1] + (l.pts[n - 1][0] - l.pts[0][0]).hypot(l.pts[n - 1][1] - l.pts[0][1]) };
        let scr = |k: usize| view.to_screen(rect, l.pts[k % n][0] + ox, l.pts[k % n][1] + oy);
        let flush = |run: &mut Vec<Pos2>, shapes: &mut Vec<Shape>| {
            if run.len() >= 2 {
                shapes.push(Shape::line(std::mem::take(run), stroke));
            }
            run.clear();
        };
        for k in 0..total.saturating_sub(1) {
            let (sa, sb) = (s_at(k), s_at(k + 1));
            let (pa, pb) = (scr(k), scr(k + 1));
            // the parts of this segment outside the label gaps
            let mut pieces = vec![(sa, sb)];
            for g in &gaps {
                let mut next = Vec::new();
                for (a, b) in pieces {
                    if g.1 <= a || g.0 >= b {
                        next.push((a, b));
                    } else {
                        if g.0 > a {
                            next.push((a, g.0));
                        }
                        if g.1 < b {
                            next.push((g.1, b));
                        }
                    }
                }
                pieces = next;
            }
            let at = |s: f64| {
                let f = if sb > sa { ((s - sa) / (sb - sa)) as f32 } else { 0.0 };
                pa + (pb - pa) * f
            };
            for (a, b) in pieces {
                let (qa, qb) = (at(a), at(b));
                match run.last() {
                    Some(&q) if (q - qa).length_sq() < 0.01 => {}
                    _ => {
                        flush(&mut run, &mut shapes);
                        run.push(qa);
                    }
                }
                // level of detail: skip points closer than 0.6 px
                if (qb - *run.last().unwrap()).length_sq() >= 0.36 || b >= sb {
                    run.push(qb);
                }
            }
            if pieces_len_zero(&gaps, sa, sb) {
                flush(&mut run, &mut shapes);
            }
        }
        flush(&mut run, &mut shapes);
        drawn += 1;
        if show_labels {
            for lb in &l.labels {
                let p = view.to_screen(rect, lb.pos[0] + ox, lb.pos[1] + oy);
                if !rect.expand(40.0).contains(p) {
                    continue;
                }
                // screen angle (y down) and keep the text upright
                let mut ang = -(lb.angle as f32);
                if ang > std::f32::consts::FRAC_PI_2 {
                    ang -= std::f32::consts::PI;
                } else if ang < -std::f32::consts::FRAC_PI_2 {
                    ang += std::f32::consts::PI;
                }
                let galley = painter.layout_no_wrap(text.clone(), FontId::proportional(st.font_px), st.label_color);
                labels.push(Shape::Text(egui::epaint::TextShape::new(p, galley, st.label_color).with_angle_and_anchor(ang, Align2::CENTER_CENTER)));
            }
        }
    }
    painter.extend(shapes);
    painter.extend(labels);
    drawn
}

/// Text with a light halo so it stays readable on any background.
pub fn halo_text(painter: &Painter, pos: Pos2, anchor: Align2, text: &str, size: f32, color: Color32, halo: Color32) {
    let font = FontId::proportional(size);
    for d in [egui::vec2(-1.0, 0.0), egui::vec2(1.0, 0.0), egui::vec2(0.0, -1.0), egui::vec2(0.0, 1.0)] {
        painter.text(pos + d, anchor, text, font.clone(), halo);
    }
    painter.text(pos, anchor, text, font, color);
}
