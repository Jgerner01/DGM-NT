//! Menü „Analyse“: ein zweites Geländemodell einlesen, die Überschneidung in x/y prüfen und
//! Auftrags-/Abtragsvolumen sowie den Höhenunterschied beider Modelle rechnen und zeigen.
//!
//! Verfahren (Vorgabe des Anwenders):
//! 1. Gemeinsamer Umring: die Fläche, die in beiden Dreiecksnetzen liegt.
//! 2. Beide Netze werden überlagert: jede Teilfläche liegt in genau einem Dreieck jedes Modells,
//!    die Höhen kommen also linear aus dem jeweiligen Modell (Punkte des einen Modells werden im
//!    anderen interpoliert). Bruchkanten sind Dreieckskanten und bleiben dadurch erhalten.
//! 3. Extrapolation nur, wo die Ränder beider Modelle weniger als 10 cm auseinander liegen:
//!    dort wird das Randdreieck um höchstens 10 cm über den Rand hinaus fortgesetzt.
//! 4. Schnittkante: wo sich die Flächen schneiden (dz = 0), wird jede Teilfläche geteilt.
//! 5. Volumen: exakte Prismen über den Teilflächen; dz = Neu − Bestand ist darin linear.
//!    Auftrag = dz > 0 (Neu über Bestand), Abtrag = dz < 0. Auflockerung aus den DGM-Parametern.

use std::collections::HashMap;
use std::path::Path;

use egui::{Align2, Color32, FontId, Mesh, Pos2, Rect, Shape, Stroke, StrokeKind, Ui, Vec2};

use crate::contours::{self, Contours};
use crate::files;
use crate::model::{Params, Project};
use crate::render::{self, ContourStyle};
use crate::tin;
use crate::view::{nice_step, View};

/// Größter Abstand der Ränder, bis zu dem extrapoliert wird (m).
pub const EXTRAP_MAX: f64 = 0.10;
const EPS_AREA: f64 = 1e-9;

type P2 = [f64; 2];
type Poly = Vec<P2>;
/// lineare Funktion f(x, y) = c0 + c1·x + c2·y (Ebene oder Höhenunterschied)
type Lin = [f64; 3];

fn lin(f: &Lin, q: P2) -> f64 {
    f[0] + f[1] * q[0] + f[2] * q[1]
}

fn neg(f: &Lin) -> Lin {
    [-f[0], -f[1], -f[2]]
}

fn plane_of(a: [f64; 3], b: [f64; 3], c: [f64; 3]) -> Lin {
    let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    let n = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
    let p1 = -n[0] / n[2];
    let p2 = -n[1] / n[2];
    [a[2] - p1 * a[0] - p2 * a[1], p1, p2]
}

// ------------------------------------------------------------------ Polygone (konvex, gegen den Uhrzeigersinn)

fn area(p: &[P2]) -> f64 {
    if p.len() < 3 {
        return 0.0;
    }
    let o = p[0];
    let mut s = 0.0;
    for i in 1..p.len() - 1 {
        let (a, b) = (p[i], p[i + 1]);
        s += (a[0] - o[0]) * (b[1] - o[1]) - (b[0] - o[0]) * (a[1] - o[1]);
    }
    s * 0.5
}

fn centroid(p: &[P2]) -> P2 {
    let o = p[0];
    let (mut cx, mut cy, mut a2) = (0.0, 0.0, 0.0);
    for i in 1..p.len().saturating_sub(1) {
        let (a, b) = ([p[i][0] - o[0], p[i][1] - o[1]], [p[i + 1][0] - o[0], p[i + 1][1] - o[1]]);
        let cr = a[0] * b[1] - b[0] * a[1];
        a2 += cr;
        cx += (a[0] + b[0]) * cr;
        cy += (a[1] + b[1]) * cr;
    }
    if a2.abs() < 1e-24 {
        let n = p.len() as f64;
        return [p.iter().map(|q| q[0]).sum::<f64>() / n, p.iter().map(|q| q[1]).sum::<f64>() / n];
    }
    [o[0] + cx / (3.0 * a2), o[1] + cy / (3.0 * a2)]
}

fn poly_bbox(p: &[P2]) -> [f64; 4] {
    let mut b = [f64::MAX, f64::MAX, f64::MIN, f64::MIN];
    for q in p {
        b[0] = b[0].min(q[0]);
        b[1] = b[1].min(q[1]);
        b[2] = b[2].max(q[0]);
        b[3] = b[3].max(q[1]);
    }
    b
}

fn bbox_overlap(a: [f64; 4], b: [f64; 4]) -> bool {
    a[0] <= b[2] && b[0] <= a[2] && a[1] <= b[3] && b[1] <= a[3]
}

/// Teil des Polygons mit f ≥ 0 (Sutherland–Hodgman).
fn clip_half(p: &[P2], f: &Lin) -> Poly {
    let n = p.len();
    let mut out = Vec::with_capacity(n + 2);
    for i in 0..n {
        let (a, b) = (p[i], p[(i + 1) % n]);
        let (fa, fb) = (lin(f, a), lin(f, b));
        if fa >= 0.0 {
            out.push(a);
        }
        if (fa >= 0.0) != (fb >= 0.0) {
            let t = fa / (fa - fb);
            out.push([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]);
        }
    }
    out
}

/// Halbebene links der Kante u → v (normiert, f = Abstand in m).
fn edge_lin(u: P2, v: P2) -> Lin {
    let l = (v[0] - u[0]).hypot(v[1] - u[1]).max(1e-300);
    let c1 = -(v[1] - u[1]) / l;
    let c2 = (v[0] - u[0]) / l;
    [-(c1 * u[0] + c2 * u[1]), c1, c2]
}

fn clip_convex(p: &[P2], q: &[P2]) -> Poly {
    let mut r = p.to_vec();
    for i in 0..q.len() {
        r = clip_half(&r, &edge_lin(q[i], q[(i + 1) % q.len()]));
        if r.len() < 3 {
            return Vec::new();
        }
    }
    r
}

/// p ohne q, als konvexe Teile.
fn subtract_convex(p: &[P2], q: &[P2]) -> Vec<Poly> {
    if !bbox_overlap(poly_bbox(p), poly_bbox(q)) {
        return vec![p.to_vec()];
    }
    let mut rest = p.to_vec();
    let mut out = Vec::new();
    for i in 0..q.len() {
        let e = edge_lin(q[i], q[(i + 1) % q.len()]);
        let outside = clip_half(&rest, &neg(&e));
        if area(&outside) > EPS_AREA {
            out.push(outside);
        }
        rest = clip_half(&rest, &e);
        if area(&rest) <= EPS_AREA {
            return out;
        }
    }
    out
}

fn seg_dist(p: P2, a: P2, b: P2) -> f64 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let l2 = dx * dx + dy * dy;
    let t = if l2 > 0.0 { (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / l2).clamp(0.0, 1.0) } else { 0.0 };
    (p[0] - a[0] - t * dx).hypot(p[1] - a[1] - t * dy)
}

/// Parameter t ∈ (0,1) auf p→q, an dem die Strecke c→d geschnitten wird.
fn seg_cross(p: P2, q: P2, c: P2, d: P2) -> Option<f64> {
    let r = [q[0] - p[0], q[1] - p[1]];
    let s = [d[0] - c[0], d[1] - c[1]];
    let den = r[0] * s[1] - r[1] * s[0];
    if den.abs() < 1e-15 {
        return None;
    }
    let w = [c[0] - p[0], c[1] - p[1]];
    let t = (w[0] * s[1] - w[1] * s[0]) / den;
    let u = (w[0] * r[1] - w[1] * r[0]) / den;
    (t > 0.0 && t < 1.0 && (-1e-12..=1.0 + 1e-12).contains(&u)).then_some(t)
}

// ------------------------------------------------------------------ Dreiecksnetz (linear)

struct Tin {
    pts: Vec<[f64; 3]>,
    tris: Vec<[usize; 3]>,
    planes: Vec<Lin>,
    grid: HashMap<(i64, i64), Vec<u32>>,
    cell: f64,
    bbox: [f64; 4],
    /// Randkanten (von, nach, Dreieck); das Innere liegt links
    border: Vec<(usize, usize, usize)>,
    area: f64,
}

impl Tin {
    /// Aktive Dreiecke eines Projekts in lokalen Koordinaten (Welt − o).
    fn new(pr: &Project, o: P2) -> Option<Tin> {
        let pts: Vec<[f64; 3]> = pr.st.points.iter().map(|p| [p.x - o[0], p.y - o[1], p.z]).collect();
        let mut tris = Vec::new();
        let mut planes = Vec::new();
        for t in pr.st.triangles.iter().filter(|t| t.active && t.v.iter().all(|&i| i < pts.len())) {
            let [a, b, c] = t.v;
            let cr = (pts[b][0] - pts[a][0]) * (pts[c][1] - pts[a][1]) - (pts[b][1] - pts[a][1]) * (pts[c][0] - pts[a][0]);
            if cr.abs() < 1e-12 {
                continue;
            }
            let v = if cr < 0.0 { [a, c, b] } else { [a, b, c] };
            planes.push(plane_of(pts[v[0]], pts[v[1]], pts[v[2]]));
            tris.push(v);
        }
        if tris.is_empty() {
            return None;
        }
        let mut bbox = [f64::MAX, f64::MAX, f64::MIN, f64::MIN];
        let mut total = 0.0;
        for t in &tris {
            let p: Vec<P2> = t.iter().map(|&i| [pts[i][0], pts[i][1]]).collect();
            let b = poly_bbox(&p);
            bbox = [bbox[0].min(b[0]), bbox[1].min(b[1]), bbox[2].max(b[2]), bbox[3].max(b[3])];
            total += area(&p);
        }
        let cell = ((total / tris.len() as f64).sqrt() * 1.5).max(1e-3);
        let mut tin = Tin { pts, tris, planes, grid: HashMap::new(), cell, bbox, border: Vec::new(), area: total };
        for ti in 0..tin.tris.len() {
            let b = poly_bbox(&tin.tri_poly(ti));
            let (i0, j0, i1, j1) = tin.cell_range(b);
            for i in i0..=i1 {
                for j in j0..=j1 {
                    tin.grid.entry((i, j)).or_default().push(ti as u32);
                }
            }
        }
        let mut edges: HashMap<(usize, usize), (u32, (usize, usize, usize))> = HashMap::new();
        for (ti, t) in tin.tris.iter().enumerate() {
            for e in 0..3 {
                let (a, b) = (t[e], t[(e + 1) % 3]);
                let en = edges.entry((a.min(b), a.max(b))).or_insert((0, (a, b, ti)));
                en.0 += 1;
            }
        }
        tin.border = edges.values().filter(|(n, _)| *n == 1).map(|(_, e)| *e).collect();
        tin.border.sort_unstable();
        Some(tin)
    }

