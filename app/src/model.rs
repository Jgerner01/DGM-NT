//! Project data: parameters, points, breaklines, rings, plan lines, triangles.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

use crate::files;

/// Limits of the GM9 reader in the original (0x50793c).
pub const MAX_UMRING_POINTS: usize = 8000;
pub const MAX_INRING_POINTS: usize = 100;
pub const MAX_INRINGS: usize = 16;

/// Values of the GM0 parameter file (defaults from 0x51322c, see MECHANICS.md).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Params {
    pub contour_interval: f64,
    pub max_side: f64,
    pub min_angle_gon: f64,
    pub subdivision: u32,
    pub scale_xy: f64,
    pub scale_h: f64,
    pub win: [f64; 4],
    pub h_low: f64,
    pub h_high: f64,
    pub decimals_h: u32,
    pub decimals_index: u32,
    pub scale: f64,
    pub v16: f64,
    pub v17: f64,
    pub border_mm: f64,
    pub grid_mm: f64,
    pub thin_mm: f64,
    pub min_dist_mm: f64,
    pub separator: String,
    pub smoothing: u32,
    pub path_line: String,
    pub layers: [i32; 13],
    /// Every n-th contour is an index contour (Zähllinie). Estimated, see MECHANICS.md.
    pub index_every: u32,
    pub use_breaklines: bool,
    /// Auflockerungsfaktor Abtrag (Analyse, Modellvergleich); Standard 1.1
    pub loosen_cut: f64,
    /// Auflockerungsfaktor Auftrag (Analyse, Modellvergleich); Standard 1.5
    pub loosen_fill: f64,
    /// Export (DXF/PDF), Zeilen 38.. der Parameterdatei: Radius der Punktkreise (mm auf dem Papier)
    pub exp_circle_mm: f64,
    /// Schrifthöhen (mm auf dem Papier): Punktnummer, Punkthöhe, Zähllinien, Gitter/Koordinaten
    pub exp_text_nr_mm: f64,
    pub exp_text_h_mm: f64,
    pub exp_text_index_mm: f64,
    pub exp_text_grid_mm: f64,
    /// CAD-Farbnummern (CADdy 1..12) je Element, Reihenfolge siehe `EXP_COLOR_NAMES`
    pub exp_colors: [i32; 13],
}

/// Namen der Farbfelder in `Params::exp_colors` (Reihenfolge = Zeilen 43..55 der Parameterdatei).
pub const EXP_COLOR_NAMES: [&str; 13] = [
    "Punktsymbole",
    "Punktnummern",
    "Punkthöhen",
    "Umring / Inringe",
    "Höhenlinien",
    "Zähllinien",
    "Bruchkanten",
    "Koordinatengitter",
    "Dreiecke",
    "Grundriss",
    "Abtrag (Isolinien)",
    "Auftrag (Isolinien)",
    "Schnittkante",
];

