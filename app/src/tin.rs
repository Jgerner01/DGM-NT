//! Triangulation (Vermaschung) and the curved surface over the triangles.

use spade::{ConstrainedDelaunayTriangulation, HasPosition, Point2, Triangulation};
use std::collections::{HashMap, HashSet};

use crate::model::{Project, Tri};

#[derive(Clone, Copy)]
struct V {
    p: Point2<f64>,
    idx: usize,
}

impl HasPosition for V {
    type Scalar = f64;
    fn position(&self) -> Point2<f64> {
        self.p
    }
}

pub struct MeshReport {
    pub points_used: usize,
    pub duplicates: usize,
    pub triangles: usize,
    pub constraints_skipped: usize,
}

/// Minimum distance below which two points count as identical (dgm.sys line 14: 0.05 m).
pub const IDENTICAL_DIST: f64 = 0.05;

pub fn point_in_poly(x: f64, y: f64, poly: &[(f64, f64)]) -> bool {
    let mut inside = false;
    let n = poly.len();
    let mut j = n.wrapping_sub(1);
    for i in 0..n {
        let (xi, yi) = poly[i];
        let (xj, yj) = poly[j];
        if ((yi > y) != (yj > y)) && (x < (xj - xi) * (y - yi) / (yj - yi) + xi) {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// Build the triangles for the active points of a project.
pub fn triangulate(pr: &Project) -> (Vec<Tri>, MeshReport) {
    let st = &pr.st;
    let (ox, oy) = pr.bbox().map(|b| (b[0], b[1])).unwrap_or((0.0, 0.0));
    // dedupe within IDENTICAL_DIST using a hash grid
    let mut grid: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
    let mut used: Vec<usize> = Vec::new();
    let mut dups = 0;
    let cell = IDENTICAL_DIST.max(1e-6);
    for (i, p) in st.points.iter().enumerate() {
        if !p.active || !pr.code_ok(p) {
            continue;
        }
        let key = (((p.x - ox) / cell).floor() as i64, ((p.y - oy) / cell).floor() as i64);
        let mut dup = false;
        'outer: for dx in -1..=1 {
            for dy in -1..=1 {
                if let Some(v) = grid.get(&(key.0 + dx, key.1 + dy)) {
                    for &j in v {
                        let q = &st.points[j];
                        if (q.x - p.x).hypot(q.y - p.y) < IDENTICAL_DIST {
                            dup = true;
                            break 'outer;
                        }
                    }
                }
            }
        }
        if dup {
            dups += 1;
            continue;
        }
        grid.entry(key).or_default().push(i);
        used.push(i);
    }
    let verts: Vec<V> = used
        .iter()
        .map(|&i| V { p: Point2::new(st.points[i].x - ox, st.points[i].y - oy), idx: i })
        .collect();
    let Ok(mut cdt) = ConstrainedDelaunayTriangulation::<V>::bulk_load(verts) else {
        return (vec![], MeshReport { points_used: 0, duplicates: dups, triangles: 0, constraints_skipped: 0 });
    };
    let mut handle = HashMap::new();
    for v in cdt.vertices() {
        handle.insert(v.data().idx, v.fix());
    }
    // constraint edges: breaklines, Umring, Inringe
    let mut segs: Vec<(usize, usize)> = Vec::new();
    if st.params.use_breaklines {
        for b in &st.breaklines {
            for w in b.pts.windows(2) {
                segs.push((w[0], w[1]));
            }
        }
    }
    let mut ring_edges = |r: &Vec<usize>| {
        for i in 0..r.len() {
            segs.push((r[i], r[(i + 1) % r.len()]));
        }
    };
    if st.umring.len() >= 3 {
        ring_edges(&st.umring);
    }
    for r in &st.inrings {
        if r.len() >= 3 {
            ring_edges(r);
        }
    }
    let mut skipped = 0;
    let mut cons: HashSet<(usize, usize)> = HashSet::new();
    for (a, b) in segs {
        let (Some(&ha), Some(&hb)) = (handle.get(&a), handle.get(&b)) else {
            skipped += 1;
            continue;
        };
        if ha == hb {
            continue;
        }
        if cdt.can_add_constraint(ha, hb) {
            cdt.add_constraint(ha, hb);
            cons.insert((a.min(b), a.max(b)));
        } else {
            skipped += 1;
        }
    }
    let poly = |r: &Vec<usize>| -> Vec<(f64, f64)> { r.iter().map(|&i| (st.points[i].x, st.points[i].y)).collect() };
    let um = if st.umring.len() >= 3 { Some(poly(&st.umring)) } else { None };
    let ins: Vec<Vec<(f64, f64)>> = st.inrings.iter().filter(|r| r.len() >= 3).map(poly).collect();
    let max2 = st.params.max_side * st.params.max_side;
    let mut tris: Vec<[usize; 3]> = Vec::new();
    for f in cdt.inner_faces() {
        let vs = f.vertices();
        let ids = [vs[0].data().idx, vs[1].data().idx, vs[2].data().idx];
        let p = |i: usize| &st.points[ids[i]];
        let mut long = false;
        for e in 0..3 {
            let (a, b) = (ids[e], ids[(e + 1) % 3]);
            let d2 = (p(e).x - p((e + 1) % 3).x).powi(2) + (p(e).y - p((e + 1) % 3).y).powi(2);
            if d2 > max2 && !cons.contains(&(a.min(b), a.max(b))) {
                long = true;
            }
        }
        if long {
            continue;
        }
        let cx = (p(0).x + p(1).x + p(2).x) / 3.0;
        let cy = (p(0).y + p(1).y + p(2).y) / 3.0;
        if let Some(u) = &um {
            if !point_in_poly(cx, cy, u) {
                continue;
            }
        }
        if ins.iter().any(|r| point_in_poly(cx, cy, r)) {
            continue;
        }
        tris.push(ids);
    }
    // remove narrow border triangles (min. Dreieckswinkel, gon), repeatedly
    let min_ang = st.params.min_angle_gon * std::f64::consts::PI / 200.0;
    if min_ang > 0.0 {
        for _ in 0..50 {
            let mut edge_count: HashMap<(usize, usize), u32> = HashMap::new();
            for t in &tris {
                for e in 0..3 {
                    let (a, b) = (t[e], t[(e + 1) % 3]);
                    *edge_count.entry((a.min(b), a.max(b))).or_default() += 1;
                }
            }
            let before = tris.len();
            tris.retain(|t| {
                let border = (0..3).any(|e| {
                    let (a, b) = (t[e], t[(e + 1) % 3]);
                    edge_count[&(a.min(b), a.max(b))] == 1 && !cons.contains(&(a.min(b), a.max(b)))
                });
                !(border && min_angle(st, t) < min_ang)
            });
            if tris.len() == before {
                break;
            }
        }
    }
    let n = tris.len();
    (
        tris.into_iter().map(|v| Tri { v, active: true }).collect(),
        MeshReport { points_used: used.len(), duplicates: dups, triangles: n, constraints_skipped: skipped },
    )
}

fn min_angle(st: &crate::model::ProjectState, t: &[usize; 3]) -> f64 {
    let p: Vec<(f64, f64)> = t.iter().map(|&i| (st.points[i].x, st.points[i].y)).collect();
    let mut m = f64::MAX;
    for i in 0..3 {
        let a = p[i];
        let b = p[(i + 1) % 3];
        let c = p[(i + 2) % 3];
        let v1 = (b.0 - a.0, b.1 - a.1);
        let v2 = (c.0 - a.0, c.1 - a.1);
        let l = (v1.0.hypot(v1.1) * v2.0.hypot(v2.1)).max(1e-12);
        let cos = ((v1.0 * v2.0 + v1.1 * v2.1) / l).clamp(-1.0, 1.0);
        m = m.min(cos.acos());
    }
    m
}

/// The model surface: a continuous, curved surface through all points (kinks only at hard breaklines).
pub struct Surface {
    pub ox: f64,
    pub oy: f64,
    /// local coordinates and heights of all project points
    pub pos: Vec<[f64; 3]>,
    pub tris: Vec<[usize; 3]>,
    /// per triangle, per edge (opposite vertex k): (slope_i, slope_j) along the edge for the Hermite bulge
    edge_slopes: Vec<[(f64, f64, f64); 3]>,
    grid: HashMap<(i64, i64), Vec<u32>>,
    cell: f64,
    pub zmin: f64,
    pub zmax: f64,
    pub curved: bool,
    /// smoothed gradient at every point (for shading)
    pub vgrad: Vec<[f64; 2]>,
}

impl Surface {
    pub fn new(pr: &Project, curved: bool) -> Surface {
        let st = &pr.st;
        let (ox, oy) = pr.bbox().map(|b| (b[0], b[1])).unwrap_or((0.0, 0.0));
        let pos: Vec<[f64; 3]> = st.points.iter().map(|p| [p.x - ox, p.y - oy, p.z]).collect();
        let tris: Vec<[usize; 3]> = st
            .triangles
            .iter()
            .filter(|t| t.active && t.v.iter().all(|&i| i < pos.len()))
            .map(|t| {
                // keep counter-clockwise order
                let [a, b, c] = t.v;
                let cr = (pos[b][0] - pos[a][0]) * (pos[c][1] - pos[a][1]) - (pos[b][1] - pos[a][1]) * (pos[c][0] - pos[a][0]);
                if cr < 0.0 {
                    [a, c, b]
                } else {
                    [a, b, c]
                }
            })
            .collect();
        // triangles around each vertex, and border edges (used by one triangle only)
        let mut incident: Vec<Vec<usize>> = vec![Vec::new(); pos.len()];
        let mut edge_use: HashMap<(usize, usize), u32> = HashMap::new();
        for (ti, t) in tris.iter().enumerate() {
            for e in 0..3 {
                incident[t[e]].push(ti);
                let (a, b) = (t[e], t[(e + 1) % 3]);
                *edge_use.entry((a.min(b), a.max(b))).or_default() += 1;
            }
        }
        let mut border_v = vec![false; pos.len()];
        for (&(a, b), &n) in &edge_use {
            if n == 1 {
                border_v[a] = true;
                border_v[b] = true;
            }
        }
        // hard breaklines
        let mut hard_v = vec![false; pos.len()];
        let mut hard_e: HashSet<(usize, usize)> = HashSet::new();
        if st.params.use_breaklines {
            for b in st.breaklines.iter().filter(|b| !b.soft) {
                for w in b.pts.windows(2) {
                    hard_e.insert((w[0].min(w[1]), w[0].max(w[1])));
                    hard_v[w[0]] = true;
                    hard_v[w[1]] = true;
                }
            }
        }
        // least-squares gradient of the plane through vertex i and the given neighbours
        let fit = |i: usize, nbs: &[usize]| -> [f64; 2] {
            let (mut sxx, mut sxy, mut syy, mut sxz, mut syz) = (0.0, 0.0, 0.0, 0.0, 0.0);
            for &j in nbs {
                let dx = pos[j][0] - pos[i][0];
                let dy = pos[j][1] - pos[i][1];
                let dz = pos[j][2] - pos[i][2];
                let w = 1.0 / (dx * dx + dy * dy).max(1e-9);
                sxx += w * dx * dx;
                sxy += w * dx * dy;
                syy += w * dy * dy;
                sxz += w * dx * dz;
                syz += w * dy * dz;
            }
            let det = sxx * syy - sxy * sxy;
            if nbs.len() < 2 || det.abs() < 1e-9 * (sxx * syy).max(1e-30) {
                [0.0, 0.0]
            } else {
                let g = [(sxz * syy - syz * sxy) / det, (syz * sxx - sxz * sxy) / det];
                // at the border the fit only sees one side: damp it (Randproblem)
                if border_v[i] {
                    [g[0] * 0.5, g[1] * 0.5]
                } else {
                    g
                }
            }
        };
        let verts_of = |tl: &[usize], skip: usize| -> Vec<usize> {
            let mut v: Vec<usize> = Vec::new();
            for &t in tl {
                for &k in &tris[t] {
                    if k != skip && !v.contains(&k) {
                        v.push(k);
                    }
                }
            }
            v
        };
        let grad: Vec<[f64; 2]> = (0..pos.len()).map(|i| fit(i, &verts_of(&incident[i], i))).collect();
        // On a hard breakline a vertex has one gradient per side: only the triangles reachable
        // around it without crossing a hard edge are used (Bruchkanten).
        let sector_grad = |v: usize, t0: usize| -> [f64; 2] {
            let mut sector = vec![t0];
            let mut k = 0;
            while k < sector.len() {
                let t = tris[sector[k]];
                for &w in &t {
                    if w == v || hard_e.contains(&(v.min(w), v.max(w))) {
                        continue;
                    }
                    for &u in &incident[v] {
                        if !sector.contains(&u) && tris[u].contains(&w) {
                            sector.push(u);
                        }
                    }
                }
                k += 1;
            }
            fit(v, &verts_of(&sector, v))
        };
        let edge_slopes = tris
            .iter()
            .enumerate()
            .map(|(ti, t)| {
                let mut r = [(0.0, 0.0, 0.0); 3];
                for k in 0..3 {
                    let i = t[(k + 1) % 3];
                    let j = t[(k + 2) % 3];
                    if !curved || hard_e.contains(&(i.min(j), i.max(j))) {
                        continue; // straight along a hard breakline: the model has a kink here
                    }
                    let gi = if hard_v[i] { sector_grad(i, ti) } else { grad[i] };
                    let gj = if hard_v[j] { sector_grad(j, ti) } else { grad[j] };
                    let dx = pos[j][0] - pos[i][0];
                    let dy = pos[j][1] - pos[i][1];
                    let d = pos[j][2] - pos[i][2];
                    let len = dx.hypot(dy);
                    let si = gi[0] * dx + gi[1] * dy;
                    let sj = gj[0] * dx + gj[1] * dy;
                    // limit of the edge bulge: half the height difference plus a share of the slope
                    let gmin = (gi[0].hypot(gi[1])).min(gj[0].hypot(gj[1]));
                    let mut bound = 0.5 * d.abs() + 0.05 * len * gmin;
                    if edge_use.get(&(i.min(j), i.max(j))).copied().unwrap_or(0) == 1 {
                        bound *= 0.5; // border edge
                    }
                    r[k] = (si, sj, bound);
                }
                r
            })
            .collect();
        let mut zmin = f64::MAX;
        let mut zmax = f64::MIN;
        for t in &tris {
            for &i in t {
                zmin = zmin.min(pos[i][2]);
                zmax = zmax.max(pos[i][2]);
            }
        }
        if zmin > zmax {
            zmin = 0.0;
            zmax = 1.0;
        }
        // spatial grid for point location
        let (mut bx0, mut by0, mut bx1, mut by1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
        for t in &tris {
            for &i in t {
                bx0 = bx0.min(pos[i][0]);
                by0 = by0.min(pos[i][1]);
                bx1 = bx1.max(pos[i][0]);
                by1 = by1.max(pos[i][1]);
            }
        }
        let area = ((bx1 - bx0) * (by1 - by0)).max(1.0);
        let cell = (area / (tris.len().max(1) as f64)).sqrt() * 2.0;
        let cell = if cell.is_finite() && cell > 0.0 { cell } else { 10.0 };
        let mut grid: HashMap<(i64, i64), Vec<u32>> = HashMap::new();
        for (ti, t) in tris.iter().enumerate() {
            let xs = t.map(|i| pos[i][0]);
            let ys = t.map(|i| pos[i][1]);
            let x0 = (xs.iter().cloned().fold(f64::MAX, f64::min) / cell).floor() as i64;
            let x1 = (xs.iter().cloned().fold(f64::MIN, f64::max) / cell).floor() as i64;
            let y0 = (ys.iter().cloned().fold(f64::MAX, f64::min) / cell).floor() as i64;
            let y1 = (ys.iter().cloned().fold(f64::MIN, f64::max) / cell).floor() as i64;
            for gx in x0..=x1 {
                for gy in y0..=y1 {
                    grid.entry((gx, gy)).or_default().push(ti as u32);
                }
            }
        }
        let vgrad = grad.clone();
        Surface { ox, oy, pos, tris, edge_slopes, grid, cell, zmin, zmax, curved, vgrad }
    }

    /// Height inside triangle `t` at barycentric coordinates `b` (matching the triangle's vertex order).
    pub fn eval(&self, t: usize, b: [f64; 3]) -> f64 {
        let v = self.tris[t];
        let z = [self.pos[v[0]][2], self.pos[v[1]][2], self.pos[v[2]][2]];
        let mut h = b[0] * z[0] + b[1] * z[1] + b[2] * z[2];
        if !self.curved {
            return h;
        }
        // Rational ("hyperbolic") blending of the three edge bulges: the weight of edge k is
        // (b_i·b_j)² / Σ, so on an edge only that edge counts (smooth, matching the neighbour) and
        // inside the triangle the result is a convex mix of the edge bulges, each limited by tanh,
        // so the interior never overshoots.
        let w = [(b[1] * b[2]).powi(2), (b[2] * b[0]).powi(2), (b[0] * b[1]).powi(2)];
        let sw = w[0] + w[1] + w[2];
        if sw <= 1e-30 {
            return h;
        }
        for k in 0..3 {
            let i = (k + 1) % 3;
            let j = (k + 2) % 3;
            let (si, sj, bound) = self.edge_slopes[t][k];
            if bound <= 0.0 || w[k] == 0.0 {
                continue;
            }
            let s = b[i] + b[j];
            if s <= 1e-12 {
                continue;
            }
            let u = b[j] / s;
            let d = z[j] - z[i];
            // cubic Hermite along the edge minus the straight line
            let dev = (u * u * u - 2.0 * u * u + u) * si + (u * u * u - u * u) * sj + (3.0 * u * u - 2.0 * u * u * u - u) * d;
            h += (w[k] / sw) * bound * (dev / bound).tanh();
        }
        h
    }

    pub fn locate(&self, lx: f64, ly: f64) -> Option<(usize, [f64; 3])> {
        let key = ((lx / self.cell).floor() as i64, (ly / self.cell).floor() as i64);
        let cand = self.grid.get(&key)?;
        for &ti in cand {
            let t = self.tris[ti as usize];
            let (a, b, c) = (self.pos[t[0]], self.pos[t[1]], self.pos[t[2]]);
            let det = (b[1] - c[1]) * (a[0] - c[0]) + (c[0] - b[0]) * (a[1] - c[1]);
            if det.abs() < 1e-14 {
                continue;
            }
            let l1 = ((b[1] - c[1]) * (lx - c[0]) + (c[0] - b[0]) * (ly - c[1])) / det;
            let l2 = ((c[1] - a[1]) * (lx - c[0]) + (a[0] - c[0]) * (ly - c[1])) / det;
            let l3 = 1.0 - l1 - l2;
            let e = -1e-9;
            if l1 >= e && l2 >= e && l3 >= e {
                return Some((ti as usize, [l1.max(0.0), l2.max(0.0), l3.max(0.0)]));
            }
        }
        None
    }

    /// Height at world coordinates.
    pub fn height(&self, x: f64, y: f64) -> Option<f64> {
        let (t, b) = self.locate(x - self.ox, y - self.oy)?;
        Some(self.eval(t, b))
    }

    /// Slope (gradient) at world coordinates by central differences.
    pub fn gradient(&self, x: f64, y: f64, h: f64) -> Option<[f64; 2]> {
        let zx1 = self.height(x + h, y).or_else(|| self.height(x, y))?;
        let zx0 = self.height(x - h, y).or_else(|| self.height(x, y))?;
        let zy1 = self.height(x, y + h).or_else(|| self.height(x, y))?;
        let zy0 = self.height(x, y - h).or_else(|| self.height(x, y))?;
        Some([(zx1 - zx0) / (2.0 * h), (zy1 - zy0) / (2.0 * h)])
    }
}