    fn cell_range(&self, b: [f64; 4]) -> (i64, i64, i64, i64) {
        (
            (b[0] / self.cell).floor() as i64,
            (b[1] / self.cell).floor() as i64,
            (b[2] / self.cell).floor() as i64,
            (b[3] / self.cell).floor() as i64,
        )
    }

    fn query(&self, b: [f64; 4]) -> Vec<usize> {
        let (i0, j0, i1, j1) = self.cell_range(b);
        if (i1 - i0 + 1) * (j1 - j0 + 1) > 4_000_000 {
            return (0..self.tris.len()).collect();
        }
        let mut v: Vec<usize> = Vec::new();
        for i in i0..=i1 {
            for j in j0..=j1 {
                if let Some(l) = self.grid.get(&(i, j)) {
                    v.extend(l.iter().map(|&t| t as usize));
                }
            }
        }
        v.sort_unstable();
        v.dedup();
        v
    }

    fn xy(&self, i: usize) -> P2 {
        [self.pts[i][0], self.pts[i][1]]
    }

    fn tri_poly(&self, t: usize) -> Poly {
        self.tris[t].iter().map(|&i| self.xy(i)).collect()
    }

    fn locate(&self, q: P2) -> Option<usize> {
        let l = self.grid.get(&((q[0] / self.cell).floor() as i64, (q[1] / self.cell).floor() as i64))?;
        for &ti in l {
            let t = self.tris[ti as usize];
            let (a, b, c) = (self.xy(t[0]), self.xy(t[1]), self.xy(t[2]));
            let det = (b[1] - c[1]) * (a[0] - c[0]) + (c[0] - b[0]) * (a[1] - c[1]);
            let l1 = ((b[1] - c[1]) * (q[0] - c[0]) + (c[0] - b[0]) * (q[1] - c[1])) / det;
            let l2 = ((c[1] - a[1]) * (q[0] - c[0]) + (a[0] - c[0]) * (q[1] - c[1])) / det;
            let e = -1e-9;
            if l1 >= e && l2 >= e && 1.0 - l1 - l2 >= e {
                return Some(ti as usize);
            }
        }
        None
    }

    fn border_segs(&self) -> Vec<(P2, P2)> {
        self.border.iter().map(|&(i, j, _)| (self.xy(i), self.xy(j))).collect()
    }

    fn border_dist(&self, q: P2) -> f64 {
        self.border.iter().map(|&(i, j, _)| seg_dist(q, self.xy(i), self.xy(j))).fold(f64::MAX, f64::min)
    }

    /// Randstreifen: je Randkante ein Viereck EXTRAP_MAX nach außen, Höhe aus der Ebene des Randdreiecks.
    /// Liefert je Streifen: Viereck, Ebene, Anfangspunkt der Kante, Einheitsnormale nach außen.
    fn skirts(&self) -> Vec<(Poly, Lin, P2, P2)> {
        let mut out = Vec::new();
        for &(i, j, t) in &self.border {
            let (a, b) = (self.xy(i), self.xy(j));
            let l = (b[0] - a[0]).hypot(b[1] - a[1]);
            if l < 1e-9 {
                continue;
            }
            let u = [(b[1] - a[1]) / l, -(b[0] - a[0]) / l];
            let n = [u[0] * EXTRAP_MAX, u[1] * EXTRAP_MAX];
            out.push((vec![a, [a[0] + n[0], a[1] + n[1]], [b[0] + n[0], b[1] + n[1]], b], self.planes[t], a, u));
        }
        out
    }
}

/// Teilfläche der Überlagerung: konvexes Polygon mit der Ebene von Modell A und Modell B.
struct Piece {
    poly: Poly,
    pa: Lin,
    pb: Lin,
}

/// Überlagerung beider Netze, dazu die Randstreifen (Extrapolation unter 10 cm). Liefert auch die extrapolierte Fläche.
fn overlay(a: &Tin, b: &Tin) -> (Vec<Piece>, f64) {
    let mut out = Vec::new();
    for ta in 0..a.tris.len() {
        let pa = a.tri_poly(ta);
        let bb = poly_bbox(&pa);
        for tb in b.query(bb) {
            let q = b.tri_poly(tb);
            if !bbox_overlap(bb, poly_bbox(&q)) {
                continue;
            }
            let p = clip_convex(&pa, &q);
            if area(&p) > EPS_AREA {
                out.push(Piece { poly: p, pa: a.planes[ta], pb: b.planes[tb] });
            }
        }
    }
    let mut ex = 0.0;
    ex += extend(b, a, false, &mut out);
    ex += extend(a, b, true, &mut out);
    (out, ex)
}

/// Modell `sk` über seinen Rand fortsetzen, wo das Modell `real` noch Dreiecke hat und beide Ränder
/// höchstens EXTRAP_MAX auseinander liegen.
fn extend(sk: &Tin, real: &Tin, sk_is_a: bool, out: &mut Vec<Piece>) -> f64 {
    let skirts = sk.skirts();
    let bbs: Vec<[f64; 4]> = skirts.iter().map(|s| poly_bbox(&s.0)).collect();
    let mut ex = 0.0;
    for k in 0..skirts.len() {
        let (sq, spl, ea, un) = &skirts[k];
        let bb = bbs[k];
        for tr in real.query(bb) {
            let p = clip_convex(&real.tri_poly(tr), sq);
            if area(&p) <= EPS_AREA {
                continue;
            }
            // nicht dort, wo `sk` selbst schon Dreiecke hat, und nicht doppelt mit früheren Streifen
            let mut parts = vec![p];
            for ts in sk.query(bb) {
                let q = sk.tri_poly(ts);
                parts = parts.into_iter().flat_map(|x| subtract_convex(&x, &q)).collect();
                if parts.is_empty() {
                    break;
                }
            }
            for j in 0..k {
                if parts.is_empty() {
                    break;
                }
                if bbox_overlap(bb, bbs[j]) {
                    parts = parts.into_iter().flat_map(|x| subtract_convex(&x, &skirts[j].0)).collect();
                }
            }
            for part in parts {
                let ar = area(&part);
                if ar <= EPS_AREA {
                    continue;
                }
                // nur zwischen zwei Rändern, die weniger als 10 cm auseinander liegen: vom Rand von `sk`
                // aus gesehen muss `real` innerhalb von EXTRAP_MAX nach außen enden
                let ends_before = |v: P2| {
                    let s = (v[0] - ea[0]) * un[0] + (v[1] - ea[1]) * un[1];
                    let r = EXTRAP_MAX + 1e-6 - s;
                    let end = [v[0] + un[0] * r, v[1] + un[1] * r];
                    real.locate(end).is_none() || real.border_dist(end) < 1e-6
                };
                if !part.iter().all(|&v| ends_before(v)) {
                    continue;
                }
                ex += ar;
                let (pa, pb) = if sk_is_a { (*spl, real.planes[tr]) } else { (real.planes[tr], *spl) };
                out.push(Piece { poly: part, pa, pb });
            }
        }
    }
    ex
}

/// Gemeinsamer Umring als Liste von Strecken: die Teile der Ränder beider Modelle, die den
/// gemeinsamen Bereich begrenzen (bei Rändern unter 10 cm Abstand gilt der äußere).
fn umring(a: &Tin, b: &Tin) -> Vec<(P2, P2)> {
    let mut out = Vec::new();
    umring_side(a, b, true, &mut out);
    umring_side(b, a, false, &mut out);
    out
}

fn umring_side(x: &Tin, y: &Tin, is_a: bool, out: &mut Vec<(P2, P2)>) {
    let ysegs = y.border_segs();
    for (p, q) in x.border_segs() {
        let mut ts = vec![0.0, 1.0];
        for &(c, d) in &ysegs {
            if let Some(t) = seg_cross(p, q, c, d) {
                ts.push(t);
            }
        }
        ts.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let at = |t: f64| [p[0] + (q[0] - p[0]) * t, p[1] + (q[1] - p[1]) * t];
        for w in ts.windows(2) {
            if w[1] - w[0] < 1e-9 {
                continue;
            }
            let m = at((w[0] + w[1]) * 0.5);
            let dist = ysegs.iter().map(|&(c, d)| seg_dist(m, c, d)).fold(f64::MAX, f64::min);
            let keep = if dist < 1e-6 {
                is_a // beide Ränder fallen zusammen: nur einmal
            } else if y.locate(m).is_some() {
                dist >= EXTRAP_MAX
            } else {
                dist <= EXTRAP_MAX
            };
            if keep {
                out.push((at(w[0]), at(w[1])));
            }
        }
    }
}

// ------------------------------------------------------------------ Ergebnisse

