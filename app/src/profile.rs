//! Bemaßte Profile (Längs- und Querprofile) aus dem Geländemodell: exakter Schnitt mit dem Dreiecksnetz,
//! automatische Wahl von Maßstab, Überhöhung und Blattformat, Zeichnung mit Höhenraster, Koten an den
//! Knickpunkten, Beschriftungsband (Höhe, Neigung, Abstand, Station), Querprofile im Abstand entlang einer Achse sowie Ausgabe als Vektor-PDF und überhöhtes DXF.
//! Die Zeichnung entsteht in der „Profilebene“: x = Station (m), y = (Höhe − Bezugshöhe) × Überhöhung (m).
//! Beim Maßstab 1:N erscheinen die Längen dann 1:N und die Höhen 1:(N/Überhöhung); PDF und DXF nutzen
//! dieselbe Szene (`scene::Scene`) wie die Druckausgabe.
//! Hardware: keine (reine Rechenmodule).
//! Autor: Johann Gerner

use serde::{Deserialize, Serialize};

use crate::analysis::format_station;
use crate::export::{self, Sheet};
use crate::model::Params;
use crate::pdf;
use crate::render::caddy;
use crate::scene::*;
use crate::tin::{point_in_poly, Surface};
use crate::water::PlaneFit;

/// Schrifthöhe (Großbuchstaben, mm Papier) für normale Schrift: Titel
pub const TEXT_NORMAL_MM: f64 = 5.0;
/// Schrifthöhe für Zeichnungsbeschriftung: Zeilenköpfe, Achsenbeschriftung
pub const TEXT_LABEL_MM: f64 = 3.5;
/// Schrifthöhe für Maßzahlen: Höhen, Abstände, Stationen
pub const TEXT_DIM_MM: f64 = 2.5;

/// Rand rund ums Blatt (mm)
const SHEET_MARGIN_MM: f64 = 10.0;
/// Breite des Legendenstreifens rechts (mm)
const STRIP_MM: f64 = 60.0;
/// Breite der Spalte mit den Zeilenköpfen und der Höhenskala (mm)
const HEAD_W_MM: f64 = 32.0;
/// Abstand zwischen Spalte und Profilbeginn (mm); mindestens die halbe Breite des längsten Bandtexts (2,5 mm Courier,
/// „12+345.67“), damit die Beschriftung am Anfang nicht in die Zeilenköpfe ragt
const PAD_LEFT_MM: f64 = 13.0;
/// Abstand zwischen Profilende und rechtem Rand der Zeichenfläche (mm), aus demselben Grund
const PAD_RIGHT_MM: f64 = 13.0;
/// Höhe einer Bandzeile (mm)
const BAND_ROW_MM: f64 = 8.0;
/// Höhe des Titelbereichs oben (mm)
const TITLE_H_MM: f64 = 16.0;
/// Abstand zwischen Bandoberkante und Bezugslinie (mm): die Profilgrafik steht um diesen Betrag höher
const GRAPH_LIFT_MM: f64 = 20.0;
/// freier Platz über dem höchsten Punkt (mm), Platz für die senkrecht stehenden Koten
const TOP_PAD_MM: f64 = 20.0;
/// Mindestabstand zweier Beschriftungsstationen (mm): Breite von „0+120.00“ in 2,5 mm Courier plus Luft
const MIN_LABEL_GAP_MM: f64 = 24.5;
/// kleinster Abstand der Höhenrasterlinien (mm)
const MIN_GRID_MM: f64 = 8.0;
/// größte Überhöhung bei automatischer Wahl von Längsprofilen
const EXAG_MAX_AUTO: f64 = 25.0;
/// Maßstabsreihe (N von 1:N), größter Maßstab zuerst
const SCALES: [f64; 11] = [50.0, 100.0, 200.0, 250.0, 500.0, 1000.0, 2000.0, 2500.0, 5000.0, 10000.0, 20000.0];
/// gröbster Maßstab, der bei der automatischen Wahl zuerst versucht wird (danach gröbere)
const AUTO_SCALE_MAX_N: f64 = 1000.0;
/// Überhöhungen für die automatische Wahl
const EXAGS: [f64; 7] = [1.0, 2.0, 5.0, 10.0, 20.0, 25.0, 50.0];
/// Beschriftungsintervalle (m)
const LABEL_STEPS: [f64; 12] = [5.0, 10.0, 20.0, 25.0, 50.0, 100.0, 200.0, 250.0, 500.0, 1000.0, 2000.0, 5000.0];
/// Höhenrasterschritte (m)
const GRID_STEPS: [f64; 10] = [0.1, 0.2, 0.5, 1.0, 2.0, 5.0, 10.0, 20.0, 50.0, 100.0];
/// Blattbreite und -höhe A4 quer (mm)
const A4_LANDSCAPE: (f64, f64) = (297.0, 210.0);
/// Blattbreite und -höhe A3 quer (mm)
const A3_LANDSCAPE: (f64, f64) = (420.0, 297.0);
/// Rollenbreite (A3-Breite) in mm
const ROLL_WIDTH_MM: f64 = 297.0;
/// kürzeste und längste Rolle (mm)
const ROLL_MIN_MM: f64 = 420.0;
pub const ROLL_MAX_MM: f64 = 4000.0;
/// Rundung der Rollenlänge (mm)
const ROLL_ROUND_MM: f64 = 10.0;
/// Versatz nach innen beim Abfragen der Höhe an Intervallenden (Anteil des Intervalls)
const EDGE_EPS: f64 = 1e-9;
/// Mindestlänge eines Teilstücks (m)
const MIN_PIECE_M: f64 = 1e-9;
/// Vorgabe der Bereinigung der Knickpunktbeschriftung (m), wie im Leitfaden des LBB Rheinland-Pfalz
pub const KINK_CLEAN_DEFAULT_M: f64 = 5.0;
/// kleinste Richtungsänderung (Bogenmaß) eines Knickpunkts
const KINK_MIN_RAD: f64 = 0.05;
/// kleinster Abstand zweier Koten auf dem Papier (mm): Höhe der senkrechten 2,5-mm-Schrift plus Luft
const KOTE_MIN_GAP_MM: f64 = 4.0;
/// Radius des Punktzeichens an einer Kote (mm)
const KOTE_MARK_R_MM: f64 = 0.6;
/// Abstand des Kotentexts über dem Gelände (mm)
const KOTE_LIFT_MM: f64 = 1.5;
/// Höhe der Großbuchstaben von Courier (Anteil der Schriftgröße) und Zeichenbreite, wie im PDF-Schreiber
const MONO_CAP: f64 = 0.562;
const MONO_ADVANCE: f64 = 0.6;
/// Höchstzahl der Querprofile, die aus einer Achse entstehen
pub const MAX_CROSS_SECTIONS: usize = 500;
/// Ebenennamen der Profile im PDF (Index = Ebene der Szene; die Ebenen der Karte sind hier umgewidmet)
const PROFILE_LAYER_NAMES: [&str; 13] = [
    "Punkte",
    "Rahmen und Raster",
    "Punktnummern",
    "Band, Koten und Maße",
    "Umring und Inringe",
    "Gelände",
    "Bezugslinie",
    "Grundriss",
    "Beschriftung",
    "Bruchkanten",
    "Zweites Gelände",
    "Dreiecke",
    "Wasserspiegel",
];
/// Mindestkürzung, ab der eine Schnittlinie am Modellrand beschnitten wird (m)
const CLIP_EPS_M: f64 = 1e-6;
/// Textstil der Normschrift im DXF (Name und Schriftdatei, DIN EN ISO 3098)
pub const DXF_STYLE_NAME: &str = "ISO3098";
pub const DXF_STYLE_FONT: &str = "isocp.shx";
/// kürzestes Stück des Wasserspiegels (mm Papier), das eine Beschriftung bekommt
const WATER_LABEL_MIN_MM: f64 = 32.0;
/// Abstand zwischen zwei Profilen auf einem Blatt (mm)
const CELL_GAP_MM: f64 = 6.0;
/// Länge der Endstriche an Maßlinien (mm)
const DIM_TICK_MM: f64 = 1.5;
/// Abstand des Maßtexts von der Maßlinie (mm)
const DIM_TEXT_GAP_MM: f64 = 2.5;
/// Farbe des zweiten Geländes im PDF
const RGB_SECOND: Rgb = [200, 40, 40];
/// Linienbreiten (mm)
const W_TERRAIN: f64 = 0.5;
const W_DATUM: f64 = 0.35;
const W_BAND: f64 = 0.25;
const W_GRID: f64 = 0.12;
/// Farben im PDF
const RGB_TERRAIN: Rgb = [110, 70, 20];
const RGB_GRID: Rgb = [185, 185, 185];
const RGB_BLACK: Rgb = [0, 0, 0];

/// Darstellung: Längsprofil (Station) oder Querprofil (Abstand vom Nullpunkt, links −, rechts +)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ProfileView {
    #[default]
    Laengs,
    Quer,
}

/// Blattformat
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SheetFormat {
    /// kleinstes passendes Blatt: A4, A3, Rolle
    #[default]
    Auto,
    A4,
    A3,
    /// A3-Breite, Länge bis 4000 mm
    Roll,
}

impl SheetFormat {
    pub fn name(&self) -> &'static str {
        match self {
            SheetFormat::Auto => "automatisch",
            SheetFormat::A4 => "A4 quer",
            SheetFormat::A3 => "A3 quer",
            SheetFormat::Roll => "Rolle (A3-Breite)",
        }
    }
}

/// Ein Profil: Schnittlinie im Gelände und Darstellungswünsche (0 = automatisch)
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ProfileDef {
    pub name: String,
    pub view: ProfileView,
    /// Schnittlinie bzw. Achse in Weltkoordinaten
    pub line: Vec<[f64; 2]>,
    /// Station am Linienanfang (Längsprofil) bzw. Station des Nullpunkts (Querprofil)
    pub start_station: f64,
    /// Querprofil: Nullpunkt als Länge ab Linienanfang (m); None = Mitte
    pub zero: Option<f64>,
    /// Maßstabszahl N (1:N); 0 = automatisch
    pub scale_n: f64,
    /// Überhöhung; 0 = automatisch
    pub exag: f64,
    /// Beschriftungsintervall (m); 0 = automatisch
    pub label_step: f64,
    pub format: SheetFormat,
    /// Koten an Knickpunkten weglassen
    pub no_koten: bool,
    /// Bereinigung der Knickpunkte (m): kleinster Abstand beschrifteter Punkte; None = 5 m, 0 = alle
    pub kink_clean: Option<f64>,
    /// zweites Gelände (Modellvergleich) als gestrichelte Linie und eigene Bandzeile
    pub show_second: bool,
    /// Maßkette zwischen den Knickpunkten als eigene Bandzeile
    pub dim_chain: bool,
    /// Höhenunterschied-Maßlinie zwischen zwei Stationen (angezeigte Werte)
    pub dim_h: Option<(f64, f64)>,
}

impl ProfileDef {
    /// Länge der Schnittlinie (m)
    pub fn length(&self) -> f64 {
        self.line.windows(2).map(|w| (w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1])).sum()
    }
    /// Verschiebung von „Länge ab Linienanfang“ zur angezeigten Station bzw. zum Abstand
    pub fn offset(&self) -> f64 {
        match self.view {
            ProfileView::Laengs => self.start_station,
            ProfileView::Quer => -self.zero.unwrap_or(self.length() / 2.0),
        }
    }
    pub fn title(&self) -> String {
        let kind = match self.view {
            ProfileView::Laengs => "Längsprofil",
            ProfileView::Quer => "Querprofil",
        };
        if self.name.is_empty() {
            kind.to_string()
        } else {
            format!("{} {}", kind, self.name)
        }
    }
}

