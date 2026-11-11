//! Ausgabe der Bildschirmdarstellung: DXF (mit den Export-Parametern der Parameterdatei) und
//! Druckausgabe als vektorbasierte PDF-Datei auf A4 (hoch oder quer) mit Rahmen, Legende, Maßstab und Nordpfeil.

use std::collections::HashSet;

use egui::Color32;

use crate::app::Layers;
use crate::compare::Compare;
use crate::contours::{ContourLine, Contours};
use crate::files::DxfWriter;
use crate::model::{Params, Project};
use crate::pdf::{self, Content};
use crate::render::{self, caddy, FillCache, Palette};
use crate::scene::*;
use crate::tin::Surface;

/// Alles, was die Szene aus der Bildschirmdarstellung braucht.
pub struct Input<'a> {
    pub project: &'a Project,
    pub surface: Option<&'a Surface>,
    pub contours: Option<&'a Contours>,
    pub fill: Option<&'a FillCache>,
    pub layers: &'a Layers,
    pub fill_mode: u8,
    pub palette: &'a Palette,
    pub cmp: &'a Compare,
    pub a_name: &'a str,
    /// Weltausschnitt (x0, y0, x1, y1)
    pub region: [f64; 4],
    /// Maßstabszahl N (1:N); 1 mm auf dem Papier = N/1000 m
    pub scale_n: f64,
    pub dxf: bool,
    /// Farbflächen auch ins DXF schreiben (als SOLID)
    pub dxf_fills: bool,
}

pub fn rgb_of(c: Color32) -> Rgb {
    // vormultipliziert über Weiß mischen
    let a = c.a() as u32;
    let m = |v: u8| (v as u32 + 255 * (255 - a) / 255).min(255) as u8;
    [m(c.r()), m(c.g()), m(c.b())]
}

fn upright(deg: f64) -> f64 {
    let mut d = deg;
    while d > 90.0 {
        d -= 180.0;
    }
    while d < -90.0 {
        d += 180.0;
    }
    d
}

/// Farbe eines Elements: im DXF aus den CAD-Farbnummern der Parameterdatei, im PDF wie auf dem Bildschirm.
struct Colors<'a> {
    dxf: bool,
    p: &'a Params,
}

impl Colors<'_> {
    fn get(&self, slot: usize, screen: Color32) -> (Rgb, i32) {
        if self.dxf {
            let c = self.p.exp_colors[slot];
            (rgb_of(caddy(c)), c)
        } else {
            (rgb_of(screen), 0)
        }
    }
}

/// Rasterabstand der Gitterkreuze: Abstand der Parameterdatei (mm Papier) × Maßstab, auf eine runde Zahl.
fn grid_step(grid_mm: f64, n: f64, extent: f64) -> f64 {
    // Wunsch aus der Parameterdatei, aber höchstens ein Viertel des Ausschnitts: so enthält der Ausschnitt in
    // jeder Richtung mindestens vier Teilungen (Gitterkreuze) und das Gitter füllt die ganze Zeichenfläche
    let cap = (extent / 4.0).max(1e-6);
    let want = ((if grid_mm > 0.0 { grid_mm } else { 100.0 }) / 1000.0 * n).min(cap).max(1e-6);
    let e = want.log10().floor();
    let mut best: Option<(f64, f64)> = None;
    for ee in [e - 1.0, e, e + 1.0] {
        for m in [1.0, 2.0, 5.0] {
            let c = m * 10f64.powf(ee);
            if c > cap * 1.000_001 {
                continue; // nie gröber als die Obergrenze
            }
            let d = (c / want).ln().abs();
            if best.map_or(true, |b| d < b.0) {
                best = Some((d, c));
            }
        }
    }
    best.map_or(want, |b| b.1)
}

struct ContourStyle {
    layer_n: u8,
    layer_i: u8,
    layer_l: u8,
    normal: (Rgb, i32, f64),
    index: (Rgb, i32, f64),
    label: (Rgb, i32),
    dec: usize,
    labels: bool,
    label_h_mm: f64,
}

/// Linienzug ohne die Lücken (Bogenlängenbereiche) unter den Beschriftungen.
fn runs(l: &ContourLine, gaps: &[(f64, f64)], min_step: f64) -> Vec<Vec<P2>> {
    let n = l.pts.len();
    if n < 2 {
        return Vec::new();
    }
    let total = n + if l.closed { 1 } else { 0 };
    let s_at = |k: usize| if k < n { l.len[k] } else { l.len[n - 1] + (l.pts[n - 1][0] - l.pts[0][0]).hypot(l.pts[n - 1][1] - l.pts[0][1]) };
    let mut out: Vec<Vec<P2>> = Vec::new();
    let mut cur: Vec<P2> = Vec::new();
    let flush = |cur: &mut Vec<P2>, out: &mut Vec<Vec<P2>>| {
        if cur.len() >= 2 {
            out.push(std::mem::take(cur));
        }
        cur.clear();
    };
    for k in 0..total - 1 {
        let (sa, sb) = (s_at(k), s_at(k + 1));
        let (pa, pb) = (l.pts[k % n], l.pts[(k + 1) % n]);
        let mut pieces = vec![(sa, sb)];
        for g in gaps {
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
            let f = if sb > sa { (s - sa) / (sb - sa) } else { 0.0 };
            [pa[0] + (pb[0] - pa[0]) * f, pa[1] + (pb[1] - pa[1]) * f]
        };
        for (a, b) in pieces {
            let (qa, qb) = (at(a), at(b));
            let continues = cur.last().map_or(false, |q| (q[0] - qa[0]).hypot(q[1] - qa[1]) < 1e-9);
            if !continues {
                flush(&mut cur, &mut out);
                cur.push(qa);
            }
            // Punkte, die dichter als `min_step` liegen, entfallen (außer dem Ende der Strecke)
            let last = *cur.last().unwrap();
            if (qb[0] - last[0]).hypot(qb[1] - last[1]) >= min_step || b >= sb {
                cur.push(qb);
            }
        }
    }
    flush(&mut cur, &mut out);
    out
}

fn push_contours(sc: &mut Scene, c: &Contours, o: P2, reg: [f64; 4], k: f64, st: &ContourStyle) {
    let r = [reg[0] - o[0], reg[1] - o[1], reg[2] - o[0], reg[3] - o[1]];
    let size = pdf::font_size_for_cap(st.label_h_mm);
    for l in &c.lines {
        if !bbox_hits(l.bbox, r) {
            continue;
        }
        let text = format!("{:.*}", st.dec, l.z);
        let half = (pdf::text_width(&text, size, false) * 0.5 + 0.7) * k;
        let show = st.labels && l.index && !l.labels.is_empty();
        let gaps: Vec<(f64, f64)> = if show { l.labels.iter().map(|lb| (lb.s - half, lb.s + half)).collect() } else { Vec::new() };
        let (layer, (rgb, cad, w)) = if l.index { (st.layer_i, st.index) } else { (st.layer_n, st.normal) };
        for run in runs(l, &gaps, 0.08 * k) {
            let pts: Vec<P2> = run.iter().map(|q| [q[0] + o[0], q[1] + o[1]]).collect();
            sc.items.push(Item::Line { layer, rgb, cad, w_mm: w, pts, z: l.z, closed: false, dash: false });
        }
        if show {
            for lb in &l.labels {
                let pos = [lb.pos[0] + o[0], lb.pos[1] + o[1]];
                if pos[0] < reg[0] || pos[0] > reg[2] || pos[1] < reg[1] || pos[1] > reg[3] {
                    continue;
                }
                sc.items.push(Item::Text {
                    layer: st.layer_l,
                    rgb: st.label.0,
                    cad: st.label.1,
                    pos,
                    h_mm: st.label_h_mm,
                    rot_deg: upright(lb.angle.to_degrees()),
                    ax: 0.5,
                    ay: 0.5,
                    text: text.clone(),
                    halo: true,
                });
            }
        }
    }
}

