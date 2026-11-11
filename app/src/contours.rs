//! Contour lines (Höhenlinien): subdivided triangles, linear interpolation in the sub-triangles,
//! chaining, thinning (Ausdünnung) and smoothing (Glättung 0..10). See MECHANICS.md.

use std::collections::HashMap;

use crate::model::Params;
use crate::tin::Surface;

#[derive(Clone, Copy)]
pub struct Label {
    /// position along the line (arc length)
    pub s: f64,
    pub pos: [f64; 2],
    /// direction of the line at the label (radians, local coordinates, y up)
    pub angle: f64,
}

#[derive(Clone)]
pub struct ContourLine {
    pub labels: Vec<Label>,
    pub z: f64,
    pub index: bool,
    pub closed: bool,
    /// local coordinates (world minus surface origin)
    pub pts: Vec<[f64; 2]>,
    pub bbox: [f64; 4],
    /// cumulative length along the line
    pub len: Vec<f64>,
}

pub struct Contours {
    pub lines: Vec<ContourLine>,
    pub interval: f64,
    /// Anzahl der Teilstücke vor dem Verketten (Diagnose, wird in Tests ausgewertet)
    #[cfg_attr(not(test), allow(dead_code))]
    pub segments: usize,
    /// typical horizontal distance between neighbouring contours (interval / median slope)
    pub typical_spacing: f64,
}

type Key = (i64, i64);

fn key(p: [f64; 2]) -> Key {
    ((p[0] * 1e4).round() as i64, (p[1] * 1e4).round() as i64)
}

pub fn compute(s: &Surface, p: &Params) -> Contours {
    let dz = if p.contour_interval > 0.0 { p.contour_interval } else { 1.0 };
    let n = p.subdivision.max(1) as usize;
    let mut first = (s.zmin / dz).ceil() as i64;
    let mut last = (s.zmax / dz).floor() as i64;
    if last - first > 3000 {
        // too many levels for this interval: keep the count sane
        let mid = (first + last) / 2;
        first = mid - 1500;
        last = mid + 1500;
    }
    let mut segs: HashMap<i64, Vec<([f64; 2], [f64; 2])>> = HashMap::new();
    let mut nseg = 0usize;
    let mut sub_pos: Vec<[f64; 3]> = Vec::new();
    let mut idx = vec![vec![0usize; n + 1]; n + 1];
    for (ti, t) in s.tris.iter().enumerate() {
        let (a, b, c) = (s.pos[t[0]], s.pos[t[1]], s.pos[t[2]]);
        let tz0 = a[2].min(b[2]).min(c[2]);
        let tz1 = a[2].max(b[2]).max(c[2]);
        // a quick reject: the curved surface stays near the corner heights
        let pad = (tz1 - tz0) * 0.5 + 1e-9;
        if tz1 + pad < first as f64 * dz || tz0 - pad > last as f64 * dz {
            continue;
        }
        sub_pos.clear();
        for i in 0..=n {
            for j in 0..=(n - i) {
                let k = n - i - j;
                let (fk, fi, fj) = (k as f64, i as f64, j as f64);
                let x = (a[0] * fk + b[0] * fi + c[0] * fj) / n as f64;
                let y = (a[1] * fk + b[1] * fi + c[1] * fj) / n as f64;
                let z = if n == 1 && !s.curved {
                    [a[2], b[2], c[2]][if i == 1 { 1 } else if j == 1 { 2 } else { 0 }]
                } else {
                    s.eval(ti, [fk / n as f64, fi / n as f64, fj / n as f64])
                };
                idx[i][j] = sub_pos.len();
                sub_pos.push([x, y, z]);
            }
        }
        let mut emit = |p0: [f64; 3], p1: [f64; 3], p2: [f64; 3]| {
            let lo = p0[2].min(p1[2]).min(p2[2]);
            let hi = p0[2].max(p1[2]).max(p2[2]);
            let l0 = ((lo / dz).ceil() as i64).max(first);
            let l1 = ((hi / dz).floor() as i64).min(last);
            for l in l0..=l1 {
                let lv = l as f64 * dz;
                let v = [p0, p1, p2];
                let above: Vec<bool> = v.iter().map(|q| q[2] >= lv).collect();
                let na = above.iter().filter(|x| **x).count();
                if na == 0 || na == 3 {
                    continue;
                }
                let mut cross = Vec::with_capacity(2);
                for e in 0..3 {
                    let (u, w) = (v[e], v[(e + 1) % 3]);
                    if above[e] != above[(e + 1) % 3] {
                        // interpolate from a canonical end so neighbours get identical points
                        let (u, w) = if (u[0], u[1]) < (w[0], w[1]) { (u, w) } else { (w, u) };
                        let f = (lv - u[2]) / (w[2] - u[2]);
                        cross.push([u[0] + (w[0] - u[0]) * f, u[1] + (w[1] - u[1]) * f]);
                    }
                }
                if cross.len() == 2 && key(cross[0]) != key(cross[1]) {
                    segs.entry(l).or_default().push((cross[0], cross[1]));
                    nseg += 1;
                }
            }
        };
        for i in 0..n {
            for j in 0..(n - i) {
                emit(sub_pos[idx[i][j]], sub_pos[idx[i + 1][j]], sub_pos[idx[i][j + 1]]);
                if i + j + 2 <= n {
                    emit(sub_pos[idx[i + 1][j]], sub_pos[idx[i + 1][j + 1]], sub_pos[idx[i][j + 1]]);
                }
            }
        }
    }
    let m_per_mm = p.scale / 1000.0;
    let mut lines = Vec::new();
    let mut levels: Vec<i64> = segs.keys().copied().collect();
    levels.sort();
    let every = p.index_every.max(1) as i64;
    for l in levels {
        let sv = &segs[&l];
        for (pts, closed) in chain(sv) {
            let pts = thin(&pts, closed, p.thin_mm * m_per_mm, p.min_dist_mm * m_per_mm * 10.0);
            let pts = smooth(&pts, closed, p.smoothing, (p.min_dist_mm * m_per_mm).max(1e-3));
            if pts.len() < 2 {
                continue;
            }
            lines.push(make_line(l as f64 * dz, l.rem_euclid(every) == 0, closed, pts));
        }
    }
    let mut slopes: Vec<f64> = s.vgrad.iter().map(|g| g[0].hypot(g[1])).filter(|v| *v > 0.0).collect();
    slopes.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let med = slopes.get(slopes.len() / 2).copied().unwrap_or(0.1).max(1e-3);
    Contours { lines, interval: dz, segments: nseg, typical_spacing: dz / med }
}