/// Ein Punkt des Geländeschnitts
#[derive(Clone, Copy, Debug)]
pub struct CutPt {
    /// Länge ab Linienanfang (m)
    pub s: f64,
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

/// Wasserfläche für das Profil: Umriss (x, y) und Ausgleichungsebene
pub type WaterPoly = (Vec<(f64, f64)>, PlaneFit);

/// Geländeschnitt: zusammenhängende Teilstücke (zwischen ihnen liegt die Linie außerhalb des Modells)
#[derive(Clone, Debug, Default)]
pub struct Cut {
    pub runs: Vec<Vec<CutPt>>,
    pub length: f64,
    /// Wasserspiegel: Stücke der Schnittlinie innerhalb von Wasserflächen, je (Länge ab Anfang, Höhe) an beiden Enden
    pub waters: Vec<[(f64, f64); 2]>,
    /// zweites Gelände (Schnitt mit dem zweiten Modell) und sein Name
    pub second: Vec<Vec<CutPt>>,
    pub second_label: String,
}

/// Zusätzliche Daten für ein Profil: Wasserflächen und zweites Gelände
#[derive(Default, Clone, Copy)]
pub struct Extras<'a> {
    pub waters: &'a [WaterPoly],
    pub second: Option<(&'a Surface, &'a str)>,
}

fn z_in(runs: &[Vec<CutPt>], s: f64) -> Option<f64> {
    for r in runs {
        let (Some(a), Some(b)) = (r.first(), r.last()) else { continue };
        if s < a.s - 1e-9 || s > b.s + 1e-9 {
            continue;
        }
        for w in r.windows(2) {
            if s <= w[1].s + 1e-9 {
                let d = w[1].s - w[0].s;
                let f = if d > 0.0 { ((s - w[0].s) / d).clamp(0.0, 1.0) } else { 0.0 };
                return Some(w[0].z + (w[1].z - w[0].z) * f);
            }
        }
        return Some(b.z);
    }
    None
}

impl Cut {
    /// Höhe des zweiten Geländes bei Länge `s`
    pub fn z2_at(&self, s: f64) -> Option<f64> {
        z_in(&self.second, s)
    }
    /// Höhe bei Länge `s` (linear zwischen den Stützstellen), None außerhalb des Modells
    pub fn z_at(&self, s: f64) -> Option<f64> {
        z_in(&self.runs, s)
    }
    /// kleinste und größte Höhe
    pub fn z_range(&self) -> Option<(f64, f64)> {
        let mut r: Option<(f64, f64)> = None;
        let zs = self.runs.iter().flatten().chain(self.second.iter().flatten()).map(|p| p.z).chain(self.waters.iter().flatten().map(|p| p.1));
        for z in zs {
            r = Some(r.map_or((z, z), |(lo, hi)| (lo.min(z), hi.max(z))));
        }
        r
    }
}

/// Schnitt der Linie mit dem Geländemodell: Stützstellen an allen Dreieckskanten, an den Linienknicken
/// und (bei gekrümmter Fläche) zusätzlich alle `max_step` m.
pub fn cut(sf: &Surface, line: &[[f64; 2]], max_step: f64) -> Cut {
    let mut runs: Vec<Vec<CutPt>> = Vec::new();
    let mut cur: Vec<CutPt> = Vec::new();
    let mut base = 0.0;
    for w in line.windows(2) {
        let (a, b) = (w[0], w[1]);
        let d = (b[0] - a[0]).hypot(b[1] - a[1]);
        if d <= MIN_PIECE_M {
            continue;
        }
        let mut ts = vec![0.0];
        ts.extend(sf.edge_crossings(a, b));
        ts.push(1.0);
        let pt = |t: f64| (a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t);
        for iv in ts.windows(2) {
            let (t0, t1) = (iv[0], iv[1]);
            if (t1 - t0) * d < MIN_PIECE_M {
                continue;
            }
            let mid = pt((t0 + t1) / 2.0);
            if sf.height(mid.0, mid.1).is_none() {
                if cur.len() >= 2 {
                    runs.push(std::mem::take(&mut cur));
                } else {
                    cur.clear();
                }
                continue;
            }
            let parts = if sf.curved && max_step > 0.0 { ((t1 - t0) * d / max_step).ceil().max(1.0) as usize } else { 1 };
            for k in 0..=parts {
                let f = k as f64 / parts as f64;
                let t = t0 + (t1 - t0) * f;
                let (x, y) = pt(t);
                // genau auf der Kante; bei Rundungsproblemen am Modellrand leicht nach innen versetzt
                let q = pt(t0 + (t1 - t0) * f.clamp(EDGE_EPS, 1.0 - EDGE_EPS));
                let Some(z) = sf.height(x, y).or_else(|| sf.height(q.0, q.1)) else { continue };
                let p = CutPt { s: base + d * t, x, y, z };
                if k == 0 {
                    if let Some(l) = cur.last() {
                        if (p.s - l.s).abs() < MIN_PIECE_M {
                            continue;
                        }
                    }
                }
                cur.push(p);
            }
        }
        base += d;
    }
    if cur.len() >= 2 {
        runs.push(cur);
    }
    Cut { runs, length: base, waters: Vec::new(), second: Vec::new(), second_label: String::new() }
}

/// Stücke der Schnittlinie, die in Wasserflächen liegen, mit der Höhe der Ausgleichungsebene an den Enden.
pub fn add_waters(c: &mut Cut, line: &[[f64; 2]], waters: &[WaterPoly]) {
    let mut base = 0.0;
    for w in line.windows(2) {
        let (a, b) = (w[0], w[1]);
        let d = (b[0] - a[0]).hypot(b[1] - a[1]);
        if d <= MIN_PIECE_M {
            continue;
        }
        let pt = |t: f64| (a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t);
        for (poly, plane) in waters {
            if poly.len() < 3 {
                continue;
            }
            // Schnitte der Strecke mit dem Umriss
            let mut ts = vec![0.0, 1.0];
            for i in 0..poly.len() {
                let (p, q) = (poly[i], poly[(i + 1) % poly.len()]);
                let (d1, d2) = ((b[0] - a[0], b[1] - a[1]), (q.0 - p.0, q.1 - p.1));
                let den = d1.0 * d2.1 - d1.1 * d2.0;
                if den.abs() < 1e-14 {
                    continue;
                }
                let (w0, w1) = (p.0 - a[0], p.1 - a[1]);
                let t = (w0 * d2.1 - w1 * d2.0) / den;
                let u = (w0 * d1.1 - w1 * d1.0) / den;
                if t > 0.0 && t < 1.0 && (0.0..=1.0).contains(&u) {
                    ts.push(t);
                }
            }
            ts.sort_by(|x, y| x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal));
            for iv in ts.windows(2) {
                if (iv[1] - iv[0]) * d < MIN_PIECE_M {
                    continue;
                }
                let m = pt((iv[0] + iv[1]) / 2.0);
                if point_in_poly(m.0, m.1, poly) {
                    let (p0, p1) = (pt(iv[0]), pt(iv[1]));
                    c.waters.push([(base + d * iv[0], plane.z_at(p0.0, p0.1)), (base + d * iv[1], plane.z_at(p1.0, p1.1))]);
                }
            }
        }
        base += d;
    }
}

/// Maßstab, Überhöhung, Raster und Blatt eines Profils
#[derive(Clone, Debug)]
pub struct Layout {
    pub n: f64,
    pub exag: f64,
    pub label_step: f64,
    pub grid_dz: f64,
    /// Bezugshöhe (unterste Linie der Zeichnung)
    pub datum: f64,
    /// Höhe der obersten Rasterlinie
    pub top: f64,
    pub sheet: Sheet,
    /// false: auch im kleinsten Maßstab passt das Profil nicht auf ein Blatt
    pub fits: bool,
    /// Anzahl der Bandzeilen
    pub rows: usize,
    /// Rahmen um die Zeichenfläche mitzeichnen (mehrere Profile auf einem Blatt)
    pub cell_frame: bool,
}

/// Zeilen des Beschriftungsbands
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Row {
    Hoehe,
    Hoehe2,
    Neigung,
    Abstand,
    Kette,
    Station,
}

fn band_rows(def: &ProfileDef, has_second: bool) -> Vec<Row> {
    let mut v = vec![Row::Hoehe];
    if has_second {
        v.push(Row::Hoehe2);
    }
    v.push(Row::Neigung);
    v.push(Row::Abstand);
    if def.dim_chain {
        v.push(Row::Kette);
    }
    v.push(Row::Station);
    v
}

/// Breite eines Textes in Courier (mm) bei Großbuchstabenhöhe `cap`
fn mono_w(text: &str, cap: f64) -> f64 {
    text.chars().count() as f64 * cap / MONO_CAP * MONO_ADVANCE
}

impl Layout {
    /// Papier-mm je Höhenmeter
    fn k(&self) -> f64 {
        self.exag * 1000.0 / self.n
    }
}

fn band_height(rows: usize) -> f64 {
    BAND_ROW_MM * rows as f64
}

fn page_of(fmt: SheetFormat, area_w_needed: f64) -> Option<(f64, f64)> {
    let extra = 2.0 * SHEET_MARGIN_MM + STRIP_MM;
    match fmt {
        SheetFormat::A4 => (area_w_needed + extra <= A4_LANDSCAPE.0).then_some(A4_LANDSCAPE),
        SheetFormat::A3 => (area_w_needed + extra <= A3_LANDSCAPE.0).then_some(A3_LANDSCAPE),
        SheetFormat::Roll => {
            let w = ((area_w_needed + extra) / ROLL_ROUND_MM).ceil() * ROLL_ROUND_MM;
            (w <= ROLL_MAX_MM).then_some((w.max(ROLL_MIN_MM), ROLL_WIDTH_MM))
        }
        SheetFormat::Auto => None,
    }
}

fn grid_step_for(k: f64) -> f64 {
    GRID_STEPS.iter().copied().find(|dz| dz * k >= MIN_GRID_MM).unwrap_or(GRID_STEPS[GRID_STEPS.len() - 1])
}

fn try_fit(def: &ProfileDef, fmt: SheetFormat, n: f64, length: f64, zmin: f64, zmax: f64, rows: usize) -> Option<Layout> {
    let area_w = HEAD_W_MM + PAD_LEFT_MM + length * 1000.0 / n + PAD_RIGHT_MM;
    let page = page_of(fmt, area_w)?;
    let avail = page.1 - 2.0 * SHEET_MARGIN_MM - TITLE_H_MM - band_height(rows) - GRAPH_LIFT_MM - TOP_PAD_MM;
    // Längsprofile: größte passende Überhöhung, Querprofile: kleinste (bevorzugt 1)
    let mut exags: Vec<f64> = if def.exag > 0.0 {
        vec![def.exag]
    } else if def.view == ProfileView::Quer {
        EXAGS.to_vec()
    } else {
        EXAGS.iter().copied().filter(|&e| e <= EXAG_MAX_AUTO).rev().collect()
    };
    if def.exag <= 0.0 && def.view == ProfileView::Quer {
        exags.truncate(1);
    }
    for e in exags {
        let k = e * 1000.0 / n;
        let dz = grid_step_for(k);
        let datum = (zmin / dz).floor() * dz;
        let top = (zmax / dz).ceil().max(datum / dz + 1.0) * dz;
        if (top - datum) * k <= avail {
            let step = if def.label_step > 0.0 {
                def.label_step
            } else {
                LABEL_STEPS.iter().copied().find(|s| s * 1000.0 / n >= MIN_LABEL_GAP_MM).unwrap_or(LABEL_STEPS[LABEL_STEPS.len() - 1])
            };
            return Some(Layout { n, exag: e, label_step: step, grid_dz: dz, datum, top, sheet: Sheet::custom(page.0, page.1, STRIP_MM), fits: true, rows, cell_frame: false });
        }
    }
    None
}

