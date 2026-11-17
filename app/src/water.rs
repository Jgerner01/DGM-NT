//! Wasserflächen: geschlossene Polygone aus Geländepunkten, über deren Höhen eine vermittelnde (ausgleichende)
//! Ebene gerechnet wird. Die Ebene darf schief sein (Fließrichtung). Dazu Protokoll der Höhenabweichungen
//! und eine Dreieckszerlegung der Fläche für die Farbfüllung.
//! Hardware: keine (reine Rechenmodule).
//! Autor: Johann Gerner

use serde::{Deserialize, Serialize};

use crate::model::Point;

/// Höchstzahl der Wasserflächen eines Projekts
pub const MAX_WATERS: usize = 10;
/// Mindestzahl der Punkte einer Wasserfläche
pub const MIN_WATER_POINTS: usize = 3;
/// Abweichung (m) vom Ausgleichungsergebnis, ab der ein Punkt als Ausreißer gewarnt wird
pub const WATER_TOL_M: f64 = 0.10;
/// Vorgabefarbe der Wasserfläche: helles Cyan
pub const DEFAULT_WATER_RGB: [u8; 3] = [160, 235, 245];
/// Umrechnung Meter nach Zentimeter
const CM_PER_M: f64 = 100.0;
/// Umrechnung Bogenmaß nach Gon
const GON_PER_RAD: f64 = 200.0 / std::f64::consts::PI;
/// Steigung (m/m) unterhalb der keine Fließrichtung angegeben wird
const MIN_FLOW_SLOPE: f64 = 1e-7;
/// Bruchteil der Spur der Normalgleichungen, unter dem die Punkte als auf einer Geraden liegend gelten
const COLLINEAR_EPS: f64 = 1e-9;

/// Eine Wasserfläche: Name und Punktfolge (Indizes in `ProjectState::points`)
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct Water {
    pub name: String,
    pub pts: Vec<usize>,
}

/// Ausgleichungsebene z = z0 + a·(x − x0) + b·(y − y0)
#[derive(Clone, Copy, Debug)]
pub struct PlaneFit {
    pub x0: f64,
    pub y0: f64,
    pub z0: f64,
    pub a: f64,
    pub b: f64,
}

impl PlaneFit {
    pub fn z_at(&self, x: f64, y: f64) -> f64 {
        self.z0 + self.a * (x - self.x0) + self.b * (y - self.y0)
    }

    /// Neigung in Prozent
    pub fn slope_percent(&self) -> f64 {
        self.a.hypot(self.b) * 100.0
    }

    /// Richtung des Gefälles (Fließrichtung) als Richtungswinkel von Nord im Uhrzeigersinn in gon; `None` bei waagrechter Ebene.
    pub fn flow_gon(&self) -> Option<f64> {
        if self.a.hypot(self.b) < MIN_FLOW_SLOPE {
            return None;
        }
        Some(((-self.a).atan2(-self.b) * GON_PER_RAD).rem_euclid(400.0))
    }
}

/// Ausgleichende Ebene nach der Methode der kleinsten Quadrate; `None` bei weniger als 3 Punkten oder Punkten auf einer Geraden.
pub fn fit_plane(pts: &[(f64, f64, f64)]) -> Option<PlaneFit> {
    let n = pts.len() as f64;
    if pts.len() < MIN_WATER_POINTS {
        return None;
    }
    let (mx, my, mz) = (pts.iter().map(|p| p.0).sum::<f64>() / n, pts.iter().map(|p| p.1).sum::<f64>() / n, pts.iter().map(|p| p.2).sum::<f64>() / n);
    let (mut sxx, mut sxy, mut syy, mut sxz, mut syz) = (0.0, 0.0, 0.0, 0.0, 0.0);
    for &(x, y, z) in pts {
        let (dx, dy, dz) = (x - mx, y - my, z - mz);
        sxx += dx * dx;
        sxy += dx * dy;
        syy += dy * dy;
        sxz += dx * dz;
        syz += dy * dz;
    }
    let det = sxx * syy - sxy * sxy;
    if det <= COLLINEAR_EPS * (sxx + syy).powi(2) {
        return None;
    }
    Some(PlaneFit { x0: mx, y0: my, z0: mz, a: (sxz * syy - syz * sxy) / det, b: (syz * sxx - sxz * sxy) / det })
}

/// Eine Zeile des Protokolls: ein Punkt der Wasserfläche
#[derive(Clone, Debug)]
pub struct Residual {
    pub idx: usize,
    /// Höhe des Punktes minus Höhe der Ebene (m)
    pub dz: f64,
    pub plane_z: f64,
    pub outlier: bool,
}