pub struct Second {
    pub pr: Project,
    pub label: String,
    pub ntris: usize,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Overview,
    Volume,
    Diff,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum MapMode {
    Off,
    Diff,
    Volume,
}

#[derive(Clone, Default)]
pub struct Stats {
    pub cut_vol: f64,
    pub fill_vol: f64,
    pub cut_area: f64,
    pub fill_area: f64,
    pub valid_area: f64,
    pub dz_min: f64,
    pub dz_max: f64,
    pub dz_mean: f64,
    /// Länge der Schnittkante (dz = 0)
    pub zero_len: f64,
}

pub struct OverlapInfo {
    pub bbox_a: [f64; 4],
    pub bbox_b: [f64; 4],
    pub inter: Option<[f64; 4]>,
    /// Fläche innerhalb des gemeinsamen Umrings
    pub overlap_area: f64,
    pub extrap_area: f64,
    pub area_a: f64,
    pub area_b: f64,
    pub umring_len: f64,
    pub pts_b_in_a: usize,
    pub pts_a_in_b: usize,
    pub pieces: usize,
}

#[derive(Clone, Copy, Default)]
pub struct ClassRow {
    pub lo: f64,
    pub hi: f64,
    pub area: f64,
    pub vol: f64,
}

/// Farbiges Dreiecksnetz in Weltkoordinaten (einmal gebaut, jedes Bild verschoben).
struct MeshCache {
    pos: Vec<P2>,
    col: Vec<Color32>,
    idx: Vec<u32>,
    bbox: [f64; 4],
}

impl MeshCache {
    fn new() -> Self {
        MeshCache { pos: Vec::new(), col: Vec::new(), idx: Vec::new(), bbox: [f64::MAX, f64::MAX, f64::MIN, f64::MIN] }
    }

    fn push(&mut self, poly: &[P2], o: P2, col: impl Fn(P2) -> Color32) {
        if poly.len() < 3 {
            return;
        }
        let base = self.pos.len() as u32;
        for &v in poly {
            let w = [v[0] + o[0], v[1] + o[1]];
            self.bbox = [self.bbox[0].min(w[0]), self.bbox[1].min(w[1]), self.bbox[2].max(w[0]), self.bbox[3].max(w[1])];
            self.pos.push(w);
            self.col.push(col(v));
        }
        for i in 1..poly.len() as u32 - 1 {
            self.idx.extend_from_slice(&[base, base + i, base + i + 1]);
        }
    }