/// Wählt Blatt, Maßstab, Überhöhung, Raster und Beschriftungsintervall: der größte Maßstab, bei dem das
/// Profil auf das kleinste passende Blatt (A4, A3, Rolle bis 4000 mm) passt. Eigene Vorgaben im Profil gelten.
pub fn auto_layout(def: &ProfileDef, length: f64, zmin: f64, zmax: f64, rows: usize) -> Layout {
    let formats: Vec<SheetFormat> = match def.format {
        SheetFormat::Auto => vec![SheetFormat::A4, SheetFormat::A3, SheetFormat::Roll],
        f => vec![f],
    };
    let scales: Vec<f64> = if def.scale_n > 0.0 { vec![def.scale_n] } else { SCALES.to_vec() };
    // Zuerst Maßstäbe bis 1:AUTO_SCALE_MAX_N (kleinstes passendes Blatt), erst dann gröbere
    let (fine, coarse): (Vec<f64>, Vec<f64>) = scales.iter().partition(|&&n| n <= AUTO_SCALE_MAX_N || def.scale_n > 0.0);
    for group in [fine, coarse] {
        for fmt in &formats {
            for &n in &group {
                if let Some(l) = try_fit(def, *fmt, n, length, zmin, zmax, rows) {
                    return l;
                }
            }
        }
    }
    // passt nirgends: größte Rolle im kleinsten Maßstab, Überhöhung 1
    let n = scales.last().copied().unwrap_or(SCALES[SCALES.len() - 1]);
    let dz = grid_step_for(1000.0 / n);
    let datum = (zmin / dz).floor() * dz;
    let top = (zmax / dz).ceil().max(datum / dz + 1.0) * dz;
    let step = if def.label_step > 0.0 { def.label_step } else { LABEL_STEPS[LABEL_STEPS.len() - 1] };
    Layout { n, exag: if def.exag > 0.0 { def.exag } else { 1.0 }, label_step: step, grid_dz: dz, datum, top, sheet: Sheet::custom(ROLL_MAX_MM, ROLL_WIDTH_MM, STRIP_MM), fits: false, rows, cell_frame: false }
}

/// Stationen (angezeigte Werte), an denen das Band beschriftet wird: Anfang, Ende und alle Vielfachen des
/// Intervalls, die genügend Abstand zu den anderen haben; `extra` = zusätzlich verlangte Stelle (Nullpunkt).
pub fn label_positions(d0: f64, d1: f64, step: f64, min_gap: f64, extra: Option<f64>) -> Vec<f64> {
    let mut out = vec![d0];
    if d1 - d0 > 1e-9 {
        out.push(d1);
    }
    if let Some(x) = extra {
        if x > d0 + min_gap * 0.999 && x < d1 - min_gap * 0.999 {
            out.push(x);
        }
    }
    if step > 0.0 {
        let mut v = (d0 / step).ceil() * step;
        while v <= d1 + 1e-9 {
            if out.iter().all(|&o| (o - v).abs() >= min_gap * 0.999) {
                out.push(v);
            }
            v += step;
        }
    }
    out.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    out
}

/// Knickpunkte eines Teilstücks, die eine Kote bekommen: die stärksten Richtungsänderungen zuerst, jeweils
/// nur wenn der Abstand zu bereits gewählten mindestens `min_gap` (m) beträgt. Anfang und Ende zählen mit.
pub fn key_points(run: &[CutPt], min_gap: f64) -> Vec<CutPt> {
    let (Some(first), Some(last)) = (run.first(), run.last()) else { return Vec::new() };
    let mut cand: Vec<(f64, CutPt)> = Vec::new();
    for w in run.windows(3) {
        let (d1, d2) = (w[1].s - w[0].s, w[2].s - w[1].s);
        if d1 <= 0.0 || d2 <= 0.0 {
            continue;
        }
        let a1 = ((w[1].z - w[0].z) / d1).atan();
        let a2 = ((w[2].z - w[1].z) / d2).atan();
        let turn = (a2 - a1).abs();
        if turn >= KINK_MIN_RAD {
            cand.push((turn, w[1]));
        }
    }
    cand.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    let mut out: Vec<CutPt> = vec![*first];
    if last.s - first.s > 0.0 {
        out.push(*last);
    }
    for (_, p) in cand {
        if out.iter().all(|o| (o.s - p.s).abs() >= min_gap) {
            out.push(p);
        }
    }
    out.sort_by(|a, b| a.s.partial_cmp(&b.s).unwrap_or(std::cmp::Ordering::Equal));
    out
}

/// Querprofile entlang einer Achse: alle `step` m und am Ende, senkrecht zur Achse (an Knicken in der
/// Winkelhalbierenden), je `half_width` m links und rechts. Die Schnittlinie läuft von links nach rechts,
/// der Nullpunkt liegt auf der Achse. Höchstens `MAX_CROSS_SECTIONS`.
pub fn cross_sections(axis: &[[f64; 2]], start_station: f64, step: f64, half_width: f64) -> Vec<ProfileDef> {
    cross_sections_at(axis, start_station, &regular_stations(axis_length(axis), step), half_width)
}

/// Länge einer Achse (m).
pub fn axis_length(axis: &[[f64; 2]]) -> f64 {
    axis.windows(2).map(|w| (w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1])).sum()
}

/// Stationen (Länge ab Achsanfang) alle `step` m und am Ende, höchstens `MAX_CROSS_SECTIONS`.
pub fn regular_stations(total: f64, step: f64) -> Vec<f64> {
    let mut stations: Vec<f64> = Vec::new();
    if total <= 0.0 || step <= 0.0 {
        return stations;
    }
    let mut s = 0.0;
    while s < total - 1e-9 && stations.len() < MAX_CROSS_SECTIONS {
        stations.push(s);
        s += step;
    }
    if stations.len() < MAX_CROSS_SECTIONS {
        stations.push(total);
    }
    stations
}

/// Stationen der Achspunkte (Knicke und Enden) einer Achse, höchstens `MAX_CROSS_SECTIONS`.
pub fn vertex_stations(axis: &[[f64; 2]]) -> Vec<f64> {
    let mut out = vec![0.0];
    let mut acc = 0.0;
    for w in axis.windows(2) {
        let d = (w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1]);
        if d > 0.0 {
            acc += d;
            out.push(acc);
        }
    }
    out.truncate(MAX_CROSS_SECTIONS);
    if out.len() < 2 {
        out.clear();
    }
    out
}

/// Mögliche Abstände der Querprofile für eine Achse der Länge `total`: runde Werte (1-2-5-Reihe), die
/// mindestens 2 und höchstens `MAX_CROSS_SECTIONS` Profile ergeben.
pub fn possible_steps(total: f64) -> Vec<f64> {
    let mut out = Vec::new();
    let mut decade = 0.01;
    while decade <= total.max(1.0) {
        for m in [1.0, 2.0, 5.0] {
            let st = decade * m;
            let count = (total / st).ceil() + 1.0;
            if st < total && count <= MAX_CROSS_SECTIONS as f64 {
                out.push(st);
            }
        }
        decade *= 10.0;
    }
    out
}

/// Schneidet eine Schnittlinie auf den Teil im Geländemodell zu; der Nullpunkt bleibt an seiner Stelle.
/// `None`, wenn die Linie das Modell nicht trifft.
pub fn clip_to_model(sf: &Surface, mut def: ProfileDef) -> Option<ProfileDef> {
    let len = def.length();
    let c = cut(sf, &def.line, 0.0);
    let s0 = c.runs.first()?.first()?.s;
    let s1 = c.runs.last()?.last()?.s;
    if s1 - s0 < MIN_PIECE_M {
        return None;
    }
    if s0 > CLIP_EPS_M || s1 < len - CLIP_EPS_M {
        let zero = def.zero.unwrap_or(len / 2.0);
        def.line = sub_line(&def.line, s0, s1);
        def.zero = Some((zero - s0).clamp(0.0, s1 - s0));
    }
    Some(def)
}

/// Achse aus einer Textdatei: je Zeile ein Punkt `x y` (Trenner Leerzeichen, Tabulator, Semikolon oder Komma,
/// Dezimalkomma erlaubt); Zeilen ohne zwei Zahlen am Anfang (z. B. Kopfzeilen) werden übersprungen.
pub fn parse_axis(text: &str) -> Vec<[f64; 2]> {
    let mut out = Vec::new();
    for line in text.lines() {
        let semi = line.contains(';') || line.contains('\t');
        let nums: Vec<f64> = line
            .split(|c: char| c == ';' || c == '\t' || (!semi && (c == ' ' || c == ',')))
            .map(|t| t.trim())
            .filter(|t| !t.is_empty())
            .map(|t| t.replace(',', ".").parse::<f64>())
            .take_while(|r| r.is_ok())
            .filter_map(|r| r.ok())
            .collect();
        if nums.len() >= 2 {
            out.push([nums[0], nums[1]]);
        }
    }
    out
}

/// Querprofile an den angegebenen Stationen der Achse (siehe `cross_sections`).
pub fn cross_sections_at(axis: &[[f64; 2]], start_station: f64, stations: &[f64], half_width: f64) -> Vec<ProfileDef> {
    let mut out = Vec::new();
    if axis.len() < 2 || half_width <= 0.0 {
        return out;
    }
    for &st in stations {
        // Segment, in dem die Station liegt, und Richtung (am Knick: Mittel aus beiden Richtungen)
        let mut acc = 0.0;
        let mut dir = (1.0, 0.0);
        let mut pos = axis[0];
        for (i, w) in axis.windows(2).enumerate() {
            let d = (w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1]);
            if d <= 0.0 {
                continue;
            }
            let u = ((w[1][0] - w[0][0]) / d, (w[1][1] - w[0][1]) / d);
            if st <= acc + d + 1e-9 {
                let f = ((st - acc) / d).clamp(0.0, 1.0);
                pos = [w[0][0] + (w[1][0] - w[0][0]) * f, w[0][1] + (w[1][1] - w[0][1]) * f];
                dir = u;
                // am Knick mit dem Nachbarsegment mitteln
                let nb = if f <= 1e-9 && i > 0 {
                    Some((axis[i - 1], axis[i]))
                } else if f >= 1.0 - 1e-9 && i + 2 < axis.len() {
                    Some((axis[i + 1], axis[i + 2]))
                } else {
                    None
                };
                if let Some((a, b)) = nb {
                    let dn = (b[0] - a[0]).hypot(b[1] - a[1]);
                    if dn > 0.0 {
                        let v = (u.0 + (b[0] - a[0]) / dn, u.1 + (b[1] - a[1]) / dn);
                        let l = v.0.hypot(v.1);
                        if l > 1e-9 {
                            dir = (v.0 / l, v.1 / l);
                        }
                    }
                }
                break;
            }
            acc += d;
        }
        let left = (-dir.1, dir.0);
        let line = vec![[pos[0] + left.0 * half_width, pos[1] + left.1 * half_width], [pos[0] - left.0 * half_width, pos[1] - left.1 * half_width]];
        let station = start_station + st;
        out.push(ProfileDef { name: format_station(station), view: ProfileView::Quer, line, start_station: station, ..Default::default() });
    }
    out
}