pub fn make_line(z: f64, index: bool, closed: bool, pts: Vec<[f64; 2]>) -> ContourLine {
    let mut bb = [f64::MAX, f64::MAX, f64::MIN, f64::MIN];
    let mut len = Vec::with_capacity(pts.len());
    let mut acc = 0.0;
    for (i, q) in pts.iter().enumerate() {
        bb[0] = bb[0].min(q[0]);
        bb[1] = bb[1].min(q[1]);
        bb[2] = bb[2].max(q[0]);
        bb[3] = bb[3].max(q[1]);
        if i > 0 {
            acc += (q[0] - pts[i - 1][0]).hypot(q[1] - pts[i - 1][1]);
        }
        len.push(acc);
    }
    ContourLine { labels: Vec::new(), z, index, closed, pts, bbox: bb, len }
}

fn point_at(l: &ContourLine, s: f64) -> [f64; 2] {
    let i = match l.len.binary_search_by(|v| v.partial_cmp(&s).unwrap()) {
        Ok(i) => return l.pts[i],
        Err(i) => i.clamp(1, l.pts.len() - 1),
    };
    let (a, b) = (l.pts[i - 1], l.pts[i]);
    let seg = (l.len[i] - l.len[i - 1]).max(1e-12);
    let f = ((s - l.len[i - 1]) / seg).clamp(0.0, 1.0);
    [a[0] + (b[0] - a[0]) * f, a[1] + (b[1] - a[1]) * f]
}

/// Place the labels of the index contours: about every `spacing` along the line (20 % of the
/// largest extent of the model, as the person asked), each moved to the least curved spot near it.
/// `half` is half the label length in metres.
pub fn place_labels(c: &mut Contours, extent: f64, half: f64) {
    let spacing = (extent * 0.2).max(half * 6.0);
    for l in c.lines.iter_mut().filter(|l| l.index) {
        l.labels = spots(l, spacing, half);
    }
}