    fn draw(&self, painter: &egui::Painter, view: &View, rect: Rect, alpha: f32) {
        let vis = view.world_rect(rect);
        if self.pos.is_empty() || !bbox_overlap(self.bbox, vis) {
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

struct Visuals {
    range: f64,
    class_w: f64,
    iso: f64,
    mesh_diff: MeshCache,
    mesh_vol: MeshCache,
    neg: Contours,
    zero: Contours,
    pos: Contours,
    fill_classes: [ClassRow; 4],
    cut_classes: [ClassRow; 4],
}

pub struct Compare {
    pub second: Option<Second>,
    pub swap: bool,
    pub open: bool,
    pub tab: Tab,
    pub show_in_map: bool,
    pub show_color: bool,
    pub show_iso: bool,
    pub stepped: bool,
    pub labels: bool,
    pub range_user: f64,
    pub iso_user: f64,
    pub want_load: bool,
    pub want_params: bool,
    pub want_gm1: bool,
    pub want_pdf: bool,
    pub want_dxf: bool,
    pub overlap: Option<OverlapInfo>,
    origin: P2,
    tins: Option<(Tin, Tin)>,
    pieces: Vec<Piece>,
    umring: Vec<(P2, P2)>,
    stats: Option<Stats>,
    vis: Option<Visuals>,
    computed_for: Option<usize>,
    dirty: bool,
    built_key: (f64, f64, bool, u32),
}

impl Default for Compare {
    fn default() -> Self {
        Compare {
            second: None,
            swap: false,
            open: false,
            tab: Tab::Overview,
            show_in_map: true,
            show_color: true,
            show_iso: true,
            stepped: false,
            labels: true,
            range_user: 0.0,
            iso_user: 0.0,
            want_load: false,
            want_params: false,
            want_gm1: false,
            want_pdf: false,
            want_dxf: false,
            overlap: None,
            origin: [0.0, 0.0],
            tins: None,
            pieces: Vec::new(),
            umring: Vec::new(),
            stats: None,
            vis: None,
            computed_for: None,
            dirty: true,
            built_key: (-1.0, -1.0, false, 0),
        }
    }
}

fn intersect(a: [f64; 4], b: [f64; 4]) -> Option<[f64; 4]> {
    let r = [a[0].max(b[0]), a[1].max(b[1]), a[2].min(b[2]), a[3].min(b[3])];
    (r[2] > r[0] && r[3] > r[1]).then_some(r)
}

/// dz = Neu − Bestand als lineare Funktion der Teilfläche.
fn dlin(p: &Piece, swap: bool) -> Lin {
    let (n, b) = if swap { (&p.pa, &p.pb) } else { (&p.pb, &p.pa) };
    [n[0] - b[0], n[1] - b[1], n[2] - b[2]]
}

/// Schnitt der Linie f = l mit dem konvexen Polygon.
fn level_segment(poly: &[P2], d: &Lin, l: f64) -> Option<(P2, P2)> {
    let n = poly.len();
    let mut pts: Vec<P2> = Vec::with_capacity(2);
    for i in 0..n {
        let (p, q) = (poly[i], poly[(i + 1) % n]);
        let (fp, fq) = (lin(d, p) - l, lin(d, q) - l);
        if (fp >= 0.0) != (fq >= 0.0) {
            let t = fp / (fp - fq);
            pts.push([p[0] + (q[0] - p[0]) * t, p[1] + (q[1] - p[1]) * t]);
        }
    }
    (pts.len() >= 2).then(|| (pts[0], pts[1]))
}

/// Teil des Polygons mit lo ≤ f ≤ hi.
fn band(poly: &[P2], d: &Lin, lo: f64, hi: f64) -> Poly {
    let p = if lo.is_finite() { clip_half(poly, &[d[0] - lo, d[1], d[2]]) } else { poly.to_vec() };
    if p.len() < 3 || !hi.is_finite() {
        return p;
    }
    clip_half(&p, &[hi - d[0], -d[1], -d[2]])
}

fn stats_of(pieces: &[Piece], swap: bool) -> Stats {
    let mut s = Stats { dz_min: f64::MAX, dz_max: f64::MIN, ..Default::default() };
    for pc in pieces {
        let d = dlin(pc, swap);
        s.valid_area += area(&pc.poly);
        let (mut mn, mut mx) = (f64::MAX, f64::MIN);
        for &v in &pc.poly {
            let z = lin(&d, v);
            mn = mn.min(z);
            mx = mx.max(z);
        }
        s.dz_min = s.dz_min.min(mn);
        s.dz_max = s.dz_max.max(mx);
        if mx.abs().max(mn.abs()) < 1e-9 {
            continue; // kein Unterschied
        }
        // Schnittkante: die Teilfläche an dz = 0 teilen, dann Prismen (Fläche × dz im Schwerpunkt)
        let fp = clip_half(&pc.poly, &d);
        let fa = area(&fp);
        if fa > EPS_AREA {
            s.fill_area += fa;
            s.fill_vol += fa * lin(&d, centroid(&fp));
        }
        let cp = clip_half(&pc.poly, &neg(&d));
        let ca = area(&cp);
        if ca > EPS_AREA {
            s.cut_area += ca;
            s.cut_vol -= ca * lin(&d, centroid(&cp));
        }
        if mn < 0.0 && mx > 0.0 {
            if let Some((p, q)) = level_segment(&pc.poly, &d, 0.0) {
                s.zero_len += (q[0] - p[0]).hypot(q[1] - p[1]);
            }
        }
    }
    if pieces.is_empty() {
        s.dz_min = 0.0;
        s.dz_max = 0.0;
    }
    s.dz_mean = if s.valid_area > 0.0 { (s.fill_vol - s.cut_vol) / s.valid_area } else { 0.0 };
    s
}

// ------------------------------------------------------------------ Farben

/// Divergierend: t = −1 (Abtrag, rot) … 0 (hell) … +1 (Auftrag, blau).
fn diff_color(t: f64) -> Color32 {
    let stops: [(f64, [f32; 3]); 7] = [
        (-1.0, [178.0, 24.0, 43.0]),
        (-0.6, [239.0, 138.0, 98.0]),
        (-0.2, [253.0, 219.0, 199.0]),
        (0.0, [247.0, 247.0, 247.0]),
        (0.2, [209.0, 229.0, 240.0]),
        (0.6, [103.0, 169.0, 207.0]),
        (1.0, [33.0, 102.0, 172.0]),
    ];
    let t = t.clamp(-1.0, 1.0);
    for w in stops.windows(2) {
        let (a, ca) = w[0];
        let (b, cb) = w[1];
        if t <= b {
            let f = ((t - a) / (b - a)) as f32;
            return Color32::from_rgb((ca[0] + (cb[0] - ca[0]) * f) as u8, (ca[1] + (cb[1] - ca[1]) * f) as u8, (ca[2] + (cb[2] - ca[2]) * f) as u8);
        }
    }
    Color32::WHITE
}

fn class_color(fill: bool, class: usize) -> Color32 {
    const BLUE: [[u8; 3]; 4] = [[198, 219, 239], [107, 174, 214], [49, 130, 189], [8, 81, 156]];
    const RED: [[u8; 3]; 4] = [[252, 187, 161], [251, 106, 74], [203, 24, 29], [103, 0, 13]];
    let c = if fill { BLUE[class.min(3)] } else { RED[class.min(3)] };
    Color32::from_rgb(c[0], c[1], c[2])
}

/// Aufrunden auf 1; 1,5; 2; 2,5; 3; 4; 5; 6; 8; 10 × 10^n (feiner als 1-2-5).
fn nice_up(v: f64) -> f64 {
    if v <= 0.0 || !v.is_finite() {
        return 1.0;
    }
    let b = 10f64.powf(v.log10().floor());
    [1.0, 1.5, 2.0, 2.5, 3.0, 4.0, 5.0, 6.0, 8.0, 10.0].iter().map(|m| m * b).find(|x| *x >= v * 0.999_999).unwrap_or(10.0 * b)
}

// ------------------------------------------------------------------ Formatierung

fn fmt_num(v: f64, d: usize) -> String {
    let s = format!("{:.*}", d, v.abs());
    let (int, frac) = match s.split_once('.') {
        Some((i, f)) => (i.to_string(), Some(f.to_string())),
        None => (s.clone(), None),
    };
    let mut out = String::new();
    for (k, ch) in int.chars().enumerate() {
        if k > 0 && (int.len() - k) % 3 == 0 {
            out.push('\u{202f}');
        }
        out.push(ch);
    }
    if let Some(f) = frac {
        out.push('.');
        out.push_str(&f);
    }
    if v < 0.0 && s.chars().any(|c| c.is_ascii_digit() && c != '0') {
        out.insert(0, '−');
    }
    out
}

fn decimals_for(step: f64) -> usize {
    if step <= 0.0 {
        return 2;
    }
    ((-step.log10().floor()).max(0.0) as usize).min(4)
}

// ------------------------------------------------------------------ Compare

impl Compare {
    pub fn is_loaded(&self) -> bool {
        self.second.is_some()
    }

    pub fn clear(&mut self) {
        *self = Compare { show_color: self.show_color, show_iso: self.show_iso, stepped: self.stepped, labels: self.labels, ..Default::default() };
    }

    pub fn show_tab(&mut self, tab: Tab) {
        self.open = true;
        self.tab = tab;
    }

    /// Zweites Modell aus einer GM1-Datei laden (GM0/GM2/GM9 daneben werden mitgelesen) und vermaschen.
    pub fn load(&mut self, path: &Path, cur: &Params) -> Result<String, String> {
        let dir = path.parent().map(|d| d.to_path_buf()).unwrap_or_default();
        let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        let mut pr = Project::load_original(dir.clone(), &stem)?;
        if files::find_ext(&dir, &stem, "gm0").is_none() {
            // keine eigene Parameterdatei: Parameter des aktuellen Modells verwenden
            pr.st.params = cur.clone();
            for p in &mut pr.st.points {
                p.x *= cur.scale_xy;
                p.y *= cur.scale_xy;
                p.z *= cur.scale_h;
            }
        }
        if pr.st.points.len() < 3 {
            return Err(format!("{}: weniger als 3 Punkte", path.display()));
        }
        let (tris, _) = tin::triangulate(&pr);
        pr.st.triangles = tris;
        let ntris = pr.st.triangles.iter().filter(|t| t.active).count();
        let msg = format!("Zweites Modell {} geladen: {} Punkte, {} Dreiecke.", stem, pr.st.points.len(), ntris);
        self.second = Some(Second { pr, label: stem, ntris });
        self.overlap = None;
        self.tins = None;
        self.pieces.clear();
        self.umring.clear();
        self.stats = None;
        self.vis = None;
        self.computed_for = None;
        self.dirty = true;
        Ok(msg)
    }

    /// Je Bild aufrufen: rechnet neu, wenn sich Modelle oder Einstellungen geändert haben.
    pub fn update(&mut self, a: Option<&Project>, a_id: usize, params: &Params) {
        let Some(a) = a else { return };
        if self.second.is_none() {
            return;
        }
        if self.dirty || self.computed_for != Some(a_id) {
            self.compute(a);
            self.computed_for = Some(a_id);
            self.dirty = false;
            self.built_key = (-1.0, -1.0, false, 0);
        }
        if !self.pieces.is_empty() {
            let key = (self.range_user, self.iso_user, self.stepped, params.index_every);
            if key != self.built_key {
                self.build_visuals(params.index_every);
                self.built_key = key;
            }
        }
    }

    fn compute(&mut self, a_pr: &Project) {
        self.tins = None;
        self.pieces.clear();
        self.umring.clear();
        self.stats = None;
        self.vis = None;
        self.overlap = None;
        let Some(sec) = &self.second else { return };
        let o = a_pr.bbox().map(|b| [b[0], b[1]]).unwrap_or([0.0, 0.0]);
        let (Some(ta), Some(tb)) = (Tin::new(a_pr, o), Tin::new(&sec.pr, o)) else { return };
        let w = |b: [f64; 4]| [b[0] + o[0], b[1] + o[1], b[2] + o[0], b[3] + o[1]];
        let grow = |b: [f64; 4]| [b[0] - EXTRAP_MAX, b[1] - EXTRAP_MAX, b[2] + EXTRAP_MAX, b[3] + EXTRAP_MAX];
        let inter = intersect(ta.bbox, tb.bbox);
        let mut info = OverlapInfo {
            bbox_a: w(ta.bbox),
            bbox_b: w(tb.bbox),
            inter: inter.map(w),
            overlap_area: 0.0,
            extrap_area: 0.0,
            area_a: ta.area,
            area_b: tb.area,
            umring_len: 0.0,
            pts_b_in_a: 0,
            pts_a_in_b: 0,
            pieces: 0,
        };
        if intersect(grow(ta.bbox), grow(tb.bbox)).is_some() {
            let (pieces, ex) = overlay(&ta, &tb);
            info.overlap_area = pieces.iter().map(|p| area(&p.poly)).sum();
            info.extrap_area = ex;
            info.pieces = pieces.len();
            self.pieces = pieces;
            if info.overlap_area > 0.0 {
                self.umring = umring(&ta, &tb);
                info.umring_len = self.umring.iter().map(|(p, q)| (q[0] - p[0]).hypot(q[1] - p[1])).sum();
                let used = |t: &Tin| {
                    let mut v: Vec<usize> = t.tris.iter().flatten().copied().collect();
                    v.sort_unstable();
                    v.dedup();
                    v
                };
                info.pts_b_in_a = used(&tb).into_iter().filter(|&i| ta.locate(tb.xy(i)).is_some()).count();
                info.pts_a_in_b = used(&ta).into_iter().filter(|&i| tb.locate(ta.xy(i)).is_some()).count();
                self.stats = Some(stats_of(&self.pieces, self.swap));
            }
        }
        self.origin = o;
        self.tins = Some((ta, tb));
        self.overlap = Some(info);
    }

    fn build_visuals(&mut self, index_every: u32) {
        let o = self.origin;
        let swap = self.swap;
        let mut abs: Vec<f64> = self.pieces.iter().flat_map(|pc| {
            let d = dlin(pc, swap);
            pc.poly.iter().map(move |&v| lin(&d, v).abs()).collect::<Vec<_>>()
        }).collect();
        if abs.is_empty() {
            return;
        }
        abs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let p99 = abs[((abs.len() - 1) as f64 * 0.99) as usize].max(1e-6);
        let range = if self.range_user > 0.0 { self.range_user } else { nice_up(p99) };
        let iso = if self.iso_user > 0.0 { self.iso_user } else { nice_step(range / 6.0) };
        let class_w = if self.range_user > 0.0 { range / 4.0 } else { nice_step(p99 / 4.0) };
        let mut mesh_diff = MeshCache::new();
        let mut mesh_vol = MeshCache::new();
        let mut fill_classes = [ClassRow::default(); 4];
        let mut cut_classes = [ClassRow::default(); 4];
        for k in 0..4 {
            let (lo, hi) = (class_w * k as f64, if k == 3 { f64::INFINITY } else { class_w * (k + 1) as f64 });
            fill_classes[k] = ClassRow { lo, hi, ..Default::default() };
            cut_classes[k] = ClassRow { lo, hi, ..Default::default() };
        }
        let mut segs: HashMap<i64, Vec<(P2, P2)>> = HashMap::new();
        let mut grads: Vec<f64> = Vec::new();
        for pc in &self.pieces {
            let d = dlin(pc, swap);
            let (mut mn, mut mx) = (f64::MAX, f64::MIN);
            for &v in &pc.poly {
                let z = lin(&d, v);
                mn = mn.min(z);
                mx = mx.max(z);
            }
            grads.push(d[1].hypot(d[2]));
            // Höhenunterschied: stufenlos (Farbe je Ecke) oder gestuft (Bänder je Isolinienabstand)
            if self.stepped {
                let (k0, k1) = ((mn / iso).floor() as i64, (mx / iso).floor() as i64);
                if k1 - k0 <= 400 {
                    for k in k0..=k1 {
                        let p = band(&pc.poly, &d, k as f64 * iso, (k + 1) as f64 * iso);
                        let c = diff_color((k as f64 + 0.5) * iso / range);
                        mesh_diff.push(&p, o, |_| c);
                    }
                }
            } else {
                mesh_diff.push(&pc.poly, o, |v| diff_color(lin(&d, v) / range));
            }
            // Auftrag/Abtrag in Dicke-Klassen
            for k in 0..4 {
                let r = fill_classes[k];
                if mx > r.lo && mn < r.hi {
                    let p = band(&pc.poly, &d, r.lo, r.hi);
                    let ar = area(&p);
                    if ar > EPS_AREA {
                        fill_classes[k].area += ar;
                        fill_classes[k].vol += ar * lin(&d, centroid(&p));
                        mesh_vol.push(&p, o, |_| class_color(true, k));
                    }
                }
                let r = cut_classes[k];
                if mn < -r.lo && mx > -r.hi {
                    let p = band(&pc.poly, &d, -r.hi, -r.lo);
                    let ar = area(&p);
                    if ar > EPS_AREA {
                        cut_classes[k].area += ar;
                        cut_classes[k].vol -= ar * lin(&d, centroid(&p));
                        mesh_vol.push(&p, o, |_| class_color(false, k));
                    }
                }
            }
            // Isolinien: in jeder Teilfläche eine gerade Strecke je Höhe
            let (k0, k1) = ((mn / iso).ceil() as i64, (mx / iso).floor() as i64);
            if k1 >= k0 && k1 - k0 <= 400 {
                for k in k0..=k1 {
                    if let Some(s) = level_segment(&pc.poly, &d, k as f64 * iso) {
                        segs.entry(k).or_default().push(s);
                    }
                }
            }
        }
        grads.retain(|g| *g > 0.0);
        grads.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let med = grads.get(grads.len() / 2).copied().unwrap_or(0.1).max(1e-3);
        let spacing = iso / med;
        let every = index_every.max(1) as i64;
        let (mut nl, mut zl, mut pl) = (Vec::new(), Vec::new(), Vec::new());
        let mut nseg = 0;
        let mut keys: Vec<i64> = segs.keys().copied().collect();
        keys.sort_unstable();
        for k in keys {
            let s = &segs[&k];
            nseg += s.len();
            for (pts, closed) in contours::chain(s) {
                if pts.len() < 2 {
                    continue;
                }
                let line = contours::make_line(k as f64 * iso, k.rem_euclid(every) == 0, closed, pts);
                match k.cmp(&0) {
                    std::cmp::Ordering::Less => nl.push(line),
                    std::cmp::Ordering::Equal => zl.push(line),
                    std::cmp::Ordering::Greater => pl.push(line),
                }
            }
        }
        let ext = self.tins.as_ref().map_or(100.0, |(a, _)| (a.bbox[2] - a.bbox[0]).max(a.bbox[3] - a.bbox[1])).max(1.0);
        let mk = |lines: Vec<contours::ContourLine>| {
            let mut c = Contours { lines, interval: iso, segments: nseg, typical_spacing: spacing };
            contours::place_labels(&mut c, ext, (ext * 0.012).max(0.3));
            c
        };
        self.vis = Some(Visuals { range, class_w, iso, mesh_diff, mesh_vol, neg: mk(nl), zero: mk(zl), pos: mk(pl), fill_classes, cut_classes });
    }

    pub fn map_mode(&self) -> MapMode {
        if !self.open || !self.show_in_map || self.vis.is_none() {
            return MapMode::Off;
        }
        match self.tab {
            Tab::Volume => MapMode::Volume,
            Tab::Diff if self.show_color || self.show_iso => MapMode::Diff,
            _ => MapMode::Off,
        }
    }

    /// Höhenunterschied an einer Stelle (für die Statuszeile).
    pub fn dz_at(&self, x: f64, y: f64) -> Option<f64> {
        if self.map_mode() == MapMode::Off {
            return None;
        }
        let (a, b) = self.tins.as_ref()?;
        let q = [x - self.origin[0], y - self.origin[1]];
        let (ta, tb) = (a.locate(q)?, b.locate(q)?);
        let (za, zb) = (lin(&a.planes[ta], q), lin(&b.planes[tb], q));
        Some(if self.swap { za - zb } else { zb - za })
    }

    // -------------------------------------------------------------- Ausgabe (GM1, PDF, DXF)

    /// Ergebnis liegt vor (gemeinsamer Umring mit Prismen)?
    pub fn has_result(&self) -> bool {
        self.stats.is_some() && !self.pieces.is_empty()
    }

    /// Differenzmodell als Punkte für eine GM1-Datei: alle Eckpunkte der Teilflächen (Punkte beider Modelle
    /// im gemeinsamen Umring, Umringpunkte, Punkte auf der Schnittkante) mit `dz = Neu − Bestand` als Höhe.
    /// Punkte auf der Schnittkante (dz = 0) tragen den Code `SK`.
    pub fn gm1_points(&self) -> Vec<(String, f64, f64, f64, String)> {
        let o = self.origin;
        let mut seen: HashMap<(i64, i64), usize> = HashMap::new();
        let mut out: Vec<(String, f64, f64, f64, String)> = Vec::new();
        for pc in &self.pieces {
            let d = dlin(pc, self.swap);
            for &v in &pc.poly {
                let (x, y) = (v[0] + o[0], v[1] + o[1]);
                let key = ((x * 1000.0).round() as i64, (y * 1000.0).round() as i64);
                if seen.contains_key(&key) {
                    continue;
                }
                let dz = lin(&d, v);
                let (dz, code) = if dz.abs() < 1e-9 { (0.0, "SK") } else { (dz, "") };
                seen.insert(key, out.len());
                out.push((format!("D{}", out.len() + 1), x, y, dz, code.to_string()));
            }
        }
        out
    }

    /// Daten der gerade gezeigten Analyse für die Ausgabe (None, wenn keine Analyse angezeigt wird).
    pub fn analysis_data(&self) -> Option<AnalysisData<'_>> {
        let mode = self.map_mode();
        let v = self.vis.as_ref()?;
        if mode == MapMode::Off {
            return None;
        }
        let o = self.origin;
        let mesh = match mode {
            MapMode::Diff if self.show_color => Some(&v.mesh_diff),
            MapMode::Volume => Some(&v.mesh_vol),
            _ => None,
        };
        let umring = contours::chain(&self.umring)
            .into_iter()
            .map(|(pts, closed)| (pts.into_iter().map(|q| [q[0] + o[0], q[1] + o[1]]).collect(), closed))
            .collect();
        Some(AnalysisData {
            show_iso: mode == MapMode::Diff && self.show_iso,
            show_labels: self.labels,
            mesh_pos: mesh.map_or(&[][..], |m| &m.pos[..]),
            mesh_col: mesh.map_or(&[][..], |m| &m.col[..]),
            mesh_idx: mesh.map_or(&[][..], |m| &m.idx[..]),
            neg: &v.neg,
            zero: &v.zero,
            pos: &v.pos,
            origin: o,
            decimals: decimals_for(v.iso),
            umring,
        })
    }

    /// Legende der gerade gezeigten Analyse für die Druckausgabe.
    pub fn legend(&self, p: &Params, a_name: &str) -> Option<crate::scene::Legend> {
        use crate::scene::{Block, Legend};
        let mode = self.map_mode();
        let (v, st, sec) = (self.vis.as_ref()?, self.stats.as_ref()?, self.second.as_ref()?);
        let (best, neu) = if self.swap { (sec.label.as_str(), a_name) } else { (a_name, sec.label.as_str()) };
        let roles = Block::Lines(vec![(format!("Bestand: {}", best), false), (format!("Neu: {}", neu), false)]);
        match mode {
            MapMode::Diff => {
                let stops: Vec<(f64, [u8; 3])> = (0..=16).map(|k| (k as f64 / 16.0, { let c = diff_color(2.0 * k as f64 / 16.0 - 1.0); [c.r(), c.g(), c.b()] })).collect();
                let dec = decimals_for(v.range / 2.0).max(if v.range < 1.0 { 1 } else { 0 });
                let ticks = vec![
                    (0.0, format!("−{:.*}", dec, v.range)),
                    (0.25, format!("−{:.*}", dec, v.range / 2.0)),
                    (0.5, format!("{:.*}", dec, 0.0)),
                    (0.75, format!("+{:.*}", dec, v.range / 2.0)),
                    (1.0, format!("+{:.*}", dec, v.range)),
                ];
                let mut lines = vec![("Rot = Abtrag, blau = Auftrag".to_string(), false)];
                if self.show_iso {
                    lines.push((format!("Isolinien alle {} m, jede 5. beschriftet", fmt_num(v.iso, decimals_for(v.iso))), false));
                    lines.push(("Schwarz: Schnittkante (Höhenunterschied 0)".to_string(), false));
                }
                Some(Legend { title: "Höhenunterschied Neu − Bestand".into(), blocks: vec![roles, Block::Bar { caption: "Höhenunterschied in m".into(), stops, ticks }, Block::Lines(lines)] })
            }
            MapMode::Volume => {
                let dec = decimals_for(v.class_w).max(if v.class_w < 1.0 { 1 } else { 0 });
                let class_txt = |r: &ClassRow| if r.hi.is_finite() { format!("{:.*} – {:.*} m", dec, r.lo, dec, r.hi) } else { format!("über {:.*} m", dec, r.lo) };
                let mut items = Vec::new();
                for k in (0..4).rev() {
                    let c = class_color(true, k);
                    items.push(([c.r(), c.g(), c.b()], format!("Auftrag {}", class_txt(&v.fill_classes[k]))));
                }
                for k in 0..4 {
                    let c = class_color(false, k);
                    items.push(([c.r(), c.g(), c.b()], format!("Abtrag {}", class_txt(&v.cut_classes[k]))));
                }
                let vol = |x: f64| format!("{} m³", fmt_num(x, 1));
                let lines = vec![
                    ("Volumen (Dreiecksprismen)".to_string(), true),
                    (format!("Abtrag Original: {}", vol(st.cut_vol)), false),
                    (format!("Abtrag aufgelockert (× {:.2}): {}", p.loosen_cut, vol(st.cut_vol * p.loosen_cut)), false),
                    (format!("Auftrag Original: {}", vol(st.fill_vol)), false),
                    (format!("Auftrag aufgelockert (× {:.2}): {}", p.loosen_fill, vol(st.fill_vol * p.loosen_fill)), false),
                    (format!("Saldo Original: {}", vol(st.fill_vol - st.cut_vol)), false),
                ];
                Some(Legend { title: "Auftrag / Abtrag".into(), blocks: vec![roles, Block::Lines(lines), Block::Swatches { caption: "Dicke in m".into(), items }] })
            }
            MapMode::Off => None,
        }
    }

    // -------------------------------------------------------------- Zeichnen

    /// Farbbild, Isolinien (statt des normalen Geländebilds), Ränder beider Modelle und gemeinsamer Umring.
    pub fn draw(&self, painter: &egui::Painter, view: &View, rect: Rect) {
        let mode = self.map_mode();
        let o = self.origin;
        if let Some(v) = &self.vis {
            match mode {
                MapMode::Diff => {
                    v.mesh_diff.draw(painter, view, rect, if self.show_color { 1.0 } else { 0.18 });
                    if self.show_iso {
                        self.draw_iso(painter, view, rect, v);
                    }
                }
                MapMode::Volume => {
                    v.mesh_vol.draw(painter, view, rect, 1.0);
                    let st = ContourStyle { normal: Stroke::new(2.2, Color32::BLACK), index: Stroke::new(2.2, Color32::BLACK), label_color: Color32::BLACK, font_px: 12.5, decimals: 0, show_labels: false };
                    render::draw_contours(painter, view, rect, &v.zero, o[0], o[1], &st);
                }
                MapMode::Off => {}
            }
        }
        if !self.open {
            return;
        }
        let Some((ta, tb)) = &self.tins else { return };
        let vis = view.world_rect(rect);
        let seg = |p: P2, q: P2| [view.to_screen(rect, p[0] + o[0], p[1] + o[1]), view.to_screen(rect, q[0] + o[0], q[1] + o[1])];
        let in_view = |p: P2, q: P2| bbox_overlap(poly_bbox(&[[p[0] + o[0], p[1] + o[1]], [q[0] + o[0], q[1] + o[1]]]), vis);
        let (ca, cb) = (Color32::from_rgb(20, 130, 60), Color32::from_rgb(220, 120, 0));
        let mut shapes = Vec::new();
        for (t, c) in [(ta, ca), (tb, cb)] {
            for (p, q) in t.border_segs() {
                if in_view(p, q) {
                    shapes.push(Shape::line_segment(seg(p, q), Stroke::new(1.2, c)));
                }
            }
        }
        for &(p, q) in &self.umring {
            if in_view(p, q) {
                shapes.push(Shape::line_segment(seg(p, q), Stroke::new(2.6, Color32::from_rgb(200, 0, 120))));
            }
        }
        painter.extend(shapes);
    }

    fn draw_iso(&self, painter: &egui::Painter, view: &View, rect: Rect, v: &Visuals) {
        let o = self.origin;
        let dec = decimals_for(v.iso);
        let mk = |col: Color32, w: f32| ContourStyle { normal: Stroke::new(w, col), index: Stroke::new(w + 1.0, col), label_color: col, font_px: 12.5, decimals: dec, show_labels: self.labels };
        render::draw_contours(painter, view, rect, &v.neg, o[0], o[1], &mk(Color32::from_rgb(110, 20, 20), 1.1));
        render::draw_contours(painter, view, rect, &v.pos, o[0], o[1], &mk(Color32::from_rgb(15, 45, 120), 1.1));
        let zero = ContourStyle { normal: Stroke::new(2.6, Color32::BLACK), index: Stroke::new(2.6, Color32::BLACK), label_color: Color32::BLACK, font_px: 12.5, decimals: dec, show_labels: self.labels };
        render::draw_contours(painter, view, rect, &v.zero, o[0], o[1], &zero);
    }

    /// Farbskala bzw. Klassenlegende in der Ecke der Karte.
    pub fn draw_legend(&self, painter: &egui::Painter, rect: Rect) {
        let (Some(v), mode) = (&self.vis, self.map_mode()) else { return };
        let font = FontId::proportional(12.0);
        let dark = Color32::from_gray(25);
        let origin = rect.left_top() + Vec2::new(12.0, 12.0);
        match mode {
            MapMode::Diff if self.show_color => {
                let (w, h) = (22.0f32, 170.0f32);
                let bg = Rect::from_min_size(origin, Vec2::new(w + 150.0, h + 58.0));
                painter.rect_filled(bg, 4.0, Color32::from_white_alpha(225));
                painter.text(origin + Vec2::new(8.0, 6.0), Align2::LEFT_TOP, "Höhenunterschied (m)", font.clone(), dark);
                painter.text(origin + Vec2::new(8.0, 21.0), Align2::LEFT_TOP, "Neu − Bestand", FontId::proportional(11.0), Color32::from_gray(80));
                let bar = Rect::from_min_size(origin + Vec2::new(10.0, 40.0), Vec2::new(w, h));
                let steps = 60;
                for k in 0..steps {
                    let t = 1.0 - 2.0 * (k as f64 + 0.5) / steps as f64;
                    let seg = Rect::from_min_size(bar.min + Vec2::new(0.0, h * k as f32 / steps as f32), Vec2::new(w, h / steps as f32 + 0.5));
                    painter.rect_filled(seg, 0.0, diff_color(t));
                }
                painter.rect_stroke(bar, 0.0, Stroke::new(1.0, Color32::from_gray(90)), StrokeKind::Outside);
                let dec = decimals_for(v.range / 2.0).max(if v.range < 1.0 { 1 } else { 0 });
                for (f, txt) in [(-1.0, format!("+{:.*}", dec, v.range)), (-0.5, format!("+{:.*}", dec, v.range / 2.0)), (0.0, format!("{:.*}", dec, 0.0)), (0.5, format!("−{:.*}", dec, v.range / 2.0)), (1.0, format!("−{:.*}", dec, v.range))] {
                    let y = bar.center().y + f as f32 * h * 0.5;
                    painter.line_segment([Pos2::new(bar.right(), y), Pos2::new(bar.right() + 5.0, y)], Stroke::new(1.0, dark));
                    painter.text(Pos2::new(bar.right() + 8.0, y), Align2::LEFT_CENTER, txt, font.clone(), dark);
                }
                painter.text(Pos2::new(bar.right() + 52.0, bar.top() + 6.0), Align2::LEFT_CENTER, "Auftrag", font.clone(), Color32::from_rgb(33, 102, 172));
                painter.text(Pos2::new(bar.right() + 52.0, bar.bottom() - 6.0), Align2::LEFT_CENTER, "Abtrag", font.clone(), Color32::from_rgb(178, 24, 43));
                if self.show_iso {
                    painter.text(origin + Vec2::new(8.0, h + 44.0), Align2::LEFT_TOP, format!("Isolinien alle {} m", fmt_num(v.iso, decimals_for(v.iso))), FontId::proportional(11.0), Color32::from_gray(80));
                }
            }
            MapMode::Volume => {
                let bg = Rect::from_min_size(origin, Vec2::new(200.0, 28.0 + 2.0 * (15.0 + 4.0 * 17.0 + 6.0) + 22.0));
                painter.rect_filled(bg, 4.0, Color32::from_white_alpha(225));
                painter.text(origin + Vec2::new(8.0, 6.0), Align2::LEFT_TOP, "Auftrag / Abtrag (Dicke in m)", font.clone(), dark);
                let mut y = origin.y + 28.0;
                let dec = decimals_for(v.class_w).max(if v.class_w < 1.0 { 1 } else { 0 });
                for (fill, rows, name) in [(true, &v.fill_classes, "Auftrag (Neu über Bestand)"), (false, &v.cut_classes, "Abtrag (Neu unter Bestand)")] {
                    painter.text(Pos2::new(origin.x + 10.0, y), Align2::LEFT_TOP, name, FontId::proportional(11.0), Color32::from_gray(80));
                    y += 15.0;
                    let order: Vec<usize> = if fill { (0..4).rev().collect() } else { (0..4).collect() };
                    for k in order {
                        let r = &rows[k];
                        let sw = Rect::from_min_size(Pos2::new(origin.x + 10.0, y), Vec2::new(22.0, 13.0));
                        painter.rect_filled(sw, 0.0, class_color(fill, k));
                        painter.rect_stroke(sw, 0.0, Stroke::new(0.8, Color32::from_gray(110)), StrokeKind::Outside);
                        let txt = if r.hi.is_finite() { format!("{:.*} – {:.*}", dec, r.lo, dec, r.hi) } else { format!("über {:.*}", dec, r.lo) };
                        painter.text(Pos2::new(sw.right() + 8.0, y + 6.5), Align2::LEFT_CENTER, txt, font.clone(), dark);
                        y += 17.0;
                    }
                    y += 6.0;
                }
                painter.line_segment([Pos2::new(origin.x + 10.0, y + 3.0), Pos2::new(origin.x + 32.0, y + 3.0)], Stroke::new(2.2, Color32::BLACK));
                painter.text(Pos2::new(origin.x + 40.0, y + 3.0), Align2::LEFT_CENTER, "Schnittkante (dz = 0)", font, dark);
            }
            _ => {}
        }
    }

    // -------------------------------------------------------------- Fenster

    pub fn window(&mut self, ctx: &egui::Context, a_name: &str, a_info: &str, p: &Params) {
        if !self.open {
            return;
        }
        let mut open = true;
        let h = (ctx.content_rect().height() - 110.0).max(300.0);
        egui::Window::new("Analyse – Modellvergleich")
            .open(&mut open)
            .default_width(450.0)
            .default_pos(Pos2::new((ctx.content_rect().right() - 480.0).max(250.0), 70.0))
            .vscroll(true)
            .default_height(h)
            .max_height(h)
            .show(ctx, |ui| {
                self.ui(ui, a_name, a_info, p);
            });
        if !open {
            self.open = false;
        }
    }

    fn ui(&mut self, ui: &mut Ui, a_name: &str, a_info: &str, p: &Params) {
        let Some(sec) = &self.second else {
            ui.label("Es ist noch kein zweites Geländemodell eingelesen.");
            if ui.button("Zweites Geländemodell einlesen…").clicked() {
                self.want_load = true;
            }
            return;
        };
        let b_name = sec.label.clone();
        let b_info = format!("{} Punkte, {} Dreiecke", sec.pr.st.points.len(), sec.ntris);
        let (best_name, best_info, neu_name, neu_info) = if self.swap { (b_name, b_info, a_name.to_string(), a_info.to_string()) } else { (a_name.to_string(), a_info.to_string(), b_name, b_info) };
        egui::Grid::new("cmp_roles").num_columns(2).show(ui, |ui| {
            ui.label("Bestand:");
            ui.strong(&best_name);
            ui.end_row();
            ui.label("");
            ui.weak(&best_info);
            ui.end_row();
            ui.label("Neu:");
            ui.strong(&neu_name);
            ui.end_row();
            ui.label("");
            ui.weak(&neu_info);
            ui.end_row();
        });
        ui.horizontal(|ui| {
            if ui.button("⇄ Bestand und Neu tauschen").clicked() {
                self.swap = !self.swap;
                self.dirty = true;
            }
            if ui.button("Anderes Modell einlesen…").clicked() {
                self.want_load = true;
            }
            if ui.button("Entfernen").clicked() {
                self.clear();
            }
        });
        ui.weak("Auftrag: Neu liegt über Bestand.  Abtrag: Neu liegt unter Bestand.");
        ui.separator();

        // ---- Überschneidung und gemeinsamer Umring
        let Some(o) = &self.overlap else {
            ui.colored_label(Color32::from_rgb(190, 40, 40), "Eines der Modelle hat keine Dreiecke – bitte zuerst vermaschen.");
            return;
        };
        let bx = |b: [f64; 4]| format!("R {:.1} … {:.1}   H {:.1} … {:.1}", b[0], b[2], b[1], b[3]);
        egui::Grid::new("cmp_ov").num_columns(2).show(ui, |ui| {
            ui.label("Bereich Bestand");
            ui.monospace(bx(if self.swap { o.bbox_b } else { o.bbox_a }));
            ui.end_row();
            ui.label("Bereich Neu");
            ui.monospace(bx(if self.swap { o.bbox_a } else { o.bbox_b }));
            ui.end_row();
        });
        if o.overlap_area <= 0.0 {
            let msg = if o.inter.is_none() {
                "Die Modelle überschneiden sich in x/y nicht. Es gibt keinen Vergleich."
            } else {
                "Die Umrisse überlappen, aber die Dreiecksflächen beider Modelle treffen sich nirgends (Lücken, Umring)."
            };
            ui.colored_label(Color32::from_rgb(190, 40, 40), msg);
            return;
        }
        let ov = o.overlap_area;
        ui.colored_label(
            Color32::from_rgb(20, 120, 50),
            format!("✔ Gemeinsamer Umring: {} m² ({:.0} % des aktuellen, {:.0} % des zweiten Modells)", fmt_num(ov, 1), 100.0 * ov / o.area_a.max(1e-9), 100.0 * ov / o.area_b.max(1e-9)),
        );
        ui.horizontal(|ui| {
            for (t, name) in [(Tab::Overview, "Gemeinsamer Umring"), (Tab::Volume, "Auftrag / Abtrag"), (Tab::Diff, "Höhenunterschied")] {
                if ui.selectable_label(self.tab == t, name).clicked() {
                    self.tab = t;
                }
            }
        });
        ui.separator();
        match self.tab {
            Tab::Overview => self.overview_ui(ui),
            Tab::Volume => self.volume_ui(ui, p),
            Tab::Diff => self.diff_ui(ui),
        }
        ui.separator();
        ui.strong("Ausgabe");
        ui.horizontal_wrapped(|ui| {
            if ui.button("Differenzmodell als GM1 speichern…").on_hover_text("Punkte des gemeinsamen Netzes; die Höhe ist der Höhenunterschied Neu − Bestand").clicked() {
                self.want_gm1 = true;
            }
            if ui.button("Druckausgabe (PDF)…").clicked() {
                self.want_pdf = true;
            }
            if ui.button("DXF-Export…").clicked() {
                self.want_dxf = true;
            }
        });
    }

    fn overview_ui(&mut self, ui: &mut Ui) {
        let Some(o) = &self.overlap else { return };
        let st = self.stats.clone().unwrap_or_default();
        let (pb, pa) = if self.swap { (o.pts_a_in_b, o.pts_b_in_a) } else { (o.pts_b_in_a, o.pts_a_in_b) };
        egui::Grid::new("cmp_um").num_columns(2).striped(true).show(ui, |ui| {
            ui.label("Fläche im gemeinsamen Umring");
            ui.label(format!("{} m²", fmt_num(o.overlap_area, 1)));
            ui.end_row();
            ui.label("Länge des gemeinsamen Umrings");
            ui.label(format!("{} m", fmt_num(o.umring_len, 2)));
            ui.end_row();
            ui.label("Punkte von Neu im Bestand interpoliert");
            ui.label(format!("{}", pb));
            ui.end_row();
            ui.label("Punkte vom Bestand in Neu interpoliert");
            ui.label(format!("{}", pa));
            ui.end_row();
            ui.label(format!("extrapoliert (Ränder < {:.0} cm)", EXTRAP_MAX * 100.0));
            ui.label(format!("{} m²", fmt_num(o.extrap_area, 3)));
            ui.end_row();
            ui.label("Schnittkante (dz = 0)");
            ui.label(format!("{} m", fmt_num(st.zero_len, 2)));
            ui.end_row();
            ui.label("Prismen (Teilflächen)");
            ui.label(format!("{}", o.pieces));
            ui.end_row();
        });
        ui.add_space(4.0);
        ui.weak("Karte: rosa = gemeinsamer Umring, grün = Rand aktuelles Modell, orange = Rand zweites Modell.");
        ui.weak("Beide Netze werden überlagert; jede Teilfläche liegt in genau einem Dreieck jedes Modells, die Höhen sind linear interpoliert. An der Schnittkante werden die Flächen geteilt.");
        ui.checkbox(&mut self.show_in_map, "Ergebnis in der Karte zeigen");
    }

    fn volume_ui(&mut self, ui: &mut Ui, p: &Params) {
        let (Some(st), Some(v)) = (self.stats.clone(), &self.vis) else { return };
        let (fc, cc) = (p.loosen_fill, p.loosen_cut);
        egui::Grid::new("cmp_vol").num_columns(4).striped(true).spacing([14.0, 5.0]).show(ui, |ui| {
            ui.label("");
            ui.strong("Original (fest)");
            ui.strong("Faktor");
            ui.strong("aufgelockert");
            ui.end_row();
            ui.label("Abtrag");
            ui.colored_label(Color32::from_rgb(178, 24, 43), format!("{} m³", fmt_num(st.cut_vol, 1)));
            ui.label(format!("{:.2}", cc));
            ui.colored_label(Color32::from_rgb(178, 24, 43), format!("{} m³", fmt_num(st.cut_vol * cc, 1)));
            ui.end_row();
            ui.label("Auftrag");
            ui.colored_label(Color32::from_rgb(33, 102, 172), format!("{} m³", fmt_num(st.fill_vol, 1)));
            ui.label(format!("{:.2}", fc));
            ui.colored_label(Color32::from_rgb(33, 102, 172), format!("{} m³", fmt_num(st.fill_vol * fc, 1)));
            ui.end_row();
            ui.label("Saldo (Auftrag − Abtrag)");
            ui.strong(format!("{} m³", fmt_num(st.fill_vol - st.cut_vol, 1)));
            ui.label("");
            ui.label(format!("{} m³", fmt_num(st.fill_vol * fc - st.cut_vol * cc, 1)));
            ui.end_row();
            ui.label("Fläche Abtrag");
            ui.label(format!("{} m²", fmt_num(st.cut_area, 1)));
            ui.label("");
            ui.label("");
            ui.end_row();
            ui.label("Fläche Auftrag");
            ui.label(format!("{} m²", fmt_num(st.fill_area, 1)));
            ui.label("");
            ui.label("");
            ui.end_row();
        });
        ui.horizontal(|ui| {
            ui.weak(format!("Auflockerung: Abtrag × {:.2}, Auftrag × {:.2} (Datei → Parameter).", cc, fc));
            if ui.small_button("Parameter…").clicked() {
                self.want_params = true;
            }
        });
        ui.add_space(6.0);
        let rows = [
            ("Abtrag Original", st.cut_vol, Color32::from_rgb(178, 24, 43)),
            ("Abtrag aufgelockert", st.cut_vol * cc, Color32::from_rgb(239, 138, 98)),
            ("Auftrag Original", st.fill_vol, Color32::from_rgb(33, 102, 172)),
            ("Auftrag aufgelockert", st.fill_vol * fc, Color32::from_rgb(103, 169, 207)),
        ];
        let vmax = rows.iter().map(|r| r.1).fold(1e-9, f64::max);
        let (resp, painter) = ui.allocate_painter(Vec2::new(ui.available_width().max(300.0), 4.0 * 24.0 + 4.0), egui::Sense::hover());
        let label_w = 130.0;
        let text_w = 100.0;
        let bar_w = (resp.rect.width() - label_w - text_w).max(40.0);
        for (k, (name, val, col)) in rows.iter().enumerate() {
            let y = resp.rect.top() + 2.0 + k as f32 * 24.0;
            painter.text(Pos2::new(resp.rect.left(), y + 10.0), Align2::LEFT_CENTER, *name, FontId::proportional(12.5), Color32::from_gray(40));
            let w = (bar_w as f64 * val / vmax) as f32;
            painter.rect_filled(Rect::from_min_size(Pos2::new(resp.rect.left() + label_w, y + 2.0), Vec2::new(w.max(1.0), 17.0)), 2.0, *col);
            painter.text(Pos2::new(resp.rect.left() + label_w + w + 6.0, y + 10.0), Align2::LEFT_CENTER, format!("{} m³", fmt_num(*val, 0)), FontId::proportional(12.0), Color32::from_gray(40));
        }
        ui.add_space(6.0);
        ui.strong("Nach Dicke (Karte: Klassenfarben)");
        egui::Grid::new("cmp_cls").num_columns(4).striped(true).show(ui, |ui| {
            ui.label("");
            ui.label("Klasse (m)");
            ui.label("Fläche");
            ui.label("Volumen (Original)");
            ui.end_row();
            let dec = decimals_for(v.class_w).max(if v.class_w < 1.0 { 1 } else { 0 });
            for (fill, rows, name) in [(false, &v.cut_classes, "Abtrag"), (true, &v.fill_classes, "Auftrag")] {
                for (k, r) in rows.iter().enumerate().rev() {
                    let (rect, _) = ui.allocate_exact_size(Vec2::new(22.0, 13.0), egui::Sense::hover());
                    ui.painter().rect_filled(rect, 0.0, class_color(fill, k));
                    let cls = if r.hi.is_finite() { format!("{name} {:.*} – {:.*}", dec, r.lo, dec, r.hi) } else { format!("{name} über {:.*}", dec, r.lo) };
                    ui.label(cls);
                    ui.label(format!("{} m²", fmt_num(r.area, 0)));
                    ui.label(format!("{} m³", fmt_num(r.vol, 1)));
                    ui.end_row();
                }
            }
        });
        ui.add_space(4.0);
        ui.checkbox(&mut self.show_in_map, "Ergebnis in der Karte zeigen");
        ui.weak("Volumen aus Dreiecksprismen über dem gemeinsamen Umring; an der Schnittkante geteilt (exakt für die Dreiecksnetze).");
    }

    fn diff_ui(&mut self, ui: &mut Ui) {
        let (Some(st), Some(v)) = (self.stats.clone(), &self.vis) else { return };
        let (range, iso) = (v.range, v.iso);
        egui::Grid::new("cmp_dz").num_columns(2).show(ui, |ui| {
            ui.label("größter Auftrag (Neu über Bestand)");
            ui.label(format!("{} m", fmt_num(st.dz_max.max(0.0), 3)));
            ui.end_row();
            ui.label("größter Abtrag (Neu unter Bestand)");
            ui.label(format!("{} m", fmt_num((-st.dz_min).max(0.0), 3)));
            ui.end_row();
            ui.label("mittlerer Unterschied");
            ui.label(format!("{} m", fmt_num(st.dz_mean, 3)));
            ui.end_row();
        });
        ui.separator();
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.show_color, "Farbschema");
            ui.checkbox(&mut self.show_iso, "Isolinien");
            ui.checkbox(&mut self.labels, "Beschriftung");
            ui.checkbox(&mut self.stepped, "Farbe gestuft");
        });
        ui.horizontal(|ui| {
            ui.label("Farbbereich ±");
            let mut r = if self.range_user > 0.0 { self.range_user } else { range };
            if ui.add(egui::DragValue::new(&mut r).speed(0.01).range(0.001..=10000.0)).changed() {
                self.range_user = r;
            }
            ui.label("m");
            if ui.small_button("auto").clicked() {
                self.range_user = 0.0;
            }
            ui.separator();
            ui.label("Isolinienabstand");
            let mut s = if self.iso_user > 0.0 { self.iso_user } else { iso };
            if ui.add(egui::DragValue::new(&mut s).speed(0.01).range(0.001..=10000.0)).changed() {
                self.iso_user = s;
            }
            ui.label("m");
            if ui.small_button("auto").clicked() {
                self.iso_user = 0.0;
            }
        });
        ui.checkbox(&mut self.show_in_map, "Ergebnis in der Karte zeigen");
        ui.weak("Schwarz: Schnittkante (kein Höhenunterschied). Rot: Abtrag, blau: Auftrag. Jede 5. Isolinie ist beschriftet.");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Point, ProjectState};