fn push_mesh(sc: &mut Scene, lay: u8, pos: &[P2], col: &[Color32], idx: &[u32], reg: [f64; 4]) {
    let mut p = Vec::new();
    let mut c = Vec::new();
    for t in idx.chunks_exact(3) {
        let v = [pos[t[0] as usize], pos[t[1] as usize], pos[t[2] as usize]];
        if !bbox_hits(pts_bbox(&v), reg) {
            continue;
        }
        for (i, &q) in v.iter().enumerate() {
            p.push(q);
            c.push(rgb_of(col[t[i] as usize]));
        }
    }
    if !p.is_empty() {
        let idx: Vec<u32> = (0..p.len() as u32).collect();
        sc.items.push(Item::Mesh { layer: lay, pos: p, col: c, idx });
    }
}

fn inside(q: P2, reg: [f64; 4]) -> bool {
    q[0] >= reg[0] && q[0] <= reg[2] && q[1] >= reg[1] && q[1] <= reg[3]
}

/// Szene aus der Bildschirmdarstellung (sichtbare Ebenen, Ausschnitt `region`).
pub fn build(inp: &Input) -> Scene {
    let mut sc = Scene::default();
    let pr = inp.project;
    let p = &pr.st.params;
    let l = inp.layers;
    let pal = inp.palette;
    let reg = inp.region;
    let k = inp.scale_n / 1000.0; // m je mm Papier
    let col = Colors { dxf: inp.dxf, p };
    let analysis = inp.cmp.analysis_data();
    let want_fill = !inp.dxf || inp.dxf_fills;

    // ---- Farbflächen: Analyse oder Höhen-/Neigungsfarben
    if let Some(a) = &analysis {
        if want_fill && !a.mesh_pos.is_empty() {
            push_mesh(&mut sc, layer::FILL, a.mesh_pos, a.mesh_col, a.mesh_idx, reg);
        }
    } else if want_fill && inp.fill_mode != 0 {
        if let Some(f) = inp.fill {
            let (pos, cl, idx, bb) = f.mesh();
            if bbox_hits(bb, reg) {
                push_mesh(&mut sc, layer::FILL, pos, cl, idx, reg);
            }
        }
    }

    // ---- Koordinatengitter: Kreuze und Beschriftung am Rand des Ausschnitts
    if l.grid {
        let step = grid_step(p.grid_mm, inp.scale_n, (reg[2] - reg[0]).min(reg[3] - reg[1]));
        let (rgb, cad) = col.get(slot::GRID, pal.grid);
        let arm = 2.5 * k;
        let (x0, y0) = ((reg[0] / step).ceil() * step, (reg[1] / step).ceil() * step);
        let dec = if step < 1.0 { 2 } else { 0 };
        let th = p.exp_text_grid_mm;
        let mut count = 0;
        let mut x = x0;
        while x <= reg[2] && count < 40_000 {
            let mut y = y0;
            while y <= reg[3] && count < 40_000 {
                for pts in [vec![[x - arm, y], [x + arm, y]], vec![[x, y - arm], [x, y + arm]]] {
                    sc.items.push(Item::Line { layer: layer::GRID, rgb, cad, w_mm: 0.12, pts, z: 0.0, closed: false, dash: false });
                }
                y += step;
                count += 1;
            }
            if th > 0.0 {
                sc.items.push(Item::Text { layer: layer::GRID, rgb, cad, pos: [x, reg[1] + 1.0 * k], h_mm: th, rot_deg: 0.0, ax: 0.5, ay: 0.0, text: format!("{:.*}", dec, x), halo: true });
            }
            x += step;
        }
        if th > 0.0 {
            let mut y = y0;
            while y <= reg[3] {
                sc.items.push(Item::Text { layer: layer::GRID, rgb, cad, pos: [reg[0] + 1.0 * k, y], h_mm: th, rot_deg: 0.0, ax: 0.0, ay: 0.5, text: format!("{:.*}", dec, y), halo: true });
                y += step;
            }
        }
    }

    // ---- Dreiecke
    if l.triangles {
        let (rgb, cad) = col.get(slot::TRI, pal.tri);
        let mut seen: HashSet<(usize, usize)> = HashSet::new();
        for t in pr.st.triangles.iter().filter(|t| t.active) {
            for e in 0..3 {
                let (a, b) = (t.v[e], t.v[(e + 1) % 3]);
                if !seen.insert((a.min(b), a.max(b))) {
                    continue;
                }
                let q = [[pr.st.points[a].x, pr.st.points[a].y], [pr.st.points[b].x, pr.st.points[b].y]];
                if bbox_hits(pts_bbox(&q), reg) {
                    sc.items.push(Item::Line { layer: layer::TRI, rgb, cad, w_mm: 0.08, pts: q.to_vec(), z: 0.0, closed: false, dash: false });
                }
            }
        }
    }

    // ---- Höhenlinien (nicht bei der Analyse, dort ersetzen die Differenzlinien sie)
    if analysis.is_none() && l.contours {
        if let (Some(c), Some(s)) = (inp.contours, inp.surface) {
            let (n_rgb, n_cad) = col.get(slot::CONTOUR, pal.contour);
            let (i_rgb, i_cad) = col.get(slot::INDEX, pal.index);
            let st = ContourStyle {
                layer_n: layer::CONTOUR,
                layer_i: layer::INDEX,
                layer_l: layer::LABELS,
                normal: (n_rgb, n_cad, 0.12),
                index: (i_rgb, i_cad, 0.3),
                label: (i_rgb, i_cad),
                dec: p.decimals_index as usize,
                labels: l.labels,
                label_h_mm: p.exp_text_index_mm.max(0.5),
            };
            push_contours(&mut sc, c, [s.ox, s.oy], reg, k, &st);
        }
        for ml in &pr.st.manual_labels {
            if inside([ml.x, ml.y], reg) {
                let (rgb, cad) = col.get(slot::INDEX, pal.index);
                sc.items.push(Item::Text {
                    layer: layer::LABELS,
                    rgb,
                    cad,
                    pos: [ml.x, ml.y],
                    h_mm: p.exp_text_index_mm.max(0.5),
                    rot_deg: upright((ml.angle as f64).to_degrees()),
                    ax: 0.5,
                    ay: 0.5,
                    text: format!("{:.*}", p.decimals_index as usize, ml.z),
                    halo: true,
                });
            }
        }
    }

    // ---- Analyse: Differenzlinien, Schnittkante, gemeinsamer Umring
    if let Some(a) = &analysis {
        let h = p.exp_text_index_mm.max(0.5);
        let mk = |slot: usize, screen: Color32, w: f64, wi: f64, labels: bool| {
            let (rgb, cad) = col.get(slot, screen);
            ContourStyle { layer_n: layer::DIFF, layer_i: layer::DIFF, layer_l: layer::LABELS, normal: (rgb, cad, w), index: (rgb, cad, wi), label: (rgb, cad), dec: a.decimals, labels, label_h_mm: h }
        };
        if a.show_iso {
            push_contours(&mut sc, a.neg, a.origin, reg, k, &mk(slot::CUT, Color32::from_rgb(110, 20, 20), 0.15, 0.3, a.show_labels));
            push_contours(&mut sc, a.pos, a.origin, reg, k, &mk(slot::FILL, Color32::from_rgb(15, 45, 120), 0.15, 0.3, a.show_labels));
        }
        push_contours(&mut sc, a.zero, a.origin, reg, k, &mk(slot::ZERO, Color32::BLACK, 0.5, 0.5, a.show_iso && a.show_labels));
        let (rgb, cad) = col.get(slot::RING, Color32::from_rgb(200, 0, 120));
        for (pts, closed) in &a.umring {
            if bbox_hits(pts_bbox(pts), reg) {
                sc.items.push(Item::Line { layer: layer::DIFF, rgb, cad, w_mm: 0.5, pts: pts.clone(), z: 0.0, closed: *closed, dash: false });
            }
        }
    }

    // ---- Umring / Inringe, Grundriss, Bruchkanten
    let pt = |i: usize| [pr.st.points[i].x, pr.st.points[i].y];
    if l.rings {
        let (rgb, cad) = col.get(slot::RING, pal.ring);
        for (ri, r) in std::iter::once(&pr.st.umring).chain(pr.st.inrings.iter()).enumerate() {
            if r.len() >= 3 {
                let pts: Vec<P2> = r.iter().map(|&i| pt(i)).collect();
                if bbox_hits(pts_bbox(&pts), reg) {
                    sc.items.push(Item::Line { layer: layer::RING, rgb, cad, w_mm: 0.35, pts, z: 0.0, closed: true, dash: ri > 0 });
                }
            }
        }
    }
    if l.plan {
        let (rgb, cad) = col.get(slot::PLAN, pal.plan);
        for b in &pr.st.plan_lines {
            let pts: Vec<P2> = b.pts.iter().map(|&i| pt(i)).collect();
            if bbox_hits(pts_bbox(&pts), reg) {
                sc.items.push(Item::Line { layer: layer::PLAN, rgb, cad, w_mm: 0.25, pts, z: 0.0, closed: false, dash: false });
            }
        }
    }
    if l.breaklines {
        let (rgb, cad) = col.get(slot::BREAK, pal.breakline);
        for b in &pr.st.breaklines {
            let pts: Vec<P2> = b.pts.iter().map(|&i| pt(i)).collect();
            if bbox_hits(pts_bbox(&pts), reg) {
                sc.items.push(Item::Line { layer: layer::BREAK, rgb, cad, w_mm: 0.25, pts, z: 0.0, closed: false, dash: b.soft });
            }
        }
    }

    // ---- Punkte: Kreise mit Nummer und Höhe (Beschriftung nur, wo sie auf dem Papier Platz hat)
    if l.points {
        let vis: Vec<&crate::model::Point> = pr.st.points.iter().filter(|q| q.active && inside([q.x, q.y], reg)).collect();
        if !vis.is_empty() {
            let area = (reg[2] - reg[0]) * (reg[3] - reg[1]);
            let spacing_mm = (area / vis.len() as f64).sqrt() / k;
            // PDF: Beschriftung nur, wo sie auf dem Papier Platz hat. DXF: immer alle Punkte beschriften
            // (das CAD-Programm blendet nach Bedarf aus).
            let show_text = inp.dxf || (spacing_mm > 9.0 && (l.numbers || l.heights));
            let (c_rgb, c_cad) = col.get(slot::POINT, pal.point);
            let (n_rgb, n_cad) = col.get(slot::POINT_NR, pal.point_nr);
            let (h_rgb, h_cad) = col.get(slot::HEIGHT, Color32::from_rgb(0, 105, 125));
            let (hn, hh) = (p.exp_text_nr_mm, p.exp_text_h_mm);
            let mut occupied: HashSet<(i64, i64)> = HashSet::new();
            for q in vis {
                if p.exp_circle_mm > 0.0 {
                    sc.items.push(Item::Circle { layer: layer::POINTS, rgb: c_rgb, cad: c_cad, c: [q.x, q.y], r_mm: p.exp_circle_mm });
                }
                if !show_text {
                    continue;
                }
                if !inp.dxf && !occupied.insert(((q.x / k / 8.0).floor() as i64, (q.y / k / 6.0).floor() as i64)) {
                    continue;
                }
                // Lage 0 (Vorgabe): Punktnummer etwas rechts oberhalb des Punkts, Höhe rechts vom Punkt.
                // Die übrigen Lagen (vom Anwender per Punktnummer-Position gewählt) spiegeln das nach
                // rechts unten (1), links unten (2) und links oben (3); die Höhe steht immer seitlich.
                let (right, up) = match q.label_pos {
                    0 => (true, true),
                    1 => (true, false),
                    2 => (false, false),
                    _ => (false, true),
                };
                let ax = if right { 0.0 } else { 1.0 };
                let dx = if right { 0.9 } else { -0.9 } * k;
                let half_h = if l.heights { hh * 0.5 } else { 0.0 };
                if l.numbers && hn > 0.0 {
                    let off = half_h + if l.heights { 0.3 } else { 0.5 };
                    let (dy, ay) = if up { (off, 0.0) } else { (-off, 1.0) };
                    sc.items.push(Item::Text { layer: layer::NUMBERS, rgb: n_rgb, cad: n_cad, pos: [q.x + dx, q.y + dy * k], h_mm: hn, rot_deg: 0.0, ax, ay, text: q.name.clone(), halo: false });
                }
                if l.heights && hh > 0.0 {
                    sc.items.push(Item::Text { layer: layer::HEIGHTS, rgb: h_rgb, cad: h_cad, pos: [q.x + dx, q.y], h_mm: hh, rot_deg: 0.0, ax, ay: 0.5, text: format!("{:.*}", p.decimals_h as usize, q.z), halo: false });
                }
            }
        }
    }

    if !inp.dxf {
        sc.legend = legend_for(inp, analysis.is_some());
    }
    sc
}