/// Gemeinsamer Maßstab, gemeinsame Überhöhung und Beschriftung für eine Reihe von Profilen (gröbster Maßstab,
/// kleinste Überhöhung, größtes Intervall der Einzelwahl), damit sich die Blätter vergleichen lassen.
pub fn common_layout(sf: &Surface, defs: &[ProfileDef], max_step: f64, ex: &Extras) -> Option<(f64, f64, f64)> {
    let mut r: Option<(f64, f64, f64)> = None;
    for d in defs {
        if let Ok((_, lay)) = prepare(sf, d, max_step, ex) {
            r = Some(match r {
                None => (lay.n, lay.exag, lay.label_step),
                Some((n, e, st)) => (n.max(lay.n), e.min(lay.exag), st.max(lay.label_step)),
            });
        }
    }
    r
}

fn decimals_for(dz: f64) -> usize {
    if dz < 0.1 {
        2
    } else if dz < 1.0 {
        1
    } else {
        0
    }
}

/// Zeichnung des Profils. Rückgabe: Szene in der Profilebene und Weltkoordinaten der linken unteren Ecke
/// der Zeichenfläche (für `render_pdf_ext`).
pub fn build_scene(cut: &Cut, def: &ProfileDef, lay: &Layout, p: &Params, dxf: bool) -> (Scene, P2) {
    let mut items: Vec<Item> = Vec::new();
    let (ax, ay, aw, ah) = lay.sheet.area();
    let _ = (ax, ay);
    let mm_m = lay.n / 1000.0; // m je mm Papier
    let off = def.offset();
    let k = lay.k();
    let rows = band_rows(def, !cut.second.is_empty());
    let band = band_height(rows.len());
    // Grundlinie der Grafik (Bezugshöhe) über dem Band
    let base = band + GRAPH_LIFT_MM;
    // Profilebene: Papier-mm (von der linken unteren Ecke der Zeichenfläche) → Ebene
    let ox = off - (HEAD_W_MM + PAD_LEFT_MM) * mm_m;
    let oy = -band * mm_m;
    let origin = [ox, oy];
    let fx = |x_mm: f64| ox + x_mm * mm_m;
    let fy = |y_mm: f64| oy + y_mm * mm_m;
    let yz = |z: f64| (z - lay.datum) * lay.exag;
    let col = |slot: usize, pdf: Rgb| -> (Rgb, i32) {
        let c = p.exp_colors[slot];
        if dxf {
            (export::rgb_of(caddy(c)), c)
        } else {
            (pdf, c)
        }
    };
    let line = |items: &mut Vec<Item>, layer: u8, slot: usize, pdf: Rgb, w: f64, pts: Vec<P2>| {
        let (rgb, cad) = col(slot, pdf);
        items.push(Item::Line { layer, rgb, cad, w_mm: w, pts, z: 0.0, closed: false, dash: false });
    };
    let text = |items: &mut Vec<Item>, layer: u8, slot: usize, pos: P2, h: f64, tax: f64, tay: f64, s: String| {
        let (rgb, cad) = col(slot, RGB_BLACK);
        items.push(Item::Text { layer, rgb, cad, pos, h_mm: h, rot_deg: 0.0, ax: tax, ay: tay, text: s, halo: false });
    };

    let (d0, d1) = (off, off + cut.length);
    let x_right_mm = aw - PAD_RIGHT_MM;
    let top_mm = ah - TITLE_H_MM; // Oberkante des Profilbereichs

    // ---- Rahmen (im PDF zeichnet render_pdf ihn selbst, außer bei mehreren Profilen je Blatt)
    if dxf || lay.cell_frame {
        let (rgb, cad) = col(slot::GRID, RGB_BLACK);
        items.push(Item::Line { layer: layer::GRID, rgb, cad, w_mm: W_DATUM, pts: vec![[fx(0.0), fy(0.0)], [fx(aw), fy(0.0)], [fx(aw), fy(ah)], [fx(0.0), fy(ah)]], z: 0.0, closed: true, dash: false });
    }

    // ---- Titel
    let kind_station = match def.view {
        ProfileView::Laengs => format!("Länge {:.2} m", cut.length),
        ProfileView::Quer => format!("Station {}", format_station(def.start_station)),
    };
    text(&mut items, layer::LABELS, slot::INDEX, [fx(2.0), fy(ah - 2.0)], TEXT_NORMAL_MM, 0.0, 1.0, def.title());
    text(
        &mut items,
        layer::LABELS,
        slot::INDEX,
        [fx(2.0), fy(ah - 2.0 - TEXT_NORMAL_MM - 2.5)],
        TEXT_LABEL_MM,
        0.0,
        1.0,
        format!("1:{}/1:{} {}fach {}", export::trim_num(lay.n), export::trim_num(lay.n / lay.exag), export::trim_num(lay.exag), kind_station),
    );

    // ---- Höhenraster mit Höhenzahlen
    let dec = decimals_for(lay.grid_dz);
    text(&mut items, layer::LABELS, slot::INDEX, [fx(HEAD_W_MM - 1.5), fy(top_mm - 1.0)], TEXT_LABEL_MM, 1.0, 1.0, "Höhe m".into());
    let mut z = lay.datum;
    let mut guard = 0;
    while z <= lay.top + lay.grid_dz * 0.01 && guard < 10_000 {
        let y_mm = base + (z - lay.datum) * k;
        if y_mm <= top_mm - TEXT_LABEL_MM - 3.0 {
            line(&mut items, layer::GRID, slot::GRID, RGB_GRID, W_GRID, vec![[fx(HEAD_W_MM), fy(y_mm)], [fx(x_right_mm), fy(y_mm)]]);
            text(&mut items, layer::HEIGHTS, slot::HEIGHT, [fx(HEAD_W_MM - 1.5), fy(y_mm)], TEXT_DIM_MM, 1.0, 0.5, format!("{:.*}", dec, z));
        }
        z += lay.grid_dz;
        guard += 1;
    }

    // ---- Beschriftungsstationen: Führungslinien und Bandwerte
    let min_gap = MIN_LABEL_GAP_MM * mm_m;
    let extra = (def.view == ProfileView::Quer).then_some(0.0);
    let stations = label_positions(d0, d1, lay.label_step, min_gap, extra);
    for &v in &stations {
        let s = v - off;
        let y_end = cut.z_at(s).map_or(base, |zz| base + (zz - lay.datum) * k);
        line(&mut items, layer::GRID, slot::GRID, RGB_GRID, W_GRID, vec![[v, fy(0.0)], [v, fy(y_end.min(top_mm))]]);
    }

    // ---- Gelände
    for r in &cut.runs {
        let pts: Vec<P2> = r.iter().map(|q| [q.s + off, fy(base) + yz(q.z)]).collect();
        line(&mut items, layer::CONTOUR, slot::CONTOUR, RGB_TERRAIN, W_TERRAIN, pts);
    }

    // ---- zweites Gelände (gestrichelt)
    for r in &cut.second {
        let pts: Vec<P2> = r.iter().map(|q| [q.s + off, fy(base) + yz(q.z)]).collect();
        let (rgb, cad) = col(slot::CUT, RGB_SECOND);
        items.push(Item::Line { layer: layer::DIFF, rgb, cad, w_mm: W_TERRAIN, pts, z: 0.0, closed: false, dash: true });
    }

    // ---- Wasserspiegel (Ausgleichungsebene der Wasserfläche) mit Beschriftung
    for wseg in &cut.waters {
        let pts: Vec<P2> = wseg.iter().map(|&(sv, z)| [sv + off, fy(base) + yz(z)]).collect();
        let (rgb, cad) = col(slot::FILL, p.water_rgb);
        items.push(Item::Line { layer: layer::FILL, rgb, cad, w_mm: W_TERRAIN, pts, z: 0.0, closed: false, dash: false });
        let len_mm = (wseg[1].0 - wseg[0].0) * 1000.0 / lay.n;
        let zm = (wseg[0].1 + wseg[1].1) / 2.0;
        let y_mm = base + (zm - lay.datum) * k + KOTE_LIFT_MM;
        if len_mm >= WATER_LABEL_MIN_MM && y_mm + TEXT_DIM_MM <= top_mm - 1.0 {
            items.push(Item::Text { layer: layer::FILL, rgb, cad, pos: [(wseg[0].0 + wseg[1].0) / 2.0 + off, fy(y_mm)], h_mm: TEXT_DIM_MM, rot_deg: 0.0, ax: 0.5, ay: 0.0, text: format!("WSP {:.2}", zm), halo: false });
        }
    }

    // ---- Bezugslinie
    line(&mut items, layer::INDEX, slot::INDEX, RGB_BLACK, W_DATUM, vec![[fx(HEAD_W_MM), fy(base)], [fx(x_right_mm), fy(base)]]);

    // ---- Koten an den Knickpunkten (Höhe senkrecht über dem Gelände), Bereinigung wie im LBB-Leitfaden
    if !def.no_koten {
        let clean = def.kink_clean.unwrap_or(KINK_CLEAN_DEFAULT_M).max(KOTE_MIN_GAP_MM * mm_m);
        let kote_w = |t: &str| t.chars().count() as f64 * TEXT_DIM_MM / MONO_CAP * MONO_ADVANCE;
        for r in &cut.runs {
            for kp in key_points(r, clean) {
                let y_mm = base + (kp.z - lay.datum) * k;
                let label = format!("{:.2}", kp.z);
                let (rgb, cad) = col(slot::HEIGHT, RGB_BLACK);
                items.push(Item::Circle { layer: layer::HEIGHTS, rgb, cad, c: [kp.s + off, fy(y_mm)], r_mm: KOTE_MARK_R_MM });
                if y_mm + KOTE_LIFT_MM + kote_w(&label) <= top_mm - 1.0 {
                    items.push(Item::Text { layer: layer::HEIGHTS, rgb, cad, pos: [kp.s + off, fy(y_mm + KOTE_LIFT_MM)], h_mm: TEXT_DIM_MM, rot_deg: 90.0, ax: 0.0, ay: 0.5, text: label, halo: false });
                }
            }
        }
    }

    // ---- Höhenunterschied-Maßlinie zwischen zwei Stationen
    if let Some((da, db)) = def.dim_h {
        if let (Some(za), Some(zb)) = (cut.z_at(da - off), cut.z_at(db - off)) {
            let (ya, yb) = (base + (za - lay.datum) * k, base + (zb - lay.datum) * k);
            if ya.max(yb) + 1.0 <= top_mm - 1.0 {
                let (rgb, cad) = col(slot::HEIGHT, RGB_BLACK);
                let ln = |items: &mut Vec<Item>, pts: Vec<P2>, dash: bool| items.push(Item::Line { layer: layer::HEIGHTS, rgb, cad, w_mm: W_BAND, pts, z: 0.0, closed: false, dash });
                // Hilfslinie waagrecht in Höhe von a, Maßlinie senkrecht bei b mit Endstrichen
                ln(&mut items, vec![[da, fy(ya)], [db, fy(ya)]], true);
                ln(&mut items, vec![[db, fy(ya)], [db, fy(yb)]], false);
                for y in [ya, yb] {
                    ln(&mut items, vec![[db - DIM_TICK_MM * mm_m, fy(y)], [db + DIM_TICK_MM * mm_m, fy(y)]], false);
                }
                items.push(Item::Text { layer: layer::HEIGHTS, rgb, cad, pos: [db + DIM_TEXT_GAP_MM * mm_m, fy((ya + yb) / 2.0)], h_mm: TEXT_DIM_MM, rot_deg: 90.0, ax: 0.5, ay: 0.5, text: format!("{:+.2} m", zb - za), halo: false });
                items.push(Item::Text { layer: layer::HEIGHTS, rgb, cad, pos: [(da + db) / 2.0, fy(ya + 1.0)], h_mm: TEXT_DIM_MM, rot_deg: 0.0, ax: 0.5, ay: 0.0, text: format!("{:.2} m", db - da), halo: false });
            }
        }
    }

    // ---- Band: Linien, Zeilenköpfe, Werte
    let nrows = rows.len();
    for i in 0..=nrows {
        let y_mm = i as f64 * BAND_ROW_MM;
        line(&mut items, layer::GRID, slot::GRID, RGB_BLACK, W_BAND, vec![[fx(0.0), fy(y_mm)], [fx(aw), fy(y_mm)]]);
    }
    line(&mut items, layer::GRID, slot::GRID, RGB_BLACK, W_BAND, vec![[fx(HEAD_W_MM), fy(0.0)], [fx(HEAD_W_MM), fy(band)]]);
    let row_mid = |i: usize| fy(band - (i as f64 + 0.5) * BAND_ROW_MM);
    for (i, r) in rows.iter().enumerate() {
        let head = match r {
            Row::Hoehe => "Höhe",
            Row::Hoehe2 => "Höhe 2",
            Row::Neigung => "Neigung",
            Row::Abstand => "Abstand",
            Row::Kette => "Kette",
            Row::Station => {
                if def.view == ProfileView::Laengs {
                    "Station"
                } else {
                    "Entf."
                }
            }
        };
        text(&mut items, layer::LABELS, slot::INDEX, [fx(1.5), row_mid(i)], TEXT_LABEL_MM, 0.0, 0.5, head.to_string());
    }
    let mut prev: Option<f64> = None;
    let mut prev_z: Option<f64> = None;
    for &v in &stations {
        let s = v - off;
        line(&mut items, layer::GRID, slot::GRID, RGB_GRID, W_GRID, vec![[v, fy(0.0)], [v, fy(band)]]);
        let z_here = cut.z_at(s);
        for (i, r) in rows.iter().enumerate() {
            let yy = row_mid(i);
            match r {
                Row::Hoehe => text(&mut items, layer::HEIGHTS, slot::HEIGHT, [v, yy], TEXT_DIM_MM, 0.5, 0.5, z_here.map_or("-".to_string(), |zz| format!("{:.2}", zz))),
                Row::Hoehe2 => text(&mut items, layer::HEIGHTS, slot::HEIGHT, [v, yy], TEXT_DIM_MM, 0.5, 0.5, cut.z2_at(s).map_or("-".to_string(), |zz| format!("{:.2}", zz))),
                Row::Station => {
                    let st = match def.view {
                        ProfileView::Laengs => format_station(v),
                        ProfileView::Quer => format!("{:+.2}", v),
                    };
                    text(&mut items, layer::HEIGHTS, slot::HEIGHT, [v, yy], TEXT_DIM_MM, 0.5, 0.5, st);
                }
                Row::Neigung => {
                    if let Some(pv) = prev {
                        let incl = match (prev_z, z_here) {
                            (Some(a), Some(b)) if v - pv > 0.0 => format!("{:+.1}%", (b - a) / (v - pv) * 100.0),
                            _ => "-".to_string(),
                        };
                        text(&mut items, layer::HEIGHTS, slot::HEIGHT, [(v + pv) / 2.0, yy], TEXT_DIM_MM, 0.5, 0.5, incl);
                    }
                }
                Row::Abstand => {
                    if let Some(pv) = prev {
                        text(&mut items, layer::HEIGHTS, slot::HEIGHT, [(v + pv) / 2.0, yy], TEXT_DIM_MM, 0.5, 0.5, format!("{:.2}", v - pv));
                    }
                }
                Row::Kette => {}
            }
        }
        prev = Some(v);
        prev_z = z_here;
    }
    // Maßkette: Teilabstände zwischen den Knickpunkten, Endstriche an jedem Punkt
    if let Some(ci) = rows.iter().position(|r| *r == Row::Kette) {
        let (y0, y1) = (band - (ci as f64 + 1.0) * BAND_ROW_MM, band - ci as f64 * BAND_ROW_MM);
        let ym = (y0 + y1) / 2.0;
        let clean = def.kink_clean.unwrap_or(KINK_CLEAN_DEFAULT_M).max(KOTE_MIN_GAP_MM * mm_m);
        for r in &cut.runs {
            let kps = key_points(r, clean);
            if kps.len() < 2 {
                continue;
            }
            line(&mut items, layer::HEIGHTS, slot::HEIGHT, RGB_BLACK, W_BAND, vec![[kps[0].s + off, fy(ym)], [kps[kps.len() - 1].s + off, fy(ym)]]);
            for kp in &kps {
                line(&mut items, layer::HEIGHTS, slot::HEIGHT, RGB_BLACK, W_BAND, vec![[kp.s + off, fy(y0 + 1.0)], [kp.s + off, fy(y1 - 1.0)]]);
            }
            for w in kps.windows(2) {
                let d = w[1].s - w[0].s;
                let t = format!("{:.2}", d);
                if d * 1000.0 / lay.n >= mono_w(&t, TEXT_DIM_MM) + 1.0 {
                    text(&mut items, layer::HEIGHTS, slot::HEIGHT, [(w[0].s + w[1].s) / 2.0 + off, fy(ym + 0.8)], TEXT_DIM_MM, 0.5, 0.0, t);
                }
            }
        }
    }

    // ---- Legende
    let (zlo, zhi) = cut.z_range().unwrap_or((lay.datum, lay.top));
    let mut lines = vec![
        (format!("Länge {:.2} m", cut.length), false),
        (format!("Höhen {:.2} bis {:.2} m", zlo, zhi), false),
        (format!("Maßstab Länge 1:{}", export::trim_num(lay.n)), false),
        (format!("Maßstab Höhe 1:{}", export::trim_num(lay.n / lay.exag)), false),
        (format!("Überhöhung {}-fach", export::trim_num(lay.exag)), false),
        (format!("Bezugshöhe {:.*} m (untere Linie)", dec.max(1), lay.datum), false),
        (format!("Beschriftung alle {} m", export::trim_num(lay.label_step)), false),
    ];
    if !cut.second.is_empty() {
        lines.push((format!("Gestrichelt: {}", if cut.second_label.is_empty() { "zweites Gelände" } else { cut.second_label.as_str() }), false));
    }
    if let Some((da, db)) = def.dim_h {
        if let (Some(za), Some(zb)) = (cut.z_at(da - off), cut.z_at(db - off)) {
            lines.push((format!("Höhenunterschied {:+.2} m auf {:.2} m", zb - za, db - da), false));
        }
    }
    if !cut.waters.is_empty() {
        lines.push((format!("Wasserspiegel: {} Abschnitt(e)", cut.waters.len()), false));
    }
    if !lay.fits {
        lines.push(("Achtung: passt nicht auf ein Blatt".into(), true));
    }
    let legend = Legend { title: def.title(), blocks: vec![Block::Lines(lines)] };
    (Scene { items, legend, names: &PROFILE_LAYER_NAMES }, origin)
}