    /// Regelmäßiges Punktgitter n × n über [x0, x1] × [y0, y1], vermascht (linear).
    fn model(x0: f64, x1: f64, y0: f64, y1: f64, n: usize, f: impl Fn(f64, f64) -> f64) -> Project {
        let mut pts = Vec::new();
        for i in 0..n {
            for j in 0..n {
                let x = x0 + (x1 - x0) * i as f64 / (n - 1) as f64;
                let y = y0 + (y1 - y0) * j as f64 / (n - 1) as f64;
                pts.push(Point { name: format!("{}_{}", i, j), x, y, z: f(x, y), code: String::new(), active: true, label_pos: 0, added: false });
            }
        }
        let mut params = Params::default();
        params.max_side = 1000.0;
        params.min_angle_gon = 0.0;
        params.subdivision = 0;
        let mut pr = Project { dir: std::env::temp_dir(), st: ProjectState { name: "t".into(), params, points: pts, ..Default::default() }, index: Default::default() };
        pr.rebuild_index();
        let (t, _) = tin::triangulate(&pr);
        pr.st.triangles = t;
        pr
    }

    fn run(a: &Project, b: Project, swap: bool) -> Compare {
        let mut c = Compare::default();
        c.second = Some(Second { pr: b, label: "b".into(), ntris: 0 });
        c.swap = swap;
        c.compute(a);
        c
    }