fn legend_for(inp: &Input, analysis: bool) -> Legend {
    let pr = inp.project;
    let p = &pr.st.params;
    if analysis {
        if let Some(l) = inp.cmp.legend(p, inp.a_name) {
            return l;
        }
    }
    let mut blocks = Vec::new();
    if inp.fill_mode != 0 {
        if let Some(s) = inp.surface {
            let (z0, z1) = (s.zmin, s.zmax);
            let dec = p.decimals_h.min(2) as usize;
            if inp.fill_mode == 1 {
                let stops = (0..=12).map(|i| (i as f64 / 12.0, rgb_of(render::height_color(i as f64 / 12.0)))).collect();
                let ticks = (0..=4).map(|i| (i as f64 / 4.0, format!("{:.*}", dec.max(1), z0 + (z1 - z0) * i as f64 / 4.0))).collect();
                blocks.push(Block::Bar { caption: "Höhe in m".into(), stops, ticks });
            } else {
                let stops = (0..=12).map(|i| (i as f64 / 12.0, rgb_of(render::slope_color(0.6 * i as f64 / 12.0)))).collect();
                let ticks = (0..=4).map(|i| (i as f64 / 4.0, format!("{} %", 15 * i))).collect();
                blocks.push(Block::Bar { caption: "Neigung".into(), stops, ticks });
            }
        }
    }
    let mut lines = Vec::new();
    if inp.layers.contours {
        lines.push((format!("Höhenlinien alle {} m", trim_num(p.contour_interval)), false));
        lines.push((format!("Zähllinien: jede {}. Linie beschriftet", p.index_every), false));
    }
    if inp.layers.points {
        lines.push((format!("Punkte: {}", pr.st.points.iter().filter(|q| q.active).count()), false));
    }
    if !lines.is_empty() {
        blocks.push(Block::Lines(lines));
    }
    Legend { title: pr.st.name.clone(), blocks }
}