/// Geländeschnitt und Layout eines Profils (mit Wasserspiegel und zweitem Gelände, soweit gewünscht).
pub fn prepare(sf: &Surface, def: &ProfileDef, max_step: f64, ex: &Extras) -> Result<(Cut, Layout), String> {
    if def.line.len() < 2 || def.length() <= 0.0 {
        return Err("Die Schnittlinie braucht mindestens zwei verschiedene Punkte.".into());
    }
    let mut c = cut(sf, &def.line, max_step);
    if c.runs.is_empty() {
        return Err("Die Schnittlinie liegt außerhalb des Geländemodells.".into());
    }
    add_waters(&mut c, &def.line, ex.waters);
    if def.show_second {
        if let Some((s2, label)) = ex.second {
            c.second = cut(s2, &def.line, max_step).runs;
            c.second_label = label.to_string();
        }
    }
    let Some((zlo, zhi)) = c.z_range() else {
        return Err("Die Schnittlinie liegt außerhalb des Geländemodells.".into());
    };
    let rows = band_rows(def, !c.second.is_empty()).len();
    let lay = auto_layout(def, c.length, zlo, zhi, rows);
    Ok((c, lay))
}

/// Profile einer Reihe mit gemeinsamem Maßstab, gemeinsamer Überhöhung und gemeinsamem Beschriftungsintervall
/// (Vorgaben im Profil gelten weiter).
pub fn prepare_series(sf: &Surface, defs: &[ProfileDef], max_step: f64, ex: &Extras) -> Vec<(Cut, ProfileDef, Layout)> {
    let Some((n, e, step)) = common_layout(sf, defs, max_step, ex) else { return Vec::new() };
    let mut out = Vec::new();
    for d in defs {
        let mut d = d.clone();
        if d.scale_n <= 0.0 {
            d.scale_n = n;
        }
        if d.exag <= 0.0 {
            d.exag = e;
        }
        if d.label_step <= 0.0 {
            d.label_step = step;
        }
        if let Ok((c, lay)) = prepare(sf, &d, max_step, ex) {
            out.push((c, d, lay));
        }
    }
    out
}

/// Teil einer Linie zwischen den Längen `s0` und `s1` ab Anfang.
fn sub_line(line: &[[f64; 2]], s0: f64, s1: f64) -> Vec<[f64; 2]> {
    let mut out: Vec<[f64; 2]> = Vec::new();
    let mut acc = 0.0;
    for w in line.windows(2) {
        let d = (w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1]);
        if d <= 0.0 {
            continue;
        }
        let (a0, a1) = (acc, acc + d);
        acc = a1;
        if a1 < s0 || a0 > s1 {
            continue;
        }
        let pt = |s: f64| {
            let f = ((s - a0) / d).clamp(0.0, 1.0);
            [w[0][0] + (w[1][0] - w[0][0]) * f, w[0][1] + (w[1][1] - w[0][1]) * f]
        };
        if out.is_empty() {
            out.push(pt(s0.max(a0)));
        }
        let q = pt(s1.min(a1));
        if out.last().map_or(true, |l| (l[0] - q[0]).hypot(l[1] - q[1]) > 1e-9) {
            out.push(q);
        }
    }
    out
}

/// Zerlegt ein Längsprofil, das auch auf der größten Rolle (4000 mm) nicht in der Länge passt, in gleich lange
/// Teile mit angepasster Anfangsstation; passt es, kommt das Profil unverändert zurück.
pub fn split_parts(def: &ProfileDef, length: f64, lay: &Layout) -> Vec<ProfileDef> {
    let max_w = ROLL_MAX_MM - 2.0 * SHEET_MARGIN_MM - STRIP_MM - HEAD_W_MM - PAD_LEFT_MM - PAD_RIGHT_MM;
    let part_m = (max_w * lay.n / 1000.0).floor();
    if def.view != ProfileView::Laengs || part_m <= 0.0 || length <= part_m {
        return vec![def.clone()];
    }
    let nparts = (length / part_m).ceil() as usize;
    let pl = length / nparts as f64;
    (0..nparts)
        .map(|i| {
            let mut d = def.clone();
            d.line = sub_line(&def.line, i as f64 * pl, (i + 1) as f64 * pl);
            d.start_station = def.start_station + i as f64 * pl;
            d.name = format!("{} Blatt {}/{}", def.name, i + 1, nparts);
            d.scale_n = lay.n;
            d.format = SheetFormat::Roll;
            d
        })
        .collect()
}

/// Ein Blatt mit mehreren Profilen
pub struct SeriesPage {
    pub scene: Scene,
    pub sheet: Sheet,
    pub n: f64,
    pub count: usize,
}

