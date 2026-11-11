//! Gemeinsame Zeichnungsbeschreibung für die PDF- und die DXF-Ausgabe.
//!
//! Lagen sind Weltkoordinaten (m), alle Größen (Linienbreite, Schrifthöhe, Kreisradius) in mm auf dem
//! Papier. Die PDF-Ausgabe rechnet mit dem Druckmaßstab 1:N um, die DXF-Ausgabe macht daraus Meter
//! (mm × N / 1000), so wie es das Original mit der „Maßstabszahl Plott“ tut.

pub type P2 = [f64; 2];
pub type Rgb = [u8; 3];

/// Farbfelder in `Params::exp_colors`.
pub mod slot {
    pub const POINT: usize = 0;
    pub const POINT_NR: usize = 1;
    pub const HEIGHT: usize = 2;
    pub const RING: usize = 3;
    pub const CONTOUR: usize = 4;
    pub const INDEX: usize = 5;
    pub const BREAK: usize = 6;
    pub const GRID: usize = 7;
    pub const TRI: usize = 8;
    pub const PLAN: usize = 9;
    pub const CUT: usize = 10;
    pub const FILL: usize = 11;
    pub const ZERO: usize = 12;
}

/// Ebenen (Index in `Params::layers`, die CAD-Ebenen 11..23 der Parameterdatei).
pub mod layer {
    pub const POINTS: u8 = 0;
    pub const GRID: u8 = 1;
    pub const NUMBERS: u8 = 2;
    pub const HEIGHTS: u8 = 3;
    pub const RING: u8 = 4;
    pub const CONTOUR: u8 = 5;
    pub const INDEX: u8 = 6;
    pub const PLAN: u8 = 7;
    pub const LABELS: u8 = 8;
    pub const BREAK: u8 = 9;
    pub const DIFF: u8 = 10;
    pub const TRI: u8 = 11;
    pub const FILL: u8 = 12;
}

#[derive(Clone, Debug)]
pub enum Item {
    Line { layer: u8, rgb: Rgb, cad: i32, w_mm: f64, pts: Vec<P2>, z: f64, closed: bool, dash: bool },
    Circle { layer: u8, rgb: Rgb, cad: i32, c: P2, r_mm: f64 },
    Text { layer: u8, rgb: Rgb, cad: i32, pos: P2, h_mm: f64, rot_deg: f64, ax: f64, ay: f64, text: String, halo: bool },
    /// Gouraud-Dreiecksnetz (Farbflächen)
    Mesh { layer: u8, pos: Vec<P2>, col: Vec<Rgb>, idx: Vec<u32> },
}

#[derive(Clone, Debug)]
pub enum Block {
    /// Textzeilen (Text, fett)
    Lines(Vec<(String, bool)>),
    /// Farbleiste: Stützstellen (Lage 0..1 von unten/links, Farbe), Teilstriche (Lage, Text)
    Bar { caption: String, stops: Vec<(f64, Rgb)>, ticks: Vec<(f64, String)> },
    /// Farbfelder mit Text
    Swatches { caption: String, items: Vec<(Rgb, String)> },
}

#[derive(Clone, Debug, Default)]
pub struct Legend {
    pub title: String,
    pub blocks: Vec<Block>,
}

#[derive(Default)]
pub struct Scene {
    pub items: Vec<Item>,
    pub legend: Legend,
}

pub fn bbox_hits(a: [f64; 4], b: [f64; 4]) -> bool {
    a[0] <= b[2] && b[0] <= a[2] && a[1] <= b[3] && b[1] <= a[3]
}

pub fn pts_bbox(p: &[P2]) -> [f64; 4] {
    let mut b = [f64::MAX, f64::MAX, f64::MIN, f64::MIN];
    for q in p {
        b[0] = b[0].min(q[0]);
        b[1] = b[1].min(q[1]);
        b[2] = b[2].max(q[0]);
        b[3] = b[3].max(q[1]);
    }
    b
}