fn trim_num(v: f64) -> String {
    let s = format!("{:.3}", v);
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

// ------------------------------------------------------------------ DXF

fn cad_to_aci(c: i32) -> i32 {
    match c {
        0 => 7,
        1 => 1,
        2 => 3,
        3 => 2,
        4 => 5,
        5 => 6,
        6 => 4,
        7 => 7,
        8 => 8,
        9 => 1,
        10 => 3,
        11 => 30,
        12 => 5,
        c => c.clamp(1, 255),
    }
}

/// DXF aus der Szene: Maße in mm Papier × (N / 1000) = Meter, Ebenen und Farben aus der Parameterdatei.
pub fn to_dxf(sc: &Scene, p: &Params, scale_n: f64) -> String {
    let k = scale_n / 1000.0;
    let ln = |i: u8| p.layers[(i as usize).min(p.layers.len() - 1)].to_string();
    // Ebenentabelle: alle 13 CAD-Ebenen der Parameterdatei, Farbe nach dem Zweck der Ebene
    let layer_slot = [slot::POINT, slot::GRID, slot::POINT_NR, slot::HEIGHT, slot::RING, slot::CONTOUR, slot::INDEX, slot::PLAN, slot::INDEX, slot::BREAK, slot::ZERO, slot::TRI, slot::GRID];
    let mut table: Vec<(String, i32)> = Vec::new();
    for (i, s) in layer_slot.iter().enumerate() {
        let name = ln(i as u8);
        if !table.iter().any(|(n, _)| *n == name) {
            table.push((name, cad_to_aci(p.exp_colors[*s])));
        }
    }
    let mut d = DxfWriter::with_layers(&table);
    for it in &sc.items {
        match it {
            Item::Line { layer, rgb, cad, pts, z, closed, .. } => {
                let p3: Vec<[f64; 3]> = pts.iter().map(|q| [q[0], q[1], *z]).collect();
                d.polyline_c(&ln(*layer), cad_to_aci(*cad), *rgb, &p3, *closed);
            }
            Item::Circle { layer, rgb, cad, c, r_mm } => d.circle_c(&ln(*layer), cad_to_aci(*cad), *rgb, *c, r_mm * k),
            Item::Text { layer, rgb, cad, pos, h_mm, rot_deg, ax, ay, text, .. } => d.text_c(&ln(*layer), cad_to_aci(*cad), *rgb, *pos, h_mm * k, *rot_deg, *ax, *ay, text),
            Item::Mesh { layer, pos, col, idx } => {
                for t in idx.chunks_exact(3) {
                    let (a, b, c) = (t[0] as usize, t[1] as usize, t[2] as usize);
                    let avg = [0, 1, 2].map(|ch| ((col[a][ch] as u32 + col[b][ch] as u32 + col[c][ch] as u32) / 3) as u8);
                    d.solid_c(&ln(*layer), 0, avg, pos[a], pos[b], pos[c]);
                }
            }
        }
    }
    d.finish()
}

// ------------------------------------------------------------------ PDF

pub const A4: (f64, f64) = (210.0, 297.0);

/// Blattaufteilung der Druckausgabe (alle Maße in mm).
#[derive(Clone, Copy, Debug)]
pub struct Sheet {
    pub landscape: bool,
    /// Mindestrand rundum
    pub margin: f64,
    /// zusätzlicher Streifen für die Legende: unten bei Hochformat, rechts bei Querformat
    pub strip: f64,
}

impl Sheet {
    pub fn new(landscape: bool) -> Self {
        Sheet { landscape, margin: 10.0, strip: if landscape { 60.0 } else { 45.0 } }
    }
    pub fn size(&self) -> (f64, f64) {
        if self.landscape {
            (A4.1, A4.0)
        } else {
            A4
        }
    }
    /// Ränder (links, oben, rechts, unten)
    pub fn margins(&self) -> (f64, f64, f64, f64) {
        let m = self.margin;
        if self.landscape {
            (m, m, m + self.strip, m)
        } else {
            (m, m, m, m + self.strip)
        }
    }
    /// Legendenstreifen (x, y, b, h) in mm, Ursprung links unten: im Hochformat unter der Zeichenfläche
    /// (zwischen Mindestrand unten und Zeichenfläche), im Querformat rechts daneben.
    pub fn strip_rect(&self) -> (f64, f64, f64, f64) {
        let (w, h) = self.size();
        let (l, t, r, b) = self.margins();
        let gap = 3.0;
        if self.landscape {
            (w - r + gap, self.margin, r - self.margin - gap, h - t - self.margin)
        } else {
            (l, self.margin, w - l - r, b - self.margin - gap)
        }
    }

    /// Zeichenfläche (x, y, b, h) in mm, Ursprung links unten
    pub fn area(&self) -> (f64, f64, f64, f64) {
        let (w, h) = self.size();
        let (l, t, r, b) = self.margins();
        (l, b, w - l - r, h - t - b)
    }
}

fn nice_len(max_m: f64) -> f64 {
    let e = max_m.log10().floor();
    for m in [5.0, 2.0, 1.0] {
        let c = m * 10f64.powf(e);
        if c <= max_m {
            return c;
        }
    }
    10f64.powf(e - 1.0) * 5.0
}

fn group(n: u64) -> String {
    let s = n.to_string();
    let mut o = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            o.push('.');
        }
        o.push(c);
    }
    o
}