fn shift_item(it: &mut Item, d: [f64; 2]) {
    let mv = |q: &mut P2| {
        q[0] += d[0];
        q[1] += d[1];
    };
    match it {
        Item::Line { pts, .. } => pts.iter_mut().for_each(mv),
        Item::Circle { c, .. } => mv(c),
        Item::Text { pos, .. } => mv(pos),
        Item::Mesh { pos, .. } => pos.iter_mut().for_each(mv),
    }
}

/// Mehrere Profile (gleicher Maßstab, siehe `prepare_series`) in Zeilen und Spalten auf Blätter setzen: A4, A3
/// oder Rolle (eine Rolle nimmt so viele Spalten wie bis 4000 mm passen). Jedes Profil bekommt einen Rahmen.
pub fn series_pages(items: &[(Cut, ProfileDef, Layout)], format: SheetFormat, p: &Params, dxf: bool) -> Vec<SeriesPage> {
    if items.is_empty() {
        return Vec::new();
    }
    let n = items.iter().map(|i| i.2.n).fold(0.0, f64::max);
    let mm_m = n / 1000.0;
    let (mut cell_w, mut cell_h) = (0.0f64, 0.0f64);
    for (c, d, l) in items {
        let rows = band_rows(d, !c.second.is_empty()).len();
        cell_w = cell_w.max(HEAD_W_MM + PAD_LEFT_MM + c.length * 1000.0 / n + PAD_RIGHT_MM);
        cell_h = cell_h.max(TITLE_H_MM + band_height(rows) + GRAPH_LIFT_MM + (l.top - l.datum) * l.k() + TOP_PAD_MM);
    }
    let (extra_w, extra_h) = (2.0 * SHEET_MARGIN_MM + STRIP_MM, 2.0 * SHEET_MARGIN_MM);
    let grid = |w: f64, h: f64| -> (usize, usize) { (((w - extra_w + CELL_GAP_MM) / (cell_w + CELL_GAP_MM)).floor().max(0.0) as usize, ((h - extra_h + CELL_GAP_MM) / (cell_h + CELL_GAP_MM)).floor().max(0.0) as usize) };
    let order: &[SheetFormat] = match format {
        SheetFormat::A4 => &[SheetFormat::A4, SheetFormat::A3, SheetFormat::Roll],
        SheetFormat::A3 | SheetFormat::Auto => &[SheetFormat::A3, SheetFormat::Roll],
        SheetFormat::Roll => &[SheetFormat::Roll],
    };
    let mut chosen: Option<(SheetFormat, (f64, f64), usize, usize)> = None;
    for f in order {
        let page = match f {
            SheetFormat::A4 => A4_LANDSCAPE,
            SheetFormat::A3 => A3_LANDSCAPE,
            _ => (ROLL_MAX_MM, ROLL_WIDTH_MM),
        };
        let (cols, rws) = grid(page.0, page.1);
        if cols >= 1 && rws >= 1 {
            chosen = Some((*f, page, cols, rws));
            break;
        }
    }
    let Some((fmt, page, cols_max, rows_n)) = chosen else { return Vec::new() };
    let mut pages = Vec::new();
    let mut idx = 0;
    while idx < items.len() {
        let remaining = items.len() - idx;
        let (cols, pw, ph) = if fmt == SheetFormat::Roll {
            let cols = remaining.div_ceil(rows_n).min(cols_max).max(1);
            let w = ((cols as f64 * (cell_w + CELL_GAP_MM) - CELL_GAP_MM + extra_w) / ROLL_ROUND_MM).ceil() * ROLL_ROUND_MM;
            (cols, w.max(ROLL_MIN_MM), ROLL_WIDTH_MM)
        } else {
            (cols_max, page.0, page.1)
        };
        let sheet = Sheet::custom(pw, ph, STRIP_MM);
        let (_, _, _, ah) = sheet.area();
        let take = (cols * rows_n).min(remaining);
        let mut sc_items: Vec<Item> = Vec::new();
        for j in 0..take {
            let (c, d, l) = &items[idx + j];
            let (ci, ri) = ((j % cols) as f64, (j / cols) as f64);
            let mut lay = l.clone();
            lay.sheet = Sheet::custom(cell_w + extra_w, cell_h + extra_h, STRIP_MM);
            lay.cell_frame = true;
            let (mut sc, o) = build_scene(c, d, &lay, p, dxf);
            let (x_mm, y_mm) = (ci * (cell_w + CELL_GAP_MM), ah - (ri + 1.0) * cell_h - ri * CELL_GAP_MM);
            let shift = [x_mm * mm_m - o[0], y_mm * mm_m - o[1]];
            sc.items.iter_mut().for_each(|it| shift_item(it, shift));
            sc_items.extend(sc.items);
        }
        let first = &items[idx];
        let last = &items[idx + take - 1];
        let lines = vec![
            (format!("{} Profile auf diesem Blatt", take), false),
            (format!("{} bis {}", first.1.name, last.1.name), false),
            (format!("Maßstab Länge 1:{}", export::trim_num(n)), false),
            (format!("Maßstab Höhe 1:{}", export::trim_num(n / first.2.exag)), false),
            (format!("Überhöhung {}-fach", export::trim_num(first.2.exag)), false),
            (format!("Beschriftung alle {} m", export::trim_num(first.2.label_step)), false),
        ];
        let legend = Legend { title: format!("Profile {}", first.1.title()), blocks: vec![Block::Lines(lines)] };
        pages.push(SeriesPage { scene: Scene { items: sc_items, legend, names: &PROFILE_LAYER_NAMES }, sheet, n, count: take });
        idx += take;
    }
    pages
}

/// Schrift für das PDF: eingestellte Schriftdatei, sonst eine gefundene Normschrift; „-“ = immer Courier.
fn pdf_font(p: &Params) -> Option<pdf::EmbeddedFont> {
    match p.profile_font.trim() {
        "" => pdf::find_standard_font(),
        "-" => None,
        path => pdf::load_font(std::path::Path::new(path)).ok(),
    }
}

/// Vektor-PDF des Profils: Normschrift, wenn eine gefunden wird, sonst Courier.
pub fn to_pdf(c: &Cut, def: &ProfileDef, lay: &Layout, p: &Params, project: &str) -> Vec<u8> {
    let (sc, origin) = build_scene(c, def, lay, p, false);
    export::render_pdf_ext(&sc, &lay.sheet, origin, lay.n, project, true, None, pdf_font(p).as_ref())
}

/// Vektor-PDF eines Blatts mit mehreren Profilen.
pub fn series_to_pdf(page: &SeriesPage, p: &Params, project: &str) -> Vec<u8> {
    export::render_pdf_ext(&page.scene, &page.sheet, [0.0, 0.0], page.n, project, true, None, pdf_font(p).as_ref())
}

/// DXF eines Blatts mit mehreren Profilen.
pub fn series_to_dxf(page: &SeriesPage, p: &Params) -> String {
    export::to_dxf_styled(&page.scene, p, page.n, Some((DXF_STYLE_NAME, DXF_STYLE_FONT)))
}

/// DXF des Profils: überhöht, in Metern, Schriftgrößen nach dem Maßstab.
pub fn to_dxf(c: &Cut, def: &ProfileDef, lay: &Layout, p: &Params) -> String {
    let (sc, _) = build_scene(c, def, lay, p, true);
    export::to_dxf_styled(&sc, p, lay.n, Some((DXF_STYLE_NAME, DXF_STYLE_FONT)))
}