    const E: f64 = 1e-6;

    #[test]
    fn shifted_planes_exact() {
        // A: 0..100², B: 50..150 × 0..100, B 2 m höher (gleiche Neigung) => 5000 m², 10 000 m³ Auftrag
        let f = |x: f64, y: f64| 100.0 + 0.05 * x + 0.02 * y;
        let a = model(1000.0, 1100.0, 2000.0, 2100.0, 11, f);
        let b = || model(1050.0, 1150.0, 2000.0, 2100.0, 11, |x, y| f(x, y) + 2.0);
        let c = run(&a, b(), false);
        let o = c.overlap.as_ref().unwrap();
        let st = c.stats.clone().unwrap();
        assert!((o.overlap_area - 5000.0).abs() < E, "{}", o.overlap_area);
        assert!(o.extrap_area.abs() < E);
        assert!((st.fill_vol - 10000.0).abs() < E, "{}", st.fill_vol);
        assert!(st.cut_vol.abs() < E);
        assert!((o.umring_len - 300.0).abs() < E, "Umring {}", o.umring_len);
        // getauscht: aus Auftrag wird Abtrag
        let s2 = run(&a, b(), true).stats.unwrap();
        assert!((s2.cut_vol - 10000.0).abs() < E && s2.fill_vol.abs() < E);
    }