fn wrap(text: &str, size: f64, bold: bool, maxw: f64) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for w in text.split_whitespace() {
        let t = if cur.is_empty() { w.to_string() } else { format!("{} {}", cur, w) };
        if cur.is_empty() || pdf::text_width(&t, size, bold) <= maxw {
            cur = t;
        } else {
            out.push(std::mem::take(&mut cur));
            cur = w.to_string();
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

const BLACK: Rgb = [0, 0, 0];
const GREY: Rgb = [90, 90, 90];

struct Flow {
    x: f64,
    w: f64,
    y: f64,
}

fn flow_lines(c: &mut Content, f: &mut Flow, lines: &[(String, bool)], cap: f64) {
    let size = pdf::font_size_for_cap(cap);
    for (t, bold) in lines {
        for part in wrap(t, size, *bold, f.w) {
            f.y -= cap * 1.75;
            c.text(f.x, f.y, cap, *bold, BLACK, 0.0, 0.0, 0.0, &part, false);
        }
    }
    f.y -= 1.5;
}

fn gradient(c: &mut Content, stops: &[(f64, Rgb)], horizontal: bool, x: f64, y: f64, w: f64, h: f64) {
    let mut v: Vec<((f64, f64), Rgb)> = Vec::new();
    for s in stops.windows(2) {
        let (a, b) = (s[0], s[1]);
        let pt = |t: f64, side: f64| if horizontal { (x + t * w, y + side * h) } else { (x + side * w, y + t * h) };
        let (a0, a1, b0, b1) = (pt(a.0, 0.0), pt(a.0, 1.0), pt(b.0, 0.0), pt(b.0, 1.0));
        v.extend([(a0, a.1), (a1, a.1), (b1, b.1), (a0, a.1), (b1, b.1), (b0, b.1)]);
    }
    c.mesh(&v);
}

fn flow_bar(c: &mut Content, f: &mut Flow, caption: &str, stops: &[(f64, Rgb)], ticks: &[(f64, String)], horizontal: bool) {
    let cap = 2.0;
    f.y -= cap * 1.75;
    c.text(f.x, f.y, cap, true, BLACK, 0.0, 0.0, 0.0, caption, false);
    if horizontal {
        let (bw, bh) = ((f.w - 6.0).clamp(20.0, 62.0), 4.5);
        let by = f.y - 2.0 - bh;
        let bx = f.x + 3.0;
        gradient(c, stops, true, bx, by, bw, bh);
        c.stroke_color(GREY);
        c.line_width(0.2);
        c.rect(bx, by, bw, bh, false, true);
        for (t, s) in ticks {
            let x = bx + t * bw;
            c.polyline(&[(x, by), (x, by - 1.0)], false);
            c.text(x, by - 1.4, 1.9, false, BLACK, 0.0, 0.5, 1.0, s, false);
        }
        f.y = by - 7.0;
    } else {
        let (bw, bh) = (5.0, 42.0);
        let by = f.y - 2.0 - bh;
        let bx = f.x + 1.0;
        gradient(c, stops, false, bx, by, bw, bh);
        c.stroke_color(GREY);
        c.line_width(0.2);
        c.rect(bx, by, bw, bh, false, true);
        for (t, s) in ticks {
            let y = by + t * bh;
            c.polyline(&[(bx + bw, y), (bx + bw + 1.0, y)], false);
            c.text(bx + bw + 1.8, y, 1.9, false, BLACK, 0.0, 0.0, 0.5, s, false);
        }
        f.y = by - 3.0;
    }
}

/// Höhe eines Farbleisten-Blocks (waagrecht) im Streifen
const BAR_H: f64 = 17.0;

fn lines_height(lines: &[(String, bool)], cap: f64, w: f64) -> f64 {
    let size = pdf::font_size_for_cap(cap);
    lines.iter().map(|(t, b)| wrap(t, size, *b, w).len() as f64 * cap * 1.75).sum::<f64>() + 1.5
}

/// Zwei Spalten nur, wenn der längste Text in eine halbe Spaltenbreite passt.
fn swatch_two_cols(items: &[(Rgb, String)], w: f64) -> bool {
    let size = pdf::font_size_for_cap(2.0);
    let longest = items.iter().map(|(_, t)| pdf::text_width(t, size, false)).fold(0.0, f64::max);
    longest + 7.5 <= w / 2.0
}

fn swatches_height(items: &[(Rgb, String)], w: f64) -> f64 {
    let per = if swatch_two_cols(items, w) { items.len().div_ceil(2) } else { items.len() };
    2.0 * 1.75 + 1.5 + per as f64 * 4.2 + 2.0
}

fn flow_swatches(c: &mut Content, f: &mut Flow, caption: &str, items: &[(Rgb, String)], two_cols: bool) {
    f.y -= 2.0 * 1.75;
    c.text(f.x, f.y, 2.0, true, BLACK, 0.0, 0.0, 0.0, caption, false);
    let step = 4.2;
    let (cols, per) = if two_cols { (2, items.len().div_ceil(2)) } else { (1, items.len()) };
    let cw = f.w / cols as f64;
    let top = f.y - 1.5;
    for (i, (rgb, t)) in items.iter().enumerate() {
        let (col, row) = (i / per, i % per);
        let x = f.x + col as f64 * cw;
        let y = top - (row as f64 + 1.0) * step;
        c.fill_color(*rgb);
        c.stroke_color(GREY);
        c.line_width(0.15);
        c.rect(x, y, 5.5, 3.0, true, true);
        c.text(x + 7.0, y + 0.4, 2.0, false, BLACK, 0.0, 0.0, 0.0, t, false);
    }
    f.y = top - per as f64 * step - 2.0;
}

fn north_arrow(c: &mut Content, cx: f64, cy: f64, h: f64) {
    // wie auf dem Bildschirm: rechte Hälfte dunkel, linke hell
    let tip = (cx, cy + h * 0.6);
    let (r, l, m) = ((cx + h * 0.22, cy - h * 0.4), (cx - h * 0.22, cy - h * 0.4), (cx, cy - h * 0.15));
    c.fill_color([40, 40, 40]);
    c.fill_poly(&[tip, r, m]);
    c.fill_color([255, 255, 255]);
    c.stroke_color([40, 40, 40]);
    c.line_width(0.25);
    c.polyline(&[tip, m, l], true);
    c.polyline(&[tip, r, m], true);
    c.text(cx, cy + h * 0.6 + 1.0, 3.0, true, BLACK, 0.0, 0.5, 0.0, "N", false);
}

fn scale_bar(c: &mut Content, x: f64, y: f64, n: f64, max_mm: f64) {
    let len_m = nice_len(max_mm * n / 1000.0);
    let len_mm = len_m * 1000.0 / n;
    for i in 0..4 {
        let sx = x + len_mm * i as f64 / 4.0;
        c.fill_color(if i % 2 == 0 { [25, 25, 25] } else { [255, 255, 255] });
        c.rect(sx, y, len_mm / 4.0, 1.6, true, false);
    }
    c.stroke_color([25, 25, 25]);
    c.line_width(0.2);
    c.rect(x, y, len_mm, 1.6, false, true);
    let unit = |m: f64| if m >= 1000.0 { format!("{} km", trim_num(m / 1000.0)) } else { format!("{} m", trim_num(m)) };
    c.text(x, y - 0.8, 1.9, false, BLACK, 0.0, 0.5, 1.0, "0", false);
    c.text(x + len_mm / 2.0, y - 0.8, 1.9, false, BLACK, 0.0, 0.5, 1.0, &unit(len_m / 2.0), false);
    c.text(x + len_mm, y - 0.8, 1.9, false, BLACK, 0.0, 0.5, 1.0, &unit(len_m), false);
}

/// PDF-Seite: Zeichenfläche mit Rahmen, daneben (unten bzw. rechts) Legende, Maßstab und Nordpfeil.
/// `origin` = Weltkoordinaten der linken unteren Ecke der Zeichenfläche, `n` = Maßstabszahl.
pub fn render_pdf(sc: &Scene, sheet: &Sheet, origin: P2, n: f64, p: &Params, project: &str) -> Vec<u8> {
    let (w, h) = sheet.size();
    let (ax, ay, aw, ah) = sheet.area();
    let mm = 1000.0 / n; // mm je m
    let tr = |q: P2| (ax + (q[0] - origin[0]) * mm, ay + (q[1] - origin[1]) * mm);
    let mut c = Content::new();
    c.round_joins();

    // ---- Zeichenfläche
    c.save();
    c.clip_rect(ax, ay, aw, ah);
    for it in &sc.items {
        match it {
            Item::Line { rgb, w_mm, pts, closed, dash, .. } => {
                let v: Vec<(f64, f64)> = pts.iter().map(|&q| tr(q)).collect();
                c.stroke_color(*rgb);
                c.line_width(*w_mm);
                c.dash(if *dash { Some((2.0, 1.2)) } else { None });
                c.polyline(&v, *closed);
            }
            Item::Circle { rgb, c: pos, r_mm, .. } => {
                let q = tr(*pos);
                c.fill_color(*rgb);
                c.circle(q.0, q.1, *r_mm, true);
            }
            Item::Text { rgb, pos, h_mm, rot_deg, ax: tax, ay: tay, text, halo, .. } => {
                let q = tr(*pos);
                c.text(q.0, q.1, *h_mm, false, *rgb, *rot_deg, *tax, *tay, text, *halo);
            }
            Item::Mesh { pos, col, idx, .. } => {
                let v: Vec<((f64, f64), Rgb)> = idx.iter().map(|&i| (tr(pos[i as usize]), col[i as usize])).collect();
                c.mesh(&v);
            }
        }
    }
    c.restore();
    c.dash(None);
    c.stroke_color(BLACK);
    c.line_width(0.5);
    c.rect(ax, ay, aw, ah, false, true);

    // ---- Streifen für Legende, Maßstab, Nordpfeil
    let (sx, sy, sw, sh) = sheet.strip_rect();
    c.stroke_color([150, 150, 150]);
    c.line_width(0.2);
    c.rect(sx, sy, sw, sh, false, true);
    let date = pdf::today();
    let info = vec![(format!("Maßstab 1:{}", group(n.round().max(1.0) as u64)), true), (format!("Stand: {}", date), false)];
    let lg = &sc.legend;
    let title = if lg.title.is_empty() { project.to_string() } else { lg.title.clone() };
    let pad = 2.5;
    if sheet.landscape {
        // Spalte von oben nach unten; unten Maßstab und Nordpfeil
        let mut f = Flow { x: sx + pad, w: sw - 2.0 * pad, y: sy + sh - pad };
        flow_lines(&mut c, &mut f, &[(project.to_string(), true)], 3.2);
        if title != project {
            flow_lines(&mut c, &mut f, &[(title, true)], 2.6);
        }
        for blk in &lg.blocks {
            match blk {
                Block::Lines(v) => flow_lines(&mut c, &mut f, v, 2.1),
                Block::Bar { caption, stops, ticks } => flow_bar(&mut c, &mut f, caption, stops, ticks, false),
                Block::Swatches { caption, items } => flow_swatches(&mut c, &mut f, caption, items, false),
            }
        }
        let cx = sx + sw / 2.0;
        north_arrow(&mut c, cx, sy + 44.0, 12.0);
        c.text(cx, sy + 30.0, 2.8, true, BLACK, 0.0, 0.5, 0.0, &info[0].0, false);
        c.text(cx, sy + 25.0, 2.1, false, BLACK, 0.0, 0.5, 0.0, &info[1].0, false);
        scale_bar(&mut c, sx + pad + 2.0, sy + 14.0, n, (sw - 2.0 * pad - 6.0).min(45.0));
        c.text(cx, sy + 4.0, 1.7, false, GREY, 0.0, 0.5, 0.0, "DGM-NT", false);
    } else {
        // drei Spalten: Titel/Text | Legende | Maßstab, Nordpfeil
        let (wa, wb) = (sw * 0.38, sw * 0.34);
        let (xa, xb, xc) = (sx + pad, sx + wa + pad, sx + wa + wb + pad);
        let top = sy + sh - pad;
        let limit = sy + 2.0;
        let mut fa = Flow { x: xa, w: wa - 2.0 * pad, y: top };
        let mut fb = Flow { x: xb, w: wb - 2.0 * pad, y: top };
        flow_lines(&mut c, &mut fa, &[(project.to_string(), true)], 3.2);
        if title != project {
            flow_lines(&mut c, &mut fa, &[(title, true)], 2.6);
        }
        // Legendenblöcke auf die zwei Spalten verteilen: dorthin, wo noch genug Platz im Streifen ist
        for blk in &lg.blocks {
            let need = |f: &Flow| match blk {
                Block::Lines(v) => lines_height(v, 2.1, f.w),
                Block::Bar { .. } => BAR_H,
                Block::Swatches { items, .. } => swatches_height(items, f.w),
            };
            let (fits_a, fits_b) = (fa.y - need(&fa) >= limit, fb.y - need(&fb) >= limit);
            let use_b = match (fits_a, fits_b) {
                (true, false) => false,
                (false, true) => true,
                _ => fb.y >= fa.y,
            };
            let f = if use_b { &mut fb } else { &mut fa };
            match blk {
                Block::Lines(v) => flow_lines(&mut c, f, v, 2.1),
                Block::Bar { caption, stops, ticks } => flow_bar(&mut c, f, caption, stops, ticks, true),
                Block::Swatches { caption, items } => {
                    let two = swatch_two_cols(items, f.w);
                    flow_swatches(&mut c, f, caption, items, two)
                }
            }
        }
        c.stroke_color([200, 200, 200]);
        c.line_width(0.15);
        for x in [xb - pad, xc - pad] {
            c.polyline(&[(x, sy + 2.0), (x, sy + sh - 2.0)], false);
        }
        let wc = sw - wa - wb - 2.0 * pad;
        north_arrow(&mut c, xc + wc - 8.0, sy + sh - 14.0, 12.0);
        c.text(xc, sy + sh - 6.0, 2.8, true, BLACK, 0.0, 0.0, 0.0, &info[0].0, false);
        c.text(xc, sy + sh - 11.0, 2.1, false, BLACK, 0.0, 0.0, 0.0, &info[1].0, false);
        scale_bar(&mut c, xc + 1.0, sy + 12.0, n, (wc - 6.0).min(40.0));
        c.text(xc + wc, sy + 2.5, 1.7, false, GREY, 0.0, 1.0, 0.0, "DGM-NT", false);
    }
    let _ = p;
    pdf::build(&c, w, h, &format!("{} – Druckausgabe", project))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{contours, tin};

    fn demo_scene(dxf: bool) -> (Scene, Project, [f64; 4]) {
        let mut pr = Project::demo();
        let (t, _) = tin::triangulate(&pr);
        pr.st.triangles = t;
        let s = Surface::new(&pr, true);
        let c = contours::compute(&s, &pr.st.params);
        let fill = FillCache::build(&s, render::FillMode::Height, true, 2, (s.zmin, s.zmax));
        let layers = Layers::default();
        let pal = Palette::default();
        let cmp = Compare::default();
        let b = pr.bbox().unwrap();
        let sc = build(&Input {
            project: &pr,
            surface: Some(&s),
            contours: Some(&c),
            fill: fill.as_ref(),
            layers: &layers,
            fill_mode: 1,
            palette: &pal,
            cmp: &cmp,
            a_name: "Beispiel",
            region: b,
            scale_n: 1000.0,
            dxf,
            dxf_fills: true,
        });
        (sc, pr, b)
    }

    #[test]
    fn scene_has_all_element_kinds() {
        let (sc, _, _) = demo_scene(false);
        let n = |f: fn(&Item) -> bool| sc.items.iter().filter(|i| f(i)).count();
        assert!(n(|i| matches!(i, Item::Mesh { .. })) >= 1);
        assert!(n(|i| matches!(i, Item::Circle { .. })) > 100);
        assert!(n(|i| matches!(i, Item::Line { .. })) > 20);
        assert!(n(|i| matches!(i, Item::Text { .. })) > 10);
        assert!(!sc.legend.blocks.is_empty());
    }

    #[test]
    fn dxf_uses_parameters() {
        let (sc, pr, _) = demo_scene(true);
        let mut p = pr.st.params.clone();
        p.exp_circle_mm = 0.8;
        p.layers[0] = 77;
        let dxf = to_dxf(&sc, &p, 500.0);
        assert!(dxf.contains("CIRCLE"));
        assert!(dxf.contains("POLYLINE"));
        assert!(dxf.contains("TEXT"));
        assert!(dxf.contains("SOLID"));
        assert!(dxf.contains("\n8\n77\n"), "Ebene aus der Parameterdatei");
        // Kreisradius = 0.6 mm (Vorgabe der Szene) × 500/1000 = 0.3 m
        assert!(dxf.contains("40\n0.3000\n"));
        assert!(dxf.ends_with("0\nEOF\n"));
    }

    #[test]
    fn pdf_both_orientations() {
        let (sc, pr, b) = demo_scene(false);
        for landscape in [false, true] {
            let sheet = Sheet::new(landscape);
            let pdf = render_pdf(&sc, &sheet, [b[0], b[1]], 1000.0, &pr.st.params, "Beispiel");
            assert!(pdf.starts_with(b"%PDF-1.5") && pdf.ends_with(b"%%EOF\n"));
            assert!(pdf.len() > 5000);
        }
        let s = Sheet::new(false);
        let (x, y, w, h) = s.area();
        assert_eq!((x, w), (10.0, 190.0)); // links/rechts 10 mm
        assert_eq!(y, 55.0); // unten 10 + 45
        assert_eq!(h, 297.0 - 10.0 - 55.0);
        let s = Sheet::new(true);
        let (x, y, w, h) = s.area();
        assert_eq!((x, y, h), (10.0, 10.0, 190.0));
        assert_eq!(w, 297.0 - 10.0 - 70.0); // rechts 10 + 60
    }
}

#[cfg(test)]
mod pdf_content_tests {
    use super::*;
    use crate::{contours, tin};

    /// Seiteninhalt (Objekt 4) entpacken und alle Textstrings (hex, WinAnsi) auslesen.
    fn page_texts(pdf: &[u8]) -> (String, Vec<String>) {
        let find = |hay: &[u8], needle: &[u8], from: usize| hay[from..].windows(needle.len()).position(|w| w == needle).map(|p| p + from);
        let o4 = find(pdf, b"\n4 0 obj\n", 0).unwrap();
        let s = find(pdf, b"stream\n", o4).unwrap() + 7;
        let e = find(pdf, b"\nendstream", s).unwrap();
        let raw = miniz_oxide::inflate::decompress_to_vec_zlib(&pdf[s..e]).expect("Seiteninhalt entpackbar");
        let text = String::from_utf8_lossy(&raw).to_string();
        let mut out = Vec::new();
        for part in text.split(" Tj") {
            if let Some(a) = part.rfind('<') {
                let hex = part[a + 1..].trim_end_matches('>');
                let bytes: Vec<u8> = (0..hex.len() / 2).filter_map(|i| u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).ok()).collect();
                out.push(bytes.iter().map(|&b| b as char).collect::<String>());
            }
        }
        (text, out)
    }

    #[test]
    fn pdf_page_contains_legend_scale_north_and_clip() {
        let mut pr = Project::demo();
        let (t, _) = tin::triangulate(&pr);
        pr.st.triangles = t;
        let s = Surface::new(&pr, true);
        let c = contours::compute(&s, &pr.st.params);
        let fill = FillCache::build(&s, render::FillMode::Height, true, 2, (s.zmin, s.zmax));
        let (layers, pal, cmp) = (Layers::default(), Palette::default(), Compare::default());
        let b = pr.bbox().unwrap();
        let inp = Input { project: &pr, surface: Some(&s), contours: Some(&c), fill: fill.as_ref(), layers: &layers, fill_mode: 1, palette: &pal, cmp: &cmp, a_name: "Beispiel", region: b, scale_n: 2000.0, dxf: false, dxf_fills: true };
        let sc = build(&inp);
        for landscape in [false, true] {
            let sheet = Sheet::new(landscape);
            let (ax, ay, aw, ah) = sheet.area();
            let pdf = render_pdf(&sc, &sheet, [b[0], b[1]], 2000.0, &pr.st.params, "Beispiel");
            let (content, texts) = page_texts(&pdf);
            // Zeichenfläche wird beschnitten
            assert!(content.contains(&format!("{} {} {} {} re W n", ax, ay, aw, ah)), "Clip-Rechteck {:?}", (ax, ay, aw, ah));
            // Maßstab, Datum, Titel, Nordpfeil und Legende
            assert!(texts.iter().any(|t| t == "Maßstab 1:2.000"), "{:?}", texts.iter().filter(|t| t.contains("Ma")).collect::<Vec<_>>());
            assert!(texts.iter().any(|t| t.starts_with("Stand: ")));
            assert!(texts.iter().any(|t| t == "N"));
            assert!(texts.iter().any(|t| t == "Beispiel"));
            assert!(texts.iter().any(|t| t == "Höhe in m"));
            assert!(texts.iter().any(|t| t.starts_with("Höhenlinien alle")));
            assert!(texts.iter().any(|t| t.ends_with(" m")), "Maßstabsleiste mit Längenangabe");
            // Farbflächen als Vektor-Shading
            assert!(content.contains(" sh Q"));
            // Raster-Bilder gibt es nicht
            assert!(!String::from_utf8_lossy(&pdf).contains("/Subtype /Image"));
        }
    }
}

#[cfg(test)]
mod pdf_parser_tests {
    use super::*;
    use crate::{contours, tin};
    use lopdf::content::Content as LoContent;
    use lopdf::Document;

    /// Die erzeugte Datei muss sich von einem unabhängigen PDF-Parser einlesen lassen: Aufbau, Seite,
    /// Schriften, Shadings und jede Operation des Seiteninhalts mit der richtigen Operandenzahl.
    #[test]
    fn independent_parser_accepts_the_pdf() {
        let mut pr = Project::demo();
        let (t, _) = tin::triangulate(&pr);
        pr.st.triangles = t;
        let s = Surface::new(&pr, true);
        let c = contours::compute(&s, &pr.st.params);
        let fill = FillCache::build(&s, render::FillMode::Height, true, 2, (s.zmin, s.zmax));
        let (layers, pal, cmp) = (Layers::default(), Palette::default(), Compare::default());
        let b = pr.bbox().unwrap();
        let inp = Input { project: &pr, surface: Some(&s), contours: Some(&c), fill: fill.as_ref(), layers: &layers, fill_mode: 1, palette: &pal, cmp: &cmp, a_name: "Beispiel", region: b, scale_n: 1500.0, dxf: false, dxf_fills: true };
        let sc = build(&inp);
        for landscape in [false, true] {
            let sheet = Sheet::new(landscape);
            let bytes = render_pdf(&sc, &sheet, [b[0], b[1]], 1500.0, &pr.st.params, "Beispiel");
            let doc = Document::load_mem(&bytes).expect("PDF lässt sich einlesen");
            let pages = doc.get_pages();
            assert_eq!(pages.len(), 1);
            let page_id = *pages.values().next().unwrap();
            let page = doc.get_dictionary(page_id).unwrap();
            let mb: Vec<f32> = page.get(b"MediaBox").unwrap().as_array().unwrap().iter().map(|o| o.as_float().unwrap()).collect();
            let (w, h) = sheet.size();
            assert!((mb[2] - (w * 72.0 / 25.4) as f32).abs() < 0.1 && (mb[3] - (h * 72.0 / 25.4) as f32).abs() < 0.1, "A4-Größe {:?}", mb);
            let data = doc.get_page_content(page_id);
            let ops = LoContent::decode(&data).expect("Seiteninhalt syntaktisch gültig").operations;
            let need = |op: &str| -> Option<usize> {
                Some(match op {
                    "m" | "l" | "Td" => 2,
                    "re" | "k" | "K" => 4,
                    "c" | "cm" | "Tm" => 6,
                    "rg" | "RG" => 3,
                    "w" | "j" | "J" | "Tr" => 1,
                    "d" | "Tf" => 2,
                    "Tj" | "sh" => 1,
                    "q" | "Q" | "W" | "n" | "S" | "s" | "f" | "B" | "BT" | "ET" => 0,
                    _ => return None,
                })
            };
            let (mut q, mut bt) = (0i32, 0i32);
            for o in &ops {
                let n = need(&o.operator).unwrap_or_else(|| panic!("unbekannte Operation {}", o.operator));
                assert_eq!(o.operands.len(), n, "Operandenzahl von {}", o.operator);
                match o.operator.as_str() {
                    "q" => q += 1,
                    "Q" => q -= 1,
                    "BT" => bt += 1,
                    "ET" => bt -= 1,
                    _ => {}
                }
                assert!(q >= 0 && (0..=1).contains(&bt));
            }
            assert_eq!((q, bt), (0, 0), "q/Q und BT/ET ausgewogen");
            // Shadings (Farbflächen) liegen als Typ-4-Netze vor und lassen sich entpacken
            let mut shadings = 0;
            for (_, obj) in doc.objects.iter() {
                if let Ok(st) = obj.as_stream() {
                    if st.dict.get(b"ShadingType").ok().and_then(|v| v.as_i64().ok()) == Some(4) {
                        let data = st.decompressed_content().expect("Shading entpackbar");
                        assert_eq!(data.len() % 12, 0);
                        assert_eq!(data.len() / 12 % 3, 0, "ganze Dreiecke");
                        shadings += 1;
                    }
                }
            }
            assert!(shadings >= 1);
        }
    }
}

#[cfg(test)]
mod grid_tests {
    use super::*;
    use crate::{contours, tin};

    /// Die Gitterkreuze müssen den ganzen Ausschnitt abdecken (oben bis unten, links bis rechts).
    #[test]
    fn grid_covers_the_whole_region() {
        let mut pr = Project::demo();
        let (t, _) = tin::triangulate(&pr);
        pr.st.triangles = t;
        let s = Surface::new(&pr, true);
        let c = contours::compute(&s, &pr.st.params);
        let (layers, pal, cmp) = (Layers::default(), Palette::default(), Compare::default());
        let b = pr.bbox().unwrap();
        for n in [500.0, 1000.0, 2500.0] {
            // Zeichenfläche von A4 quer (217 × 190 mm) im Maßstab 1:n gehört dazu
            let sheet_region = [b[0], b[1], b[0] + 217.0 * n / 1000.0, b[1] + 190.0 * n / 1000.0];
            for region in [b, [b[0] - 80.0, b[1] - 230.0, b[2] + 80.0, b[3] + 230.0], sheet_region] {
                let inp = Input { project: &pr, surface: Some(&s), contours: Some(&c), fill: None, layers: &layers, fill_mode: 0, palette: &pal, cmp: &cmp, a_name: "x", region, scale_n: n, dxf: true, dxf_fills: false };
                let sc = build(&inp);
                let step = grid_step(pr.st.params.grid_mm, n, (region[2] - region[0]).min(region[3] - region[1]));
                let ys: Vec<f64> = sc.items.iter().filter_map(|i| match i { Item::Line { layer: 1, pts, .. } if (pts[0][1] - pts[1][1]).abs() < 1e-9 => Some(pts[0][1]), _ => None }).collect();
                let (lo, hi) = (ys.iter().cloned().fold(f64::MAX, f64::min), ys.iter().cloned().fold(f64::MIN, f64::max));
                assert!(lo < region[1] + step + 1e-6 && hi > region[3] - step - 1e-6, "N {} Gitter y {}..{} in {}..{} (Schritt {})", n, lo, hi, region[1], region[3], step);
                let mut rows = ys.clone();
                rows.sort_by(|a, b| a.partial_cmp(b).unwrap());
                rows.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
                assert!(rows.len() >= 3, "N {}: nur {} Gitterzeilen (Schritt {}, Höhe {})", n, rows.len(), step, region[3] - region[1]);
            }
        }
    }
}

#[cfg(test)]
mod dxf_label_tests {
    use super::*;
    use crate::{contours, tin};

    /// DXF: Punktnummer rechts oberhalb, Höhe rechts vom Punkt, jeweils auf einer eigenen Ebene; alle Punkte.
    #[test]
    fn dxf_point_labels_on_own_layers() {
        let mut pr = Project::demo();
        let (t, _) = tin::triangulate(&pr);
        pr.st.triangles = t;
        let s = Surface::new(&pr, true);
        let c = contours::compute(&s, &pr.st.params);
        let (layers, pal, cmp) = (Layers::default(), Palette::default(), Compare::default());
        let b = pr.bbox().unwrap();
        // grober Maßstab: auf dem Papier wäre kein Platz für die Texte, das DXF schreibt sie trotzdem
        let n = 5000.0;
        let inp = Input { project: &pr, surface: Some(&s), contours: Some(&c), fill: None, layers: &layers, fill_mode: 0, palette: &pal, cmp: &cmp, a_name: "x", region: b, scale_n: n, dxf: true, dxf_fills: false };
        let sc = build(&inp);
        let p = &pr.st.params;
        let (nr, hh): (Vec<_>, Vec<_>) = {
            let all: Vec<&Item> = sc.items.iter().collect();
            (
                all.iter().filter_map(|i| match i { Item::Text { layer: 2, pos, text, ax, ay, .. } => Some((pos, text.clone(), *ax, *ay)), _ => None }).collect(),
                all.iter().filter_map(|i| match i { Item::Text { layer: 3, pos, text, ax, ay, .. } => Some((pos, text.clone(), *ax, *ay)), _ => None }).collect(),
            )
        };
        let active = pr.st.points.iter().filter(|q| q.active).count();
        assert_eq!(nr.len(), active, "jeder Punkt hat eine Nummer");
        assert_eq!(hh.len(), active, "jeder Punkt hat eine Höhe");
        let k = n / 1000.0;
        let q0 = &pr.st.points[0];
        let nr0 = nr.iter().find(|t| t.1 == q0.name).unwrap();
        // Nummer: rechts (linksbündig) und oberhalb des Punkts
        assert!(nr0.0[0] > q0.x && nr0.0[1] > q0.y && nr0.2 == 0.0 && nr0.3 == 0.0);
        let h0 = hh.iter().find(|t| t.1 == format!("{:.*}", p.decimals_h as usize, q0.z) && (t.0[0] - (q0.x + 0.9 * k)).abs() < 1e-9 && (t.0[1] - q0.y).abs() < 1e-9);
        assert!(h0.is_some(), "Höhe rechts vom Punkt, mittig");
        // Ebenen mit den Nummern der Parameterdatei im DXF, auch in der Ebenentabelle
        let mut pp = p.clone();
        pp.layers[2] = 31;
        pp.layers[3] = 32;
        let dxf = to_dxf(&sc, &pp, n);
        assert!(dxf.contains("0\nLAYER\n2\n31\n") && dxf.contains("0\nLAYER\n2\n32\n"), "Ebenentabelle");
        assert!(dxf.matches("0\nTEXT\n8\n31\n").count() == active && dxf.matches("0\nTEXT\n8\n32\n").count() == active);
    }
}

#[cfg(test)]
mod strip_layout_tests {
    use super::*;
    use crate::{contours, tin};
    use lopdf::content::Content as LoContent;
    use lopdf::{Document, Object};

    fn num(o: &Object) -> f64 {
        o.as_float().map(|v| v as f64).or_else(|_| o.as_i64().map(|v| v as f64)).unwrap_or(f64::NAN)
    }

    /// Schwerste Legende (Volumen mit Rollen, Zahlen und acht Farbfeldern) wie bei der Analyse.
    fn heavy_legend() -> Legend {
        let sw = |i: usize| -> Rgb { [(30 * i) as u8, 100, 200 - (20 * i) as u8] };
        Legend {
            title: "Auftrag / Abtrag".into(),
            blocks: vec![
                Block::Lines(vec![("Bestand: Test-Oben".into(), false), ("Neu: Test-Unten".into(), false)]),
                Block::Lines(vec![
                    ("Volumen (Dreiecksprismen)".into(), true),
                    ("Abtrag Original: 27 291.0 m³".into(), false),
                    ("Abtrag aufgelockert (× 1.10): 30 020.1 m³".into(), false),
                    ("Auftrag Original: 28 375.0 m³".into(), false),
                    ("Auftrag aufgelockert (× 1.50): 42 562.6 m³".into(), false),
                    ("Saldo Original: 1 084.0 m³".into(), false),
                ]),
                Block::Swatches { caption: "Dicke in m".into(), items: (0..8).map(|i| (sw(i), format!("{} 0.5 – 1.0 m", if i < 4 { "Auftrag" } else { "Abtrag" }))).collect() },
            ],
        }
    }

    fn scene(heavy: bool) -> (Scene, Project, [f64; 4]) {
        let mut pr = Project::demo();
        let (t, _) = tin::triangulate(&pr);
        pr.st.triangles = t;
        let s = Surface::new(&pr, true);
        let c = contours::compute(&s, &pr.st.params);
        let fill = FillCache::build(&s, render::FillMode::Height, true, 2, (s.zmin, s.zmax));
        let (layers, pal, cmp) = (Layers::default(), Palette::default(), Compare::default());
        let b = pr.bbox().unwrap();
        let inp = Input { project: &pr, surface: Some(&s), contours: Some(&c), fill: fill.as_ref(), layers: &layers, fill_mode: 1, palette: &pal, cmp: &cmp, a_name: "Beispiel", region: b, scale_n: 2000.0, dxf: false, dxf_fills: true };
        let mut sc = build(&inp);
        if heavy {
            sc.legend = heavy_legend();
        }
        (sc, pr, b)
    }

    #[test]
    fn legend_scale_and_north_stay_inside_the_strip() {
        for heavy in [false, true] {
            let (sc, pr, b) = scene(heavy);
            for landscape in [false, true] {
                let sheet = Sheet::new(landscape);
                let (sx, sy, sw, sh) = sheet.strip_rect();
                let (ax, ay, aw, ah) = sheet.area();
                // Streifen und Zeichenfläche überlappen nicht, beide liegen innerhalb der Ränder
                assert!(if landscape { sx >= ax + aw } else { sy + sh <= ay }, "Streifen neben/unter der Zeichenfläche {:?} {:?}", (sx, sy, sw, sh), (ax, ay, aw, ah));
                let (w, h) = sheet.size();
                assert!(sx >= sheet.margin - 1e-9 && sy >= sheet.margin - 1e-9 && sx + sw <= w - sheet.margin + 1e-9 && sy + sh <= h - sheet.margin + 1e-9);
                let pdf = render_pdf(&sc, &sheet, [b[0], b[1]], 2000.0, &pr.st.params, "Beispiel");
                let doc = Document::load_mem(&pdf).unwrap();
                let pid = *doc.get_pages().values().next().unwrap();
                let ops = LoContent::decode(&doc.get_page_content(pid)).unwrap().operations;
                // ab hier folgt alles hinter dem Beschnitt der Zeichenfläche (Rahmen, Streifen, Legende)
                let (mut depth, mut started, mut after) = (0i32, false, 0usize);
                for (i, o) in ops.iter().enumerate() {
                    match o.operator.as_str() {
                        "q" => {
                            depth += 1;
                            started = true;
                        }
                        "Q" => depth -= 1,
                        _ => {}
                    }
                    if started && depth == 0 {
                        after = i + 1;
                        break;
                    }
                }
                assert!(after > 0);
                let (mut texts, mut rects, mut lowest) = (0, 0, f64::MAX);
                for o in &ops[after..] {
                    match o.operator.as_str() {
                        "Tm" => {
                            let (x, y) = (num(&o.operands[4]), num(&o.operands[5]));
                            texts += 1;
                            lowest = lowest.min(y);
                            assert!(x >= sx - 1e-6 && x <= sx + sw + 1e-6 && y >= sy - 1e-6 && y <= sy + sh + 1e-6, "Text bei ({:.1}, {:.1}) außerhalb des Streifens {:?} (heavy {}, quer {})", x, y, (sx, sy, sw, sh), heavy, landscape);
                        }
                        "re" => {
                            let r = [num(&o.operands[0]), num(&o.operands[1]), num(&o.operands[2]), num(&o.operands[3])];
                            rects += 1;
                            // Zeichenflächen-Rahmen oder etwas im Streifen
                            let is_frame = (r[0] - ax).abs() < 1e-6 && (r[1] - ay).abs() < 1e-6;
                            assert!(is_frame || (r[0] >= sx - 1e-6 && r[1] >= sy - 1e-6 && r[0] + r[2] <= sx + sw + 1e-6 && r[1] + r[3] <= sy + sh + 1e-6), "Rechteck {:?} außerhalb des Streifens", r);
                        }
                        _ => {}
                    }
                }
                assert!(texts >= 8 && rects >= 2, "Legende, Maßstab und Nordpfeil vorhanden ({} Texte)", texts);
                // gesamter Textblock hat noch Luft nach unten (mind. 1 mm über dem Streifenrand)
                assert!(lowest >= sy + 1.0 - 1e-6, "tiefster Text y {:.1} im Streifen ab {:.1}", lowest, sy);
            }
        }
    }
}