fn spots(l: &ContourLine, spacing: f64, half: f64) -> Vec<Label> {
    {
        let total = *l.len.last().unwrap_or(&0.0);
        if total < half * 4.0 {
            return Vec::new();
        }
        // curvature at s: turning angle between the chords before and after the label
        let bend = |s: f64| -> f64 {
            let a = point_at(l, s - half);
            let m = point_at(l, s);
            let b = point_at(l, s + half);
            let a1 = (m[1] - a[1]).atan2(m[0] - a[0]);
            let a2 = (b[1] - m[1]).atan2(b[0] - m[0]);
            let mut d = (a2 - a1).abs();
            if d > std::f64::consts::PI {
                d = 2.0 * std::f64::consts::PI - d;
            }
            // also look at the wider surroundings
            let a0 = point_at(l, s - 2.0 * half);
            let b0 = point_at(l, s + 2.0 * half);
            let w1 = (a[1] - a0[1]).atan2(a[0] - a0[0]);
            let w2 = (b0[1] - b[1]).atan2(b0[0] - b[0]);
            let mut dw = (w2 - w1).abs();
            if dw > std::f64::consts::PI {
                dw = 2.0 * std::f64::consts::PI - dw;
            }
            d + 0.5 * dw
        };
        let (lo, hi) = if l.closed { (0.0, total) } else { (2.0 * half, total - 2.0 * half) };
        if hi <= lo {
            return Vec::new();
        }
        let mut found = Vec::new();
        let count = ((hi - lo) / spacing).floor().max(0.0) as usize + 1;
        let step = (hi - lo) / count as f64;
        for k in 0..count {
            let target = lo + step * (k as f64 + 0.5);
            let search = step * 0.4;
            let mut best = (f64::MAX, target);
            let n = 24;
            for j in 0..=n {
                let s = (target - search + 2.0 * search * j as f64 / n as f64).clamp(lo, hi);
                // prefer flat spots, slightly prefer the even spacing
                let cost = bend(s) + 0.05 * ((s - target).abs() / search.max(1e-9));
                if cost < best.0 {
                    best = (cost, s);
                }
            }
            let s = best.1;
            if best.0 > 0.6 {
                continue; // nowhere flat enough here
            }
            let a = point_at(l, s - half);
            let b = point_at(l, s + half);
            found.push(Label { s, pos: point_at(l, s), angle: (b[1] - a[1]).atan2(b[0] - a[0]) });
        }
        found
    }
}

/// Join segments that share end points into polylines.
pub fn chain(segs: &[([f64; 2], [f64; 2])]) -> Vec<(Vec<[f64; 2]>, bool)> {
    let mut at: HashMap<Key, Vec<(usize, bool)>> = HashMap::new();
    for (i, (a, b)) in segs.iter().enumerate() {
        at.entry(key(*a)).or_default().push((i, false));
        at.entry(key(*b)).or_default().push((i, true));
    }
    let mut used = vec![false; segs.len()];
    let mut out = Vec::new();
    let next = |k: Key, used: &Vec<bool>, at: &HashMap<Key, Vec<(usize, bool)>>| -> Option<(usize, bool)> {
        at.get(&k)?.iter().find(|(i, _)| !used[*i]).copied()
    };
    for s0 in 0..segs.len() {
        if used[s0] {
            continue;
        }
        used[s0] = true;
        let mut line: std::collections::VecDeque<[f64; 2]> = std::collections::VecDeque::new();
        line.push_back(segs[s0].0);
        line.push_back(segs[s0].1);
        // forward
        loop {
            let k = key(*line.back().unwrap());
            let Some((i, at_end)) = next(k, &used, &at) else { break };
            used[i] = true;
            let p = if at_end { segs[i].0 } else { segs[i].1 };
            line.push_back(p);
        }
        // backward
        loop {
            let k = key(*line.front().unwrap());
            let Some((i, at_end)) = next(k, &used, &at) else { break };
            used[i] = true;
            let p = if at_end { segs[i].0 } else { segs[i].1 };
            line.push_front(p);
        }
        let v: Vec<[f64; 2]> = line.into_iter().collect();
        let closed = v.len() > 3 && key(v[0]) == key(*v.last().unwrap());
        let mut v = v;
        if closed {
            v.pop();
        }
        out.push((v, closed));
    }
    out
}