    #[test]
    fn no_overlap() {
        let a = model(0.0, 40.0, 0.0, 40.0, 5, |_, _| 1.0);
        let b = model(500.0, 540.0, 500.0, 540.0, 5, |_, _| 2.0);
        let c = run(&a, b, false);
        assert_eq!(c.overlap.as_ref().unwrap().overlap_area, 0.0);
        assert!(c.stats.is_none());
    }

    #[test]
    fn crossing_surfaces_split_at_intersection() {
        // B kreuzt A: dz = 0.1·(x − 50) => je Seite 0.5 · 50 · 5 · 100 = 12 500 m³, Schnittkante 100 m
        let a = model(0.0, 100.0, 0.0, 100.0, 11, |_, _| 100.0);
        let b = model(0.0, 100.0, 0.0, 100.0, 8, |x, _| 100.0 + 0.1 * (x - 50.0));
        let c = run(&a, b, false);
        let st = c.stats.unwrap();
        assert!((st.fill_vol - 12500.0).abs() < E, "{}", st.fill_vol);
        assert!((st.cut_vol - 12500.0).abs() < E, "{}", st.cut_vol);
        assert!((st.zero_len - 100.0).abs() < E, "{}", st.zero_len);
    }

    #[test]
    fn extrapolation_only_below_10cm() {
        let f = |x: f64, y: f64| 100.0 + 0.03 * x - 0.01 * y;
        let a = model(0.0, 100.0, 0.0, 100.0, 11, f);
        // Rand 5 cm innen: wird extrapoliert => volle 100 × 100 × 2
        let c = run(&a, model(0.05, 100.0, 0.0, 100.0, 11, |x, y| f(x, y) + 2.0), false);
        let o = c.overlap.as_ref().unwrap();
        assert!((c.stats.as_ref().unwrap().fill_vol - 20000.0).abs() < E, "{}", c.stats.as_ref().unwrap().fill_vol);
        assert!((o.extrap_area - 5.0).abs() < E, "{}", o.extrap_area);
        assert!((o.umring_len - 400.0).abs() < E, "{}", o.umring_len);
        // Rand 50 cm innen: keine Extrapolation => 99.5 × 100 × 2
        let c = run(&a, model(0.5, 100.0, 0.0, 100.0, 11, |x, y| f(x, y) + 2.0), false);
        let o = c.overlap.as_ref().unwrap();
        assert!((c.stats.as_ref().unwrap().fill_vol - 19900.0).abs() < E, "{}", c.stats.as_ref().unwrap().fill_vol);
        assert!(o.extrap_area.abs() < E);
        assert!((o.umring_len - 399.0).abs() < E, "{}", o.umring_len);
    }