/// Ergebnis der Ausgleichung einer Wasserfläche
#[derive(Clone, Debug, Default)]
pub struct WaterReport {
    pub fit: Option<PlaneFit>,
    pub rows: Vec<Residual>,
    /// Standardabweichung der Höhe einer Beobachtung (m), erst ab 4 Punkten
    pub s0: Option<f64>,
    pub max_abs: f64,
    pub outliers: usize,
}

/// Ausgleichung einer Wasserfläche: Ebene, Abweichungen und Ausreißer (> `WATER_TOL_M`).
pub fn analyze(points: &[Point], w: &Water) -> WaterReport {
    let idx: Vec<usize> = w.pts.iter().copied().filter(|&i| i < points.len()).collect();
    let obs: Vec<(f64, f64, f64)> = idx.iter().map(|&i| (points[i].x, points[i].y, points[i].z)).collect();
    let mut rep = WaterReport { fit: fit_plane(&obs), ..Default::default() };
    let Some(f) = rep.fit else { return rep };
    for &i in &idx {
        let p = &points[i];
        let pz = f.z_at(p.x, p.y);
        let dz = p.z - pz;
        let outlier = dz.abs() > WATER_TOL_M;
        rep.max_abs = rep.max_abs.max(dz.abs());
        rep.outliers += outlier as usize;
        rep.rows.push(Residual { idx: i, dz, plane_z: pz, outlier });
    }
    if idx.len() > MIN_WATER_POINTS {
        rep.s0 = Some((rep.rows.iter().map(|r| r.dz * r.dz).sum::<f64>() / (idx.len() - MIN_WATER_POINTS) as f64).sqrt());
    }
    rep
}

/// Protokoll der Höhenabweichungen aller Wasserflächen als Text.
pub fn protocol(points: &[Point], waters: &[Water]) -> String {
    let mut s = String::new();
    s += "Wasserflächen – Ausgleichung der vermittelnden Ebene\n";
    s += &format!("Toleranz: {:.0} cm (Warnung bei größerer Abweichung)\n", WATER_TOL_M * CM_PER_M);
    for (k, w) in waters.iter().enumerate() {
        let rep = analyze(points, w);
        s += &format!("\n{}. {}  ({} Punkte)\n", k + 1, if w.name.is_empty() { "Wasserfläche" } else { &w.name }, w.pts.len());
        let Some(f) = rep.fit else {
            s += "   Keine Ebene möglich (weniger als 3 Punkte oder alle Punkte auf einer Geraden).\n";
            continue;
        };
        s += &format!("   Ebene: z = {:.3} m im Schwerpunkt (x = {:.2}, y = {:.2})\n", f.z0, f.x0, f.y0);
        match f.flow_gon() {
            Some(g) => s += &format!("   Neigung {:.4} %, Fließrichtung {:.2} gon\n", f.slope_percent(), g),
            None => s += "   Ebene ist waagrecht, keine Fließrichtung\n",
        }
        match rep.s0 {
            Some(v) => s += &format!("   Standardabweichung s0 = {:.1} mm (Redundanz {})\n", v * 1000.0, rep.rows.len() - MIN_WATER_POINTS),
            None => s += "   Keine Redundanz (genau 3 Punkte), keine Standardabweichung\n",
        }
        s += "   Nr  Punkt             x [m]        y [m]      z [m]   z Ebene [m]   dz [cm]\n";
        for (i, r) in rep.rows.iter().enumerate() {
            let p = &points[r.idx];
            s += &format!("  {:>3}  {:<12} {:>12.3} {:>12.3} {:>10.3} {:>11.3} {:>+9.1}{}\n", i + 1, p.name, p.x, p.y, p.z, r.plane_z, r.dz * CM_PER_M, if r.outlier { "  <-- WARNUNG" } else { "" });
        }
        if rep.outliers > 0 {
            s += &format!("   WARNUNG: {} Punkt(e) weichen mehr als {:.0} cm von der Ebene ab (größte Abweichung {:.1} cm).\n", rep.outliers, WATER_TOL_M * CM_PER_M, rep.max_abs * CM_PER_M);
        } else {
            s += &format!("   Alle Punkte innerhalb von {:.0} cm (größte Abweichung {:.1} cm).\n", WATER_TOL_M * CM_PER_M, rep.max_abs * CM_PER_M);
        }
    }
    s
}