/// Ausdünnung: drop the middle of three points when its height over the chord is below `tol`,
/// unless the chord would get longer than `max_gap`.
fn thin(pts: &[[f64; 2]], closed: bool, tol: f64, max_gap: f64) -> Vec<[f64; 2]> {
    if pts.len() < 3 || tol <= 0.0 {
        return pts.to_vec();
    }
    let mut out = vec![pts[0]];
    let mut i = 1;
    while i < pts.len() - 1 {
        let a = *out.last().unwrap();
        let b = pts[i];
        let c = pts[i + 1];
        let ac = (c[0] - a[0]).hypot(c[1] - a[1]);
        let h = if ac > 1e-12 { ((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])).abs() / ac } else { 0.0 };
        if !(h < tol && ac < max_gap) {
            out.push(b);
        }
        i += 1;
    }
    out.push(*pts.last().unwrap());
    if closed && out.len() < 3 {
        return pts.to_vec();
    }
    out
}

/// Glättung 0..10 (see MECHANICS.md): 1..7 interpolating spline with decreasing rounding,
/// 8/9 approximating spline (9 further from the points), 10 Akima.
pub fn smooth(pts: &[[f64; 2]], closed: bool, g: u32, spacing: f64) -> Vec<[f64; 2]> {
    if g == 0 || pts.len() < 3 {
        let mut v = pts.to_vec();
        if closed && !v.is_empty() {
            v.push(v[0]);
        }
        return v;
    }
    let n = pts.len();
    let get = |i: isize| -> [f64; 2] {
        if closed {
            pts[i.rem_euclid(n as isize) as usize]
        } else {
            pts[i.clamp(0, n as isize - 1) as usize]
        }
    };
    let segs = if closed { n } else { n - 1 };
    match g {
        1..=7 => {
            let tension = (g as f64 - 1.0) * 0.14;
            let mut out = Vec::new();
            for i in 0..segs as isize {
                let (p0, p1, p2, p3) = (get(i - 1), get(i), get(i + 1), get(i + 2));
                let d = (p2[0] - p1[0]).hypot(p2[1] - p1[1]);
                let steps = ((d / spacing).ceil() as usize).clamp(1, 32);
                let m1 = [(p2[0] - p0[0]) * 0.5 * (1.0 - tension), (p2[1] - p0[1]) * 0.5 * (1.0 - tension)];
                let m2 = [(p3[0] - p1[0]) * 0.5 * (1.0 - tension), (p3[1] - p1[1]) * 0.5 * (1.0 - tension)];
                // limit tangents to the local chord to avoid loops
                let lim = |m: [f64; 2]| {
                    let l = m[0].hypot(m[1]);
                    if l > d * 1.2 && l > 0.0 {
                        [m[0] * d * 1.2 / l, m[1] * d * 1.2 / l]
                    } else {
                        m
                    }
                };
                let (m1, m2) = (lim(m1), lim(m2));
                for s in 0..steps {
                    let t = s as f64 / steps as f64;
                    let (t2, t3) = (t * t, t * t * t);
                    let h00 = 2.0 * t3 - 3.0 * t2 + 1.0;
                    let h10 = t3 - 2.0 * t2 + t;
                    let h01 = -2.0 * t3 + 3.0 * t2;
                    let h11 = t3 - t2;
                    out.push([
                        h00 * p1[0] + h10 * m1[0] + h01 * p2[0] + h11 * m2[0],
                        h00 * p1[1] + h10 * m1[1] + h01 * p2[1] + h11 * m2[1],
                    ]);
                }
            }
            out.push(if closed { pts[0] } else { pts[n - 1] });
            out
        }
        8 | 9 => {
            // approximating B-spline by corner cutting; 9 cuts more (lies further from the points)
            let iters = if g == 8 { 2 } else { 4 };
            let mut v = pts.to_vec();
            for _ in 0..iters {
                let m = v.len();
                let mut nv = Vec::with_capacity(m * 2);
                if !closed {
                    nv.push(v[0]);
                }
                let segs = if closed { m } else { m - 1 };
                for i in 0..segs {
                    let a = v[i];
                    let b = v[(i + 1) % m];
                    nv.push([0.75 * a[0] + 0.25 * b[0], 0.75 * a[1] + 0.25 * b[1]]);
                    nv.push([0.25 * a[0] + 0.75 * b[0], 0.25 * a[1] + 0.75 * b[1]]);
                }
                if !closed {
                    nv.push(v[m - 1]);
                }
                v = nv;
            }
            if closed {
                v.push(v[0]);
            }
            v
        }
        _ => akima(pts, closed, spacing),
    }
}