    #[test]
    fn real_test_files() {
        let (o, u) = (Path::new(r"C:\DGM-NT\Test\Test-Oben.gm1"), Path::new(r"C:\DGM-NT\Test\Test-Unten.gm1"));
        if !o.exists() || !u.exists() {
            return;
        }
        let mut a = Project::load_original(o.parent().unwrap().to_path_buf(), "Test-Oben").unwrap();
        let (t, _) = tin::triangulate(&a);
        a.st.triangles = t;
        let mut c = Compare::default();
        c.load(u, &a.st.params.clone()).unwrap();
        let t0 = std::time::Instant::now();
        c.compute(&a);
        let ms = t0.elapsed().as_millis();
        let ov = c.overlap.as_ref().unwrap();
        let st = c.stats.clone().unwrap();
        println!(
            "{} ms: Umring {:.2} m², Länge {:.1} m, extrapoliert {:.4} m², {} Teilflächen, Abtrag {:.1} m³, Auftrag {:.1} m³, Schnittkante {:.1} m, dz {:.3}..{:.3}",
            ms, ov.overlap_area, ov.umring_len, ov.extrap_area, ov.pieces, st.cut_vol, st.fill_vol, st.zero_len, st.dz_min, st.dz_max
        );
        assert!(ov.overlap_area > 0.0);
        // Volumenbilanz: Auftrag − Abtrag = Σ (Prismen Neu) − Σ (Prismen Bestand) über dem Umring
        let (sum_a, sum_b): (f64, f64) = c.pieces.iter().fold((0.0, 0.0), |acc, p| {
            let (ar, ce) = (area(&p.poly), centroid(&p.poly));
            (acc.0 + ar * lin(&p.pa, ce), acc.1 + ar * lin(&p.pb, ce))
        });
        assert!(((sum_b - sum_a) - (st.fill_vol - st.cut_vol)).abs() < 1e-3 * (st.fill_vol + st.cut_vol));
        // Flächen: Auftrag + Abtrag = Umring
        assert!((st.fill_area + st.cut_area - ov.overlap_area).abs() < 1e-3 * ov.overlap_area);
        c.build_visuals(5);
        assert!(c.vis.is_some());
    }
}

/// Gerade angezeigte Analyse, wie sie die Ausgabe (PDF/DXF) braucht.
pub struct AnalysisData<'a> {
    pub show_iso: bool,
    pub show_labels: bool,
    /// Farbnetz in Weltkoordinaten (leer, wenn ohne Farbe)
    pub mesh_pos: &'a [P2],
    pub mesh_col: &'a [Color32],
    pub mesh_idx: &'a [u32],
    pub neg: &'a Contours,
    pub zero: &'a Contours,
    pub pos: &'a Contours,
    /// Ursprung der lokalen Linienkoordinaten
    pub origin: P2,
    pub decimals: usize,
    /// gemeinsamer Umring (Weltkoordinaten)
    pub umring: Vec<(Vec<P2>, bool)>,
}