/// Dreiecksindizes (in `poly`) der Zerlegung eines einfachen Polygons (Ohrenabschneiden); Drehsinn beliebig.
pub fn triangulate(poly: &[(f64, f64)]) -> Vec<[usize; 3]> {
    let n = poly.len();
    if n < 3 {
        return Vec::new();
    }
    let area2: f64 = (0..n).map(|i| cross2(poly[i], poly[(i + 1) % n])).sum();
    let sign = if area2 >= 0.0 { 1.0 } else { -1.0 };
    let mut rest: Vec<usize> = (0..n).collect();
    let mut out = Vec::with_capacity(n - 2);
    let mut guard = 0usize;
    while rest.len() > 3 && guard < n * n {
        guard += 1;
        let m = rest.len();
        let mut cut = None;
        for k in 0..m {
            let (ia, ib, ic) = (rest[(k + m - 1) % m], rest[k], rest[(k + 1) % m]);
            let (a, b, c) = (poly[ia], poly[ib], poly[ic]);
            if sign * orient(a, b, c) <= 0.0 {
                continue;
            }
            if rest.iter().any(|&j| j != ia && j != ib && j != ic && in_triangle(poly[j], a, b, c)) {
                continue;
            }
            cut = Some(k);
            break;
        }
        // Entartetes Polygon: notfalls die nächste Ecke abschneiden, damit die Schleife endet
        let k = cut.unwrap_or(0);
        let m = rest.len();
        out.push([rest[(k + m - 1) % m], rest[k], rest[(k + 1) % m]]);
        rest.remove(k);
    }
    if rest.len() == 3 {
        out.push([rest[0], rest[1], rest[2]]);
    }
    out
}

fn cross2(p: (f64, f64), q: (f64, f64)) -> f64 {
    p.0 * q.1 - q.0 * p.1
}

fn orient(a: (f64, f64), b: (f64, f64), c: (f64, f64)) -> f64 {
    (b.0 - a.0) * (c.1 - a.1) - (b.1 - a.1) * (c.0 - a.0)
}

fn in_triangle(p: (f64, f64), a: (f64, f64), b: (f64, f64), c: (f64, f64)) -> bool {
    let (d1, d2, d3) = (orient(a, b, p), orient(b, c, p), orient(c, a, p));
    let neg = d1 < 0.0 || d2 < 0.0 || d3 < 0.0;
    let pos = d1 > 0.0 || d2 > 0.0 || d3 > 0.0;
    !(neg && pos)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pt(name: &str, x: f64, y: f64, z: f64) -> Point {
        Point { name: name.into(), x, y, z, code: String::new(), active: true, label_pos: 0, added: false }
    }

    #[test]
    fn tilted_plane_is_recovered_exactly() {
        // z = 100 + 0.01·x − 0.02·y
        let obs: Vec<(f64, f64, f64)> = [(0.0, 0.0), (10.0, 0.0), (10.0, 8.0), (0.0, 8.0), (5.0, 3.0)].iter().map(|&(x, y)| (x, y, 100.0 + 0.01 * x - 0.02 * y)).collect();
        let f = fit_plane(&obs).unwrap();
        assert!((f.a - 0.01).abs() < 1e-12 && (f.b + 0.02).abs() < 1e-12);
        // Gefälle zeigt nach +x? a>0 steigt nach Osten, b<0 fällt nach Norden: Fließrichtung Nordnordwest-Seite (Norden und Westen)
        let g = f.flow_gon().unwrap();
        assert!(g > 300.0 && g < 400.0, "{}", g);
    }

    #[test]
    fn collinear_points_give_no_plane() {
        assert!(fit_plane(&[(0.0, 0.0, 1.0), (1.0, 1.0, 1.0), (2.0, 2.0, 1.0)]).is_none());
    }

    #[test]
    fn outlier_is_flagged_above_ten_cm() {
        let pts = vec![pt("1", 0.0, 0.0, 10.0), pt("2", 10.0, 0.0, 10.0), pt("3", 10.0, 10.0, 10.0), pt("4", 0.0, 10.0, 10.0), pt("5", 5.0, 5.0, 10.0), pt("6", 5.0, 0.0, 10.0), pt("7", 0.0, 5.0, 10.6)];
        let w = Water { name: "See".into(), pts: (0..7).collect() };
        let rep = analyze(&pts, &w);
        assert!(rep.outliers >= 1 && rep.rows[6].outlier);
        assert!(protocol(&pts, &[w]).contains("WARNUNG"));
    }

    #[test]
    fn triangulation_covers_concave_polygon() {
        // L-Form, Fläche 3, in beiden Drehsinnen
        let l = [(0.0, 0.0), (2.0, 0.0), (2.0, 1.0), (1.0, 1.0), (1.0, 2.0), (0.0, 2.0)];
        for poly in [l.to_vec(), l.iter().rev().copied().collect::<Vec<_>>()] {
            let t = triangulate(&poly);
            assert_eq!(t.len(), 4);
            let area: f64 = t.iter().map(|k| orient(poly[k[0]], poly[k[1]], poly[k[2]]).abs() * 0.5).sum();
            assert!((area - 3.0).abs() < 1e-9, "{}", area);
        }
    }
}