fn akima(pts: &[[f64; 2]], closed: bool, spacing: f64) -> Vec<[f64; 2]> {
    let mut p: Vec<[f64; 2]> = pts.to_vec();
    if closed {
        p.push(pts[0]);
    }
    let n = p.len();
    if n < 3 {
        return p;
    }
    let mut t = vec![0.0; n];
    for i in 1..n {
        t[i] = t[i - 1] + (p[i][0] - p[i - 1][0]).hypot(p[i][1] - p[i - 1][1]).max(1e-9);
    }
    let slopes = |c: usize| -> Vec<f64> {
        let m: Vec<f64> = (0..n - 1).map(|i| (p[i + 1][c] - p[i][c]) / (t[i + 1] - t[i])).collect();
        let mm = |i: isize| -> f64 {
            let k = m.len() as isize;
            if closed {
                m[i.rem_euclid(k) as usize]
            } else if i < 0 {
                2.0 * m[0] - m[1.min(m.len() - 1)]
            } else if i >= k {
                2.0 * m[(k - 1) as usize] - m[(k - 2).max(0) as usize]
            } else {
                m[i as usize]
            }
        };
        (0..n as isize)
            .map(|i| {
                let (m0, m1, m2, m3) = (mm(i - 2), mm(i - 1), mm(i), mm(i + 1));
                let w1 = (m3 - m2).abs();
                let w2 = (m1 - m0).abs();
                if w1 + w2 < 1e-12 {
                    0.5 * (m1 + m2)
                } else {
                    (w1 * m1 + w2 * m2) / (w1 + w2)
                }
            })
            .collect()
    };
    let sx = slopes(0);
    let sy = slopes(1);
    let mut out = Vec::new();
    for i in 0..n - 1 {
        let h = t[i + 1] - t[i];
        let steps = ((h / spacing).ceil() as usize).clamp(1, 32);
        for s in 0..steps {
            let u = s as f64 / steps as f64;
            let (u2, u3) = (u * u, u * u * u);
            let h00 = 2.0 * u3 - 3.0 * u2 + 1.0;
            let h10 = u3 - 2.0 * u2 + u;
            let h01 = -2.0 * u3 + 3.0 * u2;
            let h11 = u3 - u2;
            out.push([
                h00 * p[i][0] + h10 * h * sx[i] + h01 * p[i + 1][0] + h11 * h * sx[i + 1],
                h00 * p[i][1] + h10 * h * sy[i] + h01 * p[i + 1][1] + h11 * h * sy[i + 1],
            ]);
        }
    }
    out.push(p[n - 1]);
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn da_dgm1_stats() {
        let dir = std::path::PathBuf::from(r"C:\dgm\daten");
        if !dir.exists() {
            return;
        }
        let mut pr = crate::model::Project::load_original(dir, "DA-DGM1").unwrap();
        let (t, rep) = crate::tin::triangulate(&pr);
        pr.st.triangles = t;
        println!("tris {} skipped {}", rep.triangles, rep.constraints_skipped);
        println!("params {:?}", pr.st.params);
        let s = crate::tin::Surface::new(&pr, true);
        let c = super::compute(&s, &pr.st.params);
        println!("segments {} lines {}", c.segments, c.lines.len());
        let mut lv: std::collections::BTreeMap<i64, (usize, usize)> = Default::default();
        for l in &c.lines {
            let e = lv.entry((l.z * 100.0) as i64).or_default();
            e.0 += 1;
            e.1 += l.pts.len();
        }
        for (k, v) in lv.iter().take(12) {
            println!("z {} lines {} pts {}", *k as f64 / 100.0, v.0, v.1);
        }
    }
}