impl Default for Params {
    fn default() -> Self {
        Params {
            contour_interval: 10.0,
            max_side: 100.0,
            min_angle_gon: 5.0,
            subdivision: 10,
            scale_xy: 1.0,
            scale_h: 1.0,
            win: [0.0, 0.0, 10.0, 10.0],
            h_low: 0.0,
            h_high: 0.0,
            decimals_h: 2,
            decimals_index: 0,
            scale: 1000.0,
            v16: 2.0,
            v17: 3.0,
            border_mm: 20.0,
            grid_mm: 100.0,
            thin_mm: 0.05,
            min_dist_mm: 2.0,
            separator: String::new(),
            smoothing: 0,
            path_line: String::new(),
            layers: [11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23],
            index_every: 5,
            use_breaklines: true,
            loosen_cut: 1.1,
            loosen_fill: 1.5,
            exp_circle_mm: 0.6,
            exp_text_nr_mm: 1.5,
            exp_text_h_mm: 1.5,
            exp_text_index_mm: 2.0,
            exp_text_grid_mm: 2.0,
            // dgm.sys: Punkte 1, Nummern 1, Höhe 6, Umring 5, Höhenlinien 2, Zähllinien 3, Bruchkanten 11,
            // Gitter 4, Dreiecke 6, Grundriss 7; dazu Abtrag 9, Auftrag 12, Schnittkante 7
            exp_colors: [1, 1, 6, 5, 2, 3, 11, 4, 6, 7, 9, 12, 7],
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Point {
    pub name: String,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    #[serde(default)]
    pub code: String,
    pub active: bool,
    #[serde(default)]
    pub label_pos: u8,
    #[serde(default)]
    pub added: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Polyline {
    pub pts: Vec<usize>,
    #[serde(default)]
    pub soft: bool,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Tri {
    pub v: [usize; 3],
    pub active: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ManualLabel {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub angle: f32,
}

/// Everything that is saved (manually or automatically) for a project.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ProjectState {
    pub name: String,
    pub params: Params,
    pub points: Vec<Point>,
    pub breaklines: Vec<Polyline>,
    pub umring: Vec<usize>,
    pub inrings: Vec<Vec<usize>>,
    pub plan_lines: Vec<Polyline>,
    pub triangles: Vec<Tri>,
    pub manual_labels: Vec<ManualLabel>,
    pub code_filter: Vec<String>,
    pub saved_unix: u64,
}

pub struct Project {
    pub dir: PathBuf,
    pub st: ProjectState,
    pub index: HashMap<String, usize>,
}

impl Project {
    pub fn rebuild_index(&mut self) {
        self.index.clear();
        for (i, p) in self.st.points.iter().enumerate() {
            self.index.entry(p.name.clone()).or_insert(i);
        }
    }

    pub fn autosave_path(&self) -> PathBuf {
        self.dir.join(format!("{}.dgmnt.json", self.st.name))
    }

    /// Load from the old files of a data folder (GM0, GM1, GM2, GM9, GMG).
    pub fn load_original(dir: PathBuf, name: &str) -> Result<Project, String> {
        let params = files::find_ext(&dir, name, "gm0")
            .and_then(|p| files::read_gm0(&p))
            .unwrap_or_default();
        let gm1 = files::find_ext(&dir, name, "gm1").ok_or_else(|| format!("Keine Punktdatei {}.gm1 gefunden", name))?;
        let points = files::read_gm1(&gm1, &params).map_err(|e| format!("{}: {}", gm1.display(), e))?;
        let mut pr = Project {
            dir: dir.clone(),
            st: ProjectState { name: name.to_string(), params, points, ..Default::default() },
            index: HashMap::new(),
        };
        pr.rebuild_index();
        if let Some(p) = files::find_ext(&dir, name, "gm2") {
            for (names, soft) in files::read_polylines(&p) {
                let pts: Vec<usize> = names.iter().filter_map(|n| pr.index.get(n).copied()).collect();
                if pts.len() >= 2 {
                    pr.st.breaklines.push(Polyline { pts, soft });
                }
            }
        }
        if let Some(p) = files::find_ext(&dir, name, "gm9") {
            // 0x50793c: first group = Umring, following groups = Inringe (with the original's limits).
            for (gi, (names, _)) in files::read_polylines(&p).into_iter().enumerate() {
                if gi > MAX_INRINGS {
                    break;
                }
                let limit = if gi == 0 { MAX_UMRING_POINTS } else { MAX_INRING_POINTS };
                let mut pts: Vec<usize> = names.iter().take(limit).filter_map(|n| pr.index.get(n).copied()).collect();
                if pts.len() > 1 && pts.first() == pts.last() {
                    pts.pop();
                }
                pts.dedup();
                if pts.len() < 3 {
                    continue;
                }
                if gi == 0 {
                    pr.st.umring = pts;
                } else {
                    pr.st.inrings.push(pts);
                }
            }
        }
        if let Some(p) = files::find_ext(&dir, name, "gmg") {
            for (names, _) in files::read_polylines(&p) {
                let pts: Vec<usize> = names.iter().filter_map(|n| pr.index.get(n).copied()).collect();
                if pts.len() >= 2 {
                    pr.st.plan_lines.push(Polyline { pts, soft: false });
                }
            }
        }
        Ok(pr)
    }

    /// Write the ASCII files of the original (GM0, GM2, GM9, GMG) so the old program can read them.
    pub fn write_original_files(&self) -> std::io::Result<()> {
        let nm = |i: &usize| self.st.points[*i].name.clone();
        let gm0 = files::find_ext(&self.dir, &self.st.name, "gm0").unwrap_or(self.dir.join(format!("{}.gm0", self.st.name)));
        files::write_gm0(&gm0, &self.st.params)?;
        let bl: Vec<(Vec<String>, bool)> = self.st.breaklines.iter().map(|b| (b.pts.iter().map(nm).collect(), b.soft)).collect();
        let gm2 = files::find_ext(&self.dir, &self.st.name, "gm2").unwrap_or(self.dir.join(format!("{}.gm2", self.st.name)));
        files::write_polylines(&gm2, &bl, true)?;
        let mut rings: Vec<(Vec<String>, bool)> = Vec::new();
        if !self.st.umring.is_empty() || !self.st.inrings.is_empty() {
            let mut u = self.st.umring.clone();
            if self.ring_area(&u) > 0.0 {
                u.reverse(); // Umring clockwise
            }
            rings.push((u.iter().map(nm).collect(), false));
            for r in &self.st.inrings {
                let mut r = r.clone();
                if self.ring_area(&r) < 0.0 {
                    r.reverse(); // Inring counter-clockwise
                }
                rings.push((r.iter().map(nm).collect(), false));
            }
            let gm9 = files::find_ext(&self.dir, &self.st.name, "gm9").unwrap_or(self.dir.join(format!("{}.gm9", self.st.name)));
            files::write_polylines(&gm9, &rings, rings.len() > 1)?;
        }
        if !self.st.plan_lines.is_empty() {
            let gl: Vec<(Vec<String>, bool)> = self.st.plan_lines.iter().map(|b| (b.pts.iter().map(nm).collect(), false)).collect();
            let gmg = files::find_ext(&self.dir, &self.st.name, "gmg").unwrap_or(self.dir.join(format!("{}.gmg", self.st.name)));
            files::write_polylines(&gmg, &gl, true)?;
        }
        Ok(())
    }

    /// Signed area (positive = counter-clockwise).
    pub fn ring_area(&self, r: &[usize]) -> f64 {
        let mut a = 0.0;
        for i in 0..r.len() {
            let p = &self.st.points[r[i]];
            let q = &self.st.points[r[(i + 1) % r.len()]];
            a += p.x * q.y - q.x * p.y;
        }
        a * 0.5
    }

    pub fn bbox(&self) -> Option<[f64; 4]> {
        let mut b = [f64::MAX, f64::MAX, f64::MIN, f64::MIN];
        let mut any = false;
        for p in &self.st.points {
            any = true;
            b[0] = b[0].min(p.x);
            b[1] = b[1].min(p.y);
            b[2] = b[2].max(p.x);
            b[3] = b[3].max(p.y);
        }
        if any {
            Some(b)
        } else {
            None
        }
    }

    pub fn z_range(&self) -> (f64, f64) {
        let mut lo = f64::MAX;
        let mut hi = f64::MIN;
        for p in self.st.points.iter().filter(|p| p.active) {
            lo = lo.min(p.z);
            hi = hi.max(p.z);
        }
        if lo > hi {
            (0.0, 1.0)
        } else {
            (lo, hi)
        }
    }

    /// Points whose code passes the code selection (Code-Auswahl).
    pub fn code_ok(&self, p: &Point) -> bool {
        self.st.code_filter.is_empty() || self.st.code_filter.iter().any(|c| *c == p.code)
    }

    /// A synthetic demo terrain, used when no data folder exists.
    pub fn demo() -> Project {
        let mut pts = Vec::new();
        let mut seed: u64 = 12345;
        let mut rnd = || {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((seed >> 33) as f64) / (1u64 << 31) as f64
        };
        let h = |x: f64, y: f64| {
            420.0 + 35.0 * (-(((x - 140.0).powi(2) + (y - 160.0).powi(2)) / 9000.0)).exp()
                + 18.0 * (-(((x - 300.0).powi(2) + (y - 90.0).powi(2)) / 6000.0)).exp()
                + 0.06 * x
                - 8.0 * ((x * 0.015).sin() * (y * 0.02).cos())
        };
        for i in 0..900 {
            let x = rnd() * 400.0;
            let y = rnd() * 300.0;
            pts.push(Point {
                name: format!("{}", 1000 + i),
                x: 4500000.0 + x,
                y: 5400000.0 + y,
                z: h(x, y),
                code: "GP".into(),
                active: true,
                label_pos: 0,
                added: false,
            });
        }
        let mut params = Params::default();
        params.contour_interval = 2.0;
        params.max_side = 60.0;
        params.subdivision = 6;
        params.smoothing = 3;
        let mut pr = Project {
            dir: std::env::temp_dir(),
            st: ProjectState { name: "Beispiel".into(), params, points: pts, ..Default::default() },
            index: HashMap::new(),
        };
        pr.rebuild_index();
        pr
    }
}
