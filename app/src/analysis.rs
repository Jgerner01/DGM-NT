//! Volume (prism method), profiles and heights for points.

use crate::tin::Surface;

pub struct VolumeResult {
    pub ref_h: f64,
    pub area_2d: f64,
    pub area_3d: f64,
    pub vol_above: f64,
    pub vol_below: f64,
    pub area_above: f64,
    pub area_below: f64,
    pub triangles: usize,
}

/// Prism volume of the (planar) triangles against a horizontal reference height,
/// split into the parts above and below it. With `subdiv` > 1 the curved surface is used.
pub fn volume(s: &Surface, ref_h: f64, subdiv: usize) -> VolumeResult {
    let n = subdiv.max(1);
    let mut r = VolumeResult { ref_h, area_2d: 0.0, area_3d: 0.0, vol_above: 0.0, vol_below: 0.0, area_above: 0.0, area_below: 0.0, triangles: s.tris.len() };
    for (ti, t) in s.tris.iter().enumerate() {
        let (a, b, c) = (s.pos[t[0]], s.pos[t[1]], s.pos[t[2]]);
        let p = |i: usize, j: usize| -> [f64; 3] {
            let k = n - i - j;
            let (fk, fi, fj) = (k as f64 / n as f64, i as f64 / n as f64, j as f64 / n as f64);
            [
                a[0] * fk + b[0] * fi + c[0] * fj,
                a[1] * fk + b[1] * fi + c[1] * fj,
                if n == 1 { a[2] * fk + b[2] * fi + c[2] * fj } else { s.eval(ti, [fk, fi, fj]) },
            ]
        };
        for i in 0..n {
            for j in 0..(n - i) {
                prism(&mut r, p(i, j), p(i + 1, j), p(i, j + 1));
                if i + j + 2 <= n {
                    prism(&mut r, p(i + 1, j), p(i + 1, j + 1), p(i, j + 1));
                }
            }
        }
    }
    r
}

fn area2(a: [f64; 3], b: [f64; 3], c: [f64; 3]) -> f64 {
    ((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])).abs() * 0.5
}

fn prism(r: &mut VolumeResult, a: [f64; 3], b: [f64; 3], c: [f64; 3]) {
    let ar = area2(a, b, c);
    r.area_2d += ar;
    let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    let cx = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
    r.area_3d += 0.5 * (cx[0] * cx[0] + cx[1] * cx[1] + cx[2] * cx[2]).sqrt();
    // clip the triangle at the reference plane; the volume of a linear piece is area × mean height
    let h = [a[2] - r.ref_h, b[2] - r.ref_h, c[2] - r.ref_h];
    let pts = [a, b, c];
    let (above, below) = split(&pts, &h);
    for poly in above {
        let (ar, mh) = poly_area_mean(&poly, r.ref_h);
        r.area_above += ar;
        r.vol_above += ar * mh;
    }
    for poly in below {
        let (ar, mh) = poly_area_mean(&poly, r.ref_h);
        r.area_below += ar;
        r.vol_below += ar * -mh;
    }
}

/// Split a triangle into the polygons above and below height 0 (relative heights in `h`).
fn split(p: &[[f64; 3]; 3], h: &[f64; 3]) -> (Vec<Vec<[f64; 3]>>, Vec<Vec<[f64; 3]>>) {
    let mut up = Vec::new();
    let mut dn = Vec::new();
    for i in 0..3 {
        let j = (i + 1) % 3;
        if h[i] >= 0.0 {
            up.push(p[i]);
        } else {
            dn.push(p[i]);
        }
        if (h[i] >= 0.0) != (h[j] >= 0.0) {
            let f = h[i] / (h[i] - h[j]);
            let q = [p[i][0] + (p[j][0] - p[i][0]) * f, p[i][1] + (p[j][1] - p[i][1]) * f, p[i][2] + (p[j][2] - p[i][2]) * f];
            up.push(q);
            dn.push(q);
        }
    }
    let a = if up.len() >= 3 { vec![up] } else { vec![] };
    let b = if dn.len() >= 3 { vec![dn] } else { vec![] };
    (a, b)
}

fn poly_area_mean(poly: &[[f64; 3]], ref_h: f64) -> (f64, f64) {
    // fan triangulation, each with its own mean height
    let mut ar = 0.0;
    let mut vol = 0.0;
    for i in 1..poly.len() - 1 {
        let t = area2(poly[0], poly[i], poly[i + 1]);
        let m = (poly[0][2] + poly[i][2] + poly[i + 1][2]) / 3.0 - ref_h;
        ar += t;
        vol += t * m;
    }
    (ar, if ar > 0.0 { vol / ar } else { 0.0 })
}

#[derive(Clone)]
pub struct ProfilePoint {
    pub station: f64,
    pub x: f64,
    pub y: f64,
    pub z: Option<f64>,
}

/// Längsprofil along a polyline (world coordinates) with a point every `step` metres
/// plus every polyline vertex.
pub fn profile(s: &Surface, line: &[[f64; 2]], step: f64) -> Vec<ProfilePoint> {
    let mut out = Vec::new();
    let step = step.max(0.01);
    let mut st = 0.0;
    for w in line.windows(2) {
        let (a, b) = (w[0], w[1]);
        let d = (b[0] - a[0]).hypot(b[1] - a[1]);
        let n = (d / step).ceil().max(1.0) as usize;
        for k in 0..n {
            let f = k as f64 / n as f64;
            let (x, y) = (a[0] + (b[0] - a[0]) * f, a[1] + (b[1] - a[1]) * f);
            out.push(ProfilePoint { station: st + d * f, x, y, z: s.height(x, y) });
        }
        st += d;
    }
    if let Some(l) = line.last() {
        out.push(ProfilePoint { station: st, x: l[0], y: l[1], z: s.height(l[0], l[1]) });
    }
    out
}

pub fn format_station(s: f64) -> String {
    let km = (s / 1000.0).floor();
    format!("{}+{:06.2}", km as i64, s - km * 1000.0)
}