/// Wertetabelle des Profils als CSV (Trennzeichen „;“, Dezimalpunkt): Station bzw. Entfernung, Rechts-, Hochwert,
/// Höhe und Neigung (%) zum vorigen Punkt; zwischen Teilstücken eine Leerzeile.
pub fn to_csv(c: &Cut, def: &ProfileDef) -> String {
    let off = def.offset();
    let head = if def.view == ProfileView::Laengs { "Station" } else { "Entfernung" };
    let mut out = format!("{};Laenge;Rechtswert;Hochwert;Hoehe;Neigung_Prozent\n", head);
    for (i, r) in c.runs.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        let mut prev: Option<&CutPt> = None;
        for q in r {
            let incl = match prev {
                Some(pv) if q.s - pv.s > 0.0 => format!("{:.2}", (q.z - pv.z) / (q.s - pv.s) * 100.0),
                _ => String::new(),
            };
            out += &format!("{:.3};{:.3};{:.3};{:.3};{:.3};{}\n", q.s + off, q.s, q.x, q.y, q.z, incl);
            prev = Some(q);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Project;
    use crate::tin;

    fn demo() -> (Project, Surface) {
        let mut pr = Project::demo();
        let (t, _) = tin::triangulate(&pr);
        pr.st.triangles = t;
        let s = Surface::new(&pr, false);
        (pr, s)
    }

    fn demo_def(pr: &Project) -> ProfileDef {
        let b = pr.bbox().unwrap();
        let ym = (b[1] + b[3]) / 2.0;
        ProfileDef { name: "A".into(), line: vec![[b[0] + 20.0, ym], [b[2] - 20.0, ym + 10.0]], ..Default::default() }
    }

    #[test]
    fn cut_follows_the_flat_tin_exactly() {
        let (pr, s) = demo();
        let def = demo_def(&pr);
        let c = cut(&s, &def.line, 1.0);
        assert!(!c.runs.is_empty());
        let n: usize = c.runs.iter().map(|r| r.len()).sum();
        assert!(n > 10, "Stützstellen an den Dreieckskanten erwartet, {}", n);
        for r in &c.runs {
            for w in r.windows(2) {
                assert!(w[1].s >= w[0].s);
                let (mx, my) = ((w[0].x + w[1].x) / 2.0, (w[0].y + w[1].y) / 2.0);
                let zt = s.height(mx, my).expect("Mitte liegt im Modell");
                assert!((zt - (w[0].z + w[1].z) / 2.0).abs() < 1e-6, "linear zwischen den Kantenschnitten: {:?} {:?} Mitte {} statt {}", w[0], w[1], zt, (w[0].z + w[1].z) / 2.0);
            }
        }
        assert!((c.length - def.length()).abs() < 1e-9);
    }

    #[test]
    fn clip_to_model_trims_lines_that_leave_the_model() {
        let (pr, s) = demo();
        let b = pr.bbox().unwrap();
        let ym = (b[1] + b[3]) / 2.0;
        let xm = (b[0] + b[2]) / 2.0;
        // Linie ragt links und rechts weit über das Modell hinaus, Nullpunkt in der Mitte
        let wide = ProfileDef { view: ProfileView::Quer, line: vec![[b[0] - 500.0, ym], [b[2] + 500.0, ym]], ..Default::default() };
        let len = wide.length();
        let c = clip_to_model(&s, wide).expect("trifft das Modell");
        assert!(c.length() < len - 500.0);
        assert!((c.line[0][0] + c.zero.unwrap() - xm).abs() < 1e-6, "Nullpunkt bleibt an seiner Stelle");
        assert!(c.line[0][0] >= b[0] - 1e-6 && c.line.last().unwrap()[0] <= b[2] + 1e-6);
        let away = ProfileDef { line: vec![[b[0] - 900.0, ym], [b[0] - 500.0, ym]], ..Default::default() };
        assert!(clip_to_model(&s, away).is_none());
    }

    #[test]
    fn cut_outside_the_model_is_empty() {
        let (pr, s) = demo();
        let b = pr.bbox().unwrap();
        let c = cut(&s, &[[b[0] - 500.0, b[1] - 500.0], [b[0] - 400.0, b[1] - 500.0]], 1.0);
        assert!(c.z_range().is_none());
    }

    #[test]
    fn layout_picks_a_sheet_and_respects_limits() {
        let def = ProfileDef::default();
        let l = auto_layout(&def, 300.0, 410.0, 430.0, 4);
        assert!(l.fits && SCALES.contains(&l.n) && l.exag <= EXAG_MAX_AUTO);
        assert!(l.label_step * 1000.0 / l.n >= MIN_LABEL_GAP_MM);
        let (w, h) = l.sheet.size();
        assert!(w <= ROLL_MAX_MM && h <= ROLL_WIDTH_MM.max(A3_LANDSCAPE.1));
        // sehr langes Profil: Rolle, höchstens 4000 mm
        let l2 = auto_layout(&def, 3000.0, 410.0, 430.0, 4);
        assert!(l2.fits);
        assert_eq!(l2.sheet.size().1, ROLL_WIDTH_MM);
        assert!(l2.sheet.size().0 <= ROLL_MAX_MM && l2.sheet.size().0 >= ROLL_MIN_MM);
        // Querprofil: Überhöhung 1
        let q = ProfileDef { view: ProfileView::Quer, ..Default::default() };
        assert_eq!(auto_layout(&q, 40.0, 410.0, 414.0, 4).exag, 1.0);
        // Vorgaben gelten
        let user = ProfileDef { scale_n: 1000.0, exag: 5.0, label_step: 100.0, format: SheetFormat::A3, ..Default::default() };
        let l3 = auto_layout(&user, 250.0, 410.0, 415.0, 4);
        assert_eq!((l3.n, l3.exag, l3.label_step), (1000.0, 5.0, 100.0));
        assert_eq!(l3.sheet.size(), A3_LANDSCAPE);
        // unmöglich lang: Meldung statt Absturz
        assert!(!auto_layout(&def, 400_000.0, 0.0, 10.0, 4).fits);
    }

    #[test]
    fn labels_keep_their_distance_and_include_the_ends() {
        let v = label_positions(0.0, 100.0, 20.0, 15.0, None);
        assert_eq!(v, vec![0.0, 20.0, 40.0, 60.0, 80.0, 100.0]);
        let v = label_positions(5.0, 47.0, 20.0, 15.0, None);
        assert_eq!(v, vec![5.0, 20.0, 47.0]);
        let q = label_positions(-10.0, 10.0, 5.0, 4.0, Some(0.0));
        assert!(q.contains(&0.0) && q.first() == Some(&-10.0) && q.last() == Some(&10.0));
    }

    #[test]
    fn scene_uses_the_three_text_sizes_and_the_view_labels() {
        let (pr, s) = demo();
        let mut def = demo_def(&pr);
        def.start_station = 120.0;
        let (c, lay) = prepare(&s, &def, 1.0, &Extras::default()).unwrap();
        let (sc, _) = build_scene(&c, &def, &lay, &pr.st.params, false);
        let sizes: Vec<f64> = sc.items.iter().filter_map(|i| if let Item::Text { h_mm, .. } = i { Some(*h_mm) } else { None }).collect();
        for want in [TEXT_NORMAL_MM, TEXT_LABEL_MM, TEXT_DIM_MM] {
            assert!(sizes.contains(&want), "Schrifthöhe {} fehlt", want);
        }
        assert!(sizes.iter().all(|h| [TEXT_NORMAL_MM, TEXT_LABEL_MM, TEXT_DIM_MM].contains(h)));
        let texts: Vec<String> = sc.items.iter().filter_map(|i| if let Item::Text { text, .. } = i { Some(text.clone()) } else { None }).collect();
        assert!(texts.iter().any(|t| t == "Station") && texts.iter().any(|t| t.starts_with("0+120")));
        let mut q = def.clone();
        q.view = ProfileView::Quer;
        let (c2, lay2) = prepare(&s, &q, 1.0, &Extras::default()).unwrap();
        let (sq, _) = build_scene(&c2, &q, &lay2, &pr.st.params, false);
        let tq: Vec<String> = sq.items.iter().filter_map(|i| if let Item::Text { text, .. } = i { Some(text.clone()) } else { None }).collect();
        assert!(tq.iter().any(|t| t == "Entf.") && tq.iter().any(|t| t.starts_with('-')) && tq.iter().any(|t| t.starts_with('+')));
    }

    #[test]
    fn drawing_stays_inside_its_regions() {
        let (pr, s) = demo();
        let def = demo_def(&pr);
        let (c, lay) = prepare(&s, &def, 1.0, &Extras::default()).unwrap();
        let (sc, o) = build_scene(&c, &def, &lay, &pr.st.params, false);
        let mm = 1000.0 / lay.n;
        let (_, _, aw, ah) = lay.sheet.area();
        for it in &sc.items {
            let pts: Vec<P2> = match it {
                Item::Line { pts, .. } => pts.clone(),
                Item::Text { pos, .. } => vec![*pos],
                _ => vec![],
            };
            for q in pts {
                let (x, y) = ((q[0] - o[0]) * mm, (q[1] - o[1]) * mm);
                assert!(x >= -1e-6 && x <= aw + 1e-6 && y >= -1e-6 && y <= ah + 1e-6, "außerhalb: {:?} -> ({}, {})", it, x, y);
            }
        }
        // Gelände liegt über der Bandoberkante und unter dem Titelbereich
        for it in &sc.items {
            if let Item::Line { layer: l, pts, w_mm, .. } = it {
                if *l == layer::CONTOUR && *w_mm == W_TERRAIN {
                    for q in pts {
                        let y = (q[1] - o[1]) * mm;
                        assert!(y >= band_height(lay.rows) + GRAPH_LIFT_MM - 1e-6 && y <= ah - TITLE_H_MM, "Gelände y = {}", y);
                    }
                }
            }
        }
    }

    #[test]
    fn key_points_respect_the_cleaning_distance() {
        let mk = |s: f64, z: f64| CutPt { s, x: s, y: 0.0, z };
        // Dach: steigt bis 10, fällt bis 20; kleiner Knick bei 12 und 12.5
        let run = vec![mk(0.0, 0.0), mk(10.0, 5.0), mk(12.0, 3.9), mk(12.5, 3.2), mk(20.0, 0.0)];
        let all = key_points(&run, 0.0);
        assert_eq!(all.len(), 5);
        let few = key_points(&run, 5.0);
        // Anfang, Ende und die stärkste Spitze bei 10 bleiben; Punkte näher als 5 m daneben entfallen
        assert!(few.iter().any(|p| p.s == 10.0) && few.first().unwrap().s == 0.0 && few.last().unwrap().s == 20.0);
        assert!(few.windows(2).all(|w| w[1].s - w[0].s >= 5.0 - 1e-9));
        // gerade Strecke: nur Anfang und Ende
        let line = vec![mk(0.0, 0.0), mk(5.0, 1.0), mk(10.0, 2.0)];
        assert_eq!(key_points(&line, 0.0).len(), 2);
    }

    #[test]
    fn possible_steps_stations_and_axis_parsing() {
        let st = possible_steps(100.0);
        assert!(st.contains(&10.0) && st.contains(&50.0) && !st.contains(&100.0));
        assert!(st.iter().all(|s| (100.0 / s).ceil() + 1.0 <= MAX_CROSS_SECTIONS as f64));
        let axis = [[0.0, 0.0], [30.0, 0.0], [30.0, 40.0]];
        assert_eq!(vertex_stations(&axis), vec![0.0, 30.0, 70.0]);
        assert_eq!(regular_stations(25.0, 10.0), vec![0.0, 10.0, 20.0, 25.0]);
        let a = parse_axis("x;y\n1,5;2,5\n3 4 9\n# Kommentar\n5\t6\n");
        assert_eq!(a, vec![[1.5, 2.5], [3.0, 4.0], [5.0, 6.0]]);
    }

    #[test]
    fn cross_sections_are_perpendicular_and_centred_on_the_axis() {
        let axis = [[0.0, 0.0], [100.0, 0.0], [100.0, 50.0]];
        let v = cross_sections(&axis, 1000.0, 20.0, 15.0);
        // Stationen 0, 20, …, 140 und Ende 150
        assert_eq!(v.len(), 9);
        assert_eq!(v[0].start_station, 1000.0);
        assert!((v[1].line[0][1] - 15.0).abs() < 1e-9 && (v[1].line[1][1] + 15.0).abs() < 1e-9, "links = +y bei Richtung +x: {:?}", v[1].line);
        assert!((v[1].line[0][0] - 20.0).abs() < 1e-9);
        assert!(v.iter().all(|d| d.view == ProfileView::Quer && (d.length() - 30.0).abs() < 1e-9));
        // am Knick (Station 100 liegt bei Index 5 = 100 m): Winkelhalbierende
        let k = cross_sections(&axis, 0.0, 100.0, 10.0);
        let m = &k[1];
        let mid = [(m.line[0][0] + m.line[1][0]) / 2.0, (m.line[0][1] + m.line[1][1]) / 2.0];
        assert!((mid[0] - 100.0).abs() < 1e-9 && mid[1].abs() < 1e-9);
        let (dx, dy) = (m.line[1][0] - m.line[0][0], m.line[1][1] - m.line[0][1]);
        assert!((dx - dy).abs() < 1e-9 || (dx + dy).abs() < 1e-9, "45°-Richtung am Knick: ({}, {})", dx, dy);
        assert!(cross_sections(&axis, 0.0, 0.0, 10.0).is_empty());
        assert!(cross_sections(&axis, 0.0, 0.001, 10.0).len() <= MAX_CROSS_SECTIONS);
    }

    #[test]
    fn scene_has_koten_and_slope_row() {
        let (pr, s) = demo();
        let def = demo_def(&pr);
        let (c, lay) = prepare(&s, &def, 1.0, &Extras::default()).unwrap();
        let (sc, _) = build_scene(&c, &def, &lay, &pr.st.params, false);
        assert!(sc.items.iter().any(|i| matches!(i, Item::Circle { .. })));
        assert!(sc.items.iter().any(|i| matches!(i, Item::Text { rot_deg, .. } if *rot_deg == 90.0)));
        assert!(sc.items.iter().any(|i| matches!(i, Item::Text { text, .. } if text.ends_with('%'))));
        let mut off = def.clone();
        off.no_koten = true;
        let (sc2, _) = build_scene(&c, &off, &lay, &pr.st.params, false);
        assert!(!sc2.items.iter().any(|i| matches!(i, Item::Circle { .. })));
    }

    #[test]
    fn water_level_is_cut_and_drawn() {
        let (pr, s) = demo();
        let def = demo_def(&pr);
        let (a, b) = (def.line[0], def.line[1]);
        let m = [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0];
        let poly = vec![(m[0] - 20.0, m[1] - 40.0), (m[0] + 20.0, m[1] - 40.0), (m[0] + 20.0, m[1] + 40.0), (m[0] - 20.0, m[1] + 40.0)];
        let plane = PlaneFit { x0: m[0], y0: m[1], z0: 440.0, a: 0.01, b: 0.0 };
        let (c, lay) = prepare(&s, &def, 1.0, &Extras { waters: &[(poly, plane)], second: None }).unwrap();
        assert_eq!(c.waters.len(), 1);
        let w = c.waters[0];
        assert!(((w[1].0 - w[0].0) - 40.0).abs() < 0.5, "Länge des Stücks {}", w[1].0 - w[0].0);
        assert!((w[0].1 - 440.0).abs() < 0.5 + 0.01 * 25.0 && w[1].1 > w[0].1, "Höhe aus der Ebene, steigt mit x");
        let (sc, _) = build_scene(&c, &def, &lay, &pr.st.params, false);
        assert!(sc.items.iter().any(|i| matches!(i, Item::Line { layer: l, rgb, .. } if *l == layer::FILL && *rgb == pr.st.params.water_rgb)));
        // ohne Wasserfläche keine Wasserlinie
        let (c0, _) = prepare(&s, &def, 1.0, &Extras::default()).unwrap();
        assert!(c0.waters.is_empty());
    }

    #[test]
    fn csv_has_header_rows_and_slope() {
        let (pr, s) = demo();
        let mut def = demo_def(&pr);
        def.start_station = 100.0;
        let (c, _) = prepare(&s, &def, 1.0, &Extras::default()).unwrap();
        let csv = to_csv(&c, &def);
        let mut lines = csv.lines();
        assert!(lines.next().unwrap().starts_with("Station;Laenge;"));
        let first: Vec<&str> = lines.next().unwrap().split(';').collect();
        assert_eq!(first.len(), 6);
        assert!((first[0].parse::<f64>().unwrap() - 100.0).abs() < 1e-6 && first[5].is_empty());
        assert!(csv.lines().count() >= 1 + c.runs.iter().map(|r| r.len()).sum::<usize>());
    }

    fn line_across(pr: &Project, y_off: f64) -> ProfileDef {
        let b = pr.bbox().unwrap();
        let ym = (b[1] + b[3]) / 2.0 + y_off;
        ProfileDef { name: "A".into(), line: vec![[b[0] + 20.0, ym], [b[2] - 20.0, ym]], ..Default::default() }
    }

    #[test]
    fn second_terrain_chain_and_height_dimension() {
        let (pr, s) = demo();
        // zweites Gelände = um 1,5 m höheres Modell derselben Punkte
        let mut pr2 = Project::demo();
        for p in pr2.st.points.iter_mut() {
            p.z += 1.5;
        }
        let (t, _) = tin::triangulate(&pr2);
        pr2.st.triangles = t;
        let s2 = Surface::new(&pr2, false);
        let mut def = line_across(&pr, 0.0);
        def.show_second = true;
        def.dim_chain = true;
        def.dim_h = Some((50.0, 150.0));
        let ex = Extras { waters: &[], second: Some((&s2, "Planung")) };
        let (c, lay) = prepare(&s, &def, 1.0, &ex).unwrap();
        assert!(!c.second.is_empty() && c.second_label == "Planung");
        assert!((c.z2_at(100.0).unwrap() - c.z_at(100.0).unwrap() - 1.5).abs() < 1e-6);
        assert_eq!(lay.rows, 6, "Höhe, Höhe 2, Neigung, Abstand, Kette, Station");
        let (sc, _) = build_scene(&c, &def, &lay, &pr.st.params, false);
        assert!(sc.items.iter().any(|i| matches!(i, Item::Line { dash: true, layer: l, .. } if *l == layer::DIFF)));
        let texts: Vec<String> = sc.items.iter().filter_map(|i| if let Item::Text { text, .. } = i { Some(text.clone()) } else { None }).collect();
        assert!(texts.iter().any(|t| t == "Höhe 2") && texts.iter().any(|t| t == "Kette") && texts.iter().any(|t| t.ends_with(" m") && t.starts_with(&['+', '-'][..])));
        // ohne Schalter kein zweites Gelände
        def.show_second = false;
        assert!(prepare(&s, &def, 1.0, &ex).unwrap().0.second.is_empty());
    }

    #[test]
    fn long_profile_is_split_into_equal_parts() {
        let def = ProfileDef { name: "L".into(), line: vec![[0.0, 0.0], [6000.0, 0.0], [10000.0, 3000.0]], start_station: 100.0, scale_n: 500.0, ..Default::default() };
        let lay = auto_layout(&def, def.length(), 400.0, 420.0, 4);
        let parts = split_parts(&def, def.length(), &lay);
        assert!(parts.len() > 3, "{} Teile", parts.len());
        let total: f64 = parts.iter().map(|d| d.length()).sum();
        assert!((total - def.length()).abs() < 1e-6);
        for (i, d) in parts.iter().enumerate() {
            assert!(d.length() * 1000.0 / 500.0 <= ROLL_MAX_MM, "Teil {} zu lang", i);
            if i > 0 {
                let (a, b) = (parts[i - 1].line.last().unwrap(), d.line[0]);
                assert!((a[0] - b[0]).hypot(a[1] - b[1]) < 1e-6, "Teile schließen aneinander an");
                assert!((d.start_station - parts[i - 1].start_station - parts[i - 1].length()).abs() < 1e-6);
            }
        }
        assert_eq!(parts[0].start_station, 100.0);
        // kurzes Profil bleibt ganz
        let short = ProfileDef { line: vec![[0.0, 0.0], [100.0, 0.0]], ..Default::default() };
        assert_eq!(split_parts(&short, 100.0, &lay).len(), 1);
    }

    #[test]
    fn several_cross_sections_share_a_sheet() {
        let (pr, s) = demo();
        let b = pr.bbox().unwrap();
        let axis = [[b[0] + 60.0, (b[1] + b[3]) / 2.0 - 30.0], [b[0] + 160.0, (b[1] + b[3]) / 2.0 - 30.0]];
        let defs = cross_sections(&axis, 0.0, 20.0, 5.0);
        assert_eq!(defs.len(), 6);
        let items = prepare_series(&s, &defs, 1.0, &Extras::default());
        assert_eq!(items.len(), 6);
        assert!(items.iter().all(|i| i.2.n == items[0].2.n && i.2.exag == items[0].2.exag));
        let pages = series_pages(&items, SheetFormat::Auto, &pr.st.params, false);
        assert!(!pages.is_empty());
        assert_eq!(pages.iter().map(|p| p.count).sum::<usize>(), 6);
        assert!(pages[0].count > 1, "mehrere Profile auf dem ersten Blatt");
        for pg in &pages {
            let (_, _, aw, ah) = pg.sheet.area();
            let mm = 1000.0 / pg.n;
            for it in &pg.scene.items {
                let pts: Vec<P2> = match it {
                    Item::Line { pts, .. } => pts.clone(),
                    Item::Text { pos, .. } => vec![*pos],
                    Item::Circle { c, .. } => vec![*c],
                    _ => vec![],
                };
                for q in pts {
                    let (x, y) = (q[0] * mm, q[1] * mm);
                    assert!(x >= -1e-6 && x <= aw + 1e-6 && y >= -1e-6 && y <= ah + 1e-6, "außerhalb des Blatts: {:?} ({}, {})", it, x, y);
                }
            }
            let frames = pg.scene.items.iter().filter(|i| matches!(i, Item::Line { closed: true, .. })).count();
            assert_eq!(frames, pg.count, "ein Rahmen je Profil");
            let pdf = series_to_pdf(pg, &pr.st.params, "Beispiel");
            assert!(pdf.starts_with(b"%PDF"));
            assert!(series_to_dxf(pg, &pr.st.params).contains("SECTION"));
        }
    }

    #[test]
    fn embedded_standard_font_is_written_when_available() {
        let (pr, s) = demo();
        let def = line_across(&pr, 0.0);
        let (c, lay) = prepare(&s, &def, 1.0, &Extras::default()).unwrap();
        let mut p = pr.st.params.clone();
        // „-“ erzwingt Courier
        p.profile_font = "-".into();
        assert!(to_pdf(&c, &def, &lay, &p, "T").windows(7).any(|w| w == b"Courier"));
        // eine vorhandene Windows-Schrift wird eingebettet (Test entfällt, wenn keine da ist)
        if let Some(dir) = std::env::var_os("WINDIR") {
            let f = std::path::Path::new(&dir).join("Fonts").join("cour.ttf");
            if f.exists() {
                p.profile_font = f.display().to_string();
                let pdf = to_pdf(&c, &def, &lay, &p, "T");
                assert!(pdf.windows(9).any(|w| w == b"FontFile2"), "Schriftdatei eingebettet");
                assert!(pdf.windows(5).any(|w| w == b"/F5 9"));
                // unabhängiger Parser liest die Datei, die Schrift ist ein Font-Objekt mit Descriptor
                let doc = lopdf::Document::load_mem(&pdf).expect("PDF mit eingebetteter Schrift lesbar");
                let fonts = doc.get_page_fonts(*doc.get_pages().values().next().unwrap()).expect("Seitenschriften");
                let f5 = fonts.get(&b"F5"[..]).expect("F5 vorhanden");
                assert_eq!(f5.get(b"Subtype").unwrap().as_name().unwrap(), b"TrueType");
                assert!(f5.get(b"FontDescriptor").is_ok());
            }
        }
    }

    /// Breite eines Textes in Courier (mm), die ungünstigste der verwendeten Schriften
    fn courier_w(t: &str, cap: f64) -> f64 {
        t.chars().count() as f64 * cap / MONO_CAP * MONO_ADVANCE
    }

    #[test]
    fn band_texts_do_not_overlap_each_other_or_the_row_heads() {
        let (pr, s) = demo();
        for view in [ProfileView::Laengs, ProfileView::Quer] {
            for step in [0.0, 5.0, 50.0] {
                let mut def = line_across(&pr, 0.0);
                def.view = view;
                def.label_step = step;
                def.dim_chain = true;
                let (c, lay) = prepare(&s, &def, 1.0, &Extras::default()).unwrap();
                let (sc, o) = build_scene(&c, &def, &lay, &pr.st.params, false);
                let mm = 1000.0 / lay.n;
                let (_, _, aw, _) = lay.sheet.area();
                // Texte des Bands: waagrecht, 2,5 mm, unterhalb der Bandoberkante
                let band = band_height(lay.rows);
                let mut boxes: Vec<(f64, f64, f64, String)> = Vec::new(); // (x von, x bis, y, Text)
                for it in &sc.items {
                    if let Item::Text { pos, h_mm, rot_deg, text, .. } = it {
                        let (x, y) = ((pos[0] - o[0]) * mm, (pos[1] - o[1]) * mm);
                        if *rot_deg == 0.0 && *h_mm == TEXT_DIM_MM && y < band {
                            let w = courier_w(text, *h_mm);
                            boxes.push((x - w / 2.0, x + w / 2.0, y, text.clone()));
                        }
                    }
                }
                assert!(!boxes.is_empty());
                for b in &boxes {
                    assert!(b.0 >= HEAD_W_MM, "{:?} {}: Text „{}“ ragt in die Zeilenköpfe (x ab {:.1} mm)", view, step, b.3, b.0);
                    assert!(b.1 <= aw, "{:?} {}: Text „{}“ ragt über den rechten Rand ({:.1} > {:.1} mm)", view, step, b.3, b.1, aw);
                }
                for (i, a) in boxes.iter().enumerate() {
                    for b in &boxes[i + 1..] {
                        if (a.2 - b.2).abs() < 1e-6 {
                            assert!(a.1 <= b.0 + 1e-9 || b.1 <= a.0 + 1e-9, "{:?} {}: „{}“ und „{}“ überlappen", view, step, a.3, b.3);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn pdf_and_dxf_are_written() {
        let (pr, s) = demo();
        let def = demo_def(&pr);
        let (c, lay) = prepare(&s, &def, 1.0, &Extras::default()).unwrap();
        let pdf = to_pdf(&c, &def, &lay, &pr.st.params, "Beispiel");
        assert!(pdf.starts_with(b"%PDF") && pdf.ends_with(b"%%EOF\n"));
        assert!(pdf.windows(7).any(|w| w == b"Courier"));
        let dxf = to_dxf(&c, &def, &lay, &pr.st.params);
        assert!(dxf.contains("SECTION") && dxf.contains("Station"));
        assert!(dxf.contains("STYLE") && dxf.contains("ISO3098") && dxf.contains("isocp.shx"));
        assert!(dxf.contains("\n7\nISO3098\n"));
    }
}
