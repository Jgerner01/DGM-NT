//! The main window: menus as in the original (Datei, Bearbeiten, Zoom, Manipulieren, Hilfe),
//! the vector map with smooth zoom, the editing tools and autosave.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use egui::{Align2, Color32, FontId, Key, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, Ui, Vec2};
use serde::{Deserialize, Serialize};

use crate::analysis::{self, ProfilePoint, VolumeResult};
use crate::compare::{Compare, MapMode, Tab as CmpTab};
use crate::contours::{self, Contours};
use crate::dxf_z;
use crate::export::{self, Sheet};
use crate::files;
use crate::job::{self, Progress};
use crate::profile::{self, ProfileDef, ProfileView, SheetFormat};
use crate::scene;
use crate::model::{ManualLabel, Params, Point, Polyline, Project, ProjectState, Tri, AUTO_CALC_LIMIT_POINTS, WARN_POINTS};
use crate::render::{self, ContourStyle, FillCache, FillMode, Palette};
use crate::tin::{self, MeshReport, Surface};
use crate::view::{nice_step, View};
use crate::view3d::View3d;
use crate::water::{self, Water};

// ------------------------------------------------------------------ settings

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct Settings {
    pub data_dir: String,
    pub last_project: String,
    /// seconds between automatic saves (0 = off)
    pub autosave_secs: u64,
    pub auto_compute: bool,
    pub layers: Layers,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { data_dir: String::new(), last_project: String::new(), autosave_secs: 60, auto_compute: true, layers: Layers::default() }
    }
}

fn settings_path() -> Option<PathBuf> {
    let base = std::env::var_os("APPDATA").map(PathBuf::from).or_else(|| std::env::var_os("HOME").map(PathBuf::from))?;
    Some(base.join("DGM-NT").join("settings.json"))
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct Layers {
    pub fill: u8,
    pub shade: bool,
    pub points: bool,
    pub numbers: bool,
    pub heights: bool,
    pub triangles: bool,
    pub tri_numbers: bool,
    pub breaklines: bool,
    pub rings: bool,
    pub water: bool,
    pub contours: bool,
    pub labels: bool,
    pub grid: bool,
    pub plan: bool,
    pub minimap: bool,
}

impl Default for Layers {
    fn default() -> Self {
        Layers {
            fill: 1,
            shade: true,
            points: true,
            numbers: true,
            heights: true,
            triangles: false,
            tri_numbers: false,
            breaklines: true,
            rings: true,
            water: true,
            contours: true,
            labels: true,
            grid: true,
            plan: true,
            minimap: true,
        }
    }
}

impl Layers {
    fn fill_mode(&self) -> FillMode {
        match self.fill {
            1 => FillMode::Height,
            2 => FillMode::Slope,
            _ => FillMode::None,
        }
    }
}

// ------------------------------------------------------------------ tools

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Tool {
    View,
    ZoomRect,
    PtToggle,
    PtAddLine,
    PtAddFree,
    TriToggle,
    TriFlip,
    TriCreate,
    BkCreate { soft: bool },
    BkDelete,
    BkSoft,
    BkShow,
    RingCreate,
    RingDelete,
    WaterCreate,
    PlanCreate,
    PlanDelete,
    HeightPick,
    Profile,
    LabelManual,
    PNrPos,
    /// Koordinatenfenster am Bildschirm digitalisieren (zwei Eckpunkte)
    WindowPick,
}

impl Tool {
    fn hint(&self) -> &'static str {
        match self {
            Tool::View => "Ansehen: Mausrad = zoomen am Mauszeiger, ziehen = verschieben, Doppelklick = vergrößern, Strg+Klick = zentrieren, Pfeiltasten = halbes Bild weiter.",
            Tool::ZoomRect => "Ausschnitt: Rechteck aufziehen.",
            Tool::PtToggle => "Punkte löschen/aktivieren: Punkt anklicken.",
            Tool::PtAddLine => "Punkte erzeugen auf Linie: 1. und 2. Punkt anklicken, dann neue Punkte setzen. Rechte Maustaste = Ende.",
            Tool::PtAddFree => "Punkte erzeugen beliebig: 1. und 2. Punkt anklicken, dann neue Punkte setzen (Höhe linear). Rechte Maustaste = Ende.",
            Tool::TriToggle => "Dreiecke löschen/aktivieren: Dreiecksmitte anklicken.",
            Tool::TriFlip => "Dreiecke tauschen: gemeinsame Kante zweier Dreiecke anklicken.",
            Tool::TriCreate => "Dreieck erzeugen: drei Punkte anklicken.",
            Tool::BkCreate { soft: false } => "Bruchkante (hart) erzeugen: Punkte der Reihe nach anklicken, rechte Maustaste = fertig.",
            Tool::BkCreate { soft: true } => "Bruchkante (weich) erzeugen: Punkte der Reihe nach anklicken, rechte Maustaste = fertig.",
            Tool::BkDelete => "Bruchkante löschen: Linie anklicken.",
            Tool::BkSoft => "Bruchkante weich/hart umschalten: Linie anklicken.",
            Tool::BkShow => "Bruchkante anzeigen: Linie anklicken, das ganze Polygon blinkt.",
            Tool::RingCreate => "Ring erzeugen: Punkte anklicken, rechte Maustaste schließt. Im Uhrzeigersinn = Umring, gegen den Uhrzeigersinn = Inring.",
            Tool::RingDelete => "Umring/Inring löschen: Ringlinie anklicken.",
            Tool::WaterCreate => "Wasserfläche erzeugen: Punkte der Wasserlinie anklicken, rechte Maustaste schließt die Fläche (mindestens 3 Punkte, bis 10 Wasserflächen).",
            Tool::PlanCreate => "Grundrisslinie: Punkte anklicken, rechte Maustaste = fertig.",
            Tool::PlanDelete => "Grundrisslinie löschen: Linie anklicken.",
            Tool::HeightPick => "Höhen für Punkte: Lage anklicken, die Höhe wird aus dem Modell gerechnet (mit Stationierung).",
            Tool::Profile => "Profil: Achspunkte anklicken, rechte Maustaste = Profil rechnen.",
            Tool::LabelManual => "Höhenlinienbeschriftung manuell: auf eine Höhenlinie klicken.",
            Tool::PNrPos => "Punktnummer-Position: Punkt anklicken, die Nummer wandert weiter.",
            Tool::WindowPick => "Koordinatenfenster digitalisieren: 1. Ecke und 2. Ecke (diagonal gegenüber) anklicken. Rechte Maustaste oder Esc = abbrechen.",
        }
    }
}

/// Druckausgabe (PDF): Blattformat und Maßstab
/// Linienbreite und Endpunktradius des gewählten Profils in der Karte (Bildschirmpunkte)
const PROFILE_LINE_W: f32 = 3.0;
const PROFILE_END_R: f32 = 5.0;
/// Farbe der Vorschau der Querprofile in der Karte
const QS_PREVIEW_RGB: Color32 = Color32::from_rgb(0, 70, 230);
/// Strichmuster (Bildschirmpunkte, abwechselnd Strich/Lücke): gestrichelt und strichpunktiert
const QS_DASH: [f32; 2] = [8.0, 5.0];
const QS_DASH_DOT: [f32; 4] = [14.0, 4.0, 2.0, 4.0];

/// Linienzug mit Strichmuster (abwechselnd Strich- und Lückenlänge) als Einzelstücke.
fn dash_pattern(pts: &[Pos2], stroke: Stroke, pattern: &[f32]) -> Vec<Shape> {
    let mut out = Vec::new();
    if pts.len() < 2 || pattern.is_empty() {
        return out;
    }
    let (mut idx, mut left) = (0usize, pattern[0]);
    let mut cur: Vec<Pos2> = vec![pts[0]];
    for w in pts.windows(2) {
        let (a, b) = (w[0], w[1]);
        let len = (b - a).length();
        if len <= 0.0 {
            continue;
        }
        let mut pos = 0.0;
        while len - pos > left {
            pos += left;
            let q = a + (b - a) * (pos / len);
            if idx % 2 == 0 {
                cur.push(q);
                out.push(Shape::line(std::mem::take(&mut cur), stroke));
            } else {
                cur = vec![q];
            }
            idx += 1;
            left = pattern[idx % pattern.len()];
        }
        left -= len - pos;
        if idx % 2 == 0 {
            cur.push(b);
        }
    }
    if idx % 2 == 0 && cur.len() >= 2 {
        out.push(Shape::line(cur, stroke));
    }
    out
}

/// Kleinster Abstand und kleinste Breite der Querprofile (m)
const QS_STEP_MIN_M: f64 = 0.5;

/// Maske „Querprofile erzeugen“ zu einem Längsprofil.
struct QsDialog {
    /// Index des Längsprofils
    axis: usize,
    /// Profile an den Achspunkten statt im festen Abstand
    at_vertices: bool,
    step: f64,
    half: f64,
    n: f64,
    exag: f64,
    /// Maßstab bzw. Überhöhung vom Benutzer gesetzt (kein automatischer Vorschlag mehr)
    n_user: bool,
    e_user: bool,
    /// Vorschau und Vorschlag müssen neu berechnet werden
    pending: bool,
    /// entstehende Querprofile (am Modellrand beschnitten), für die Vorschau in der Karte
    made: Vec<ProfileDef>,
    /// Achse des gewählten Längsprofils (Vorschau)
    axis_pts: Vec<[f64; 2]>,
    /// mögliche Abstände zur Auswahl
    steps: Vec<f64>,
}

struct PrintState {
    open: bool,
    landscape: bool,
    /// zusätzlicher Streifen für die Legende (mm): unten bei Hochformat, rechts bei Querformat
    strip: f64,
    scale_edit: f64,
}

/// Deckkraft der Wasserflächen auf der Karte (0..255)
const WATER_ALPHA: u8 = 215;

/// Aktionen im Fenster Wasserflächen, die erst nach dem Zeichnen ausgeführt werden
enum WaterAct {
    Select(usize),
    Delete(usize),
    PullAll(Option<usize>),
    PullOne(usize, f64),
    RemovePoint(usize, usize),
    SaveProtocol,
}

/// PDF-Ausgabe des Gittermodells (3D): Blattformat und Maßstab
struct Print3dState {
    open: bool,
    landscape: bool,
    /// zusätzlicher Streifen für die Legende (mm)
    strip: f64,
    scale_edit: f64,
}

/// Kleinster gebräuchlicher Maßstab (1:N) mit N >= `n`.
fn standard_scale_at_least(n: f64) -> f64 {
    let base = [1.0, 1.25, 2.0, 2.5, 5.0, 10.0];
    let e = n.max(1.0).log10().floor();
    base.iter().map(|b| (b * 10f64.powf(e)).round()).find(|&c| c >= n).unwrap_or(10f64.powf(e + 1.0))
}

/// DXF-Export der Bildschirmdarstellung
struct DxfState {
    open: bool,
    /// Maßstabszahl für Schrift und Kreise (0 = Maßstabszahl Plott der Parameterdatei)
    scale: f64,
    fills: bool,
}

/// Nächster gebräuchlicher Maßstab (1:N) zu `n`.
fn nearest_standard_scale(n: f64) -> f64 {
    let base = [1.0, 1.25, 2.0, 2.5, 5.0, 10.0];
    let e = n.max(1.0).log10().floor();
    let mut best = (f64::MAX, n.round().max(1.0));
    for ee in [e - 1.0, e, e + 1.0] {
        for b in base {
            let c = (b * 10f64.powf(ee)).round().max(1.0);
            let d = (c / n.max(1.0)).ln().abs();
            if d < best.0 {
                best = (d, c);
            }
        }
    }
    best.1
}

/// Schaltflächen des Abschnitts „Koordinatenfenster“ im Parameterdialog
enum WinAction {
    Pick,
    View,
    All,
    Save,
}

/// Analyse: DXF-Datei mit Höhen aus dem Modell versehen
#[derive(Default)]
struct DxfZState {
    open: bool,
    /// gewählte Quelldatei und ihr Inhalt
    source: Option<(PathBuf, Vec<u8>)>,
    /// auch Z = 0 gilt als fehlend (viele CAD-Programme schreiben für 2D-Zeichnungen Z = 0)
    replace_zero: bool,
    /// Ergebnis: neue Datei und Auswertung
    result: Option<(Vec<u8>, dxf_z::ZReport)>,
    message: String,
}

enum Confirm {
    WriteOriginal,
    Remesh,
    ReloadOriginal,
}

// ------------------------------------------------------------------ Hintergrundarbeiten

/// Bis zu dieser Punktzahl wird die Vermaschung nach einer Bearbeitung sofort gerechnet; darüber läuft
/// sie im Hintergrund mit Fortschrittsbalken.
const SYNC_MESH_POINTS: usize = 3000;

/// Höchstzahl gleichzeitig gezeichneter Punkte; bei mehr sichtbaren Punkten wird ausgedünnt.
const MAX_DRAWN_POINTS: usize = 40_000;

#[derive(Clone, Copy, PartialEq, Eq)]
enum LoadMode {
    /// Projekt öffnen (Autospeicherstand oder Originaldateien)
    Open,
    /// Originaldateien neu einlesen (Rückgängig bleibt möglich)
    Reload,
}

/// Geländemodell aus den Dreiecken: Fläche, Höhenlinien, Höhenfarben
struct Model {
    surface: Surface,
    contours: Contours,
    fill: Option<FillCache>,
}

struct Loaded {
    project: Project,
    from_auto: bool,
    mode: LoadMode,
    meshed: Option<MeshReport>,
    model: Option<Model>,
}

struct Meshed {
    triangles: Vec<Tri>,
    report: MeshReport,
    model: Model,
}

enum JobDone {
    Loaded(Box<Loaded>),
    Meshed(Box<Meshed>),
    Converted { result: Result<usize, String>, inputs: usize, output: PathBuf },
}

struct Job {
    title: String,
    progress: Arc<Progress>,
    rx: mpsc::Receiver<Result<JobDone, String>>,
}

fn build_model(pr: &Project, mode: FillMode, shade: bool) -> Model {
    let s = Surface::new(pr, pr.st.params.subdivision > 0);
    let mut c = contours::compute(&s, &pr.st.params);
    let ext = pr.bbox().map(|b| (b[2] - b[0]).max(b[3] - b[1])).unwrap_or(100.0);
    contours::place_labels(&mut c, ext, (ext * 0.012).max(0.5));
    let fill = FillCache::build(&s, mode, shade, (pr.st.params.subdivision as usize).clamp(1, 4), (s.zmin, s.zmax));
    Model { surface: s, contours: c, fill }
}

/// Vermaschen; von Hand gelöschte Dreiecke bleiben gelöscht.
fn mesh_keeping_deleted(pr: &Project, progress: job::ProgressFn) -> (Vec<Tri>, MeshReport) {
    let deleted: HashSet<[usize; 3]> = pr.st.triangles.iter().filter(|t| !t.active).map(|t| sorted(t.v)).collect();
    let (mut tris, rep) = tin::triangulate_progress(pr, progress);
    if !deleted.is_empty() {
        for t in tris.iter_mut() {
            if deleted.contains(&sorted(t.v)) {
                t.active = false;
            }
        }
    }
    (tris, rep)
}

fn mesh_message(rep: &MeshReport) -> String {
    format!(
        "Vermaschung: {} Punkte, {} Dreiecke{}{}.",
        rep.points_used,
        rep.triangles,
        if rep.duplicates > 0 { format!(", {} identische Punkte übergangen", rep.duplicates) } else { String::new() },
        if rep.constraints_skipped > 0 { format!(", {} Kantenstücke nicht einarbeitbar (Kreuzung)", rep.constraints_skipped) } else { String::new() }
    )
}

/// Projekt laden: der neuere Autospeicherstand, sonst die Originaldateien. Zweiter Wert: aus Autospeicherstand.
fn load_project(dir: &Path, name: &str, mode: LoadMode, prog: &Progress) -> Result<(Project, bool), String> {
    if mode == LoadMode::Open {
        let auto = dir.join(format!("{}.dgmnt.json", name));
        let gm1_time = files::find_ext(dir, name, "gm1").and_then(|p| p.metadata().ok()).and_then(|m| m.modified().ok());
        let auto_time = auto.metadata().ok().and_then(|m| m.modified().ok());
        if auto_time.is_some() && (gm1_time.is_none() || auto_time >= gm1_time) {
            prog.set_indeterminate("Autospeicherstand einlesen");
            let state = std::fs::read_to_string(&auto).ok().and_then(|s| serde_json::from_str::<ProjectState>(&s).ok());
            if let Some(st) = state {
                let mut p = Project { dir: dir.to_path_buf(), st, index: Default::default() };
                p.rebuild_index();
                return Ok((p, true));
            }
        }
    }
    Project::load_original_progress(dir.to_path_buf(), name, &mut |f, st| prog.set(f, st)).map(|p| (p, false))
}

// ------------------------------------------------------------------ app

pub struct DgmApp {
    settings: Settings,
    project: Option<Project>,
    surface: Option<Surface>,
    surf_id: usize,
    contours: Option<Contours>,
    fill: Option<FillCache>,
    palette: Palette,
    view: View,
    need_fit: bool,
    tool: Tool,
    pick: Vec<usize>,
    pick_world: Vec<[f64; 2]>,
    drag_rect: Option<Pos2>,
    hover: Option<(f64, f64)>,
    status: String,
    dirty: bool,
    last_save: Instant,
    last_save_text: String,
    undo: Vec<ProjectState>,
    redo: Vec<ProjectState>,
    // dialogs
    show_params: bool,
    params_edit: Params,
    show_open: bool,
    project_list: Vec<String>,
    dir_edit: String,
    show_codes: bool,
    show_dups: bool,
    dups: Vec<(usize, usize)>,
    /// Analyse: zweites Geländemodell, Überschneidung, Auftrag/Abtrag, Höhenunterschied
    cmp: Compare,
    print: PrintState,
    print3d: Print3dState,
    dxf: DxfState,
    show_volume: bool,
    volume_ref: f64,
    volume_res: Option<VolumeResult>,
    show_heights: bool,
    heights: Vec<(String, f64, f64, Option<f64>, f64)>,
    show_profile: bool,
    /// Fenster „Profile“ (bemaßte Längs- und Querprofile)
    show_profiles: bool,
    profile_sel: usize,
    /// Werkzeug „Profil“ legt ein Querprofil an (sonst Längsprofil)
    profile_quer: bool,
    /// Maske „Querprofile erzeugen“ (offen, solange sie Werte enthält)
    qs_dialog: Option<QsDialog>,
    /// Vergrößerung der Profilvorschau (Bildschirmpunkte je mm Papier)
    profile_zoom: f32,
    /// Oberfläche des zweiten Modells (Modellvergleich) für das zweite Gelände im Profil, mit Schlüssel
    profile_second: Option<((String, usize, usize), Surface)>,
    /// Querprofile als je eine Datei ausgeben (sonst mehrere je Blatt)
    profile_series_single: bool,
    profile: Vec<ProfilePoint>,
    profile_step: f64,
    profile_exag: f32,
    show_3d: bool,
    v3d: View3d,
    show_bklist: bool,
    /// Fenster Wasserflächen: Tabelle und Protokoll
    show_water: bool,
    water_sel: usize,
    water_tab: u8,
    /// Bearbeitung einer Höhe läuft: der Rückgängig-Schritt ist schon angelegt
    water_editing: bool,
    /// Dreieckszerlegung der Wasserflächen für die Füllung (Schlüssel: Punktfolge und Koordinatensumme)
    water_cache: std::cell::RefCell<Vec<(Vec<usize>, f64, Vec<[usize; 3]>)>>,
    show_settings: bool,
    show_help: bool,
    show_about: bool,
    confirm: Option<Confirm>,
    blink: Option<(usize, Instant)>,
    // screenshot mode
    screenshot: Option<PathBuf>,
    frame: u64,
    shot: Arc<Mutex<Option<Arc<egui::ColorImage>>>>,
    shot_requested: bool,
    png_target: Option<PathBuf>,
    last_frame: Instant,
    map_rect: Rect,
    start_zoom: Option<f64>,
    auto_export: Option<(Option<PathBuf>, Option<PathBuf>, Option<PathBuf>)>,
    /// laufende Hintergrundarbeit (Einlesen, Vermaschen, Umwandeln) mit Fortschrittsbalken
    job: Option<Job>,
    /// Automatische Vermaschung/Höhenlinien sind abgeschaltet (zu viele Punkte), nur manuell möglich
    auto_off: bool,
    /// Hinweis- oder Warnfenster nach dem Einlesen
    notice: Option<String>,
    /// erste digitalisierte Ecke des Koordinatenfensters
    win_first: Option<[f64; 2]>,
    dxfz: DxfZState,
}

impl DgmApp {
    pub fn new(cc: &eframe::CreationContext<'_>, screenshot: Option<PathBuf>, open: Option<PathBuf>, zoom: Option<f64>) -> Self {
        cc.egui_ctx.set_visuals(egui::Visuals::light());
        let settings: Settings = settings_path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        let mut app = DgmApp {
            settings,
            project: None,
            surface: None,
            surf_id: 0,
            contours: None,
            fill: None,
            palette: Palette::default(),
            view: View::default(),
            need_fit: true,
            tool: Tool::View,
            pick: Vec::new(),
            pick_world: Vec::new(),
            drag_rect: None,
            hover: None,
            status: String::new(),
            dirty: false,
            last_save: Instant::now(),
            last_save_text: String::new(),
            undo: Vec::new(),
            redo: Vec::new(),
            show_params: false,
            params_edit: Params::default(),
            show_open: false,
            project_list: Vec::new(),
            dir_edit: String::new(),
            show_codes: false,
            show_dups: false,
            dups: Vec::new(),
            cmp: Compare::default(),
            print: PrintState { open: false, landscape: true, strip: 60.0, scale_edit: 0.0 },
            print3d: Print3dState { open: false, landscape: true, strip: 60.0, scale_edit: 0.0 },
            dxf: DxfState { open: false, scale: 0.0, fills: false },
            show_volume: false,
            volume_ref: 0.0,
            volume_res: None,
            show_heights: false,
            heights: Vec::new(),
            show_profile: false,
            show_profiles: false,
            profile_sel: 0,
            profile_quer: false,
            qs_dialog: None,
            profile_zoom: 2.5,
            profile_second: None,
            profile_series_single: false,
            profile: Vec::new(),
            profile_step: 1.0,
            profile_exag: 5.0,
            show_3d: false,
            v3d: View3d::default(),
            show_bklist: false,
            show_water: false,
            water_sel: 0,
            water_tab: 0,
            water_editing: false,
            water_cache: Default::default(),
            show_settings: false,
            show_help: false,
            show_about: false,
            confirm: None,
            blink: None,
            screenshot,
            frame: 0,
            shot: Arc::new(Mutex::new(None)),
            shot_requested: false,
            png_target: None,
            last_frame: Instant::now(),
            map_rect: Rect::from_min_size(Pos2::ZERO, Vec2::new(1000.0, 700.0)),
            start_zoom: zoom,
            auto_export: None,
            job: None,
            auto_off: false,
            notice: None,
            win_first: None,
            dxfz: DxfZState { replace_zero: true, ..Default::default() },
        };
        // start: a file given on the command line, the last project, else the data folder, else a demo
        if let Some(p) = open {
            app.open_path(&p);
        }
        if app.project.is_none() && app.job.is_none() && !app.settings.last_project.is_empty() && !app.settings.data_dir.is_empty() {
            let dir = PathBuf::from(&app.settings.data_dir);
            let name = app.settings.last_project.clone();
            app.open_project(dir, &name);
        }
        if app.project.is_none() && app.job.is_none() {
            for d in [r"C:\dgm\daten", r"C:\Programme\dgm\daten"] {
                let dir = PathBuf::from(d);
                let list = files::list_projects(&dir);
                if list.is_empty() {
                    continue;
                }
                let name = list.iter().find(|n| n.eq_ignore_ascii_case("Diplomarbeit")).cloned().unwrap_or(list[0].clone());
                app.settings.data_dir = d.to_string();
                app.open_project(dir, &name);
                break;
            }
        }
        // Screenshot-Modus: das Projekt muss vor dem ersten Bild vollständig geladen sein
        if app.screenshot.is_some() {
            app.wait_job();
        }
        if app.project.is_none() && app.job.is_none() {
            app.load_demo();
        }
        app
    }

    /// Beispielgelände zeigen, wenn kein Projekt geladen werden konnte.
    fn load_demo(&mut self) {
        let mut pr = Project::demo();
        let (t, _) = tin::triangulate(&pr);
        pr.st.triangles = t;
        self.project = Some(pr);
        self.auto_off = false;
        self.recompute();
        self.status = "Kein Datenordner gefunden – ein Beispielgelände wird gezeigt. Datei → Projekt laden…".into();
    }

    pub fn set_show_3d(&mut self, on: bool) {
        self.show_3d = on;
    }

    // -------------------------------------------------------------- project handling

    fn open_path(&mut self, p: &Path) {
        if p.is_dir() {
            self.settings.data_dir = p.display().to_string();
            self.dir_edit = self.settings.data_dir.clone();
            self.project_list = files::list_projects(p);
            self.show_open = true;
            return;
        }
        let dir = p.parent().map(|d| d.to_path_buf()).unwrap_or_default();
        let stem = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        let stem = stem.trim_end_matches(".dgmnt").to_string();
        self.settings.data_dir = dir.display().to_string();
        self.open_project(dir, &stem);
    }

    fn open_project(&mut self, dir: PathBuf, name: &str) {
        self.start_load(dir, name, LoadMode::Open);
    }

    /// Hintergrundarbeit starten; während sie läuft sperrt ein Fortschrittsfenster die Bedienung.
    fn spawn_job<F>(&mut self, title: &str, work: F)
    where
        F: FnOnce(&Progress) -> Result<JobDone, String> + Send + 'static,
    {
        if self.job.is_some() {
            self.status = "Es läuft bereits eine Arbeit – bitte warten.".into();
            return;
        }
        let progress = Progress::new();
        let shared = progress.clone();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(work(&shared));
        });
        self.job = Some(Job { title: title.to_string(), progress, rx });
    }

    /// Projekt einlesen (mit Fortschrittsbalken); danach wird bis zur Punktgrenze vermascht und gerechnet.
    fn start_load(&mut self, dir: PathBuf, name: &str, mode: LoadMode) {
        if mode == LoadMode::Open && self.dirty {
            self.autosave();
        }
        let name = name.to_string();
        let auto_compute = self.settings.auto_compute;
        let (fill_mode, shade) = (self.settings.layers.fill_mode(), self.settings.layers.shade);
        let title = format!("Projekt {} einlesen", name);
        self.spawn_job(&title, move |prog| {
            let (mut pr, from_auto) = load_project(&dir, &name, mode, prog)?;
            let n = pr.st.points.len();
            let compute = n <= AUTO_CALC_LIMIT_POINTS && n <= pr.st.params.max_calc_points as usize;
            let mut meshed = None;
            if compute && pr.st.triangles.is_empty() && (auto_compute || mode == LoadMode::Reload) {
                let (t, rep) = tin::triangulate_progress(&pr, &mut |f, st| prog.set(f, st));
                pr.st.triangles = t;
                meshed = Some(rep);
            }
            let model = if compute {
                prog.set_indeterminate("Geländemodell und Höhenlinien berechnen");
                Some(build_model(&pr, fill_mode, shade))
            } else {
                None
            };
            Ok(JobDone::Loaded(Box::new(Loaded { project: pr, from_auto, mode, meshed, model })))
        });
    }

    fn finish_load(&mut self, l: Loaded) {
        let Loaded { project, from_auto, mode, meshed, model } = l;
        let n = project.st.points.len();
        let max = project.st.params.max_calc_points as usize;
        let (name, dir) = (project.st.name.clone(), project.dir.clone());
        if mode == LoadMode::Reload {
            self.checkpoint();
        }
        self.project = Some(project);
        self.auto_off = n > AUTO_CALC_LIMIT_POINTS;
        self.pick.clear();
        match model {
            Some(m) => self.apply_model(m),
            None => self.clear_model(),
        }
        let mut status = match (mode, from_auto) {
            (LoadMode::Open, true) => format!("Projekt {} geladen ({} Punkte) – letzter automatisch gespeicherter Stand.", name, n),
            (LoadMode::Open, false) => format!("Projekt {} geladen ({} Punkte).", name, n),
            (LoadMode::Reload, _) => format!("Originaldateien neu eingelesen ({} Punkte).", n),
        };
        if let Some(rep) = &meshed {
            status += &format!(" {}", mesh_message(rep));
        }
        if mode == LoadMode::Open {
            self.undo.clear();
            self.redo.clear();
            self.dirty = false;
            self.tool = Tool::View;
            self.need_fit = true;
            self.settings.last_project = name;
            self.settings.data_dir = dir.display().to_string();
            self.save_settings();
            self.last_save = Instant::now();
        }
        // Grenzen für Automatik, Warnung und Rechengrenze
        let mut lines: Vec<String> = Vec::new();
        if n > max {
            status += &format!(" Berechnungen abgeschaltet (mehr als {} Punkte).", max);
            lines.push(format!("{} Punkte eingelesen – das sind mehr als die Rechengrenze von {} Punkten (Parameter).", n, max));
            lines.push("Vermaschung, Höhenlinien und alle weiteren Berechnungen sind abgeschaltet.".into());
            lines.push("Mit dem Koordinatenfenster (Parameter) einen Teil ausschneiden und als GM1-Datei speichern, oder die Rechengrenze erhöhen.".into());
        } else if n > WARN_POINTS {
            status += " Automatische Vermaschung und Höhenlinien sind abgeschaltet.";
            lines.push(format!("{} Punkte eingelesen – mehr als {} Punkte.", n, WARN_POINTS));
            lines.push("Vermaschung und Höhenlinien können sehr lange dauern und viel Speicher brauchen.".into());
            lines.push("Sie werden nicht automatisch gerechnet: Bearbeiten → Vermaschen startet sie von Hand. Besser vorher mit dem Koordinatenfenster (Parameter) einen Teil ausschneiden.".into());
        } else if self.auto_off {
            status += &format!(" Mehr als {} Punkte: Vermaschung und Höhenlinien nicht automatisch – Bearbeiten → Vermaschen.", AUTO_CALC_LIMIT_POINTS);
        }
        if !lines.is_empty() {
            self.notice = Some(lines.join("\n"));
        }
        self.status = status;
    }

    /// Modell so einpassen, dass es die Zeichenfläche des Blatts füllt.
    fn fit_print_area(&mut self) {
        let rect = self.map_rect;
        if let Some(b) = self.project.as_ref().and_then(|p| p.bbox()) {
            let (area, pxmm) = self.print_area_on_screen(rect);
            let (bw, bh) = ((b[2] - b[0]).max(1e-6), (b[3] - b[1]).max(1e-6));
            let ppm = ((area.width() as f64 * 0.96) / bw).min((area.height() as f64 * 0.96) / bh);
            let (bx, by) = ((b[0] + b[2]) / 2.0, (b[1] + b[3]) / 2.0);
            // Ansichtsmitte so wählen, dass die Modellmitte in der Mitte der Zeichenfläche liegt
            let cx = bx - (area.center().x - rect.center().x) as f64 / ppm;
            let cy = by + (area.center().y - rect.center().y) as f64 / ppm;
            self.view.animate_to(cx, cy, ppm);
            self.print.scale_edit = nearest_standard_scale(1000.0 * pxmm as f64 / ppm);
        }
    }

    /// Startoption `--print`: Druckvorschau öffnen (Screenshot-Test).
    pub fn start_print(&mut self, portrait: bool) {
        self.wait_job();
        self.print.landscape = !portrait;
        self.print.strip = if portrait { 45.0 } else { 60.0 };
        self.print.open = true;
    }

    /// Startoptionen für Tests: Dateien ohne Dialog schreiben (`--export-pdf`, `--export-dxf`, `--export-gm1`).
    pub fn start_exports(&mut self, pdf: Option<PathBuf>, dxf: Option<PathBuf>, gm1: Option<PathBuf>, portrait: bool) {
        self.wait_job();
        self.print.landscape = !portrait;
        self.print.strip = if portrait { 45.0 } else { 60.0 };
        self.auto_export = Some((pdf, dxf, gm1));
    }

    /// Startoption `--mesh`: nach dem Laden von Hand vermaschen und Höhenlinien rechnen (Test).
    pub fn start_mesh(&mut self) {
        self.wait_job();
        self.remesh_manual();
        self.wait_job();
    }

    /// Startoption `--second`: zweites Modell ohne Dialog laden und die Analyse zeigen.
    pub fn start_analysis(&mut self, second: &Path, tab: Option<&str>) {
        self.wait_job();
        let Some(pr) = &self.project else { return };
        let params = pr.st.params.clone();
        match self.cmp.load(second, &params) {
            Ok(msg) => {
                self.status = msg;
                self.cmp.show_tab(match tab {
                    Some("volume") => CmpTab::Volume,
                    Some("diff") | Some("iso") => CmpTab::Diff,
                    _ => CmpTab::Overview,
                });
                if tab == Some("diff") {
                    self.cmp.show_iso = false;
                }
            }
            Err(e) => self.status = format!("Zweites Modell: {}", e),
        }
    }

    /// Analyse: zweites Geländemodell (GM1) wählen und einlesen.
    fn load_second(&mut self) {
        let Some(pr) = &self.project else {
            self.status = "Zuerst ein Projekt laden.".into();
            return;
        };
        if self.calc_blocked() {
            self.status = self.blocked_text();
            return;
        }
        let params = pr.st.params.clone();
        let mut dlg = rfd::FileDialog::new().add_filter("Punktdatei", &["gm1", "GM1"]).set_title("Zweites Geländemodell (gm1) einlesen");
        if !self.settings.data_dir.is_empty() && Path::new(&self.settings.data_dir).is_dir() {
            dlg = dlg.set_directory(&self.settings.data_dir);
        }
        let Some(path) = dlg.pick_file() else { return };
        match self.cmp.load(&path, &params) {
            Ok(msg) => {
                // die Überschneidung wird im nächsten Bild gerechnet und im Fenster gemeldet
                self.cmp.show_tab(CmpTab::Overview);
                self.status = msg;
            }
            Err(e) => self.status = format!("Zweites Modell: {}", e),
        }
    }

    // -------------------------------------------------------------- Ausgabe: GM1, PDF, DXF

    /// Differenzmodell (Neu − Bestand) als Punktdatei GM1 speichern.
    fn save_diff_gm1(&mut self) {
        if !self.cmp.has_result() {
            self.status = "Kein Differenzmodell vorhanden (Überschneidung prüfen).".into();
            return;
        }
        let a = self.project.as_ref().map(|p| p.st.name.clone()).unwrap_or_default();
        let b = self.cmp.second.as_ref().map(|s| s.label.clone()).unwrap_or_default();
        let (best, neu) = if self.cmp.swap { (b, a) } else { (a, b) };
        let mut dlg = rfd::FileDialog::new().add_filter("Punktdatei", &["gm1"]).set_file_name(format!("{}-{}-Differenz.gm1", neu, best));
        if !self.settings.data_dir.is_empty() && Path::new(&self.settings.data_dir).is_dir() {
            dlg = dlg.set_directory(&self.settings.data_dir);
        }
        let Some(path) = dlg.save_file() else { return };
        let pts = self.cmp.gm1_points();
        self.status = match files::write_gm1(&path, &pts) {
            Ok(_) => format!("Differenzmodell {} − {}: {} Punkte (Höhe = Höhenunterschied) nach {} geschrieben.", neu, best, pts.len(), path.display()),
            Err(e) => format!("GM1-Fehler: {}", e),
        };
    }

    /// Blatt auf dem Bildschirm: Seitenrechteck und Bildschirmpunkte je mm.
    fn page_on_screen(&self, rect: Rect) -> (Rect, f32) {
        let sheet = Sheet::new(self.print.landscape);
        let (w, h) = sheet.size();
        let avail = rect.shrink(22.0);
        let s = (avail.width() / w as f32).min(avail.height() / h as f32).max(0.05);
        (Rect::from_center_size(rect.center(), Vec2::new(w as f32 * s, h as f32 * s)), s)
    }

    fn sheet(&self) -> Sheet {
        let mut s = Sheet::new(self.print.landscape);
        s.strip = self.print.strip;
        s
    }

    /// Zeichenfläche des Blatts auf dem Bildschirm.
    fn print_area_on_screen(&self, rect: Rect) -> (Rect, f32) {
        let (page, s) = self.page_on_screen(rect);
        let sheet = self.sheet();
        let (_, h) = sheet.size();
        let (ax, ay, aw, ah) = sheet.area();
        let min = Pos2::new(page.left() + ax as f32 * s, page.top() + (h - ay - ah) as f32 * s);
        (Rect::from_min_size(min, Vec2::new(aw as f32 * s, ah as f32 * s)), s)
    }

    /// Maßstabszahl N (1:N), die sich aus dem Zoom und der Blattgröße auf dem Bildschirm ergibt.
    fn print_scale_n(&self, rect: Rect) -> f64 {
        let (_, s) = self.page_on_screen(rect);
        1000.0 * s as f64 / self.view.ppm
    }

    /// Rahmen, der das A4-Blatt auf dem Bildschirm simuliert; nur die Zeichenfläche wird gedruckt.
    fn draw_print_frame(&self, painter: &egui::Painter, rect: Rect) {
        let (page, _) = self.page_on_screen(rect);
        let (area, _) = self.print_area_on_screen(rect);
        let dim = Color32::from_rgba_unmultiplied(205, 207, 212, 200);
        let paper = Color32::from_rgba_unmultiplied(255, 255, 255, 228);
        // außerhalb des Blatts abdunkeln
        for r in [
            Rect::from_min_max(rect.left_top(), Pos2::new(rect.right(), page.top())),
            Rect::from_min_max(Pos2::new(rect.left(), page.bottom()), rect.right_bottom()),
            Rect::from_min_max(Pos2::new(rect.left(), page.top()), Pos2::new(page.left(), page.bottom())),
            Rect::from_min_max(Pos2::new(page.right(), page.top()), Pos2::new(rect.right(), page.bottom())),
        ] {
            painter.rect_filled(r, 0.0, dim);
        }
        // Ränder und Legendenstreifen (werden nicht mit Karteninhalt gedruckt)
        for r in [
            Rect::from_min_max(page.left_top(), Pos2::new(page.right(), area.top())),
            Rect::from_min_max(Pos2::new(page.left(), area.bottom()), page.right_bottom()),
            Rect::from_min_max(Pos2::new(page.left(), area.top()), Pos2::new(area.left(), area.bottom())),
            Rect::from_min_max(Pos2::new(area.right(), area.top()), Pos2::new(page.right(), area.bottom())),
        ] {
            painter.rect_filled(r, 0.0, paper);
        }
        painter.rect_stroke(page, 0.0, Stroke::new(1.5, Color32::from_gray(60)), StrokeKind::Outside);
        painter.rect_stroke(area, 0.0, Stroke::new(1.5, Color32::BLACK), StrokeKind::Middle);
        // Streifen für Legende, Maßstab und Nordpfeil
        let strip = if self.print.landscape {
            Rect::from_min_max(Pos2::new(area.right(), area.top()), page.right_bottom())
        } else {
            Rect::from_min_max(Pos2::new(area.left(), area.bottom()), page.right_bottom())
        };
        let n = self.print_scale_n(rect);
        let c = strip.center();
        painter.text(c - Vec2::new(0.0, 12.0), Align2::CENTER_CENTER, "Legende", FontId::proportional(14.0), Color32::from_gray(110));
        painter.text(c + Vec2::new(0.0, 6.0), Align2::CENTER_CENTER, format!("Maßstab 1:{}", group_thousands(n.round().max(1.0) as u64)), FontId::proportional(15.0), Color32::from_rgb(160, 0, 100));
        painter.text(c + Vec2::new(0.0, 24.0), Align2::CENTER_CENTER, "Maßstabsleiste · Nordpfeil", FontId::proportional(11.0), Color32::from_gray(110));
        painter.text(Pos2::new(page.center().x, page.top() - 6.0), Align2::CENTER_BOTTOM, "Druckvorschau A4 – Ausschnitt mit Zoom und Verschieben wählen; gedruckt wird der Bereich im schwarzen Rahmen", FontId::proportional(12.0), Color32::from_gray(50));
    }

    /// Szene aus der Bildschirmdarstellung für die Ausgabe.
    fn export_scene(&self, region: [f64; 4], scale_n: f64, dxf: bool, dxf_fills: bool) -> Option<scene::Scene> {
        let pr = self.project.as_ref()?;
        let a_name = pr.st.name.clone();
        let inp = export::Input {
            project: pr,
            surface: self.surface.as_ref(),
            contours: self.contours.as_ref(),
            fill: self.fill.as_ref(),
            layers: &self.settings.layers,
            fill_mode: self.settings.layers.fill,
            palette: &self.palette,
            cmp: &self.cmp,
            a_name: &a_name,
            region,
            scale_n,
            dxf,
            dxf_fills,
        };
        Some(export::build(&inp))
    }

    /// Vektorbasierte PDF-Datei: gedruckt wird der Bereich im Druckrahmen.
    fn export_pdf(&mut self, path: &Path) {
        let rect = self.map_rect;
        let (area, s) = self.print_area_on_screen(rect);
        let n = 1000.0 * s as f64 / self.view.ppm;
        let sheet = self.sheet();
        let (_, _, aw, ah) = sheet.area();
        let (ox, oy) = self.view.to_world(rect, area.left_bottom());
        let region = [ox, oy, ox + aw * n / 1000.0, oy + ah * n / 1000.0];
        let Some(sc) = self.export_scene(region, n, false, true) else { return };
        let Some(pr) = &self.project else { return };
        let bytes = export::render_pdf(&sc, &sheet, [ox, oy], n, &pr.st.params, &pr.st.name);
        self.status = match std::fs::write(path, bytes) {
            Ok(_) => format!("PDF geschrieben: {} (A4 {}, 1:{})", path.display(), if self.print.landscape { "quer" } else { "hoch" }, group_thousands(n.round() as u64)),
            Err(e) => format!("PDF-Fehler: {}", e),
        };
    }

    /// DXF aus der aktuellen Bildschirmdarstellung (Ausschnitt, sichtbare Ebenen) mit den Export-Parametern.
    fn export_dxf_screen(&mut self, path: &Path) {
        let region = self.view.world_rect(self.map_rect);
        let Some(pr) = &self.project else { return };
        let n = if self.dxf.scale > 0.0 { self.dxf.scale } else { pr.st.params.scale.max(1.0) };
        let params = pr.st.params.clone();
        let Some(sc) = self.export_scene(region, n, true, self.dxf.fills) else { return };
        let text = export::to_dxf(&sc, &params, n);
        self.status = match files::write_text(path, &text) {
            Ok(_) => format!("DXF geschrieben: {} ({} Elemente, Maßstab 1:{})", path.display(), sc.items.len(), group_thousands(n.round() as u64)),
            Err(e) => format!("DXF-Fehler: {}", e),
        };
    }

    /// Blatt der 3D-Ausgabe.
    fn sheet3d(&self) -> Sheet {
        let mut s = Sheet::new(self.print3d.landscape);
        s.strip = self.print3d.strip;
        s
    }

    /// Vektorbasierte PDF-Datei des Gittermodells in der aktuellen Ansicht.
    fn export_pdf_3d(&mut self, path: &Path, zr: (f64, f64)) {
        let sheet = self.sheet3d();
        let Some(n) = self.v3d.print_n(&sheet) else {
            self.status = "PDF-Fehler: noch kein Gittermodell vorhanden.".into();
            return;
        };
        let Some(name) = self.project.as_ref().map(|p| p.st.name.clone()) else { return };
        let quads = self.v3d.quads(zr);
        let info = export::Model3dInfo {
            step: self.v3d.grid.as_ref().map_or(0.0, |g| g.step),
            exaggeration: self.v3d.exaggeration as f64,
            azimuth_deg: (self.v3d.azimuth as f64).to_degrees(),
            elevation_deg: (self.v3d.elevation as f64).to_degrees(),
            zr,
            shaded: self.v3d.shaded,
            water: (!self.v3d.waters.is_empty()).then(|| (self.v3d.water_rgb, self.v3d.waters.len())),
        };
        let legend = export::legend_3d(&info);
        let bytes = export::render_pdf_3d(&quads, self.v3d.pan, self.v3d.wire, &legend, self.v3d.north_dir(), &sheet, n, &name);
        self.status = match std::fs::write(path, bytes) {
            Ok(_) => format!("PDF geschrieben: {} (Gittermodell, A4 {}, 1:{})", path.display(), if self.print3d.landscape { "quer" } else { "hoch" }, group_thousands(n.round() as u64)),
            Err(e) => format!("PDF-Fehler: {}", e),
        };
    }

    fn print3d_dialog(&mut self, ctx: &egui::Context, zr: (f64, f64)) {
        if !self.print3d.open {
            return;
        }
        let mut open = true;
        let mut do_pdf = false;
        let sheet = self.sheet3d();
        let n_now = self.v3d.print_n(&sheet);
        if self.print3d.scale_edit <= 0.0 {
            self.print3d.scale_edit = nearest_standard_scale(n_now.unwrap_or(1000.0));
        }
        let mut set_scale = false;
        let mut standard = false;
        let mut fit = false;
        egui::Window::new("Gittermodell – PDF").open(&mut open).default_width(330.0).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Blatt A4:");
                ui.radio_value(&mut self.print3d.landscape, false, "Hochformat");
                ui.radio_value(&mut self.print3d.landscape, true, "Querformat");
            });
            ui.horizontal(|ui| {
                ui.label(if self.print3d.landscape { "Legendenstreifen rechts (mm):" } else { "Legendenstreifen unten (mm):" });
                ui.add(egui::DragValue::new(&mut self.print3d.strip).speed(1.0).range(25.0..=120.0));
            });
            ui.separator();
            ui.horizontal(|ui| {
                ui.label("Maßstab jetzt:");
                ui.strong(n_now.map_or("–".to_string(), |n| format!("1:{}", group_thousands(n.round().max(1.0) as u64))));
            });
            ui.horizontal(|ui| {
                ui.label("Maßstab 1:");
                ui.add(egui::DragValue::new(&mut self.print3d.scale_edit).speed(10.0).range(1.0..=10_000_000.0));
                if ui.button("setzen").clicked() {
                    set_scale = true;
                }
                if ui.button("Normmaßstab").on_hover_text("nächster gebräuchlicher Maßstab (z. B. 1:500, 1:1000)").clicked() {
                    standard = true;
                }
            });
            if ui.button("Modell auf das Blatt einpassen").clicked() {
                fit = true;
            }
            ui.separator();
            ui.weak("Ansicht drehen (ziehen), zoomen (Mausrad) und verschieben (rechte Maustaste). Gedruckt wird der Bereich im schwarzen Rahmen als Vektorgrafik; dazu Legende, Maßstab und Nordpfeil. Der Maßstab gilt waagrecht in Bildrichtung.");
            ui.add_space(4.0);
            if ui.add_sized([ui.available_width(), 30.0], egui::Button::new("PDF erzeugen…")).clicked() {
                do_pdf = true;
            }
        });
        if standard {
            self.print3d.scale_edit = nearest_standard_scale(n_now.unwrap_or(1000.0));
            set_scale = true;
        }
        if set_scale && self.print3d.scale_edit >= 1.0 {
            self.v3d.set_print_n(&sheet, self.print3d.scale_edit);
        }
        if fit {
            self.v3d.fit_print(&sheet, zr, standard_scale_at_least);
            self.print3d.scale_edit = self.v3d.print_n(&sheet).unwrap_or(self.print3d.scale_edit).round();
        }
        if do_pdf {
            let name = self.project.as_ref().map(|p| p.st.name.clone()).unwrap_or_default();
            let mut dlg = rfd::FileDialog::new().add_filter("PDF", &["pdf"]).set_file_name(format!("{}_3D.pdf", name));
            if !self.settings.data_dir.is_empty() && Path::new(&self.settings.data_dir).is_dir() {
                dlg = dlg.set_directory(&self.settings.data_dir);
            }
            if let Some(p) = dlg.save_file() {
                self.export_pdf_3d(&p, zr);
            }
        }
        self.print3d.open = open;
    }

    /// Wasserflächen mit Ausgleichungsebene für das Gittermodell übernehmen (nur bei eingeschalteter Ebene).
    fn sync_waters_3d(&mut self) {
        self.v3d.waters.clear();
        let Some(pr) = &self.project else { return };
        self.v3d.water_rgb = pr.st.params.water_rgb;
        if !self.settings.layers.water {
            return;
        }
        for w in &pr.st.waters {
            if w.pts.iter().any(|&i| i >= pr.st.points.len()) {
                continue;
            }
            if let Some(plane) = water::analyze(&pr.st.points, w).fit {
                self.v3d.waters.push(crate::view3d::WaterShape { poly: w.pts.iter().map(|&i| (pr.st.points[i].x, pr.st.points[i].y)).collect(), plane });
            }
        }
    }

    /// Oberfläche des zweiten Modells bereitstellen, wenn ein Profil sie braucht.
    fn ensure_profile_second(&mut self) {
        let want = self.project.as_ref().map_or(false, |p| p.st.profiles.iter().any(|d| d.show_second));
        let Some(sec) = self.cmp.second.as_ref().filter(|_| want) else {
            self.profile_second = None;
            return;
        };
        let key = (sec.label.clone(), sec.pr.st.points.len(), sec.pr.st.triangles.len());
        if self.profile_second.as_ref().map_or(true, |(k, _)| *k != key) {
            self.profile_second = Some((key, Surface::new(&sec.pr, false)));
        }
    }

    /// Fenster Profile: Liste, Einstellungen, Vorschau, Ausgabe als Vektor-PDF, überhöhtes DXF und CSV.
    fn profiles_window(&mut self, ctx: &egui::Context) {
        if !self.show_profiles {
            return;
        }
        self.ensure_profile_second();
        let Some(pr) = &self.project else { return };
        let before = pr.st.profiles.clone();
        let params = pr.st.params.clone();
        let waters = water_polys(pr);
        let mut defs = before.clone();
        let sel = self.profile_sel.min(defs.len().saturating_sub(1));
        let mut new_sel = sel;
        let mut open = true;
        // 0 = PDF, 1 = DXF, 2 = löschen, 3 = neues Längsprofil, 4 = neues Querprofil, 5 = Querprofile aus Achse, 6 = alle Querprofile ausgeben, 7 = CSV
        let mut act: Option<u8> = None;
        let step = self.profile_step;
        let surf = self.surface.as_ref();
        let second = self.profile_second.as_ref().map(|(k, sf)| (sf, k.0.as_str()));
        let second_loaded = self.cmp.second.is_some();
        let ex = profile::Extras { waters: &waters, second };
        egui::Window::new("Profile").open(&mut open).default_width(560.0).default_height(620.0).show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Neues Längsprofil…").on_hover_text("Achspunkte anklicken, rechte Maustaste beendet").clicked() {
                    act = Some(3);
                }
                if ui.button("Neues Querprofil…").on_hover_text("Schnittlinie anklicken, rechte Maustaste beendet").clicked() {
                    act = Some(4);
                }
            });
            ui.separator();
            if defs.is_empty() {
                ui.label("Noch keine Profile. Mit „Neues Längsprofil…“ oder „Neues Querprofil…“ eine Schnittlinie im Plan anklicken.");
                return;
            }
            egui::ScrollArea::vertical().max_height(110.0).id_salt("profile_list").show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    for (k, d) in defs.iter().enumerate() {
                        if ui.selectable_label(k == sel, d.title()).clicked() {
                            new_sel = k;
                            if d.view == ProfileView::Laengs {
                                act = Some(5);
                            }
                        }
                    }
                });
            });
            ui.separator();
            let Some(d) = defs.get_mut(sel) else { return };
            let len = d.length();
            egui::Grid::new("profile_form").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
                ui.label("Name");
                ui.text_edit_singleline(&mut d.name);
                ui.end_row();
                ui.label("Darstellung");
                egui::ComboBox::from_id_salt("profile_view")
                    .selected_text(if d.view == ProfileView::Laengs { "Längsprofil (Station)" } else { "Querprofil (Abstand links/rechts)" })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut d.view, ProfileView::Laengs, "Längsprofil (Station)");
                        ui.selectable_value(&mut d.view, ProfileView::Quer, "Querprofil (Abstand links/rechts)");
                    });
                ui.end_row();
                ui.label(if d.view == ProfileView::Laengs { "Station am Anfang (m)" } else { "Station des Nullpunkts (m)" });
                ui.add(egui::DragValue::new(&mut d.start_station).speed(1.0).max_decimals(3));
                ui.end_row();
                if d.view == ProfileView::Quer {
                    ui.label("Nullpunkt");
                    ui.horizontal(|ui| {
                        let mut mid = d.zero.is_none();
                        if ui.checkbox(&mut mid, "Mitte").changed() {
                            d.zero = if mid { None } else { Some(len / 2.0) };
                        }
                        if let Some(z) = d.zero.as_mut() {
                            ui.add(egui::DragValue::new(z).range(0.0..=len.max(0.0)).speed(0.5).max_decimals(3).suffix(" m ab Anfang"));
                        }
                    });
                    ui.end_row();
                }
                ui.label("Maßstab 1 :");
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut d.scale_n).range(0.0..=100_000.0).speed(10.0).max_decimals(0));
                    ui.weak("0 = automatisch");
                });
                ui.end_row();
                ui.label("Überhöhung");
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut d.exag).range(0.0..=200.0).speed(0.5).max_decimals(2));
                    ui.weak("0 = automatisch");
                });
                ui.end_row();
                ui.label("Beschriftung alle (m)");
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut d.label_step).range(0.0..=10_000.0).speed(1.0).max_decimals(2));
                    ui.weak("0 = automatisch");
                });
                ui.end_row();
                ui.label("Koten");
                ui.horizontal(|ui| {
                    let mut on = !d.no_koten;
                    if ui.checkbox(&mut on, "an Knickpunkten").changed() {
                        d.no_koten = !on;
                    }
                    if on {
                        let mut c = d.kink_clean.unwrap_or(profile::KINK_CLEAN_DEFAULT_M);
                        if ui.add(egui::DragValue::new(&mut c).range(0.0..=1000.0).speed(0.5).max_decimals(2).prefix("Bereinigung ").suffix(" m")).on_hover_text("kleinster Abstand beschrifteter Knickpunkte; 0 = alle").changed() {
                            d.kink_clean = Some(c);
                        }
                    }
                });
                ui.end_row();
                ui.label("Maßkette");
                ui.checkbox(&mut d.dim_chain, "Teilabstände zwischen den Knickpunkten (Bandzeile)");
                ui.end_row();
                ui.label("Höhenunterschied");
                ui.horizontal(|ui| {
                    let mut on = d.dim_h.is_some();
                    if ui.checkbox(&mut on, "Maßlinie").changed() {
                        let o = d.offset();
                        d.dim_h = if on { Some((o, o + len)) } else { None };
                    }
                    if let Some((a, b)) = d.dim_h.as_mut() {
                        let o = if d.view == ProfileView::Laengs { d.start_station } else { -d.zero.unwrap_or(len / 2.0) };
                        ui.add(egui::DragValue::new(a).range(o..=o + len).speed(1.0).max_decimals(2).prefix("von ").suffix(" m"));
                        ui.add(egui::DragValue::new(b).range(o..=o + len).speed(1.0).max_decimals(2).prefix("bis ").suffix(" m"));
                    }
                });
                ui.end_row();
                ui.label("Zweites Gelände");
                ui.add_enabled_ui(second_loaded, |ui| {
                    ui.checkbox(&mut d.show_second, "aus dem Modellvergleich (gestrichelt, eigene Bandzeile)").on_disabled_hover_text("Zuerst im Modellvergleich ein zweites Modell einlesen.");
                });
                ui.end_row();
                ui.label("Blatt");
                egui::ComboBox::from_id_salt("profile_sheet").selected_text(d.format.name()).show_ui(ui, |ui| {
                    for f in [SheetFormat::Auto, SheetFormat::A4, SheetFormat::A3, SheetFormat::Roll] {
                        ui.selectable_value(&mut d.format, f, f.name());
                    }
                });
                ui.end_row();
            });
            ui.separator();
            match surf {
                Some(sf) => match profile::prepare(sf, d, step, &ex) {
                    Ok((c, lay)) => {
                        let (w, h) = lay.sheet.size();
                        ui.label(format!(
                            "Länge {:.2} m · Maßstab 1:{} (Höhen 1:{}, Überhöhung {}fach) · Blatt {:.0} × {:.0} mm · Beschriftung alle {} m",
                            c.length,
                            export::trim_num(lay.n),
                            export::trim_num(lay.n / lay.exag),
                            export::trim_num(lay.exag),
                            w,
                            h,
                            export::trim_num(lay.label_step)
                        ));
                        if !lay.fits {
                            let parts = profile::split_parts(d, c.length, &lay).len();
                            let msg = if parts > 1 { format!("Das Profil passt nicht auf eine Rolle (4000 mm): Das PDF wird in {} Blätter geteilt.", parts) } else { "Das Profil passt nicht auf ein Blatt, auch nicht im kleinsten Maßstab.".to_string() };
                            ui.colored_label(Color32::from_rgb(200, 60, 0), msg);
                        }
                        ui.add(egui::DragValue::new(&mut self.profile_zoom).range(0.3..=12.0).speed(0.05).max_decimals(2).prefix("Vorschau ").suffix(" Punkte/mm"));
                        let (sc, origin) = profile::build_scene(&c, d, &lay, &params, false);
                        egui::ScrollArea::both().id_salt("profile_preview").max_height(280.0).show(ui, |ui| {
                            profile_preview(ui, &sc, origin, &lay, self.profile_zoom);
                        });
                    }
                    Err(e) => {
                        ui.colored_label(Color32::from_rgb(200, 60, 0), e);
                    }
                },
                None => {
                    ui.label("Kein Geländemodell berechnet.");
                }
            }
            if d.view == ProfileView::Laengs {
                if ui.button("Querprofile aus dieser Achse erzeugen…").on_hover_text("Abstand, Maßstab und Überhöhung der Querprofile festlegen").clicked() {
                    act = Some(5);
                }
            }
            ui.horizontal(|ui| {
                if ui.button("PDF…").on_hover_text("Vektor-PDF, Normschrift (sonst Courier); zu lange Längsprofile werden auf mehrere Blätter geteilt").clicked() {
                    act = Some(0);
                }
                if ui.button("DXF…").on_hover_text("überhöht, Schriftgrößen nach dem Maßstab, Textstil ISO3098").clicked() {
                    act = Some(1);
                }
                if ui.button("CSV…").on_hover_text("Wertetabelle: Station, Koordinaten, Höhe, Neigung").clicked() {
                    act = Some(7);
                }
                if ui.button("Löschen").clicked() {
                    act = Some(2);
                }
            });
            if defs.iter().filter(|x| x.view == ProfileView::Quer).count() > 1 {
                ui.horizontal(|ui| {
                    if ui.button("Alle Querprofile ausgeben (PDF und DXF)…").on_hover_text("gemeinsamer Maßstab und gemeinsame Überhöhung; Ziel: Ordner").clicked() {
                        act = Some(6);
                    }
                    ui.checkbox(&mut self.profile_series_single, "je Profil eine Datei").on_hover_text("sonst mehrere Profile auf einem Blatt (A4/A3/Rolle nach der Blattwahl)");
                });
            }
        });
        self.profile_sel = new_sel;
        if defs != before {
            if let Some(pr) = self.project.as_mut() {
                pr.st.profiles = defs;
            }
            self.dirty = true;
        }
        match act {
            Some(0) => self.export_profile(sel, true),
            Some(1) => self.export_profile(sel, false),
            Some(2) => {
                self.checkpoint();
                if let Some(pr) = self.project.as_mut() {
                    if sel < pr.st.profiles.len() {
                        pr.st.profiles.remove(sel);
                    }
                }
                self.status = "Profil gelöscht.".into();
            }
            Some(n @ (3 | 4)) => {
                self.profile_quer = n == 4;
                self.set_tool(Tool::Profile);
            }
            Some(5) => self.open_qs_dialog(new_sel),
            Some(6) => self.export_all_cross_sections(),
            Some(7) => self.export_profile_csv(sel),
            _ => {}
        }
        self.show_profiles = open;
    }

    /// Öffnet die Maske für die Querprofile des Längsprofils `k` mit den Werten aus den Parametern.
    fn open_qs_dialog(&mut self, k: usize) {
        let Some(pr) = &self.project else { return };
        let p = &pr.st.params;
        self.qs_dialog = Some(QsDialog {
            axis: k,
            at_vertices: false,
            step: p.qs_step,
            half: p.qs_half,
            n: p.qs_scale_n,
            exag: p.qs_exag,
            n_user: p.qs_scale_n > 0.0,
            e_user: p.qs_exag > 0.0,
            pending: true,
            made: Vec::new(),
            axis_pts: Vec::new(),
            steps: Vec::new(),
        });
    }

    /// Querprofile der Maske neu bestimmen (am Modellrand beschnitten) und Vorschlag für Maßstab und Überhöhung ermitteln.
    fn qs_refresh(&self, d: &mut QsDialog) {
        d.made.clear();
        d.axis_pts.clear();
        d.steps.clear();
        let (Some(pr), Some(sf)) = (self.project.as_ref(), self.surface.as_ref()) else { return };
        let Some(axis) = pr.st.profiles.get(d.axis) else { return };
        d.axis_pts = axis.line.clone();
        let total = profile::axis_length(&axis.line);
        d.steps = profile::possible_steps(total);
        let stations = if d.at_vertices { profile::vertex_stations(&axis.line) } else { profile::regular_stations(total, d.step) };
        d.made = profile::cross_sections_at(&axis.line, axis.start_station, &stations, d.half).into_iter().filter_map(|m| profile::clip_to_model(sf, m)).collect();
        if d.made.is_empty() {
            return;
        }
        let waters = water_polys(pr);
        let ex = profile::Extras { waters: &waters, second: None };
        if let Some((n, e, _)) = profile::common_layout(sf, &d.made, self.profile_step, &ex) {
            if !d.n_user {
                d.n = n;
            }
            if !d.e_user {
                d.exag = e;
            }
        }
    }

    /// Maske „Querprofile erzeugen“: Lage (frei oder an den Achspunkten), Abstand, Maßstab und Überhöhung, vorbelegt mit
    /// dem Vorschlag. Die Lage der Profile zeigt die Karte schon während der Eingabe.
    fn qs_dialog_window(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.qs_dialog.take() else { return };
        // Vorschlag erst berechnen, wenn nicht mehr gezogen wird
        if d.pending && !ctx.input(|i| i.pointer.any_down()) {
            self.qs_refresh(&mut d);
            d.pending = false;
        }
        let axes: Vec<(usize, String)> = self.project.as_ref().map(|pr| pr.st.profiles.iter().enumerate().filter(|(_, x)| x.view == ProfileView::Laengs).map(|(i, x)| (i, x.title())).collect()).unwrap_or_default();
        let mut open = true;
        let (mut create, mut suggest, mut cancel, mut load) = (false, false, false, false);
        egui::Window::new("Querprofile erzeugen").open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
            egui::Grid::new("qs_form").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
                ui.label("Längsprofil (Achse)");
                ui.horizontal(|ui| {
                    let sel = axes.iter().find(|(i, _)| *i == d.axis).map_or("–".to_string(), |(_, t)| t.clone());
                    egui::ComboBox::from_id_salt("qs_axis").selected_text(sel).show_ui(ui, |ui| {
                        for (i, t) in &axes {
                            if ui.selectable_value(&mut d.axis, *i, t).changed() {
                                d.pending = true;
                            }
                        }
                    });
                    if ui.button("Laden…").on_hover_text("Achse aus einer Textdatei (je Zeile x y) als Längsprofil laden").clicked() {
                        load = true;
                    }
                });
                ui.end_row();
                ui.label("Lage der Profile");
                ui.horizontal(|ui| {
                    let free = ui.radio_value(&mut d.at_vertices, false, "frei (Abstand)").changed();
                    let verts = ui.radio_value(&mut d.at_vertices, true, "an den Achspunkten").changed();
                    if free || verts {
                        d.pending = true;
                    }
                });
                ui.end_row();
                if !d.at_vertices {
                    ui.label("Abstand der Profile");
                    if ui.add(egui::DragValue::new(&mut d.step).range(QS_STEP_MIN_M..=10_000.0).speed(1.0).max_decimals(2).suffix(" m")).changed() {
                        d.pending = true;
                    }
                    ui.end_row();
                }
                ui.label("Breite links/rechts");
                if ui.add(egui::DragValue::new(&mut d.half).range(QS_STEP_MIN_M..=10_000.0).speed(1.0).max_decimals(2).prefix("± ").suffix(" m")).changed() {
                    d.pending = true;
                }
                ui.end_row();
                ui.label("Maßstab 1 :");
                if ui.add(egui::DragValue::new(&mut d.n).range(0.0..=100_000.0).speed(10.0).max_decimals(0)).changed() {
                    d.n_user = true;
                }
                ui.end_row();
                ui.label("Überhöhung");
                if ui.add(egui::DragValue::new(&mut d.exag).range(0.0..=200.0).speed(0.5).max_decimals(2)).changed() {
                    d.e_user = true;
                }
                ui.end_row();
            });
            if !d.at_vertices && !d.steps.is_empty() {
                ui.label(format!("Mögliche Abstände (Achslänge {} m):", export::trim_num((profile::axis_length(&d.axis_pts) * 100.0).round() / 100.0)));
                ui.horizontal_wrapped(|ui| {
                    for st in d.steps.clone() {
                        if ui.selectable_label((d.step - st).abs() < 1e-9, format!("{} m", export::trim_num(st))).clicked() {
                            d.step = st;
                            d.pending = true;
                        }
                    }
                });
            }
            ui.weak(format!(
                "{} Querprofile (höchstens {}), am Modellrand beschnitten. In der Karte: Achse blau strichpunktiert, Querprofile blau gestrichelt. 0 = Maßstab/Überhöhung automatisch je Profil.",
                d.made.len(),
                profile::MAX_CROSS_SECTIONS
            ));
            ui.horizontal(|ui| {
                if ui.add_enabled(!d.made.is_empty(), egui::Button::new("OK – Querprofile erzeugen")).clicked() {
                    create = true;
                }
                if ui.button("Vorschlag neu ermitteln").on_hover_text("Maßstab und Überhöhung aus dem Gelände entlang der Querprofile").clicked() {
                    suggest = true;
                }
                if ui.button("Abbrechen").clicked() {
                    cancel = true;
                }
            });
        });
        if load {
            self.load_axis_profile(&mut d);
        }
        if suggest {
            d.n_user = false;
            d.e_user = false;
            d.pending = true;
        }
        if create && !d.made.is_empty() {
            self.checkpoint();
            let made = std::mem::take(&mut d.made);
            let n = made.len();
            if let Some(pr) = self.project.as_mut() {
                pr.st.profiles.extend(made.into_iter().map(|mut m| {
                    m.scale_n = d.n;
                    m.exag = d.exag;
                    m
                }));
                self.profile_sel = pr.st.profiles.len() - n;
            }
            self.dirty = true;
            self.status = format!("{} Querprofile angelegt.", n);
            return;
        }
        if open && !cancel {
            self.qs_dialog = Some(d);
        }
    }

    /// Längsprofil aus einer Textdatei (je Zeile x y) laden und in der Maske als Achse wählen.
    fn load_axis_profile(&mut self, d: &mut QsDialog) {
        let Some(path) = rfd::FileDialog::new().set_title("Längsprofil laden").add_filter("Text / CSV", &["txt", "csv", "xyz", "asc", "dat"]).pick_file() else { return };
        let axis = match files::read_text(&path) {
            Ok(t) => profile::parse_axis(&t),
            Err(e) => {
                self.status = format!("Längsprofil: {}", e);
                return;
            }
        };
        if axis.len() < 2 {
            self.status = "Längsprofil: Die Datei enthält weniger als zwei Punkte (je Zeile x y).".into();
            return;
        }
        self.checkpoint();
        let name = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        if let Some(pr) = self.project.as_mut() {
            pr.st.profiles.push(ProfileDef { name, view: ProfileView::Laengs, line: axis, ..Default::default() });
            d.axis = pr.st.profiles.len() - 1;
        }
        self.dirty = true;
        d.pending = true;
        self.status = format!("Längsprofil geladen: {}", path.display());
    }

    /// Alle Querprofile in einen Ordner schreiben (PDF und DXF) mit gemeinsamem Maßstab und gemeinsamer Überhöhung:
    /// mehrere Profile je Blatt, oder je Profil eine Datei.
    fn export_all_cross_sections(&mut self) {
        let (Some(pr), Some(sf)) = (&self.project, &self.surface) else { return };
        let defs: Vec<ProfileDef> = pr.st.profiles.iter().filter(|d| d.view == ProfileView::Quer).cloned().collect();
        let waters = water_polys(pr);
        let second = self.profile_second.as_ref().map(|(k, s)| (s, k.0.as_str()));
        let ex = profile::Extras { waters: &waters, second };
        let items = profile::prepare_series(sf, &defs, self.profile_step, &ex);
        if items.is_empty() {
            self.status = "Querprofile: keine Schnittlinie liegt im Geländemodell.".into();
            return;
        }
        let Some(dir) = rfd::FileDialog::new().set_title("Ordner für die Querprofile").pick_folder() else { return };
        let params = pr.st.params.clone();
        let stem = self.file_stem();
        let (n, e) = (items[0].2.n, items[0].2.exag);
        let (mut files_done, mut failed) = (0usize, 0usize);
        let mut write = |path: std::path::PathBuf, bytes: Result<Vec<u8>, String>| match bytes {
            Ok(b) => {
                if std::fs::write(path, b).is_ok() {
                    files_done += 1;
                } else {
                    failed += 1;
                }
            }
            Err(t) => {
                if files::write_text(&path, &t).is_ok() {
                    files_done += 1;
                } else {
                    failed += 1;
                }
            }
        };
        if self.profile_series_single {
            for (c, d, lay) in &items {
                let safe: String = d.name.chars().map(|ch| if ch.is_alphanumeric() { ch } else { '_' }).collect();
                let base = dir.join(format!("{}_Querprofil_{}", stem, safe));
                write(base.with_extension("pdf"), Ok(profile::to_pdf(c, d, lay, &params, &pr.st.name)));
                write(base.with_extension("dxf"), Err(profile::to_dxf(c, d, lay, &params)));
            }
        } else {
            let fmt = defs.first().map_or(SheetFormat::Auto, |d| d.format);
            let pages = profile::series_pages(&items, fmt, &params, false);
            let pages_dxf = profile::series_pages(&items, fmt, &params, true);
            if pages.is_empty() {
                self.status = "Querprofile: ein einzelnes Profil ist breiter als die größte Rolle.".into();
                return;
            }
            for (k, (pg, pd)) in pages.iter().zip(&pages_dxf).enumerate() {
                let base = dir.join(format!("{}_Querprofile_Blatt_{}", stem, k + 1));
                write(base.with_extension("pdf"), Ok(profile::series_to_pdf(pg, &params, &pr.st.name)));
                write(base.with_extension("dxf"), Err(profile::series_to_dxf(pd, &params)));
            }
        }
        self.status = format!("{} Querprofile (1:{}, Überhöhung {}fach): {} Dateien nach {} geschrieben, {} fehlgeschlagen.", items.len(), group_thousands(n.round() as u64), export::trim_num(e), files_done, dir.display(), failed);
    }

    /// Wertetabelle des Profils `k` als CSV-Datei.
    fn export_profile_csv(&mut self, k: usize) {
        let (Some(pr), Some(sf)) = (&self.project, &self.surface) else { return };
        let Some(def) = pr.st.profiles.get(k).cloned() else { return };
        let c = match profile::prepare(sf, &def, self.profile_step, &profile::Extras::default()) {
            Ok((c, _)) => c,
            Err(e) => {
                self.status = format!("Profil-Fehler: {}", e);
                return;
            }
        };
        let safe: String = def.name.chars().map(|ch| if ch.is_alphanumeric() { ch } else { '_' }).collect();
        let name = format!("{}_Profil_{}.csv", self.file_stem(), safe);
        let Some(path) = rfd::FileDialog::new().add_filter("CSV", &["csv"]).set_file_name(name).save_file() else { return };
        self.status = match files::write_text(&path, &profile::to_csv(&c, &def)) {
            Ok(_) => format!("CSV geschrieben: {}", path.display()),
            Err(e) => format!("CSV-Fehler: {}", e),
        };
    }

    /// Profil `k` als Vektor-PDF (`pdf`, zu lange Längsprofile in mehreren Blättern) oder überhöhtes DXF schreiben.
    fn export_profile(&mut self, k: usize, pdf: bool) {
        let (Some(pr), Some(sf)) = (&self.project, &self.surface) else {
            self.status = "Profil-Fehler: kein Geländemodell.".into();
            return;
        };
        let Some(def) = pr.st.profiles.get(k).cloned() else { return };
        let waters = water_polys(pr);
        let second = self.profile_second.as_ref().map(|(k, s)| (s, k.0.as_str()));
        let ex = profile::Extras { waters: &waters, second };
        let (c, lay) = match profile::prepare(sf, &def, self.profile_step, &ex) {
            Ok(r) => r,
            Err(e) => {
                self.status = format!("Profil-Fehler: {}", e);
                return;
            }
        };
        let ext = if pdf { "pdf" } else { "dxf" };
        let safe: String = def.name.chars().map(|ch| if ch.is_alphanumeric() { ch } else { '_' }).collect();
        let stem = format!("{}_{}{}", self.file_stem(), if def.view == ProfileView::Laengs { "Laengsprofil" } else { "Querprofil" }, if safe.is_empty() { String::new() } else { format!("_{}", safe) });
        let Some(path) = rfd::FileDialog::new().add_filter(ext, &[ext]).set_file_name(format!("{}.{}", stem, ext)).save_file() else { return };
        let params = pr.st.params.clone();
        let (w, h) = lay.sheet.size();
        let parts = if pdf { profile::split_parts(&def, c.length, &lay) } else { vec![def.clone()] };
        let mut res: Result<(), String> = Ok(());
        let mut written = 0usize;
        if pdf {
            for (i, part) in parts.iter().enumerate() {
                let Ok((pc, pl)) = profile::prepare(sf, part, self.profile_step, &ex) else { continue };
                let target = if parts.len() == 1 { path.clone() } else { path.with_file_name(format!("{}_Blatt{}.pdf", path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default(), i + 1)) };
                match std::fs::write(&target, profile::to_pdf(&pc, part, &pl, &params, &pr.st.name)) {
                    Ok(_) => written += 1,
                    Err(e) => {
                        res = Err(e.to_string());
                        break;
                    }
                }
            }
        } else {
            res = files::write_text(&path, &profile::to_dxf(&c, &def, &lay, &params)).map_err(|e| e.to_string());
            written = 1;
        }
        self.status = match res {
            Ok(_) if written > 1 => format!("PDF geschrieben: {} Blätter ab {} (1:{}, Überhöhung {}fach)", written, path.display(), group_thousands(lay.n.round() as u64), export::trim_num(lay.exag)),
            Ok(_) => format!("{} geschrieben: {} (1:{}, Überhöhung {}fach, {:.0} × {:.0} mm)", ext.to_uppercase(), path.display(), group_thousands(lay.n.round() as u64), export::trim_num(lay.exag), w, h),
            Err(e) => format!("{}-Fehler: {}", ext.to_uppercase(), e),
        };
    }

    /// Fenster Wasserflächen: Liste, editierbare Punkttabelle, Ausreißer auf die Ebene ziehen, Protokoll.
    fn water_window(&mut self, ctx: &egui::Context) {
        if !self.show_water {
            return;
        }
        let Some(pr) = &self.project else {
            self.show_water = false;
            return;
        };
        let waters_before = pr.st.waters.clone();
        let mut waters = waters_before.clone();
        let reports: Vec<water::WaterReport> = waters.iter().map(|w| water::analyze(&pr.st.points, w)).collect();
        let protocol = water::protocol(&pr.st.points, &waters);
        self.water_sel = self.water_sel.min(waters.len().saturating_sub(1));
        let sel = self.water_sel;
        // Zeilen der gewählten Wasserfläche: (Punktindex, Name, x, y, z, Ebenenhöhe, Abweichung, Ausreißer)
        let rows: Vec<(usize, usize, String, f64, f64, f64, f64, f64, bool)> = match (waters.get(sel), reports.get(sel)) {
            (Some(w), Some(r)) => {
                let valid: Vec<(usize, usize)> = w.pts.iter().enumerate().filter(|(_, &i)| i < pr.st.points.len()).map(|(pos, &i)| (pos, i)).collect();
                valid.iter().enumerate().map(|(k, &(pos, i))| {
                    let p = &pr.st.points[i];
                    let (pz, dz, out) = r.rows.get(k).map_or((f64::NAN, 0.0, false), |q| (q.plane_z, q.dz, q.outlier));
                    (pos, i, p.name.clone(), p.x, p.y, p.z, pz, dz, out)
                }).collect()
            }
            _ => Vec::new(),
        };
        let total_out: usize = reports.iter().map(|r| r.outliers).sum();
        let mut open = true;
        let mut act: Option<WaterAct> = None;
        let mut new_tool = false;
        let mut z_edits: Vec<(usize, f64)> = Vec::new();
        let mut edit_done = false;
        egui::Window::new("Wasserflächen").open(&mut open).default_size([700.0, 540.0]).show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.add_enabled(waters.len() < water::MAX_WATERS, egui::Button::new("Neue Wasserfläche…")).on_hover_text("Punkte anklicken, rechte Maustaste schließt").clicked() {
                    new_tool = true;
                }
                ui.label(format!("{} von {} Wasserflächen", waters.len(), water::MAX_WATERS));
                ui.separator();
                ui.selectable_value(&mut self.water_tab, 0, "Tabelle");
                ui.selectable_value(&mut self.water_tab, 1, "Protokoll");
            });
            if total_out > 0 {
                ui.colored_label(Color32::from_rgb(200, 90, 0), format!("WARNUNG: {} Punkt(e) weichen mehr als {:.0} cm von der vermittelnden Ebene ab.", total_out, water::WATER_TOL_M * 100.0));
                if ui.button("Alle Ausreißer auf die Ebene ziehen").clicked() {
                    act = Some(WaterAct::PullAll(None));
                }
            }
            ui.separator();
            if self.water_tab == 1 {
                ui.horizontal(|ui| {
                    if ui.button("Als Textdatei speichern…").clicked() {
                        act = Some(WaterAct::SaveProtocol);
                    }
                    if ui.button("Kopieren").clicked() {
                        ui.ctx().copy_text(protocol.clone());
                    }
                });
                egui::ScrollArea::both().show(ui, |ui| {
                    let mut text = protocol.as_str();
                    ui.add(egui::TextEdit::multiline(&mut text).font(egui::TextStyle::Monospace).desired_width(f32::INFINITY));
                });
                return;
            }
            egui::Grid::new("water_list").striped(true).show(ui, |ui| {
                for h in ["Nr", "Name", "Punkte", "z Ebene [m]", "Neigung [%]", "Fließrichtung [gon]", "max. |dz| [cm]", "", ""] {
                    ui.strong(h);
                }
                ui.end_row();
                for (k, w) in waters.iter_mut().enumerate() {
                    let rep = &reports[k];
                    if ui.selectable_label(sel == k, format!("{}", k + 1)).clicked() {
                        act = Some(WaterAct::Select(k));
                    }
                    ui.add(egui::TextEdit::singleline(&mut w.name).desired_width(130.0));
                    ui.label(format!("{}", w.pts.len()));
                    match rep.fit {
                        Some(f) => {
                            ui.monospace(format!("{:.3}", f.z0));
                            ui.monospace(format!("{:.4}", f.slope_percent()));
                            ui.monospace(f.flow_gon().map_or("–".to_string(), |g| format!("{:.1}", g)));
                            let txt = format!("{:.1}", rep.max_abs * 100.0);
                            if rep.outliers > 0 {
                                ui.colored_label(Color32::from_rgb(200, 0, 0), format!("{} ({} Ausreißer)", txt, rep.outliers));
                            } else {
                                ui.monospace(txt);
                            }
                        }
                        None => {
                            ui.label("keine Ebene");
                            ui.label("");
                            ui.label("");
                            ui.label("");
                        }
                    }
                    ui.add_enabled_ui(rep.outliers > 0, |ui| {
                        if ui.button("Ausreißer ziehen").on_hover_text("Höhen der Ausreißer auf die vermittelnde Ebene setzen").clicked() {
                            act = Some(WaterAct::PullAll(Some(k)));
                        }
                    });
                    if ui.button("Löschen").clicked() {
                        act = Some(WaterAct::Delete(k));
                    }
                    ui.end_row();
                }
            });
            ui.separator();
            if let Some(w) = waters.get(sel) {
                ui.strong(format!("Punkte von „{}“ – Höhen sind editierbar", w.name));
                let can_remove = w.pts.len() > water::MIN_WATER_POINTS;
                egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                    egui::Grid::new("water_points").striped(true).show(ui, |ui| {
                        for h in ["Nr", "Punkt", "x [m]", "y [m]", "z [m]", "z Ebene [m]", "dz [cm]", "", ""] {
                            ui.strong(h);
                        }
                        ui.end_row();
                        for (k, &(pos, i, ref name, x, y, z, pz, dz, out)) in rows.iter().enumerate() {
                            ui.label(format!("{}", k + 1));
                            ui.label(name);
                            ui.monospace(format!("{:.3}", x));
                            ui.monospace(format!("{:.3}", y));
                            let mut zz = z;
                            let resp = ui.add(egui::DragValue::new(&mut zz).speed(0.005).fixed_decimals(3));
                            if resp.changed() {
                                z_edits.push((i, zz));
                            }
                            if resp.drag_stopped() || resp.lost_focus() {
                                edit_done = true;
                            }
                            ui.monospace(if pz.is_nan() { "–".to_string() } else { format!("{:.3}", pz) });
                            let t = format!("{:+.1}", dz * 100.0);
                            if out {
                                ui.colored_label(Color32::from_rgb(200, 0, 0), format!("{}  !", t));
                            } else {
                                ui.monospace(t);
                            }
                            ui.add_enabled_ui(!pz.is_nan() && dz.abs() > 0.0005, |ui| {
                                if ui.button("→ Ebene").on_hover_text("Höhe auf die vermittelnde Ebene setzen").clicked() {
                                    act = Some(WaterAct::PullOne(i, (pz * 1000.0).round() / 1000.0));
                                }
                            });
                            ui.add_enabled_ui(can_remove, |ui| {
                                if ui.button("entfernen").on_hover_text("Punkt aus dieser Wasserfläche nehmen (der Punkt bleibt im Modell)").clicked() {
                                    act = Some(WaterAct::RemovePoint(sel, pos));
                                }
                            });
                            ui.end_row();
                        }
                    });
                });
                ui.weak("Die Höhe eines Punktes gilt im ganzen Modell. Nach dem Ziehen wird die Ebene neu ausgeglichen; weitere Ausreißer können dadurch sichtbar werden.");
            } else {
                ui.label("Noch keine Wasserfläche. Manipulieren → Wasserflächen → erzeugen.");
            }
        });
        if !open {
            self.show_water = false;
        }
        if new_tool {
            self.settings.layers.water = true;
            self.set_tool(Tool::WaterCreate);
        }
        // Namen übernehmen
        let renamed = waters.iter().zip(&waters_before).any(|(a, b)| a.name != b.name);
        if renamed {
            if let Some(pr) = self.project.as_mut() {
                for (w, n) in pr.st.waters.iter_mut().zip(&waters) {
                    w.name = n.name.clone();
                }
            }
            self.dirty = true;
        }
        // Höhen editiert: ein Rückgängig-Schritt je Bearbeitung, Neuberechnung erst am Ende
        if !z_edits.is_empty() {
            if !self.water_editing {
                self.checkpoint();
                self.water_editing = true;
            }
            if let Some(pr) = self.project.as_mut() {
                for (i, z) in z_edits {
                    pr.st.points[i].z = z;
                }
            }
        }
        if edit_done {
            self.water_editing = false;
            self.recompute();
        }
        match act {
            Some(WaterAct::Select(k)) => self.water_sel = k,
            Some(WaterAct::Delete(k)) => {
                self.checkpoint();
                if let Some(pr) = self.project.as_mut() {
                    pr.st.waters.remove(k);
                }
                self.status = "Wasserfläche gelöscht.".into();
            }
            Some(WaterAct::RemovePoint(k, pos)) => {
                self.checkpoint();
                if let Some(pr) = self.project.as_mut() {
                    pr.st.waters[k].pts.remove(pos);
                }
            }
            Some(WaterAct::PullOne(i, z)) => {
                self.checkpoint();
                if let Some(pr) = self.project.as_mut() {
                    pr.st.points[i].z = z;
                }
                self.recompute();
                self.status = "Höhe auf die vermittelnde Ebene gesetzt.".into();
            }
            Some(WaterAct::PullAll(which)) => {
                let mut changes: Vec<(usize, f64)> = Vec::new();
                for (k, rep) in reports.iter().enumerate() {
                    if which.map_or(true, |w| w == k) {
                        changes.extend(rep.rows.iter().filter(|r| r.outlier).map(|r| (r.idx, (r.plane_z * 1000.0).round() / 1000.0)));
                    }
                }
                if !changes.is_empty() {
                    self.checkpoint();
                    if let Some(pr) = self.project.as_mut() {
                        for &(i, z) in &changes {
                            pr.st.points[i].z = z;
                        }
                    }
                    self.recompute();
                    self.status = format!("{} Ausreißer auf die vermittelnde Ebene gezogen.", changes.len());
                }
            }
            Some(WaterAct::SaveProtocol) => {
                let name = self.project.as_ref().map(|p| p.st.name.clone()).unwrap_or_default();
                let mut dlg = rfd::FileDialog::new().add_filter("Text", &["txt"]).set_file_name(format!("{}_Wasserflaechen.txt", name));
                if !self.settings.data_dir.is_empty() && Path::new(&self.settings.data_dir).is_dir() {
                    dlg = dlg.set_directory(&self.settings.data_dir);
                }
                if let Some(p) = dlg.save_file() {
                    self.status = match files::write_text(&p, &protocol) {
                        Ok(_) => format!("Protokoll gespeichert: {}", p.display()),
                        Err(e) => format!("Fehler beim Speichern: {}", e),
                    };
                }
            }
            None => {}
        }
    }

    fn print_dialog(&mut self, ctx: &egui::Context) {
        if !self.print.open {
            return;
        }
        let rect = self.map_rect;
        let n_now = self.print_scale_n(rect);
        if self.print.scale_edit <= 0.0 {
            self.print.scale_edit = nearest_standard_scale(n_now);
        }
        let mut open = true;
        let mut do_pdf = false;
        let mut set_scale = false;
        let mut standard = false;
        let mut fit = false;
        egui::Window::new("Druckausgabe (PDF)").open(&mut open).default_width(330.0).default_pos(Pos2::new((ctx.content_rect().right() - 360.0).max(250.0), 70.0)).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Blatt A4:");
                ui.radio_value(&mut self.print.landscape, false, "Hochformat");
                ui.radio_value(&mut self.print.landscape, true, "Querformat");
            });
            ui.horizontal(|ui| {
                ui.label(if self.print.landscape { "Legendenstreifen rechts (mm):" } else { "Legendenstreifen unten (mm):" });
                ui.add(egui::DragValue::new(&mut self.print.strip).speed(1.0).range(25.0..=120.0));
            });
            ui.weak("Rand mindestens 10 mm rundum; unten (Hochformat) bzw. rechts (Querformat) zusätzlich der Legendenstreifen.");
            ui.separator();
            ui.horizontal(|ui| {
                ui.label("Maßstab jetzt:");
                ui.strong(format!("1:{}", group_thousands(n_now.round().max(1.0) as u64)));
            });
            ui.horizontal(|ui| {
                ui.label("Maßstab 1:");
                ui.add(egui::DragValue::new(&mut self.print.scale_edit).speed(10.0).range(1.0..=10_000_000.0));
                if ui.button("setzen").clicked() {
                    set_scale = true;
                }
                if ui.button("Normmaßstab").on_hover_text("nächster gebräuchlicher Maßstab (z. B. 1:500, 1:1000)").clicked() {
                    standard = true;
                }
            });
            if ui.button("Modell auf das Blatt einpassen").clicked() {
                fit = true;
            }
            ui.separator();
            ui.weak("Mit Mausrad und Ziehen den Ausschnitt wählen. Gedruckt werden die eingeschalteten Ebenen, das Koordinatengitter, bei einer Analyse deren Darstellung; dazu Legende, Maßstab und Nordpfeil.");
            ui.add_space(4.0);
            if ui.add_sized([ui.available_width(), 30.0], egui::Button::new("PDF erzeugen…")).clicked() {
                do_pdf = true;
            }
        });
        if standard {
            self.print.scale_edit = nearest_standard_scale(n_now);
            set_scale = true;
        }
        if set_scale && self.print.scale_edit >= 1.0 {
            let (area, _) = self.print_area_on_screen(rect);
            let target_ppm = 1000.0 * self.page_on_screen(rect).1 as f64 / self.print.scale_edit;
            self.view.zoom_at(rect, area.center(), target_ppm / self.view.ppm);
        }
        if fit {
            self.fit_print_area();
        }
        if do_pdf {
            let name = self.project.as_ref().map(|p| p.st.name.clone()).unwrap_or_default();
            let mut dlg = rfd::FileDialog::new().add_filter("PDF", &["pdf"]).set_file_name(format!("{}.pdf", name));
            if !self.settings.data_dir.is_empty() && Path::new(&self.settings.data_dir).is_dir() {
                dlg = dlg.set_directory(&self.settings.data_dir);
            }
            if let Some(p) = dlg.save_file() {
                self.export_pdf(&p);
            }
        }
        self.print.open = open;
    }

    fn dxf_dialog(&mut self, ctx: &egui::Context) {
        if !self.dxf.open {
            return;
        }
        let mut open = true;
        let mut write = false;
        let mut params_btn = false;
        let plot_scale = self.project.as_ref().map_or(1000.0, |p| p.st.params.scale);
        egui::Window::new("DXF-Export der Bildschirmdarstellung").open(&mut open).default_width(380.0).default_pos(Pos2::new((ctx.content_rect().right() - 410.0).max(250.0), 70.0)).show(ctx, |ui| {
            ui.label("Exportiert wird der sichtbare Ausschnitt mit den eingeschalteten Ebenen (Punkte, Höhenlinien, Gitter, Dreiecke, Analyse …).");
            ui.separator();
            ui.horizontal(|ui| {
                ui.label("Maßstabszahl für Schrift und Kreise 1:");
                ui.add(egui::DragValue::new(&mut self.dxf.scale).speed(10.0).range(0.0..=10_000_000.0));
            });
            ui.weak(format!("0 = Maßstabszahl Plott der Parameterdatei ({}). Schrifthöhen und Kreisradien gelten in mm auf dem Papier.", plot_scale));
            ui.checkbox(&mut self.dxf.fills, "Farbflächen (Höhenfarben / Analyse) als SOLID mitschreiben");
            ui.separator();
            ui.strong("Ebenen im Export (wie in der Ansicht, gelten auch dort):");
            {
                let l = &mut self.settings.layers;
                ui.horizontal_wrapped(|ui| {
                    ui.checkbox(&mut l.points, "Punkte (Kreise)");
                    ui.add_enabled(l.points, egui::Checkbox::new(&mut l.numbers, "Punktnummern"));
                    ui.add_enabled(l.points, egui::Checkbox::new(&mut l.heights, "Punkthöhen"));
                });
                ui.horizontal_wrapped(|ui| {
                    ui.checkbox(&mut l.grid, "Koordinatengitter");
                    ui.checkbox(&mut l.contours, "Höhenlinien");
                    ui.add_enabled(l.contours, egui::Checkbox::new(&mut l.labels, "Beschriftung"));
                    ui.checkbox(&mut l.triangles, "Dreiecke");
                    ui.checkbox(&mut l.breaklines, "Bruchkanten");
                    ui.checkbox(&mut l.rings, "Umring");
                    ui.checkbox(&mut l.water, "Wasserflächen");
                    ui.checkbox(&mut l.plan, "Grundriss");
                });
                if !l.points {
                    ui.colored_label(Color32::from_rgb(190, 90, 0), "Punkte sind ausgeschaltet: es werden keine Punktkreise, Nummern und Höhen geschrieben.");
                }
            }
            ui.weak("Im DXF wird jeder Punkt beschriftet: Nummer rechts oberhalb, Höhe rechts vom Punkt, je auf einer eigenen Ebene.");
            ui.separator();
            if let Some(pr) = &self.project {
                let p = &pr.st.params;
                ui.strong("Aus der Parameterdatei:");
                egui::Grid::new("dxf_par").num_columns(2).show(ui, |ui| {
                    ui.label("Radius der Punktkreise");
                    ui.label(format!("{:.2} mm", p.exp_circle_mm));
                    ui.end_row();
                    ui.label("Schrifthöhe Punktnummer / Höhe");
                    ui.label(format!("{:.2} / {:.2} mm", p.exp_text_nr_mm, p.exp_text_h_mm));
                    ui.end_row();
                    ui.label("Schrifthöhe Zähllinien / Gitter");
                    ui.label(format!("{:.2} / {:.2} mm", p.exp_text_index_mm, p.exp_text_grid_mm));
                    ui.end_row();
                    ui.label("Abstand Gitterkreuze");
                    ui.label(format!("{:.0} mm", p.grid_mm));
                    ui.end_row();
                });
                ui.strong("Ebenen (CAD-Ebenen der Parameterdatei, Zeilen 25–37):");
                egui::Grid::new("dxf_layers").num_columns(4).show(ui, |ui| {
                    for (k, (name, i)) in [
                        ("Punktkreise", 0),
                        ("Punktnummern", 2),
                        ("Punkthöhen", 3),
                        ("Koordinatengitter", 1),
                        ("Höhenlinien", 5),
                        ("Zähllinien", 6),
                        ("Zähllinien-Beschriftung", 8),
                        ("Umring / Inringe", 4),
                        ("Bruchkanten", 9),
                        ("Grundriss", 7),
                        ("Dreiecke", 11),
                        ("Differenz (Analyse)", 10),
                        ("Farbflächen", 12),
                    ]
                    .iter()
                    .enumerate()
                    {
                        ui.label(*name);
                        ui.monospace(format!("{}", p.layers[*i]));
                        if k % 2 == 1 {
                            ui.end_row();
                        }
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    for (i, name) in crate::model::EXP_COLOR_NAMES.iter().enumerate() {
                        let c = render::caddy(p.exp_colors[i]);
                        let (r, _) = ui.allocate_exact_size(Vec2::new(12.0, 12.0), egui::Sense::hover());
                        ui.painter().rect_filled(r, 2.0, c);
                        ui.label(format!("{} {}", name, p.exp_colors[i]));
                    }
                });
            }
            ui.horizontal(|ui| {
                if ui.button("Parameter bearbeiten…").clicked() {
                    params_btn = true;
                }
            });
            ui.add_space(4.0);
            if ui.add_sized([ui.available_width(), 30.0], egui::Button::new("DXF schreiben…")).clicked() {
                write = true;
            }
        });
        if params_btn {
            if let Some(pr) = &self.project {
                self.params_edit = pr.st.params.clone();
                self.show_params = true;
            }
        }
        if write {
            let mut dlg = rfd::FileDialog::new().add_filter("DXF", &["dxf"]).set_file_name(self.file_stem() + "-Ansicht.dxf");
            if !self.settings.data_dir.is_empty() && Path::new(&self.settings.data_dir).is_dir() {
                dlg = dlg.set_directory(&self.settings.data_dir);
            }
            if let Some(p) = dlg.save_file() {
                self.export_dxf_screen(&p);
            }
        }
        self.dxf.open = open;
    }

    fn save_settings(&self) {
        if self.screenshot.is_some() {
            return;
        }
        if let Some(p) = settings_path() {
            if let Some(d) = p.parent() {
                let _ = std::fs::create_dir_all(d);
            }
            if let Ok(s) = serde_json::to_string_pretty(&self.settings) {
                let _ = std::fs::write(p, s);
            }
        }
    }

    /// Autosave: the whole project state as JSON next to the project files, written atomically,
    /// the previous state kept as .bak. Never touches the original GM files.
    fn autosave(&mut self) -> bool {
        if self.screenshot.is_some() {
            return false;
        }
        let Some(pr) = &mut self.project else { return false };
        if pr.st.name == "Beispiel" && pr.dir == std::env::temp_dir() {
            self.dirty = false;
            return false;
        }
        pr.st.saved_unix = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let path = pr.autosave_path();
        let Ok(json) = serde_json::to_string(&pr.st) else { return false };
        let tmp = path.with_extension("json.tmp");
        let ok = std::fs::write(&tmp, json).is_ok() && {
            if path.exists() {
                let _ = std::fs::copy(&path, path.with_extension("json.bak"));
            }
            std::fs::rename(&tmp, &path).is_ok()
        };
        self.last_save = Instant::now();
        if ok {
            self.dirty = false;
            self.last_save_text = format!("gespeichert {}", clock());
        } else {
            self.last_save_text = "Autospeichern fehlgeschlagen!".into();
        }
        ok
    }

    fn checkpoint(&mut self) {
        if let Some(pr) = &self.project {
            self.undo.push(pr.st.clone());
            if self.undo.len() > 40 {
                self.undo.remove(0);
            }
            self.redo.clear();
        }
        self.dirty = true;
    }

    fn undo(&mut self) {
        if let (Some(prev), Some(pr)) = (self.undo.pop(), &mut self.project) {
            self.redo.push(std::mem::replace(&mut pr.st, prev));
            pr.rebuild_index();
            self.dirty = true;
            self.recompute();
            self.status = "Rückgängig gemacht.".into();
        }
    }

    fn redo(&mut self) {
        if let (Some(next), Some(pr)) = (self.redo.pop(), &mut self.project) {
            self.undo.push(std::mem::replace(&mut pr.st, next));
            pr.rebuild_index();
            self.dirty = true;
            self.recompute();
            self.status = "Wiederhergestellt.".into();
        }
    }

    /// Mehr Punkte als die Rechengrenze (Parameter): alle weiteren Berechnungen sind abgeschaltet.
    fn calc_blocked(&self) -> bool {
        self.project.as_ref().map_or(false, |p| p.st.points.len() > p.st.params.max_calc_points as usize)
    }

    fn blocked_text(&self) -> String {
        let (n, max) = self.project.as_ref().map_or((0, 0), |p| (p.st.points.len(), p.st.params.max_calc_points));
        format!("{} Punkte: mehr als die Rechengrenze von {} Punkten – Berechnungen abgeschaltet (Rechengrenze in den Parametern, oder Ausschnitt mit Koordinatenfenster speichern).", n, max)
    }

    /// Automatisches Vermaschen nach einer Bearbeitung; bei zu vielen Punkten nur Hinweis.
    fn remesh(&mut self) {
        if self.project.is_none() {
            return;
        }
        if self.calc_blocked() {
            self.clear_model();
            self.status = self.blocked_text();
        } else if self.auto_off {
            self.status = format!("Mehr als {} Punkte: automatische Vermaschung aus – Bearbeiten → Vermaschen.", AUTO_CALC_LIMIT_POINTS);
        } else {
            self.remesh_now();
        }
    }

    /// Vermaschen auf ausdrücklichen Wunsch (Menü): schaltet die Automatik wieder ein.
    fn remesh_manual(&mut self) {
        if self.calc_blocked() {
            self.clear_model();
            self.status = self.blocked_text();
            return;
        }
        self.auto_off = false;
        self.remesh_now();
    }

    /// Vermaschen und Höhenlinien rechnen (keeps triangles the person deleted by hand deleted).
    /// Große Punktmengen laufen im Hintergrund mit Fortschrittsbalken.
    fn remesh_now(&mut self) {
        let (fill_mode, shade) = (self.settings.layers.fill_mode(), self.settings.layers.shade);
        let Some(pr) = &mut self.project else { return };
        if pr.st.points.len() <= SYNC_MESH_POINTS {
            let (tris, rep) = mesh_keeping_deleted(pr, &mut |_, _| {});
            pr.st.triangles = tris;
            self.status = mesh_message(&rep);
            self.recompute();
            return;
        }
        let mut snapshot = Project { dir: pr.dir.clone(), st: pr.st.clone(), index: Default::default() };
        self.spawn_job("Vermaschen", move |prog| {
            let (tris, rep) = mesh_keeping_deleted(&snapshot, &mut |f, st| prog.set(f, st));
            snapshot.st.triangles = tris;
            prog.set_indeterminate("Geländemodell und Höhenlinien berechnen");
            let model = build_model(&snapshot, fill_mode, shade);
            Ok(JobDone::Meshed(Box::new(Meshed { triangles: snapshot.st.triangles, report: rep, model })))
        });
    }

    fn finish_mesh(&mut self, m: Meshed) {
        let Meshed { triangles, report, model } = m;
        if let Some(pr) = &mut self.project {
            pr.st.triangles = triangles;
        }
        self.apply_model(model);
        self.status = mesh_message(&report);
    }

    fn apply_model(&mut self, m: Model) {
        let zmin = m.surface.zmin;
        self.fill = m.fill;
        self.contours = Some(m.contours);
        self.surface = Some(m.surface);
        self.surf_id += 1;
        self.volume_res = None;
        if self.volume_ref == 0.0 {
            self.volume_ref = zmin.floor();
        }
    }

    fn clear_model(&mut self) {
        self.fill = None;
        self.contours = None;
        self.surface = None;
        self.surf_id += 1;
        self.volume_res = None;
    }

    /// Model + contours + fill from the current triangles (nicht bei abgeschalteter Automatik/Rechengrenze).
    fn recompute(&mut self) {
        let Some(pr) = &self.project else { return };
        if self.calc_blocked() {
            self.clear_model();
            return;
        }
        if self.auto_off {
            return;
        }
        let m = build_model(pr, self.settings.layers.fill_mode(), self.settings.layers.shade);
        self.apply_model(m);
    }

    // -------------------------------------------------------------- Hintergrundarbeit: Abfrage und Fenster

    fn handle_job_result(&mut self, res: Result<JobDone, String>) {
        match res {
            Ok(JobDone::Loaded(l)) => self.finish_load(*l),
            Ok(JobDone::Meshed(m)) => self.finish_mesh(*m),
            Ok(JobDone::Converted { result, inputs, output }) => {
                self.status = match result {
                    Ok(count) => {
                        let mut s = format!(
                            "{} GEO-TIFF{}: {} Punkte nach {} geschrieben.",
                            inputs,
                            if inputs == 1 { "" } else { "s" },
                            count,
                            output.display()
                        );
                        if count > WARN_POINTS {
                            s += &format!(" Warnung: mehr als {} Punkte – beim Einlesen sind Vermaschung und Höhenlinien nicht automatisch; besser einen Ausschnitt wählen.", WARN_POINTS);
                        }
                        s
                    }
                    Err(e) => format!("GEO-TIFF-Import fehlgeschlagen: {}", e),
                };
            }
            Err(e) => {
                if self.project.is_none() {
                    self.load_demo();
                }
                self.status = e;
            }
        }
    }

    /// Ergebnis einer fertigen Hintergrundarbeit übernehmen; sonst weiter auf Fortschritt warten.
    fn poll_job(&mut self, ctx: &egui::Context) {
        let Some(job) = &self.job else { return };
        match job.rx.try_recv() {
            Ok(res) => {
                self.job = None;
                self.handle_job_result(res);
            }
            Err(mpsc::TryRecvError::Empty) => ctx.request_repaint_after(Duration::from_millis(50)),
            Err(mpsc::TryRecvError::Disconnected) => {
                self.job = None;
                self.handle_job_result(Err("Die Arbeit wurde unerwartet abgebrochen.".into()));
            }
        }
    }

    /// Auf eine laufende Hintergrundarbeit warten (Testmodus und Startoptionen).
    fn wait_job(&mut self) {
        if let Some(job) = self.job.take() {
            let res = job.rx.recv().unwrap_or_else(|_| Err("Die Arbeit wurde unerwartet abgebrochen.".into()));
            self.handle_job_result(res);
        }
    }

    /// Fortschrittsfenster: sperrt die Bedienung, solange eine Hintergrundarbeit läuft.
    fn job_window(&mut self, ctx: &egui::Context) {
        let Some(job) = &self.job else { return };
        egui::Modal::new(egui::Id::new("job_modal")).show(ctx, |ui| {
            ui.set_width(420.0);
            ui.heading(&job.title);
            ui.add_space(6.0);
            ui.label(job.progress.stage());
            match job.progress.fraction() {
                Some(f) => {
                    ui.add(egui::ProgressBar::new(f).show_percentage());
                }
                None => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label("bitte warten …");
                    });
                }
            }
        });
    }

    /// Hinweis-/Warnfenster (z. B. mehr als 50000 Punkte eingelesen).
    fn notice_window(&mut self, ctx: &egui::Context) {
        let Some(text) = &self.notice else { return };
        let mut close = false;
        egui::Window::new("Warnung").collapsible(false).resizable(false).anchor(Align2::CENTER_CENTER, Vec2::ZERO).show(ctx, |ui| {
            ui.set_max_width(480.0);
            ui.label(text);
            ui.add_space(6.0);
            if ui.button("OK").clicked() {
                close = true;
            }
        });
        if close {
            self.notice = None;
        }
    }

    // -------------------------------------------------------------- Koordinatenfenster (Ausschnitt)

    /// Koordinatenfenster am Bildschirm digitalisieren; die Werte erscheinen in der Parametermaske.
    fn start_window_pick(&mut self) {
        let Some(pr) = &self.project else { return };
        if !self.show_params {
            self.params_edit = pr.st.params.clone();
            self.show_params = true;
        }
        self.set_tool(Tool::WindowPick);
        self.win_first = None;
    }

    // -------------------------------------------------------------- Analyse: DXF mit Höhen

    /// Quelldatei wählen und einlesen.
    fn dxfz_choose(&mut self) {
        let mut dlg = rfd::FileDialog::new().add_filter("DXF", &["dxf", "DXF"]).set_title("DXF-Datei ohne Höhen wählen");
        if !self.settings.data_dir.is_empty() && Path::new(&self.settings.data_dir).is_dir() {
            dlg = dlg.set_directory(&self.settings.data_dir);
        }
        let Some(path) = dlg.pick_file() else { return };
        match std::fs::read(&path) {
            Ok(bytes) => {
                self.dxfz.source = Some((path, bytes));
                self.dxfz_compute();
            }
            Err(e) => {
                self.dxfz.source = None;
                self.dxfz.result = None;
                self.dxfz.message = format!("Datei nicht lesbar: {}", e);
            }
        }
    }

    /// Höhen aus dem Dreiecksmodell (lineare Interpolation) eintragen.
    fn dxfz_compute(&mut self) {
        self.dxfz.result = None;
        let Some((_, bytes)) = &self.dxfz.source else { return };
        let tin = self.project.as_ref().filter(|_| !self.calc_blocked()).and_then(dxf_z::Tin::from_project);
        let Some(tin) = tin else {
            self.dxfz.message = "Kein Geländemodell: zuerst Dreiecke bilden (Bearbeiten → Vermaschen).".into();
            return;
        };
        match dxf_z::add_heights(bytes, &tin, self.dxfz.replace_zero) {
            Ok((out, rep)) => {
                self.dxfz.message = rep.summary();
                self.dxfz.result = Some((out, rep));
            }
            Err(e) => self.dxfz.message = e,
        }
    }

    fn dxfz_save(&mut self) {
        let (Some((src, _)), Some((bytes, rep))) = (&self.dxfz.source, &self.dxfz.result) else { return };
        let stem = src.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "zeichnung".into());
        let mut dlg = rfd::FileDialog::new().add_filter("DXF", &["dxf"]).set_file_name(format!("{}-Z.dxf", stem));
        if let Some(d) = src.parent() {
            dlg = dlg.set_directory(d);
        }
        let Some(path) = dlg.save_file() else { return };
        let same = path == *src || matches!((path.canonicalize(), src.canonicalize()), (Ok(a), Ok(b)) if a == b);
        if same {
            self.dxfz.message = "Die Originaldatei wird nicht überschrieben – bitte anderen Dateinamen wählen.".into();
            return;
        }
        self.dxfz.message = match std::fs::write(&path, bytes) {
            Ok(_) => {
                self.status = format!("DXF mit Höhen gespeichert: {} ({} Elemente mit Höhen).", path.display(), rep.changed);
                format!("Gespeichert: {}", path.display())
            }
            Err(e) => format!("Speichern fehlgeschlagen: {}", e),
        };
    }

    fn dxfz_window(&mut self, ctx: &egui::Context) {
        if !self.dxfz.open {
            return;
        }
        let mut open = true;
        let (mut choose, mut recompute, mut save) = (false, false, false);
        egui::Window::new("DXF-Datei mit Höhen versehen").open(&mut open).default_width(460.0).show(ctx, |ui| {
            ui.label("Punkte ohne Z-Koordinate erhalten ihre Höhe durch lineare Interpolation im Dreiecksmodell des geladenen Projekts (gleiches Koordinatensystem vorausgesetzt).");
            ui.add_space(4.0);
            if ui.button("DXF-Datei wählen…").clicked() {
                choose = true;
            }
            if let Some((p, b)) = &self.dxfz.source {
                ui.label(format!("{}  ({} KB)", p.display(), b.len() / 1024));
            }
            if ui.checkbox(&mut self.dxfz.replace_zero, "auch Punkte mit Z = 0 neu berechnen").on_hover_text("Viele CAD-Programme schreiben für 2D-Zeichnungen ausdrücklich Z = 0").changed() {
                recompute = true;
            }
            ui.separator();
            if !self.dxfz.message.is_empty() {
                ui.label(&self.dxfz.message);
            }
            if let Some((_, rep)) = &self.dxfz.result {
                if rep.outside > 0 {
                    ui.weak("Elemente mit Punkten außerhalb des Modells bleiben unverändert.");
                }
            }
            ui.add_enabled_ui(self.dxfz.result.is_some(), |ui| {
                if ui.button("Verbesserte DXF-Datei speichern…").clicked() {
                    save = true;
                }
            });
        });
        self.dxfz.open = open;
        if choose {
            self.dxfz_choose();
        }
        if recompute {
            self.dxfz_compute();
        }
        if save {
            self.dxfz_save();
        }
    }

    /// Punkte im Koordinatenfenster als neue GM1-Datei speichern.
    fn save_window_gm1(&mut self) {
        let Some(pr) = &self.project else { return };
        let w = self.params_edit.win;
        let (x0, x1, y0, y1) = (w[0].min(w[2]), w[0].max(w[2]), w[1].min(w[3]), w[1].max(w[3]));
        if !(x1 > x0 && y1 > y0) {
            self.status = "Koordinatenfenster ungültig: die beiden Ecken müssen verschiedene Rechts- und Hochwerte haben.".into();
            return;
        }
        let inside = |p: &&Point| p.active && p.x >= x0 && p.x <= x1 && p.y >= y0 && p.y <= y1;
        let count = pr.st.points.iter().filter(inside).count();
        if count == 0 {
            self.status = "Im Koordinatenfenster liegen keine (aktiven) Punkte.".into();
            return;
        }
        let mut dlg = rfd::FileDialog::new()
            .add_filter("GM1-Punktdatei", &["gm1"])
            .set_title("Ausschnitt als GM1 speichern")
            .set_file_name(format!("{}-Ausschnitt.gm1", pr.st.name))
            .set_directory(&pr.dir);
        if !pr.dir.is_dir() {
            dlg = rfd::FileDialog::new().add_filter("GM1-Punktdatei", &["gm1"]).set_file_name(format!("{}-Ausschnitt.gm1", pr.st.name));
        }
        let Some(path) = dlg.save_file() else { return };
        // die geladene Punktdatei darf nicht durch den Ausschnitt überschrieben werden
        let same = files::find_ext(&pr.dir, &pr.st.name, "gm1")
            .map_or(false, |g| g == path || matches!((g.canonicalize(), path.canonicalize()), (Ok(a), Ok(b)) if a == b));
        if same {
            self.status = "Der Ausschnitt darf nicht die geladene Punktdatei überschreiben – anderen Dateinamen wählen.".into();
            return;
        }
        let (sxy, sh) = (pr.st.params.scale_xy, pr.st.params.scale_h);
        self.status = match files::write_gm1_points(&path, pr.st.points.iter().filter(inside), sxy, sh) {
            Ok(n) => format!("Ausschnitt: {} von {} Punkten nach {} geschrieben (Datei → Punktdatei öffnen).", n, pr.st.points.len(), path.display()),
            Err(e) => format!("GM1-Fehler: {}", e),
        };
    }

    fn rebuild_fill(&mut self) {
        if let (Some(s), Some(pr)) = (&self.surface, &self.project) {
            self.fill = FillCache::build(s, self.settings.layers.fill_mode(), self.settings.layers.shade, (pr.st.params.subdivision as usize).clamp(1, 4), (s.zmin, s.zmax));
        }
    }

    fn fit(&mut self, animate: bool) {
        if let Some(b) = self.project.as_ref().and_then(|p| p.bbox()) {
            let r = self.map_rect;
            self.view.fit(r, b, 30.0, animate);
        }
    }

    // -------------------------------------------------------------- hit tests

    fn catch_px(&self) -> f64 {
        10.0
    }

    fn nearest_point(&self, w: (f64, f64), only_active: bool) -> Option<usize> {
        let pr = self.project.as_ref()?;
        let r = self.catch_px() / self.view.ppm;
        let mut best: Option<(f64, usize)> = None;
        for (i, p) in pr.st.points.iter().enumerate() {
            if only_active && !p.active {
                continue;
            }
            let d = (p.x - w.0).hypot(p.y - w.1);
            if d <= r && best.map_or(true, |b| d < b.0) {
                best = Some((d, i));
            }
        }
        best.map(|b| b.1)
    }

    fn triangle_at(&self, w: (f64, f64)) -> Option<usize> {
        let pr = self.project.as_ref()?;
        let pts = &pr.st.points;
        pr.st.triangles.iter().position(|t| {
            let (a, b, c) = (&pts[t.v[0]], &pts[t.v[1]], &pts[t.v[2]]);
            let s1 = (b.x - a.x) * (w.1 - a.y) - (b.y - a.y) * (w.0 - a.x);
            let s2 = (c.x - b.x) * (w.1 - b.y) - (c.y - b.y) * (w.0 - b.x);
            let s3 = (a.x - c.x) * (w.1 - c.y) - (a.y - c.y) * (w.0 - c.x);
            (s1 >= 0.0 && s2 >= 0.0 && s3 >= 0.0) || (s1 <= 0.0 && s2 <= 0.0 && s3 <= 0.0)
        })
    }

    /// nearest polyline (list index) within the catch radius
    fn nearest_line(&self, lines: &[Vec<usize>], closed: bool, w: (f64, f64)) -> Option<usize> {
        let pr = self.project.as_ref()?;
        let r = self.catch_px() / self.view.ppm;
        let mut best: Option<(f64, usize)> = None;
        for (li, l) in lines.iter().enumerate() {
            let n = l.len();
            let segs = if closed { n } else { n.saturating_sub(1) };
            for k in 0..segs {
                let a = &pr.st.points[l[k]];
                let b = &pr.st.points[l[(k + 1) % n]];
                let d = seg_dist(w, (a.x, a.y), (b.x, b.y));
                if d <= r && best.map_or(true, |bb| d < bb.0) {
                    best = Some((d, li));
                }
            }
        }
        best.map(|b| b.1)
    }

    // -------------------------------------------------------------- map clicks

    fn on_click(&mut self, w: (f64, f64), ctrl: bool) {
        if ctrl {
            self.view.animate_to(w.0, w.1, self.view.ppm);
            return;
        }
        if self.project.is_none() {
            return;
        }
        match self.tool {
            Tool::View | Tool::ZoomRect => {}
            Tool::WindowPick => match self.win_first.take() {
                None => {
                    self.win_first = Some([w.0, w.1]);
                    self.status = format!("Fenster: 1. Ecke R {:.3}  H {:.3} – jetzt die gegenüberliegende Ecke anklicken.", w.0, w.1);
                }
                Some(a) => {
                    self.params_edit.win = [a[0].min(w.0), a[1].min(w.1), a[0].max(w.0), a[1].max(w.1)];
                    self.set_tool(Tool::View);
                    self.status = format!(
                        "Koordinatenfenster: R {:.3} – {:.3}, H {:.3} – {:.3} (in der Maske „Parameter“ eingetragen).",
                        self.params_edit.win[0], self.params_edit.win[2], self.params_edit.win[1], self.params_edit.win[3]
                    );
                }
            },
            Tool::PtToggle => {
                if let Some(i) = self.nearest_point(w, false) {
                    self.checkpoint();
                    let pr = self.project.as_mut().unwrap();
                    pr.st.points[i].active = !pr.st.points[i].active;
                    let on = pr.st.points[i].active;
                    let name = pr.st.points[i].name.clone();
                    self.remesh();
                    self.status = format!("Punkt {} {}.", name, if on { "aktiviert" } else { "gelöscht" });
                }
            }
            Tool::PtAddLine | Tool::PtAddFree => {
                if self.pick.len() < 2 {
                    if let Some(i) = self.nearest_point(w, true) {
                        if !self.pick.contains(&i) {
                            self.pick.push(i);
                        }
                    }
                } else {
                    let pr = self.project.as_ref().unwrap();
                    let (a, b) = (&pr.st.points[self.pick[0]], &pr.st.points[self.pick[1]]);
                    let (dx, dy) = (b.x - a.x, b.y - a.y);
                    let l2 = (dx * dx + dy * dy).max(1e-12);
                    let t = (((w.0 - a.x) * dx + (w.1 - a.y) * dy) / l2).clamp(0.0, 1.0);
                    let (x, y) = if self.tool == Tool::PtAddLine { (a.x + dx * t, a.y + dy * t) } else { w };
                    let z = a.z + (b.z - a.z) * t;
                    let name = self.new_point_name();
                    self.checkpoint();
                    let pr = self.project.as_mut().unwrap();
                    pr.st.points.push(Point { name: name.clone(), x, y, z, code: String::new(), active: true, label_pos: 0, added: true });
                    pr.rebuild_index();
                    self.remesh();
                    self.status = format!("Punkt {} erzeugt, Höhe {:.3} m.", name, z);
                }
            }
            Tool::TriToggle => {
                if let Some(t) = self.triangle_at(w) {
                    self.checkpoint();
                    let pr = self.project.as_mut().unwrap();
                    pr.st.triangles[t].active = !pr.st.triangles[t].active;
                    let on = pr.st.triangles[t].active;
                    self.recompute();
                    self.status = format!("Dreieck {} {}.", t + 1, if on { "aktiviert" } else { "gelöscht" });
                }
            }
            Tool::TriFlip => self.flip_at(w),
            Tool::TriCreate => {
                if let Some(i) = self.nearest_point(w, true) {
                    if !self.pick.contains(&i) {
                        self.pick.push(i);
                    }
                    if self.pick.len() == 3 {
                        self.checkpoint();
                        let v = [self.pick[0], self.pick[1], self.pick[2]];
                        let pr = self.project.as_mut().unwrap();
                        pr.st.triangles.push(crate::model::Tri { v, active: true });
                        self.pick.clear();
                        self.recompute();
                        self.status = "Dreieck erzeugt.".into();
                    }
                }
            }
            Tool::BkCreate { .. } | Tool::RingCreate | Tool::WaterCreate | Tool::PlanCreate => {
                if let Some(i) = self.nearest_point(w, true) {
                    if self.pick.last() != Some(&i) {
                        self.pick.push(i);
                    }
                }
            }
            Tool::BkDelete | Tool::BkSoft | Tool::BkShow => {
                let lines: Vec<Vec<usize>> = self.project.as_ref().unwrap().st.breaklines.iter().map(|b| b.pts.clone()).collect();
                if let Some(li) = self.nearest_line(&lines, false, w) {
                    match self.tool {
                        Tool::BkDelete => {
                            self.checkpoint();
                            self.project.as_mut().unwrap().st.breaklines.remove(li);
                            self.remesh();
                            self.status = "Bruchkante gelöscht.".into();
                        }
                        Tool::BkSoft => {
                            self.checkpoint();
                            let b = &mut self.project.as_mut().unwrap().st.breaklines[li];
                            b.soft = !b.soft;
                            let soft = b.soft;
                            self.recompute();
                            self.status = format!("Bruchkante ist jetzt {}.", if soft { "weich" } else { "hart" });
                        }
                        _ => {
                            self.blink = Some((li, Instant::now()));
                            let n = self.project.as_ref().unwrap().st.breaklines[li].pts.len();
                            self.status = format!("Bruchkante {}: {} Punkte.", li + 1, n);
                        }
                    }
                }
            }
            Tool::RingDelete => {
                let pr = self.project.as_ref().unwrap();
                let mut lines = vec![pr.st.umring.clone()];
                lines.extend(pr.st.inrings.iter().cloned());
                if let Some(li) = self.nearest_line(&lines, true, w) {
                    self.checkpoint();
                    let pr = self.project.as_mut().unwrap();
                    let msg = if li == 0 {
                        pr.st.umring.clear();
                        "Umring gelöscht."
                    } else {
                        pr.st.inrings.remove(li - 1);
                        "Inring gelöscht."
                    };
                    self.remesh();
                    self.status = msg.into();
                }
            }
            Tool::PlanDelete => {
                let lines: Vec<Vec<usize>> = self.project.as_ref().unwrap().st.plan_lines.iter().map(|b| b.pts.clone()).collect();
                if let Some(li) = self.nearest_line(&lines, false, w) {
                    self.checkpoint();
                    self.project.as_mut().unwrap().st.plan_lines.remove(li);
                    self.status = "Grundrisslinie gelöscht.".into();
                }
            }
            Tool::HeightPick => {
                let z = self.surface.as_ref().and_then(|s| s.height(w.0, w.1));
                let st = self.heights.last().map(|h| h.4 + (h.1 - w.0).hypot(h.2 - w.1)).unwrap_or(0.0);
                let n = format!("{}", self.heights.len() + 1);
                self.heights.push((n, w.0, w.1, z, st));
                self.show_heights = true;
                self.status = match z {
                    Some(z) => format!("Höhe {:.3} m bei R {:.3} H {:.3}", z, w.0, w.1),
                    None => "Außerhalb des Modells.".into(),
                };
            }
            Tool::Profile => self.pick_world.push([w.0, w.1]),
            Tool::LabelManual => {
                let mut label = None;
                if let (Some(s), Some(c)) = (&self.surface, &self.contours) {
                    if let Some(z) = s.height(w.0, w.1) {
                        let lv = (z / c.interval).round() * c.interval;
                        let g = s.gradient(w.0, w.1, 0.5).unwrap_or([1.0, 0.0]);
                        let ang = (-g[0]).atan2(g[1]) as f32; // along the contour
                        label = Some(ManualLabel { x: w.0, y: w.1, z: lv, angle: ang });
                    }
                }
                if let Some(l) = label {
                    self.checkpoint();
                    let z = l.z;
                    self.project.as_mut().unwrap().st.manual_labels.push(l);
                    self.status = format!("Beschriftung {:.0} gesetzt.", z);
                }
            }
            Tool::PNrPos => {
                if let Some(i) = self.nearest_point(w, false) {
                    self.checkpoint();
                    let p = &mut self.project.as_mut().unwrap().st.points[i];
                    p.label_pos = (p.label_pos + 1) % 4;
                }
            }
        }
    }

    /// right mouse button: finish the current polyline / action (as in the original)
    fn on_secondary(&mut self) {
        match self.tool {
            Tool::BkCreate { soft } if self.pick.len() >= 2 => {
                self.checkpoint();
                let pts = std::mem::take(&mut self.pick);
                self.project.as_mut().unwrap().st.breaklines.push(Polyline { pts, soft });
                self.remesh();
                self.status = format!("Bruchkante gespeichert ({}).", if soft { "weich" } else { "hart" });
            }
            Tool::RingCreate if self.pick.len() >= 3 => {
                self.checkpoint();
                let pts = std::mem::take(&mut self.pick);
                let pr = self.project.as_mut().unwrap();
                // direction decides (as in the person's data): clockwise = Umring, counter-clockwise = Inring
                let msg = if pr.ring_area(&pts) < 0.0 {
                    pr.st.umring = pts;
                    "Umring (im Uhrzeigersinn) gespeichert."
                } else {
                    pr.st.inrings.push(pts);
                    "Inring (gegen den Uhrzeigersinn) gespeichert."
                };
                self.remesh();
                self.status = msg.into();
            }
            Tool::WaterCreate if self.pick.len() >= water::MIN_WATER_POINTS => {
                let pts = std::mem::take(&mut self.pick);
                let count = self.project.as_ref().map_or(0, |p| p.st.waters.len());
                if count >= water::MAX_WATERS {
                    self.status = format!("Es sind höchstens {} Wasserflächen möglich.", water::MAX_WATERS);
                    return;
                }
                self.checkpoint();
                let pr = self.project.as_mut().unwrap();
                let w = Water { name: format!("Wasserfläche {}", count + 1), pts };
                let rep = water::analyze(&pr.st.points, &w);
                pr.st.waters.push(w);
                self.water_sel = count;
                self.show_water = true;
                self.status = if rep.fit.is_none() {
                    "Wasserfläche gespeichert, aber keine Ebene möglich (Punkte liegen auf einer Geraden).".into()
                } else if rep.outliers > 0 {
                    format!("Wasserfläche gespeichert. WARNUNG: {} Punkt(e) weichen mehr als {:.0} cm von der Ebene ab – siehe Fenster Wasserflächen.", rep.outliers, water::WATER_TOL_M * 100.0)
                } else {
                    format!("Wasserfläche gespeichert (größte Abweichung von der Ebene {:.1} cm).", rep.max_abs * 100.0)
                };
            }
            Tool::PlanCreate if self.pick.len() >= 2 => {
                self.checkpoint();
                let pts = std::mem::take(&mut self.pick);
                self.project.as_mut().unwrap().st.plan_lines.push(Polyline { pts, soft: false });
                self.status = "Grundrisslinie gespeichert.".into();
            }
            Tool::Profile if self.pick_world.len() >= 2 => {
                if let (Some(s), false) = (&self.surface, self.profile_quer) {
                    self.profile = analysis::profile(s, &self.pick_world, self.profile_step);
                    self.show_profile = true;
                }
                if self.project.is_some() {
                    self.checkpoint();
                    let line = std::mem::take(&mut self.pick_world);
                    let view = if self.profile_quer { ProfileView::Quer } else { ProfileView::Laengs };
                    if let Some(pr) = self.project.as_mut() {
                        let name = format!("{}", pr.st.profiles.iter().filter(|d| d.view == view).count() + 1);
                        pr.st.profiles.push(ProfileDef { name, view, line, ..Default::default() });
                        self.profile_sel = pr.st.profiles.len() - 1;
                    }
                    self.show_profiles = true;
                    if view == ProfileView::Laengs {
                        let k = self.project.as_ref().map_or(0, |pr| pr.st.profiles.len().saturating_sub(1));
                        self.open_qs_dialog(k);
                    }
                    self.status = "Profil angelegt – Einstellungen und Ausgabe im Fenster „Profile“.".into();
                }
            }
            _ => {
                self.pick.clear();
                self.pick_world.clear();
                if self.tool != Tool::View && self.tool != Tool::Profile {
                    self.set_tool(Tool::View);
                }
            }
        }
    }

    fn flip_at(&mut self, w: (f64, f64)) {
        let Some(pr) = &self.project else { return };
        let r = self.catch_px() / self.view.ppm;
        let pts = &pr.st.points;
        // find the nearest shared edge of two active triangles
        let mut best: Option<(f64, usize, usize)> = None;
        let mut edge_owner: std::collections::HashMap<(usize, usize), usize> = Default::default();
        for (ti, t) in pr.st.triangles.iter().enumerate() {
            if !t.active {
                continue;
            }
            for e in 0..3 {
                let (a, b) = (t.v[e], t.v[(e + 1) % 3]);
                let k = (a.min(b), a.max(b));
                if let Some(&tj) = edge_owner.get(&k) {
                    let d = seg_dist(w, (pts[a].x, pts[a].y), (pts[b].x, pts[b].y));
                    if d < r && best.map_or(true, |bb| d < bb.0) {
                        best = Some((d, ti, tj));
                    }
                } else {
                    edge_owner.insert(k, ti);
                }
            }
        }
        let Some((_, t1, t2)) = best else { return };
        let a = pr.st.triangles[t1].v;
        let b = pr.st.triangles[t2].v;
        let shared: Vec<usize> = a.iter().filter(|v| b.contains(v)).copied().collect();
        if shared.len() != 2 {
            return;
        }
        let p = *a.iter().find(|v| !shared.contains(v)).unwrap();
        let q = *b.iter().find(|v| !shared.contains(v)).unwrap();
        // only flip when the quad is convex (the new diagonal separates the old shared points)
        let cross = |o: usize, u: usize, v: usize| (pts[u].x - pts[o].x) * (pts[v].y - pts[o].y) - (pts[u].y - pts[o].y) * (pts[v].x - pts[o].x);
        let s1 = cross(p, q, shared[0]);
        let s2 = cross(p, q, shared[1]);
        if s1 * s2 >= 0.0 {
            self.status = "Tauschen nicht möglich (Viereck nicht konvex).".into();
            return;
        }
        self.checkpoint();
        let pr = self.project.as_mut().unwrap();
        pr.st.triangles[t1].v = [p, q, shared[0]];
        pr.st.triangles[t2].v = [q, p, shared[1]];
        self.recompute();
        self.status = "Dreiecke getauscht.".into();
    }

    fn new_point_name(&self) -> String {
        let pr = self.project.as_ref().unwrap();
        let mut n = pr.st.points.iter().filter_map(|p| p.name.parse::<u64>().ok()).max().unwrap_or(0) + 1;
        while pr.index.contains_key(&n.to_string()) {
            n += 1;
        }
        n.to_string()
    }

    fn set_tool(&mut self, t: Tool) {
        self.tool = t;
        self.win_first = None;
        self.pick.clear();
        self.pick_world.clear();
        self.status = t.hint().to_string();
    }

    // -------------------------------------------------------------- exports

    fn export_dxf(&mut self, path: &Path, triangles3d: bool) {
        let (Some(pr), Some(s)) = (&self.project, &self.surface) else { return };
        let mut d = files::DxfWriter::new();
        let p = &pr.st.params;
        let layer = |i: usize| p.layers[i].to_string();
        let mm = p.scale / 1000.0;
        if triangles3d {
            for t in &s.tris {
                let q = |i: usize| [s.pos[i][0] + s.ox, s.pos[i][1] + s.oy, s.pos[i][2]];
                d.face("DREIECKE3D", 6, q(t[0]), q(t[1]), q(t[2]));
            }
        } else {
            for pt in pr.st.points.iter().filter(|p| p.active) {
                d.point(&layer(0), 1, [pt.x, pt.y, pt.z]);
                d.text(&layer(2), 1, [pt.x + 0.3 * mm, pt.y], 1.5 * mm, 0.0, &pt.name);
                d.text(&layer(3), 6, [pt.x + 0.3 * mm, pt.y - 2.0 * mm], 1.5 * mm, 0.0, &format!("{:.*}", p.decimals_h as usize, pt.z));
            }
            if let Some(c) = &self.contours {
                for l in &c.lines {
                    let pts: Vec<[f64; 3]> = l.pts.iter().map(|q| [q[0] + s.ox, q[1] + s.oy, l.z]).collect();
                    d.polyline(&layer(if l.index { 6 } else { 5 }), if l.index { 3 } else { 2 }, &pts, l.closed);
                    for lb in &l.labels {
                        d.text(&layer(8), 3, [lb.pos[0] + s.ox, lb.pos[1] + s.oy], 2.0 * mm, upright(lb.angle.to_degrees()), &format!("{:.*}", p.decimals_index as usize, l.z));
                    }
                }
            }
            for b in &pr.st.breaklines {
                let pts: Vec<[f64; 3]> = b.pts.iter().map(|&i| [pr.st.points[i].x, pr.st.points[i].y, pr.st.points[i].z]).collect();
                d.polyline(&layer(9), 11, &pts, false);
            }
            for r in std::iter::once(&pr.st.umring).chain(pr.st.inrings.iter()).filter(|r| r.len() >= 3) {
                let pts: Vec<[f64; 3]> = r.iter().map(|&i| [pr.st.points[i].x, pr.st.points[i].y, pr.st.points[i].z]).collect();
                d.polyline(&layer(4), 5, &pts, true);
            }
            for t in &s.tris {
                let pts: Vec<[f64; 3]> = t.iter().map(|&i| [s.pos[i][0] + s.ox, s.pos[i][1] + s.oy, s.pos[i][2]]).collect();
                d.polyline(&layer(11), 6, &pts, true);
            }
        }
        self.status = match std::fs::write(path, d.finish()) {
            Ok(_) => format!("DXF geschrieben: {}", path.display()),
            Err(e) => format!("DXF-Fehler: {}", e),
        };
    }

    fn export_svg(&mut self, path: &Path) {
        let (Some(pr), Some(s)) = (&self.project, &self.surface) else { return };
        let vis = self.view.world_rect(self.map_rect);
        let w = vis[2] - vis[0];
        let h = vis[3] - vis[1];
        let ht = 1600.0 * h / w;
        let sx = |x: f64| (x - vis[0]) / w * 1600.0;
        let sy = |y: f64| (vis[3] - y) / h * ht;
        let mut out = format!("<svg xmlns='http://www.w3.org/2000/svg' width='1600' height='{:.0}' viewBox='0 0 1600 {:.0}'><rect width='100%' height='100%' fill='white'/>\n", ht, ht);
        if let Some(c) = &self.contours {
            for l in &c.lines {
                let mut d = String::new();
                for (i, q) in l.pts.iter().enumerate() {
                    d += &format!("{}{:.2},{:.2} ", if i == 0 { "M" } else { "L" }, sx(q[0] + s.ox), sy(q[1] + s.oy));
                }
                if l.closed {
                    d += "Z";
                }
                out += &format!("<path d='{}' fill='none' stroke='{}' stroke-width='{}'/>\n", d, if l.index { "#6e3c0f" } else { "#966428" }, if l.index { 1.6 } else { 0.7 });
                for lb in &l.labels {
                    let (x, y) = (sx(lb.pos[0] + s.ox), sy(lb.pos[1] + s.oy));
                    out += &format!(
                        "<text x='{:.2}' y='{:.2}' font-size='11' font-family='Arial' text-anchor='middle' dominant-baseline='middle' fill='#6e3c0f' stroke='white' stroke-width='3' paint-order='stroke' transform='rotate({:.1} {:.2} {:.2})'>{:.*}</text>\n",
                        x,
                        y,
                        upright(-lb.angle.to_degrees()),
                        x,
                        y,
                        pr.st.params.decimals_index as usize,
                        l.z
                    );
                }
            }
        }
        for b in &pr.st.breaklines {
            let pts: Vec<String> = b.pts.iter().map(|&i| format!("{:.2},{:.2}", sx(pr.st.points[i].x), sy(pr.st.points[i].y))).collect();
            out += &format!("<polyline points='{}' fill='none' stroke='#965519' stroke-width='1.2'{}/>\n", pts.join(" "), if b.soft { " stroke-dasharray='5,3'" } else { "" });
        }
        for p in pr.st.points.iter().filter(|p| p.active) {
            if p.x < vis[0] || p.x > vis[2] || p.y < vis[1] || p.y > vis[3] {
                continue;
            }
            out += &format!("<circle cx='{:.2}' cy='{:.2}' r='1.5' fill='#d21e1e'/>\n", sx(p.x), sy(p.y));
        }
        out += "</svg>\n";
        self.status = match std::fs::write(path, out) {
            Ok(_) => format!("SVG geschrieben: {}", path.display()),
            Err(e) => format!("SVG-Fehler: {}", e),
        };
    }

    // -------------------------------------------------------------- UI: menus

    fn menu_bar(&mut self, ui: &mut Ui) {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("Datei", |ui| {
                if ui.button("Projekt laden/wechseln…").clicked() {
                    self.dir_edit = self.settings.data_dir.clone();
                    self.project_list = files::list_projects(Path::new(&self.settings.data_dir));
                    self.show_open = true;
                    ui.close();
                }
                if ui.button("Punktdatei (gm1) öffnen…").clicked() {
                    ui.close();
                    if let Some(p) = rfd::FileDialog::new().add_filter("Punktdatei", &["gm1", "GM1"]).pick_file() {
                        self.open_path(&p);
                    }
                }
                if ui.button("GEO-TIFF in GM1 umwandeln…").clicked() {
                    ui.close();
                    self.geotiff_to_gm1();
                }
                ui.separator();
                if ui.button("Parameter…").clicked() {
                    if let Some(pr) = &self.project {
                        self.params_edit = pr.st.params.clone();
                    }
                    self.show_params = true;
                    ui.close();
                }
                if ui.button("Code-Auswahl…").clicked() {
                    self.show_codes = true;
                    ui.close();
                }
                if ui.button("Voreinstellungen / Autospeichern…").clicked() {
                    self.show_settings = true;
                    ui.close();
                }
                ui.separator();
                if ui.button("Jetzt speichern  (Strg+S)").clicked() {
                    self.dirty = true;
                    if self.autosave() {
                        self.status = "Projektstand gespeichert.".into();
                    }
                    ui.close();
                }
                if ui.button("GM-Dateien für das alte Programm schreiben…").clicked() {
                    self.confirm = Some(Confirm::WriteOriginal);
                    ui.close();
                }
                if ui.button("Originaldateien neu laden…").clicked() {
                    self.confirm = Some(Confirm::ReloadOriginal);
                    ui.close();
                }
                ui.separator();
                if ui.button("CAD-Höhenlinienplan (DXF)…").clicked() {
                    ui.close();
                    if let Some(p) = rfd::FileDialog::new().add_filter("DXF", &["dxf"]).set_file_name(self.file_stem() + ".dxf").save_file() {
                        self.export_dxf(&p, false);
                    }
                }
                if ui.button("Vektorgrafik des Ausschnitts (SVG)…").clicked() {
                    ui.close();
                    if let Some(p) = rfd::FileDialog::new().add_filter("SVG", &["svg"]).set_file_name(self.file_stem() + ".svg").save_file() {
                        self.export_svg(&p);
                    }
                }
                if ui.button("Bild der Ansicht (PNG)…").clicked() {
                    ui.close();
                    if let Some(p) = rfd::FileDialog::new().add_filter("PNG", &["png"]).set_file_name(self.file_stem() + ".png").save_file() {
                        self.png_target = Some(p);
                        let shot = self.shot.clone();
                        ui.ctx().request_screenshot(move |img| *shot.lock().unwrap() = Some(img));
                    }
                }
                if ui.button("DXF-Export der Bildschirmdarstellung…").clicked() {
                    self.dxf.open = true;
                    ui.close();
                }
                if ui.button("Druckausgabe (PDF, A4)…").clicked() {
                    self.print.open = true;
                    self.print.scale_edit = 0.0;
                    ui.close();
                }
                ui.separator();
                if ui.button("Ende").clicked() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
            ui.menu_button("Bearbeiten", |ui| {
                if ui.button("Punkte einlesen").clicked() {
                    self.confirm = Some(Confirm::ReloadOriginal);
                    ui.close();
                }
                if ui.button("Vermaschen").clicked() {
                    self.confirm = Some(Confirm::Remesh);
                    ui.close();
                }
                let mut ub = self.project.as_ref().map(|p| p.st.params.use_breaklines).unwrap_or(true);
                if ui.checkbox(&mut ub, "Bruchkanten dazu").changed() && self.project.is_some() {
                    self.checkpoint();
                    self.project.as_mut().unwrap().st.params.use_breaklines = ub;
                    self.remesh();
                }
                if ui.button("Höhenlinien…").clicked() {
                    if let Some(pr) = &self.project {
                        self.params_edit = pr.st.params.clone();
                    }
                    self.show_params = true;
                    ui.close();
                }
                if ui.button("Automatischer Ablauf").clicked() {
                    self.checkpoint();
                    self.remesh_manual();
                    ui.close();
                }
                ui.separator();
                if ui.button("Höhen für Punkte").clicked() {
                    self.set_tool(Tool::HeightPick);
                    self.show_heights = true;
                    ui.close();
                }
                if ui.button("Höhen Profile").clicked() {
                    self.profile_quer = false;
                    self.set_tool(Tool::Profile);
                    ui.close();
                }
                if ui.button("Querprofil erzeugen").on_hover_text("Schnittlinie anklicken, rechte Maustaste beendet").clicked() {
                    self.profile_quer = true;
                    self.set_tool(Tool::Profile);
                    ui.close();
                }
                if ui.add_enabled(self.project.is_some(), egui::Button::new("Profile bemaßt (PDF/DXF)…")).clicked() {
                    self.show_profiles = true;
                    ui.close();
                }
                if ui.button("Steigungen aus DGM").clicked() {
                    self.settings.layers.fill = 2;
                    self.rebuild_fill();
                    ui.close();
                }
                if ui.button("Volumenberechnung…").clicked() {
                    self.show_volume = true;
                    ui.close();
                }
                if ui.button("Gittermodell (3D)…").clicked() {
                    self.show_3d = true;
                    ui.close();
                }
                if ui.button("3D-Dreiecksmodell (dxf)…").clicked() {
                    ui.close();
                    if let Some(p) = rfd::FileDialog::new().add_filter("DXF", &["dxf"]).set_file_name(self.file_stem() + "_3d.dxf").save_file() {
                        self.export_dxf(&p, true);
                    }
                }
                ui.separator();
                if ui.add_enabled(!self.undo.is_empty(), egui::Button::new("Rückgängig  (Strg+Z)")).clicked() {
                    self.undo();
                }
                if ui.add_enabled(!self.redo.is_empty(), egui::Button::new("Wiederholen  (Strg+Y)")).clicked() {
                    self.redo();
                }
            });
            ui.menu_button("Zoom", |ui| {
                let r = self.map_rect;
                if ui.button("größer  (+)").clicked() {
                    self.view.zoom_at(r, r.center(), 2.0);
                    ui.close();
                }
                if ui.button("kleiner  (−)").clicked() {
                    self.view.zoom_at(r, r.center(), 0.5);
                    ui.close();
                }
                if ui.button("Original  (Pos1)").clicked() {
                    self.fit(true);
                    ui.close();
                }
                if ui.button("Ausschnitt (Rechteck aufziehen)").clicked() {
                    self.set_tool(Tool::ZoomRect);
                    ui.close();
                }
            });
            ui.menu_button("Manipulieren", |ui| {
                ui.menu_button("Punkte", |ui| {
                    for (t, l) in [(Tool::PtToggle, "löschen / aktivieren"), (Tool::PtAddLine, "erzeugen auf Linie"), (Tool::PtAddFree, "erzeugen beliebig")] {
                        if ui.button(l).clicked() {
                            self.set_tool(t);
                            ui.close();
                        }
                    }
                });
                ui.menu_button("Dreiecke", |ui| {
                    for (t, l) in [(Tool::TriToggle, "löschen / aktivieren"), (Tool::TriFlip, "tauschen"), (Tool::TriCreate, "erzeugen")] {
                        if ui.button(l).clicked() {
                            self.settings.layers.triangles = true;
                            self.set_tool(t);
                            ui.close();
                        }
                    }
                });
                ui.menu_button("Bruchkanten", |ui| {
                    for (t, l) in [
                        (Tool::BkCreate { soft: false }, "erzeugen (hart)"),
                        (Tool::BkCreate { soft: true }, "erzeugen (weich)"),
                        (Tool::BkDelete, "löschen"),
                        (Tool::BkSoft, "weich / hart"),
                        (Tool::BkShow, "anzeigen"),
                    ] {
                        if ui.button(l).clicked() {
                            self.set_tool(t);
                            ui.close();
                        }
                    }
                    if ui.button("Bruchkantenliste…").clicked() {
                        self.show_bklist = true;
                        ui.close();
                    }
                });
                ui.menu_button("Umring / Inringe", |ui| {
                    if ui.button("erzeugen (Richtung entscheidet)").clicked() {
                        self.set_tool(Tool::RingCreate);
                        ui.close();
                    }
                    if ui.button("löschen").clicked() {
                        self.set_tool(Tool::RingDelete);
                        ui.close();
                    }
                    if ui.button("Umring aus Punkten erzeugen (Hülle)").clicked() {
                        self.hull_ring();
                        ui.close();
                    }
                });
                ui.menu_button("Wasserflächen", |ui| {
                    let n = self.project.as_ref().map_or(0, |p| p.st.waters.len());
                    if ui.add_enabled(self.project.is_some() && n < water::MAX_WATERS, egui::Button::new(format!("erzeugen ({} von {})", n, water::MAX_WATERS))).on_hover_text("Punkte anklicken, rechte Maustaste schließt die Fläche").clicked() {
                        self.settings.layers.water = true;
                        self.set_tool(Tool::WaterCreate);
                        ui.close();
                    }
                    if ui.add_enabled(self.project.is_some(), egui::Button::new("Tabelle und Protokoll…")).clicked() {
                        self.show_water = true;
                        ui.close();
                    }
                });
                ui.menu_button("Grundriss", |ui| {
                    if ui.button("erzeugen").clicked() {
                        self.set_tool(Tool::PlanCreate);
                        ui.close();
                    }
                    if ui.button("löschen einzeln").clicked() {
                        self.set_tool(Tool::PlanDelete);
                        ui.close();
                    }
                    if ui.button("löschen alle").clicked() && self.project.is_some() {
                        self.checkpoint();
                        self.project.as_mut().unwrap().st.plan_lines.clear();
                        ui.close();
                    }
                    if ui.button("Bruchkanten als Grundriss übernehmen").clicked() && self.project.is_some() {
                        self.checkpoint();
                        let pr = self.project.as_mut().unwrap();
                        let b: Vec<Polyline> = pr.st.breaklines.iter().map(|b| Polyline { pts: b.pts.clone(), soft: false }).collect();
                        pr.st.plan_lines.extend(b);
                        ui.close();
                    }
                });
                if ui.button("PNr. Position").clicked() {
                    self.set_tool(Tool::PNrPos);
                    ui.close();
                }
                if ui.button("Koordinatenfenster digitalisieren / Ausschnitt speichern…").clicked() {
                    self.start_window_pick();
                    ui.close();
                }
                if ui.button("gm1 auf identische Punkte prüfen…").clicked() {
                    self.find_dups();
                    self.show_dups = true;
                    ui.close();
                }
                if ui.button("Höhenlinien-Beschriftung manuell").clicked() {
                    self.set_tool(Tool::LabelManual);
                    ui.close();
                }
            });
            ui.menu_button("Analyse", |ui| {
                if ui.button("Zweites Geländemodell einlesen…").clicked() {
                    ui.close();
                    self.load_second();
                }
                if ui.button("DXF-Datei mit Höhen aus dem Modell versehen…").clicked() {
                    self.dxfz.open = true;
                    ui.close();
                }
                ui.separator();
                let loaded = self.cmp.is_loaded();
                ui.add_enabled_ui(loaded, |ui| {
                    if ui.button("Überschneidung prüfen…").clicked() {
                        self.cmp.show_tab(CmpTab::Overview);
                        ui.close();
                    }
                    if ui.button("Auftrags- und Abtragsvolumen…").clicked() {
                        self.cmp.show_tab(CmpTab::Volume);
                        ui.close();
                    }
                    if ui.button("Höhenunterschied (Farbe und Isolinien)…").clicked() {
                        self.cmp.show_tab(CmpTab::Diff);
                        ui.close();
                    }
                    ui.separator();
                    ui.add_enabled_ui(self.cmp.has_result(), |ui| {
                        if ui.button("Differenzmodell als GM1 speichern…").clicked() {
                            ui.close();
                            self.save_diff_gm1();
                        }
                    });
                    if ui.button("Zweites Modell entfernen").clicked() {
                        self.cmp.clear();
                        self.status = "Zweites Modell entfernt.".into();
                        ui.close();
                    }
                });
                if !loaded {
                    ui.weak("Zuerst ein zweites Modell einlesen.");
                }
                ui.separator();
                if ui.button("Druckausgabe (PDF, A4)…").clicked() {
                    self.print.open = true;
                    self.print.scale_edit = 0.0;
                    ui.close();
                }
                if ui.button("DXF-Export der Bildschirmdarstellung…").clicked() {
                    self.dxf.open = true;
                    ui.close();
                }
            });
            ui.menu_button("Hilfe", |ui| {
                if ui.button("Bedienung").clicked() {
                    self.show_help = true;
                    ui.close();
                }
                if ui.button("Über DGM-NT").clicked() {
                    self.show_about = true;
                    ui.close();
                }
            });
        });
    }

    fn geotiff_to_gm1(&mut self) {
        let mut dlg = rfd::FileDialog::new()
            .add_filter("GeoTIFF", &["tif", "tiff", "geotiff", "TIF", "TIFF", "GEOTIFF"])
            .set_title("Ein oder mehrere GEO-TIFFs auswählen");
        if !self.settings.data_dir.is_empty() && Path::new(&self.settings.data_dir).is_dir() {
            dlg = dlg.set_directory(&self.settings.data_dir);
        }
        let Some(inputs) = dlg.pick_files() else {
            return;
        };
        if inputs.is_empty() {
            return;
        }

        let first_stem = inputs[0]
            .file_stem()
            .map(|s| s.to_string_lossy())
            .unwrap_or_default();
        let default_name = if inputs.len() == 1 {
            format!("{}.gm1", first_stem)
        } else {
            format!("{}-Kacheln.gm1", first_stem)
        };
        let mut save_dlg = rfd::FileDialog::new()
            .add_filter("GM1-Punktdatei", &["gm1"])
            .set_file_name(default_name);
        if let Some(parent) = inputs[0].parent() {
            save_dlg = save_dlg.set_directory(parent);
        }
        let Some(output) = save_dlg.save_file() else {
            return;
        };

        let count = inputs.len();
        self.spawn_job("GEO-TIFF in GM1 umwandeln", move |prog| {
            let result = crate::geotiff::write_gm1(&inputs, &output, &mut |f, st| prog.set(f, st)).map_err(|e| e.to_string());
            Ok(JobDone::Converted { result, inputs: count, output })
        });
    }

    fn file_stem(&self) -> String {
        self.project.as_ref().map(|p| p.st.name.clone()).unwrap_or_else(|| "dgm".into())
    }

    fn hull_ring(&mut self) {
        if self.calc_blocked() {
            self.status = self.blocked_text();
            return;
        }
        let Some(pr) = &self.project else { return };
        let mut idx: Vec<usize> = (0..pr.st.points.len()).filter(|&i| pr.st.points[i].active).collect();
        let pts = &pr.st.points;
        idx.sort_by(|&a, &b| (pts[a].x, pts[a].y).partial_cmp(&(pts[b].x, pts[b].y)).unwrap_or(std::cmp::Ordering::Equal));
        let cross = |o: usize, a: usize, b: usize| (pts[a].x - pts[o].x) * (pts[b].y - pts[o].y) - (pts[a].y - pts[o].y) * (pts[b].x - pts[o].x);
        let mut lower: Vec<usize> = Vec::new();
        for &i in &idx {
            while lower.len() >= 2 && cross(lower[lower.len() - 2], lower[lower.len() - 1], i) <= 0.0 {
                lower.pop();
            }
            lower.push(i);
        }
        let mut upper: Vec<usize> = Vec::new();
        for &i in idx.iter().rev() {
            while upper.len() >= 2 && cross(upper[upper.len() - 2], upper[upper.len() - 1], i) <= 0.0 {
                upper.pop();
            }
            upper.push(i);
        }
        lower.pop();
        upper.pop();
        lower.extend(upper);
        lower.reverse(); // clockwise
        if lower.len() >= 3 {
            self.checkpoint();
            self.project.as_mut().unwrap().st.umring = lower;
            self.remesh();
            self.status = "Umring aus der Punkthülle erzeugt – mit „Ring erzeugen/löschen“ anpassen.".into();
        }
    }

    fn find_dups(&mut self) {
        self.dups.clear();
        if self.calc_blocked() {
            self.status = self.blocked_text();
            return;
        }
        let Some(pr) = &self.project else { return };
        let mut order: Vec<usize> = (0..pr.st.points.len()).collect();
        let p = &pr.st.points;
        order.sort_by(|&a, &b| p[a].x.partial_cmp(&p[b].x).unwrap_or(std::cmp::Ordering::Equal));
        for k in 0..order.len() {
            let a = order[k];
            for &b in &order[k + 1..] {
                if p[b].x - p[a].x > tin::IDENTICAL_DIST {
                    break;
                }
                if (p[a].x - p[b].x).hypot(p[a].y - p[b].y) < tin::IDENTICAL_DIST {
                    self.dups.push((a, b));
                }
            }
        }
    }

    fn toolbar(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            let r = self.map_rect;
            let tb = |ui: &mut Ui, sel: bool, text: &str, tip: &str| ui.add(egui::Button::selectable(sel, text)).on_hover_text(tip).clicked();
            if tb(ui, self.tool == Tool::View, "✋ Ansehen", "Verschieben und zoomen (Esc)") {
                self.set_tool(Tool::View);
            }
            if tb(ui, self.tool == Tool::ZoomRect, "⬚ Ausschnitt", "Rechteck aufziehen") {
                self.set_tool(Tool::ZoomRect);
            }
            if tb(ui, false, "+ größer", "Vergrößern (+ oder Mausrad)") {
                self.view.zoom_at(r, r.center(), 2.0);
            }
            if tb(ui, false, "− kleiner", "Verkleinern (− oder Mausrad)") {
                self.view.zoom_at(r, r.center(), 0.5);
            }
            if tb(ui, false, "⟲ Original", "Ganzes Modell (Pos1)") {
                self.fit(true);
            }
            ui.separator();
            if tb(ui, matches!(self.tool, Tool::BkCreate { .. }), "Bruchkante", "Bruchkante erzeugen") {
                self.set_tool(Tool::BkCreate { soft: false });
            }
            if tb(ui, self.tool == Tool::RingCreate, "Ring", "Umring/Inring erzeugen") {
                self.set_tool(Tool::RingCreate);
            }
            if tb(ui, self.tool == Tool::TriToggle, "Dreieck", "Dreiecke löschen/aktivieren") {
                self.settings.layers.triangles = true;
                self.set_tool(Tool::TriToggle);
            }
            if tb(ui, self.tool == Tool::HeightPick, "Höhe", "Höhen für Punkte") {
                self.set_tool(Tool::HeightPick);
                self.show_heights = true;
            }
            if tb(ui, self.tool == Tool::Profile && !self.profile_quer, "Profil", "Längsprofil") {
                self.profile_quer = false;
                self.set_tool(Tool::Profile);
            }
            if tb(ui, self.tool == Tool::Profile && self.profile_quer, "Quer", "Querprofil (beliebige Schnittlinie)") {
                self.profile_quer = true;
                self.set_tool(Tool::Profile);
            }
            if tb(ui, self.show_3d, "3D", "Gittermodell") {
                self.show_3d = !self.show_3d;
            }
            if tb(ui, self.show_volume, "Volumen", "Volumenberechnung") {
                self.show_volume = !self.show_volume;
            }
            ui.separator();
            if ui.add_enabled(!self.undo.is_empty(), egui::Button::new("↶")).on_hover_text("Rückgängig (Strg+Z)").clicked() {
                self.undo();
            }
            if ui.add_enabled(!self.redo.is_empty(), egui::Button::new("↷")).on_hover_text("Wiederholen (Strg+Y)").clicked() {
                self.redo();
            }
        });
    }

    fn side_panel(&mut self, ui: &mut Ui) {
        egui::ScrollArea::vertical().show(ui, |ui| {
            if let Some(pr) = &self.project {
                ui.heading(&pr.st.name);
                let act = pr.st.points.iter().filter(|p| p.active).count();
                let tri = pr.st.triangles.iter().filter(|t| t.active).count();
                ui.label(format!(
                    "{} Punkte ({} aktiv)\n{} Dreiecke · {} Bruchkanten\n{} Höhenlinien, Abstand {} m",
                    pr.st.points.len(),
                    act,
                    tri,
                    pr.st.breaklines.len(),
                    self.contours.as_ref().map_or(0, |c| c.lines.len()),
                    pr.st.params.contour_interval
                ));
                if !pr.st.umring.is_empty() || !pr.st.inrings.is_empty() {
                    ui.label(format!("Umring: {} Punkte · {} Inringe", pr.st.umring.len(), pr.st.inrings.len()));
                }
            }
            ui.separator();
            ui.strong("Darstellung");
            let mut changed_fill = false;
            let l = &mut self.settings.layers;
            ui.horizontal(|ui| {
                changed_fill |= ui.radio_value(&mut l.fill, 1, "Höhenfarben").changed();
                changed_fill |= ui.radio_value(&mut l.fill, 2, "Neigung").changed();
                changed_fill |= ui.radio_value(&mut l.fill, 0, "keine").changed();
            });
            changed_fill |= ui.checkbox(&mut l.shade, "Schummerung (Licht von NW)").changed();
            ui.checkbox(&mut l.contours, "Höhenlinien");
            ui.checkbox(&mut l.labels, "Höhenlinien-Beschriftung");
            ui.checkbox(&mut l.points, "Punkte");
            ui.indent("pt", |ui| {
                ui.checkbox(&mut l.numbers, "Punktnummern");
                ui.checkbox(&mut l.heights, "Punkthöhen");
            });
            ui.checkbox(&mut l.triangles, "Dreiecke");
            ui.indent("tr", |ui| {
                ui.checkbox(&mut l.tri_numbers, "Dreiecksnummern");
            });
            ui.checkbox(&mut l.breaklines, "Bruchkanten");
            ui.checkbox(&mut l.rings, "Umring / Inringe");
            ui.checkbox(&mut l.water, "Wasserflächen");
            ui.checkbox(&mut l.plan, "Grundriss");
            ui.checkbox(&mut l.grid, "Koordinatengitter");
            ui.checkbox(&mut l.minimap, "Übersichtskarte");
            if changed_fill {
                self.rebuild_fill();
            }
            ui.separator();
            ui.strong("Werkzeug");
            ui.label(egui::RichText::new(self.tool.hint()).small());
            if matches!(self.tool, Tool::BkCreate { .. } | Tool::RingCreate | Tool::WaterCreate | Tool::PlanCreate | Tool::TriCreate) && !self.pick.is_empty() {
                ui.label(format!("{} Punkte gewählt", self.pick.len()));
                ui.horizontal(|ui| {
                    if ui.button("Fertig").clicked() {
                        self.on_secondary();
                    }
                    if ui.button("Abbrechen").clicked() {
                        self.pick.clear();
                    }
                });
            }
            if let Tool::BkCreate { soft } = self.tool {
                let mut s = soft;
                if ui.checkbox(&mut s, "weiche Bruchkante").changed() {
                    self.tool = Tool::BkCreate { soft: s };
                }
            }
            if self.tool == Tool::Profile {
                ui.add(egui::Slider::new(&mut self.profile_step, 0.1..=50.0).logarithmic(true).text("m Punktabstand"));
                if ui.button("Profil rechnen").clicked() {
                    self.on_secondary();
                }
            }
            ui.separator();
            ui.strong("Autospeichern");
            ui.label(if self.settings.autosave_secs == 0 {
                "aus".to_string()
            } else {
                format!("alle {} s · {}", self.settings.autosave_secs, if self.dirty { "ungespeicherte Änderungen" } else { "alles gesichert" })
            });
            if !self.last_save_text.is_empty() {
                ui.label(egui::RichText::new(&self.last_save_text).small());
            }
        });
    }

    // -------------------------------------------------------------- UI: the map

    fn map_ui(&mut self, ui: &mut Ui) {
        let (resp, painter) = ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
        let rect = resp.rect;
        self.map_rect = rect;
        if self.need_fit && rect.width() > 10.0 {
            self.fit(false);
            self.need_fit = false;
            if let Some(z) = self.start_zoom.take() {
                self.view.ppm *= z;
            }
        }
        let painter = painter.with_clip_rect(rect);
        painter.rect_filled(rect, 0.0, Color32::from_rgb(250, 250, 247));

        // ---- input: wheel zoom at the cursor, drag pan, keys
        let ctrl = ui.input(|i| i.modifiers.ctrl);
        if let Some(hp) = resp.hover_pos() {
            let mut notches = 0.0f32;
            ui.input(|i| {
                for e in &i.events {
                    if let egui::Event::MouseWheel { unit, delta, .. } = e {
                        notches += match unit {
                            egui::MouseWheelUnit::Point => delta.y / 50.0,
                            egui::MouseWheelUnit::Line => delta.y,
                            egui::MouseWheelUnit::Page => delta.y * 3.0,
                        };
                    }
                }
            });
            if notches != 0.0 {
                self.view.zoom_at(rect, hp, 1.25f64.powf(notches as f64));
            }
            self.hover = Some(self.view.to_world(rect, hp));
        } else {
            self.hover = None;
        }
        let pan = resp.dragged_by(egui::PointerButton::Middle) || (resp.dragged_by(egui::PointerButton::Primary) && self.tool != Tool::ZoomRect);
        if pan {
            self.view.pan_pixels(resp.drag_delta());
        }
        if self.tool == Tool::ZoomRect {
            if resp.drag_started_by(egui::PointerButton::Primary) {
                self.drag_rect = resp.interact_pointer_pos();
            }
            if resp.drag_stopped() {
                if let (Some(a), Some(b)) = (self.drag_rect.take(), resp.interact_pointer_pos()) {
                    let r = Rect::from_two_pos(a, b);
                    if r.width() > 8.0 && r.height() > 8.0 {
                        let (x0, y1) = self.view.to_world(rect, r.left_top());
                        let (x1, y0) = self.view.to_world(rect, r.right_bottom());
                        self.view.fit(rect, [x0, y0, x1, y1], 0.0, true);
                        self.set_tool(Tool::View);
                    }
                }
            }
        }
        if resp.double_clicked() && self.tool == Tool::View {
            if let Some(p) = resp.interact_pointer_pos() {
                self.view.zoom_at(rect, p, 2.0);
            }
        } else if resp.clicked_by(egui::PointerButton::Primary) {
            if let Some(p) = resp.interact_pointer_pos() {
                let w = self.view.to_world(rect, p);
                self.on_click(w, ctrl);
            }
        }
        if resp.clicked_by(egui::PointerButton::Secondary) {
            self.on_secondary();
        }
        if !ui.ctx().egui_wants_keyboard_input() {
            let keys = ui.input(|i| {
                (
                    i.key_pressed(Key::ArrowLeft),
                    i.key_pressed(Key::ArrowRight),
                    i.key_pressed(Key::ArrowUp),
                    i.key_pressed(Key::ArrowDown),
                    i.key_pressed(Key::Plus) || i.key_pressed(Key::Equals),
                    i.key_pressed(Key::Minus),
                    i.key_pressed(Key::Home),
                    i.key_pressed(Key::Escape),
                )
            });
            let hx = rect.width() as f64 * 0.5 / self.view.ppm;
            let hy = rect.height() as f64 * 0.5 / self.view.ppm;
            let (cx, cy, ppm) = (self.view.cx, self.view.cy, self.view.ppm);
            // as in the original: cursor keys move the window by half its size
            if keys.0 {
                self.view.animate_to(cx - hx, cy, ppm);
            }
            if keys.1 {
                self.view.animate_to(cx + hx, cy, ppm);
            }
            if keys.2 {
                self.view.animate_to(cx, cy + hy, ppm);
            }
            if keys.3 {
                self.view.animate_to(cx, cy - hy, ppm);
            }
            if keys.4 {
                self.view.zoom_at(rect, rect.center(), 2.0);
            }
            if keys.5 {
                self.view.zoom_at(rect, rect.center(), 0.5);
            }
            if keys.6 {
                self.fit(true);
            }
            if keys.7 {
                self.set_tool(Tool::View);
            }
        }

        // ---- drawing
        self.draw_map(&painter, rect);

        // tool overlays
        if let Some(a) = self.drag_rect {
            if let Some(b) = resp.hover_pos() {
                let r = Rect::from_two_pos(a, b);
                painter.rect_filled(r, 0.0, Color32::from_rgba_unmultiplied(40, 110, 220, 30));
                painter.rect_stroke(r, 0.0, Stroke::new(1.5, Color32::from_rgb(40, 110, 220)), StrokeKind::Inside);
            }
        }
        // Koordinatenfenster (Ausschnitt): eingetragenes Rechteck und erste digitalisierte Ecke
        if self.show_params || self.tool == Tool::WindowPick {
            let w = self.params_edit.win;
            let win_col = Color32::from_rgb(200, 0, 120);
            if w[2] > w[0] && w[3] > w[1] {
                let r = Rect::from_two_pos(self.view.to_screen(rect, w[0], w[3]), self.view.to_screen(rect, w[2], w[1]));
                painter.rect_filled(r, 0.0, Color32::from_rgba_unmultiplied(200, 0, 120, 22));
                painter.rect_stroke(r, 0.0, Stroke::new(2.0, win_col), StrokeKind::Middle);
            }
            if let Some(a) = self.win_first {
                let pa = self.view.to_screen(rect, a[0], a[1]);
                painter.circle_stroke(pa, 6.0, Stroke::new(2.0, win_col));
                if let Some(h) = resp.hover_pos() {
                    let r = Rect::from_two_pos(pa, h);
                    painter.rect_filled(r, 0.0, Color32::from_rgba_unmultiplied(200, 0, 120, 22));
                    painter.rect_stroke(r, 0.0, Stroke::new(1.5, win_col), StrokeKind::Middle);
                }
            }
        }
        if let Some(pr) = &self.project {
            let sp = |i: usize| self.view.to_screen(rect, pr.st.points[i].x, pr.st.points[i].y);
            let pick_col = Color32::from_rgb(230, 80, 0);
            if self.pick.len() >= 2 {
                let pts: Vec<Pos2> = self.pick.iter().map(|&i| sp(i)).collect();
                painter.add(Shape::line(pts, Stroke::new(2.5, pick_col)));
            }
            for &i in &self.pick {
                painter.circle_stroke(sp(i), 6.0, Stroke::new(2.0, pick_col));
            }
            if let (Some(&last), Some(h)) = (self.pick.last(), resp.hover_pos()) {
                if matches!(self.tool, Tool::BkCreate { .. } | Tool::RingCreate | Tool::WaterCreate | Tool::PlanCreate) {
                    painter.extend(Shape::dashed_line(&[sp(last), h], Stroke::new(1.5, pick_col), 6.0, 4.0));
                }
            }
            if self.show_profiles {
                if let Some(d) = pr.st.profiles.get(self.profile_sel) {
                    let pts: Vec<Pos2> = d.line.iter().map(|p| self.view.to_screen(rect, p[0], p[1])).collect();
                    let col = Color32::from_rgb(200, 0, 120);
                    painter.add(Shape::line(pts.clone(), Stroke::new(PROFILE_LINE_W, col)));
                    if let (Some(&first), Some(&last)) = (pts.first(), pts.last()) {
                        painter.circle_filled(first, PROFILE_END_R, col);
                        painter.circle_stroke(last, PROFILE_END_R, Stroke::new(2.0, col));
                        render::halo_text(&painter, first + Vec2::new(10.0, -10.0), Align2::LEFT_BOTTOM, &d.title(), 13.0, col, Color32::WHITE);
                    }
                }
            }
            if let Some(q) = &self.qs_dialog {
                let blue = Stroke::new(1.8, QS_PREVIEW_RGB);
                let scr = |l: &[[f64; 2]]| -> Vec<Pos2> { l.iter().map(|p| self.view.to_screen(rect, p[0], p[1])).collect() };
                for m in &q.made {
                    painter.extend(dash_pattern(&scr(&m.line), blue, &QS_DASH));
                }
                painter.extend(dash_pattern(&scr(&q.axis_pts), Stroke::new(2.4, QS_PREVIEW_RGB), &QS_DASH_DOT));
            }
            if !self.pick_world.is_empty() {
                let mut pts: Vec<Pos2> = self.pick_world.iter().map(|p| self.view.to_screen(rect, p[0], p[1])).collect();
                if let Some(h) = resp.hover_pos() {
                    pts.push(h);
                }
                painter.add(Shape::line(pts, Stroke::new(2.5, Color32::from_rgb(200, 0, 120))));
            }
            // highlight the point that would be caught
            if !matches!(self.tool, Tool::View | Tool::ZoomRect | Tool::WindowPick | Tool::HeightPick | Tool::Profile | Tool::LabelManual | Tool::TriToggle | Tool::TriFlip) {
                if let Some(w) = self.hover {
                    if let Some(i) = self.nearest_point(w, !matches!(self.tool, Tool::PtToggle | Tool::PNrPos)) {
                        painter.circle_stroke(sp(i), 9.0, Stroke::new(1.5, Color32::from_rgb(0, 120, 255)));
                        let p = &pr.st.points[i];
                        render::halo_text(&painter, sp(i) + Vec2::new(12.0, -12.0), Align2::LEFT_BOTTOM, &format!("{}  {:.2}", p.name, p.z), 13.0, Color32::BLACK, Color32::WHITE);
                    }
                }
            }
        }
        if self.print.open {
            self.draw_print_frame(&painter, rect);
        } else {
            self.draw_overlays(&painter, rect);
        }
        if resp.hovered() {
            let icon = match self.tool {
                Tool::View if resp.dragged() => egui::CursorIcon::Grabbing,
                Tool::View => egui::CursorIcon::Grab,
                Tool::ZoomRect => egui::CursorIcon::ZoomIn,
                _ => egui::CursorIcon::Crosshair,
            };
            ui.ctx().set_cursor_icon(icon);
        }
    }

    /// Wasserflächen: Farbfüllung (Parameter), Umriss und Name.
    fn draw_waters(&self, painter: &egui::Painter, rect: Rect) {
        let Some(pr) = &self.project else { return };
        let c = pr.st.params.water_rgb;
        let fill = Color32::from_rgba_unmultiplied(c[0], c[1], c[2], WATER_ALPHA);
        let edge = Color32::from_rgb(c[0] / 2, c[1] / 2, c[2] / 2);
        let mut cache = self.water_cache.borrow_mut();
        let keys: Vec<(Vec<usize>, f64)> = pr.st.waters.iter().map(|w| (w.pts.clone(), w.pts.iter().filter_map(|&i| pr.st.points.get(i)).map(|p| p.x + p.y).sum())).collect();
        if cache.len() != keys.len() || cache.iter().zip(&keys).any(|(a, b)| a.0 != b.0 || a.1 != b.1) {
            *cache = pr.st.waters.iter().zip(keys).map(|(w, (pts, sum))| {
                let xy: Vec<(f64, f64)> = w.pts.iter().filter_map(|&i| pr.st.points.get(i)).map(|p| (p.x, p.y)).collect();
                (pts, sum, if xy.len() == w.pts.len() { water::triangulate(&xy) } else { Vec::new() })
            }).collect();
        }
        for (k, w) in pr.st.waters.iter().enumerate() {
            let Some(tris) = cache.get(k).map(|c| &c.2) else { continue };
            let Some(sp) = w.pts.iter().map(|&i| pr.st.points.get(i).map(|p| self.view.to_screen(rect, p.x, p.y))).collect::<Option<Vec<Pos2>>>() else { continue };
            let mut mesh = egui::Mesh::default();
            for &q in &sp {
                mesh.colored_vertex(q, fill);
            }
            for t in tris {
                mesh.add_triangle(t[0] as u32, t[1] as u32, t[2] as u32);
            }
            painter.add(Shape::mesh(mesh));
            let selected = self.show_water && self.water_sel == k;
            painter.add(Shape::closed_line(sp.clone(), Stroke::new(if selected { 3.0 } else { 1.5 }, if selected { Color32::from_rgb(0, 90, 200) } else { edge })));
            let mid = sp.iter().fold(Vec2::ZERO, |a, q| a + q.to_vec2()) / sp.len() as f32;
            painter.text(mid.to_pos2(), Align2::CENTER_CENTER, &w.name, FontId::proportional(12.0), edge);
        }
    }

    fn draw_map(&self, painter: &egui::Painter, rect: Rect) {
        let Some(pr) = &self.project else { return };
        let v = &self.view;
        let l = &self.settings.layers;
        let pal = &self.palette;
        let vis = v.world_rect(rect);
        let in_view = |x: f64, y: f64| x >= vis[0] && x <= vis[2] && y >= vis[1] && y <= vis[3];
        // Analyse (Höhenunterschied / Auftrag-Abtrag) ersetzt das normale Geländebild
        let cmp_on = self.cmp.map_mode() != MapMode::Off;
        // shaded height fill
        if cmp_on {
            self.cmp.draw(painter, v, rect);
        } else {
            if let Some(f) = &self.fill {
                f.draw(painter, v, rect, if l.contours { 0.85 } else { 1.0 });
            }
            self.cmp.draw(painter, v, rect);
        }
        // coordinate grid crosses (Gitterkreuze), adaptive spacing
        if l.grid {
            let step = nice_step(140.0 / v.ppm);
            let x0 = (vis[0] / step).floor() * step;
            let y0 = (vis[1] / step).floor() * step;
            let cross = 7.0;
            let stroke = Stroke::new(1.0, pal.grid.gamma_multiply(0.8));
            let mut count = 0;
            let mut x = x0;
            while x <= vis[2] && count < 4000 {
                let mut y = y0;
                while y <= vis[3] && count < 4000 {
                    let p = v.to_screen(rect, x, y);
                    painter.line_segment([p - Vec2::new(cross, 0.0), p + Vec2::new(cross, 0.0)], stroke);
                    painter.line_segment([p - Vec2::new(0.0, cross), p + Vec2::new(0.0, cross)], stroke);
                    y += step;
                    count += 1;
                }
                x += step;
            }
            let dec = if step < 1.0 { 2 } else { 0 };
            let mut x = x0;
            while x <= vis[2] {
                let p = v.to_screen(rect, x, vis[1]);
                render::halo_text(painter, Pos2::new(p.x, rect.bottom() - 4.0), Align2::CENTER_BOTTOM, &format!("{:.*}", dec, x), 11.0, pal.grid, Color32::from_white_alpha(220));
                x += step;
            }
            let mut y = y0;
            while y <= vis[3] {
                let p = v.to_screen(rect, vis[0], y);
                render::halo_text(painter, Pos2::new(rect.left() + 4.0, p.y), Align2::LEFT_CENTER, &format!("{:.*}", dec, y), 11.0, pal.grid, Color32::from_white_alpha(220));
                y += step;
            }
        }
        // triangles
        if l.triangles {
            let mut shapes = Vec::new();
            let mut seen: std::collections::HashSet<(usize, usize)> = Default::default();
            let pts = &pr.st.points;
            let thin = Stroke::new(if v.ppm > 3.0 { 1.0 } else { 0.6 }, pal.tri);
            let deleted = Stroke::new(1.0, Color32::from_rgba_unmultiplied(220, 0, 0, 140));
            for (ti, t) in pr.st.triangles.iter().enumerate() {
                let (a, b, c) = (&pts[t.v[0]], &pts[t.v[1]], &pts[t.v[2]]);
                let bx0 = a.x.min(b.x).min(c.x);
                let bx1 = a.x.max(b.x).max(c.x);
                let by0 = a.y.min(b.y).min(c.y);
                let by1 = a.y.max(b.y).max(c.y);
                if bx1 < vis[0] || bx0 > vis[2] || by1 < vis[1] || by0 > vis[3] {
                    continue;
                }
                let sp = [v.to_screen(rect, a.x, a.y), v.to_screen(rect, b.x, b.y), v.to_screen(rect, c.x, c.y)];
                // Kanten einzeln (und jede nur einmal) zeichnen: geschlossene Linienzüge erzeugen an sehr
                // spitzen Winkeln lange Eckspitzen (Nadeln), die weit über das Modell hinausragen
                for e in 0..3 {
                    let (i, j) = (t.v[e], t.v[(e + 1) % 3]);
                    let seg = [sp[e], sp[(e + 1) % 3]];
                    if t.active {
                        if seen.insert((i.min(j), i.max(j))) {
                            shapes.push(Shape::line_segment(seg, thin));
                        }
                    } else {
                        shapes.extend(Shape::dashed_line(&seg, deleted, 4.0, 3.0));
                    }
                }
                if l.tri_numbers && (bx1 - bx0) * v.ppm > 40.0 {
                    let c = Pos2::new((sp[0].x + sp[1].x + sp[2].x) / 3.0, (sp[0].y + sp[1].y + sp[2].y) / 3.0);
                    painter.text(c, Align2::CENTER_CENTER, format!("{}", ti + 1), FontId::proportional(10.0), pal.tri);
                }
            }
            painter.extend(shapes);
        }
        // contours
        if l.contours && !cmp_on {
            if let (Some(c), Some(s)) = (&self.contours, &self.surface) {
                let st = ContourStyle {
                    normal: Stroke::new(1.2, pal.contour),
                    index: Stroke::new(2.2, pal.index),
                    label_color: pal.index,
                    font_px: 12.5,
                    decimals: pr.st.params.decimals_index as usize,
                    show_labels: l.labels,
                };
                render::draw_contours(painter, v, rect, c, s.ox, s.oy, &st);
            }
            for ml in &pr.st.manual_labels {
                if in_view(ml.x, ml.y) {
                    let p = v.to_screen(rect, ml.x, ml.y);
                    let mut ang = -ml.angle;
                    if ang > std::f32::consts::FRAC_PI_2 {
                        ang -= std::f32::consts::PI;
                    } else if ang < -std::f32::consts::FRAC_PI_2 {
                        ang += std::f32::consts::PI;
                    }
                    let g = painter.layout_no_wrap(format!("{:.*}", pr.st.params.decimals_index as usize, ml.z), FontId::proportional(12.5), pal.index);
                    painter.add(Shape::Text(egui::epaint::TextShape::new(p, g, pal.index).with_angle_and_anchor(ang, Align2::CENTER_CENTER)));
                }
            }
        }
        if l.water {
            self.draw_waters(painter, rect);
        }
        // rings
        if l.rings {
            let stroke = Stroke::new(2.0, pal.ring);
            for (ri, r) in std::iter::once(&pr.st.umring).chain(pr.st.inrings.iter()).enumerate() {
                if r.len() < 3 {
                    continue;
                }
                let mut pts: Vec<Pos2> = r.iter().map(|&i| v.to_screen(rect, pr.st.points[i].x, pr.st.points[i].y)).collect();
                if ri == 0 {
                    painter.add(Shape::closed_line(pts, stroke));
                } else {
                    pts.push(pts[0]);
                    painter.extend(Shape::dashed_line(&pts, stroke, 8.0, 4.0));
                }
            }
        }
        // plan lines (Grundriss)
        if l.plan {
            for b in &pr.st.plan_lines {
                let pts: Vec<Pos2> = b.pts.iter().map(|&i| v.to_screen(rect, pr.st.points[i].x, pr.st.points[i].y)).collect();
                painter.add(Shape::line(pts, Stroke::new(1.6, pal.plan)));
            }
        }
        // breaklines
        if l.breaklines {
            let blink = self.blink.and_then(|(li, t)| {
                let ms = t.elapsed().as_millis();
                if ms < 2400 && (ms / 300) % 2 == 0 {
                    Some(li)
                } else {
                    None
                }
            });
            for (bi, b) in pr.st.breaklines.iter().enumerate() {
                let pts: Vec<Pos2> = b.pts.iter().map(|&i| v.to_screen(rect, pr.st.points[i].x, pr.st.points[i].y)).collect();
                let hl = blink == Some(bi);
                let stroke = Stroke::new(if hl { 5.0 } else { 2.0 }, if hl { Color32::from_rgb(255, 0, 200) } else { Color32::from_rgb(pr.st.params.break_rgb[0], pr.st.params.break_rgb[1], pr.st.params.break_rgb[2]) });
                if b.soft {
                    painter.extend(Shape::dashed_line(&pts, stroke, 7.0, 4.0));
                } else {
                    painter.add(Shape::line(pts, stroke));
                }
            }
        }
        // points with level-of-detail labels (no overlaps)
        if l.points {
            let mut occupied: std::collections::HashSet<(i32, i32)> = Default::default();
            let cell = 30.0;
            let font = FontId::proportional(11.0);
            let count_vis = pr.st.points.iter().filter(|p| in_view(p.x, p.y)).count();
            // level of detail: average screen distance between the visible points
            let spacing = ((rect.area() as f64) / (count_vis.max(1) as f64)).sqrt();
            let sym = (spacing * 0.09).clamp(1.0, 3.0) as f32;
            let show_text = spacing > 34.0;
            let dec = pr.st.params.decimals_h as usize;
            // bei sehr vielen sichtbaren Punkten nur jeden n-ten zeichnen (sonst sprengt die Grafikliste den Puffer)
            let thin = count_vis.div_ceil(MAX_DRAWN_POINTS).max(1);
            let mut seen = 0usize;
            for p in &pr.st.points {
                if !in_view(p.x, p.y) {
                    continue;
                }
                seen += 1;
                if thin > 1 && (seen - 1) % thin != 0 {
                    continue;
                }
                let sp = v.to_screen(rect, p.x, p.y);
                if p.active {
                    if sym > 2.0 {
                        painter.circle_filled(sp, sym + 1.0, Color32::WHITE);
                    }
                    painter.circle_filled(sp, sym, if sym < 1.6 { pal.point.gamma_multiply(0.6) } else { pal.point });
                } else {
                    painter.line_segment([sp - Vec2::splat(4.0), sp + Vec2::splat(4.0)], Stroke::new(1.5, Color32::RED));
                    painter.line_segment([sp + Vec2::new(-4.0, 4.0), sp + Vec2::new(4.0, -4.0)], Stroke::new(1.5, Color32::RED));
                }
                if (l.numbers || l.heights) && show_text {
                    let key = ((sp.x / cell).floor() as i32, (sp.y / (cell * 0.75)).floor() as i32);
                    if occupied.contains(&key) {
                        continue;
                    }
                    occupied.insert(key);
                    let off = match p.label_pos {
                        0 => Vec2::new(4.0, -2.0),
                        1 => Vec2::new(4.0, 13.0),
                        2 => Vec2::new(-4.0, 13.0),
                        _ => Vec2::new(-4.0, -2.0),
                    };
                    let align = if p.label_pos >= 2 { Align2::RIGHT_BOTTOM } else { Align2::LEFT_BOTTOM };
                    if l.numbers {
                        painter.text(sp + off, align, &p.name, font.clone(), pal.point_nr);
                    }
                    if l.heights {
                        painter.text(sp + off + Vec2::new(0.0, if l.numbers { 11.0 } else { 0.0 }), align, format!("{:.*}", dec, p.z), font.clone(), Color32::from_rgb(0, 105, 125));
                    }
                }
            }
        }
        // heights picked with the mouse
        for h in &self.heights {
            if in_view(h.1, h.2) {
                let sp = v.to_screen(rect, h.1, h.2);
                painter.circle_stroke(sp, 5.0, Stroke::new(2.0, Color32::from_rgb(200, 0, 120)));
                if let Some(z) = h.3 {
                    render::halo_text(painter, sp + Vec2::new(7.0, 0.0), Align2::LEFT_CENTER, &format!("{} {:.2}", h.0, z), 12.0, Color32::from_rgb(160, 0, 100), Color32::WHITE);
                }
            }
        }
    }

    /// scale bar, north arrow and the overview map
    fn draw_overlays(&self, painter: &egui::Painter, rect: Rect) {
        let v = &self.view;
        let len_m = nice_step(120.0 / v.ppm);
        let len_px = (len_m * v.ppm) as f32;
        let base = Pos2::new(rect.right() - 30.0 - len_px, rect.bottom() - 30.0);
        let bg = Rect::from_min_max(base - Vec2::new(10.0, 26.0), Pos2::new(rect.right() - 14.0, rect.bottom() - 12.0));
        painter.rect_filled(bg, 4.0, Color32::from_white_alpha(215));
        for k in 0..4 {
            let x0 = base.x + len_px * k as f32 / 4.0;
            let r = Rect::from_min_size(Pos2::new(x0, base.y), Vec2::new(len_px / 4.0, 5.0));
            painter.rect_filled(r, 0.0, if k % 2 == 0 { Color32::from_gray(30) } else { Color32::WHITE });
        }
        painter.rect_stroke(Rect::from_min_size(base, Vec2::new(len_px, 5.0)), 0.0, Stroke::new(1.0, Color32::from_gray(30)), StrokeKind::Middle);
        let unit = if len_m >= 1000.0 {
            format!("{} km", len_m / 1000.0)
        } else if len_m >= 1.0 {
            format!("{} m", len_m)
        } else {
            format!("{} cm", (len_m * 100.0).round())
        };
        painter.text(base + Vec2::new(len_px * 0.5, -3.0), Align2::CENTER_BOTTOM, unit, FontId::proportional(12.0), Color32::from_gray(20));
        // north arrow
        let n = Pos2::new(rect.right() - 28.0, rect.top() + 34.0);
        painter.add(Shape::convex_polygon(vec![n + Vec2::new(0.0, -16.0), n + Vec2::new(7.0, 8.0), n + Vec2::new(0.0, 3.0)], Color32::from_gray(40), Stroke::NONE));
        painter.add(Shape::convex_polygon(vec![n + Vec2::new(0.0, -16.0), n + Vec2::new(0.0, 3.0), n + Vec2::new(-7.0, 8.0)], Color32::WHITE, Stroke::new(1.0, Color32::from_gray(40))));
        painter.text(n + Vec2::new(0.0, -18.0), Align2::CENTER_BOTTOM, "N", FontId::proportional(12.0), Color32::from_gray(30));
        self.cmp.draw_legend(painter, rect);
        // overview map
        if self.settings.layers.minimap {
            if let Some(b) = self.project.as_ref().and_then(|p| p.bbox()) {
                let w = (b[2] - b[0]).max(1e-6);
                let h = (b[3] - b[1]).max(1e-6);
                let mw = 170.0f32;
                let mh = (mw as f64 * h / w).clamp(40.0, 170.0) as f32;
                let mr = Rect::from_min_size(Pos2::new(rect.right() - mw - 16.0, rect.bottom() - mh - 56.0), Vec2::new(mw, mh));
                painter.rect_filled(mr.expand(3.0), 4.0, Color32::from_white_alpha(225));
                painter.rect_stroke(mr.expand(3.0), 4.0, Stroke::new(1.0, Color32::from_gray(150)), StrokeKind::Outside);
                let sc = (mw as f64 / w).min(mh as f64 / h);
                let to = |x: f64, y: f64| Pos2::new(mr.left() + ((x - b[0]) * sc) as f32, mr.bottom() - ((y - b[1]) * sc) as f32);
                if let Some(pr) = &self.project {
                    let stepn = (pr.st.points.len() / 3000).max(1);
                    for p in pr.st.points.iter().step_by(stepn) {
                        painter.circle_filled(to(p.x, p.y), 0.8, Color32::from_rgb(160, 120, 80));
                    }
                }
                let vis = v.world_rect(rect);
                let vr = Rect::from_two_pos(to(vis[0], vis[1]), to(vis[2], vis[3])).intersect(mr.expand(2.0));
                painter.rect_stroke(vr, 0.0, Stroke::new(1.5, Color32::from_rgb(220, 40, 40)), StrokeKind::Middle);
            }
        }
    }

    // -------------------------------------------------------------- status bar

    fn status_bar(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            if let Some((x, y)) = self.hover {
                ui.monospace(format!("R {:.2}  H {:.2}", x, y));
                if let Some(z) = self.surface.as_ref().and_then(|s| s.height(x, y)) {
                    ui.monospace(format!("Höhe {:.2} m", z));
                }
                if let Some(dz) = self.cmp.dz_at(x, y) {
                    ui.monospace(format!("Δ {:+.3} m", dz));
                }
                ui.separator();
            }
            // 1 screen point ≈ 0.2646 mm at 96 dpi
            let m = 1.0 / (self.view.ppm * 0.000_264_6);
            ui.label(format!("Maßstab 1:{}", group_thousands(round_scale(m))));
            ui.separator();
            ui.label(&self.status);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let (txt, col) = if self.settings.autosave_secs == 0 {
                    ("Autospeichern aus".to_string(), Color32::GRAY)
                } else if self.dirty {
                    let left = self.settings.autosave_secs.saturating_sub(self.last_save.elapsed().as_secs());
                    (format!("● Änderungen – speichert in {} s", left), Color32::from_rgb(200, 120, 0))
                } else if self.last_save_text.is_empty() {
                    ("✔ Autospeichern bereit".to_string(), Color32::from_rgb(30, 140, 60))
                } else {
                    (format!("✔ {}", self.last_save_text), Color32::from_rgb(30, 140, 60))
                };
                ui.colored_label(col, txt);
            });
        });
    }

    // -------------------------------------------------------------- dialogs

    fn dialogs(&mut self, ctx: &egui::Context) {
        self.print_dialog(ctx);
        self.dxf_dialog(ctx);
        if self.show_open {
            let mut open = true;
            let mut chosen: Option<String> = None;
            egui::Window::new("Projekt laden/wechseln").open(&mut open).default_width(380.0).show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Datenordner:");
                    ui.text_edit_singleline(&mut self.dir_edit);
                    if ui.button("…").clicked() {
                        if let Some(d) = rfd::FileDialog::new().pick_folder() {
                            self.dir_edit = d.display().to_string();
                        }
                    }
                    if ui.button("Lesen").clicked() || self.dir_edit != self.settings.data_dir {
                        self.settings.data_dir = self.dir_edit.clone();
                        self.project_list = files::list_projects(Path::new(&self.dir_edit));
                    }
                });
                ui.separator();
                if self.project_list.is_empty() {
                    ui.label("Keine Punktdateien (*.gm1) in diesem Ordner.");
                }
                egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
                    for n in &self.project_list {
                        let cur = self.project.as_ref().map_or(false, |p| p.st.name.eq_ignore_ascii_case(n));
                        if ui.selectable_label(cur, n).clicked() {
                            chosen = Some(n.clone());
                        }
                    }
                });
            });
            if let Some(n) = chosen {
                let dir = PathBuf::from(&self.settings.data_dir);
                self.open_project(dir, &n);
                open = false;
            }
            self.show_open = open;
        }

        if self.show_params {
            let mut open = true;
            let mut apply = false;
            let mut win_action: Option<WinAction> = None;
            egui::Window::new("DGM Parameter / Höhenlinien").open(&mut open).default_width(360.0).show(ctx, |ui| {
                let p = &mut self.params_edit;
                egui::Grid::new("params").num_columns(2).striped(true).show(ui, |ui| {
                    ui.label("Abstand Höhenlinien (m)");
                    ui.add(egui::DragValue::new(&mut p.contour_interval).speed(0.1).range(0.01..=1000.0));
                    ui.end_row();
                    ui.label("Zähllinien: jede n-te Linie");
                    ui.add(egui::DragValue::new(&mut p.index_every).range(1..=20));
                    ui.end_row();
                    ui.label("max. Dreieckseite (m)");
                    ui.add(egui::DragValue::new(&mut p.max_side).speed(1.0).range(0.1..=100000.0));
                    ui.end_row();
                    ui.label("min. Dreieckswinkel (gon)");
                    ui.add(egui::DragValue::new(&mut p.min_angle_gon).speed(0.1).range(0.0..=60.0));
                    ui.end_row();
                    ui.label("Dreiecksunterteilung (0..15)");
                    ui.add(egui::DragValue::new(&mut p.subdivision).range(0..=15));
                    ui.end_row();
                    ui.label("Glättung Höhenlinien (0..10)");
                    ui.horizontal(|ui| {
                        ui.add(egui::DragValue::new(&mut p.smoothing).range(0..=10));
                        ui.label(match p.smoothing {
                            0 => "keine",
                            1 => "kubischer Spline",
                            2..=7 => "Spline, abnehmende Ausrundung",
                            8 => "ausgleichend, eng",
                            9 => "ausgleichend, weit",
                            _ => "Akima",
                        });
                    });
                    ui.end_row();
                    ui.label("Ausdünnung (mm)");
                    ui.add(egui::DragValue::new(&mut p.thin_mm).speed(0.01).range(0.0..=10.0));
                    ui.end_row();
                    ui.label("Mind.abstand HLP (mm)");
                    ui.add(egui::DragValue::new(&mut p.min_dist_mm).speed(0.1).range(0.01..=50.0));
                    ui.end_row();
                    ui.label("Maßstabszahl Plott");
                    ui.add(egui::DragValue::new(&mut p.scale).speed(10.0).range(1.0..=1_000_000.0));
                    ui.end_row();
                    ui.label("Nachkomma Höhen");
                    ui.add(egui::DragValue::new(&mut p.decimals_h).range(0..=4));
                    ui.end_row();
                    ui.label("Nachkomma Zähllinien");
                    ui.add(egui::DragValue::new(&mut p.decimals_index).range(0..=3));
                    ui.end_row();
                    ui.label("Bruchkanten verwenden");
                    ui.checkbox(&mut p.use_breaklines, "");
                    ui.end_row();
                    ui.label("Auflockerung Abtrag (Faktor)");
                    ui.add(egui::DragValue::new(&mut p.loosen_cut).speed(0.01).range(0.5..=3.0).fixed_decimals(2));
                    ui.end_row();
                    ui.label("Auflockerung Auftrag (Faktor)");
                    ui.add(egui::DragValue::new(&mut p.loosen_fill).speed(0.01).range(0.5..=3.0).fixed_decimals(2));
                    ui.end_row();
                    ui.label("Rechengrenze (Punkte)");
                    ui.add(egui::DragValue::new(&mut p.max_calc_points).speed(1000.0).range(1000..=100_000_000))
                        .on_hover_text("Bei mehr Punkten sind Vermaschung, Höhenlinien und weitere Berechnungen abgeschaltet.");
                    ui.end_row();
                });
                egui::CollapsingHeader::new("Koordinatenfenster (Ausschnitt)").default_open(true).show(ui, |ui| {
                    egui::Grid::new("params_window").num_columns(2).striped(true).show(ui, |ui| {
                        for (i, label) in ["Rechtswert links unten", "Hochwert links unten", "Rechtswert rechts oben", "Hochwert rechts oben"].iter().enumerate() {
                            ui.label(*label);
                            ui.add(egui::DragValue::new(&mut p.win[i]).speed(1.0).fixed_decimals(3));
                            ui.end_row();
                        }
                    });
                    ui.horizontal_wrapped(|ui| {
                        if ui.button("Am Bildschirm digitalisieren").on_hover_text("Zwei gegenüberliegende Ecken im Plan anklicken").clicked() {
                            win_action = Some(WinAction::Pick);
                        }
                        if ui.button("Aus Bildschirmausschnitt").clicked() {
                            win_action = Some(WinAction::View);
                        }
                        if ui.button("Ganzes Modell").clicked() {
                            win_action = Some(WinAction::All);
                        }
                    });
                    if ui.button("Ausschnitt als GM1 speichern…").on_hover_text("Alle aktiven Punkte im Fenster in eine neue Punktdatei schreiben").clicked() {
                        win_action = Some(WinAction::Save);
                    }
                });
                egui::CollapsingHeader::new("Gittermodell (3D)").default_open(false).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Maschenweite (m, 0 = automatisch)");
                        ui.add(egui::DragValue::new(&mut p.mesh_width_3d).speed(0.01).range(0.0..=10000.0).max_decimals(3));
                    });
                    ui.weak("Kleinster Wert 0,05 m. Bei sehr großen Gebieten wird die Maschenweite so vergröbert, dass höchstens eine Million Rasterfelder entstehen.");
                });
                egui::CollapsingHeader::new("Profile (PDF)").default_open(false).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Schriftdatei (TTF)");
                        ui.text_edit_singleline(&mut p.profile_font);
                        if ui.button("…").on_hover_text("TrueType-Schrift wählen, z. B. eine Normschrift nach ISO 3098").clicked() {
                            if let Some(f) = rfd::FileDialog::new().add_filter("TrueType", &["ttf"]).pick_file() {
                                p.profile_font = f.display().to_string();
                            }
                        }
                    });
                    ui.weak("Leer = Normschrift im Windows-Schriftenordner suchen (ISOCPEUR, GENISO, NLCS-ISO …); „-“ = Courier. Die Schrift wird ins PDF eingebettet, sofern ihre Lizenz es erlaubt.");
                });
                egui::CollapsingHeader::new("Querprofile (aus Längsprofil)").default_open(false).show(ui, |ui| {
                    egui::Grid::new("qs_params").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
                        ui.label("Abstand der Profile (m)");
                        ui.add(egui::DragValue::new(&mut p.qs_step).range(QS_STEP_MIN_M..=10_000.0).speed(1.0).max_decimals(2));
                        ui.end_row();
                        ui.label("Breite links/rechts (± m)");
                        ui.add(egui::DragValue::new(&mut p.qs_half).range(QS_STEP_MIN_M..=10_000.0).speed(1.0).max_decimals(2));
                        ui.end_row();
                        ui.label("Maßstab 1 : (0 = Vorschlag)");
                        ui.add(egui::DragValue::new(&mut p.qs_scale_n).range(0.0..=100_000.0).speed(10.0).max_decimals(0));
                        ui.end_row();
                        ui.label("Überhöhung (0 = Vorschlag)");
                        ui.add(egui::DragValue::new(&mut p.qs_exag).range(0.0..=200.0).speed(0.5).max_decimals(2));
                        ui.end_row();
                    });
                    ui.weak("Vorbelegung der Maske, die nach dem Anlegen eines Längsprofils erscheint. Maßstab und Überhöhung 0 = aus dem Gelände ermittelt.");
                });
                egui::CollapsingHeader::new("Bruchkanten").default_open(false).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Farbe der Bruchkanten");
                        ui.color_edit_button_srgb(&mut p.break_rgb);
                        if ui.button("Grün").on_hover_text("Standardfarbe wiederherstellen").clicked() {
                            p.break_rgb = crate::model::DEFAULT_BREAK_RGB;
                        }
                    });
                    ui.weak("Gilt für die Karte und das PDF. Im DXF bestimmt die CAD-Farbnummer „Bruchkanten“ (Standard 2 = grün) im Abschnitt Export. Gespeichert in der Projektdatei (.dgmnt.json).");
                });
                egui::CollapsingHeader::new("Wasserfläche").default_open(false).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Farbe der Wasserflächen");
                        ui.color_edit_button_srgb(&mut p.water_rgb);
                        if ui.button("helles Cyan").on_hover_text("Standardfarbe wiederherstellen").clicked() {
                            p.water_rgb = water::DEFAULT_WATER_RGB;
                        }
                    });
                    ui.weak("Gilt für die Karte, PDF und DXF. Gespeichert in der Projektdatei (.dgmnt.json).");
                });
                egui::CollapsingHeader::new("Export DXF / PDF (Schriften, Kreise, Farben)").default_open(false).show(ui, |ui| {
                    egui::Grid::new("params_export").num_columns(2).striped(true).show(ui, |ui| {
                        ui.label("Radius Punktkreis (mm Papier)");
                        ui.add(egui::DragValue::new(&mut p.exp_circle_mm).speed(0.05).range(0.0..=10.0).fixed_decimals(2));
                        ui.end_row();
                        ui.label("Schrifthöhe Punktnummer (mm)");
                        ui.add(egui::DragValue::new(&mut p.exp_text_nr_mm).speed(0.05).range(0.0..=10.0).fixed_decimals(2));
                        ui.end_row();
                        ui.label("Schrifthöhe Punkthöhe (mm)");
                        ui.add(egui::DragValue::new(&mut p.exp_text_h_mm).speed(0.05).range(0.0..=10.0).fixed_decimals(2));
                        ui.end_row();
                        ui.label("Schrifthöhe Zähllinien (mm)");
                        ui.add(egui::DragValue::new(&mut p.exp_text_index_mm).speed(0.05).range(0.0..=10.0).fixed_decimals(2));
                        ui.end_row();
                        ui.label("Schrifthöhe Gitter (mm)");
                        ui.add(egui::DragValue::new(&mut p.exp_text_grid_mm).speed(0.05).range(0.0..=10.0).fixed_decimals(2));
                        ui.end_row();
                        ui.label("Abstand Gitterkreuze (mm)");
                        ui.add(egui::DragValue::new(&mut p.grid_mm).speed(1.0).range(10.0..=1000.0));
                        ui.end_row();
                        for (i, name) in crate::model::EXP_COLOR_NAMES.iter().enumerate() {
                            ui.label(format!("Farbe {} (CAD 1–12)", name));
                            ui.horizontal(|ui| {
                                ui.add(egui::DragValue::new(&mut p.exp_colors[i]).range(1..=12));
                                let (r, _) = ui.allocate_exact_size(Vec2::new(26.0, 14.0), egui::Sense::hover());
                                ui.painter().rect_filled(r, 2.0, render::caddy(p.exp_colors[i]));
                            });
                            ui.end_row();
                        }
                    });
                    ui.weak("Diese Werte stehen in den Zeilen 38–55 der Parameterdatei (gm0); das alte Programm liest nur 37 Zeilen und ignoriert sie.");
                });
                ui.separator();
                if ui.button("OK – neu rechnen").clicked() {
                    apply = true;
                }
            });
            match win_action {
                Some(WinAction::Pick) => self.start_window_pick(),
                Some(WinAction::View) => {
                    let v = self.view.world_rect(self.map_rect);
                    self.params_edit.win = [v[0], v[1], v[2], v[3]];
                }
                Some(WinAction::All) => {
                    if let Some(b) = self.project.as_ref().and_then(|p| p.bbox()) {
                        self.params_edit.win = b;
                    }
                }
                Some(WinAction::Save) => self.save_window_gm1(),
                None => {}
            }
            if apply && self.project.is_some() {
                self.checkpoint();
                let pr = self.project.as_mut().unwrap();
                let remesh = pr.st.params.max_side != self.params_edit.max_side
                    || pr.st.params.min_angle_gon != self.params_edit.min_angle_gon
                    || pr.st.params.use_breaklines != self.params_edit.use_breaklines;
                pr.st.params = self.params_edit.clone();
                if pr.st.params.mesh_width_3d > 0.0 {
                    pr.st.params.mesh_width_3d = pr.st.params.mesh_width_3d.max(crate::view3d::MESH_WIDTH_MIN);
                }
                if remesh {
                    self.remesh();
                } else {
                    self.recompute();
                }
                self.status = if self.calc_blocked() {
                    self.blocked_text()
                } else if self.auto_off {
                    format!("Parameter übernommen. Mehr als {} Punkte: Vermaschung und Höhenlinien von Hand starten (Bearbeiten → Vermaschen).", AUTO_CALC_LIMIT_POINTS)
                } else {
                    "Parameter übernommen.".into()
                };
                open = false;
            }
            self.show_params = open;
        }

        if self.show_codes {
            let mut open = true;
            let mut changed = false;
            egui::Window::new("Code-Auswahl").open(&mut open).show(ctx, |ui| {
                if let Some(pr) = &mut self.project {
                    let mut codes: Vec<String> = pr.st.points.iter().map(|p| p.code.clone()).collect();
                    codes.sort();
                    codes.dedup();
                    ui.label("Nur Punkte mit den angehakten Codes ins DGM übernehmen:");
                    egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                        for c in &codes {
                            let n = pr.st.points.iter().filter(|p| &p.code == c).count();
                            let mut on = pr.st.code_filter.contains(c);
                            if ui.checkbox(&mut on, format!("{} ({} Punkte)", if c.is_empty() { "(ohne Code)" } else { c }, n)).changed() {
                                if on {
                                    pr.st.code_filter.push(c.clone());
                                } else {
                                    pr.st.code_filter.retain(|x| x != c);
                                }
                                changed = true;
                            }
                        }
                    });
                    if ui.button("keine Auswahl (alle Punkte)").clicked() {
                        pr.st.code_filter.clear();
                        changed = true;
                    }
                }
            });
            if changed {
                self.dirty = true;
                self.remesh();
            }
            self.show_codes = open;
        }

        if self.show_dups {
            let mut open = true;
            let mut deactivate = false;
            egui::Window::new("Identische Punkte").open(&mut open).show(ctx, |ui| {
                if let Some(pr) = &self.project {
                    ui.label(format!("{} Paare näher als {} m:", self.dups.len(), tin::IDENTICAL_DIST));
                    egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                        for (a, b) in &self.dups {
                            let (pa, pb) = (&pr.st.points[*a], &pr.st.points[*b]);
                            ui.monospace(format!("{:>12} {:>9.3} | {:>12} {:>9.3}", pa.name, pa.z, pb.name, pb.z));
                        }
                    });
                    if !self.dups.is_empty() && ui.button("Jeweils den zweiten Punkt deaktivieren").clicked() {
                        deactivate = true;
                    }
                }
            });
            if deactivate && self.project.is_some() {
                self.checkpoint();
                let d = self.dups.clone();
                let pr = self.project.as_mut().unwrap();
                for (_, b) in d {
                    pr.st.points[b].active = false;
                }
                self.dups.clear();
                self.remesh();
            }
            self.show_dups = open;
        }

        if self.show_volume {
            let mut open = true;
            egui::Window::new("Volumenberechnung").open(&mut open).default_width(330.0).show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Bezugshöhe (m):");
                    ui.add(egui::DragValue::new(&mut self.volume_ref).speed(0.1));
                });
                if ui.button("Rechnen (Prismenmethode)").clicked() {
                    if let (Some(s), Some(pr)) = (&self.surface, &self.project) {
                        self.volume_res = Some(analysis::volume(s, self.volume_ref, (pr.st.params.subdivision as usize).clamp(1, 4)));
                    }
                }
                if let Some(r) = &self.volume_res {
                    egui::Grid::new("vol").num_columns(2).show(ui, |ui| {
                        ui.label("Dreiecke");
                        ui.label(format!("{}", r.triangles));
                        ui.end_row();
                        ui.label("Grundfläche");
                        ui.label(format!("{:.1} m²", r.area_2d));
                        ui.end_row();
                        ui.label("Oberfläche");
                        ui.label(format!("{:.1} m²", r.area_3d));
                        ui.end_row();
                        ui.label("Volumen über Bezugshöhe");
                        ui.strong(format!("{:.1} m³", r.vol_above));
                        ui.end_row();
                        ui.label("  auf Fläche");
                        ui.label(format!("{:.1} m²", r.area_above));
                        ui.end_row();
                        ui.label("Volumen unter Bezugshöhe");
                        ui.strong(format!("{:.1} m³", r.vol_below));
                        ui.end_row();
                        ui.label("  auf Fläche");
                        ui.label(format!("{:.1} m²", r.area_below));
                        ui.end_row();
                    });
                }
            });
            self.show_volume = open;
        }

        if self.show_heights {
            let mut open = true;
            let mut save = false;
            egui::Window::new("Höhen für Punkte").open(&mut open).default_width(380.0).show(ctx, |ui| {
                ui.label("Lage im Plan anklicken (Werkzeug „Höhe“).");
                egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                    egui::Grid::new("hts").striped(true).show(ui, |ui| {
                        ui.strong("Nr");
                        ui.strong("Station");
                        ui.strong("Rechtswert");
                        ui.strong("Hochwert");
                        ui.strong("Höhe");
                        ui.end_row();
                        for h in &self.heights {
                            ui.label(&h.0);
                            ui.monospace(analysis::format_station(h.4));
                            ui.monospace(format!("{:.3}", h.1));
                            ui.monospace(format!("{:.3}", h.2));
                            ui.monospace(h.3.map_or("–".to_string(), |z| format!("{:.3}", z)));
                            ui.end_row();
                        }
                    });
                });
                ui.horizontal(|ui| {
                    if ui.button("Als KOR-Datei speichern…").clicked() {
                        save = true;
                    }
                    if ui.button("Liste leeren").clicked() {
                        self.heights.clear();
                    }
                });
            });
            if save {
                if let Some(p) = rfd::FileDialog::new().add_filter("KOR", &["kor"]).set_file_name(self.file_stem() + ".kor").save_file() {
                    let mut s = String::new();
                    for h in &self.heights {
                        s += &format!("{:>15} {:>14.3} {:>14.3} {:>10.3} {:>12}\n", h.0, h.1, h.2, h.3.unwrap_or(0.0), analysis::format_station(h.4));
                    }
                    let _ = files::write_text(&p, &s);
                    self.status = format!("Gespeichert: {}", p.display());
                }
            }
            self.show_heights = open;
        }

        if self.show_profile {
            let mut open = true;
            let mut save = false;
            egui::Window::new("Längsprofil").open(&mut open).default_size([720.0, 360.0]).show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.add(egui::Slider::new(&mut self.profile_exag, 1.0..=20.0).text("Überhöhung"));
                    if ui.button("Als LNG-Datei speichern…").clicked() {
                        save = true;
                    }
                    if ui.button("Neues Profil").clicked() {
                        self.pick_world.clear();
                        self.tool = Tool::Profile;
                    }
                });
                profile_plot(ui, &self.profile, self.profile_exag);
            });
            if save {
                if let Some(p) = rfd::FileDialog::new().add_filter("LNG", &["lng"]).set_file_name(self.file_stem() + ".lng").save_file() {
                    let mut s = String::new();
                    for (i, q) in self.profile.iter().enumerate() {
                        s += &format!("{:>6} {:>12} {:>14.3} {:>14.3} {:>10.3}\n", i + 1, analysis::format_station(q.station), q.x, q.y, q.z.unwrap_or(0.0));
                    }
                    let _ = files::write_text(&p, &s);
                    self.status = format!("Gespeichert: {}", p.display());
                }
            }
            self.show_profile = open;
        }

        if self.show_3d {
            let mut open = true;
            let area = self.project.as_ref().and_then(|p| p.bbox());
            // Maschenweite aus den Parametern übernehmen (0 = automatisch)
            let mw_param = self.project.as_ref().map_or(0.0, |p| p.st.params.mesh_width_3d);
            if mw_param > 0.0 && (self.v3d.mesh_width - mw_param).abs() > 1e-12 {
                self.v3d.mesh_width = mw_param.max(crate::view3d::MESH_WIDTH_MIN);
                self.v3d.grid = None;
            }
            if let (Some(s), Some(a)) = (&self.surface, area) {
                self.v3d.build(s, a, self.surf_id);
            }
            let zr = self.surface.as_ref().map(|s| (s.zmin, s.zmax)).unwrap_or((0.0, 1.0));
            egui::Window::new("Gittermodell").open(&mut open).default_size([760.0, 520.0]).show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.add(egui::Slider::new(&mut self.v3d.exaggeration, 0.5..=10.0).text("Überhöhung"));
                    let mut mw = self.v3d.mesh_width;
                    if ui.add(egui::DragValue::new(&mut mw).speed(0.01).range(crate::view3d::MESH_WIDTH_MIN..=10000.0).max_decimals(3).prefix("Maschenweite ").suffix(" m")).changed() {
                        self.v3d.mesh_width = mw;
                        if let Some(pr) = self.project.as_mut() {
                            pr.st.params.mesh_width_3d = mw;
                            self.dirty = true;
                        }
                        self.v3d.grid = None;
                    }
                    ui.checkbox(&mut self.v3d.shaded, "farbig");
                    ui.checkbox(&mut self.v3d.wire, "Gitter");
                    ui.checkbox(&mut self.settings.layers.water, "Wasser");
                    if ui.button("PDF…").on_hover_text("Gittermodell als vektorbasierte PDF-Datei ausgeben").clicked() {
                        self.print3d.open = true;
                    }
                    for (lbl, az) in [("N", 0.0f32), ("O", 1.5708), ("S", 3.1416), ("W", 4.7124)] {
                        if ui.small_button(lbl).on_hover_text("Ansicht aus dieser Himmelsrichtung").clicked() {
                            self.v3d.azimuth = az;
                        }
                    }
                });
                self.v3d.frame = self.print3d.open.then(|| self.sheet3d());
                self.sync_waters_3d();
                self.v3d.ui(ui, zr);
            });
            self.show_3d = open;
            self.print3d_dialog(ctx, zr);
        }

        self.water_window(ctx);
        self.profiles_window(ctx);
        self.qs_dialog_window(ctx);

        if self.show_bklist {
            let mut open = true;
            egui::Window::new("Bruchkantenliste").open(&mut open).show(ctx, |ui| {
                if let Some(pr) = &self.project {
                    egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
                        for (i, b) in pr.st.breaklines.iter().enumerate() {
                            let names: Vec<&str> = b.pts.iter().map(|&k| pr.st.points[k].name.as_str()).collect();
                            ui.label(format!("{:>3}: {}  {}", i + 1, names.join(" – "), if b.soft { "##" } else { "#" }));
                        }
                    });
                }
            });
            self.show_bklist = open;
        }

        if self.show_settings {
            let mut open = true;
            egui::Window::new("Voreinstellungen").open(&mut open).show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Automatisch speichern alle");
                    ui.add(egui::DragValue::new(&mut self.settings.autosave_secs).range(0..=3600).suffix(" s"));
                    ui.label("(0 = aus)");
                });
                ui.label("Gespeichert wird der ganze Projektstand als <Projekt>.dgmnt.json im Datenordner;\nder vorherige Stand bleibt als .bak erhalten. Die alten GM-Dateien werden dabei nicht verändert.");
                ui.checkbox(&mut self.settings.auto_compute, "Beim Laden automatisch vermaschen und Höhenlinien rechnen (bis 11000 Punkte)");
                if ui.button("Übernehmen").clicked() {
                    self.save_settings();
                }
            });
            if !open {
                self.save_settings();
            }
            self.show_settings = open;
        }

        if self.show_help {
            let mut open = true;
            egui::Window::new("Bedienung").open(&mut open).default_width(460.0).show(ctx, |ui| {
                ui.label("Zoom: Mausrad am Mauszeiger (stufenlos), Doppelklick vergrößert, + / − Tasten, Pos1 = Original.\nVerschieben: mit gedrückter Maustaste ziehen (mittlere Taste geht immer), Pfeiltasten = halbes Bild weiter, Strg+Klick zentriert.\nAusschnitt: Werkzeug „Ausschnitt“ und Rechteck aufziehen.\nLinien, Schriften und Punkte werden bei jedem Zoom neu und scharf gezeichnet; dichte Beschriftungen werden ausgeblendet, bis Platz ist.\n\nPolygone (Bruchkanten, Ringe, Grundriss, Profile): Punkte anklicken, rechte Maustaste beendet, Esc bricht ab.\nRinge: im Uhrzeigersinn = Umring, gegen den Uhrzeigersinn = Inring.\n\nAutospeichern: alle Änderungen werden nach der eingestellten Zeit und beim Beenden gesichert; Strg+S speichert sofort, Strg+Z macht rückgängig.");
            });
            self.show_help = open;
        }

        if self.show_about {
            let mut open = true;
            egui::Window::new("Über DGM-NT").open(&mut open).show(ctx, |ui| {
                ui.label("DGM-NT – digitales Geländemodell.\nNeubau des DGMX-Programms mit Vektorgrafik und Autospeichern.\nLiest die Projektdateien GM0, GM1, GM2, GM9 und GMG.");
            });
            self.show_about = open;
        }

        if let Some(c) = &self.confirm {
            let (title, text) = match c {
                Confirm::WriteOriginal => ("GM-Dateien schreiben", "Parameter (gm0), Bruchkanten (gm2), Umring/Inringe (gm9) und Grundriss (gmg) im Datenordner überschreiben? Die Punktdatei gm1 bleibt unverändert."),
                Confirm::Remesh => ("Vermaschen", "Dreiecke neu bilden? Von Hand getauschte oder erzeugte Dreiecke gehen dabei verloren (gelöschte bleiben gelöscht)."),
                Confirm::ReloadOriginal => ("Punkte einlesen", "Die Originaldateien (gm1, gm2, gm9 …) neu einlesen? Der bisherige Stand bleibt als Rückgängig-Schritt erhalten."),
            };
            let mut answer: Option<bool> = None;
            egui::Window::new(title).collapsible(false).resizable(false).anchor(Align2::CENTER_CENTER, Vec2::ZERO).show(ctx, |ui| {
                ui.label(text);
                ui.horizontal(|ui| {
                    if ui.button("OK").clicked() {
                        answer = Some(true);
                    }
                    if ui.button("Abbruch").clicked() {
                        answer = Some(false);
                    }
                });
            });
            if let Some(a) = answer {
                let c = self.confirm.take().unwrap();
                if a {
                    match c {
                        Confirm::WriteOriginal => {
                            if let Some(pr) = &self.project {
                                self.status = match pr.write_original_files() {
                                    Ok(_) => "GM-Dateien geschrieben.".into(),
                                    Err(e) => format!("Fehler beim Schreiben: {}", e),
                                };
                            }
                        }
                        Confirm::Remesh => {
                            self.checkpoint();
                            self.remesh_manual();
                        }
                        Confirm::ReloadOriginal => {
                            if let Some(pr) = &self.project {
                                let (dir, name) = (pr.dir.clone(), pr.st.name.clone());
                                self.start_load(dir, &name, LoadMode::Reload);
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Wasserflächen des Projekts mit Ausgleichungsebene für die Profile.
fn water_polys(pr: &Project) -> Vec<profile::WaterPoly> {
    let mut out = Vec::new();
    for w in &pr.st.waters {
        if w.pts.iter().any(|&i| i >= pr.st.points.len()) {
            continue;
        }
        if let Some(plane) = water::analyze(&pr.st.points, w).fit {
            out.push((w.pts.iter().map(|&i| (pr.st.points[i].x, pr.st.points[i].y)).collect(), plane));
        }
    }
    out
}

/// Vorschau der Profilzeichnung: dieselbe Szene wie im PDF und DXF, `zoom` Bildschirmpunkte je mm Papier.
fn profile_preview(ui: &mut Ui, sc: &scene::Scene, origin: scene::P2, lay: &profile::Layout, zoom: f32) {
    /// Höhe der Großbuchstaben der Bildschirmschrift (Anteil der Schriftgröße)
    const SCREEN_CAP: f32 = 0.7;
    let (_, _, aw, ah) = lay.sheet.area();
    let s = zoom.max(0.1);
    let (resp, painter) = ui.allocate_painter(Vec2::new(aw as f32 * s, ah as f32 * s), Sense::hover());
    let r = resp.rect;
    painter.rect_filled(r, 0.0, Color32::WHITE);
    let mm = 1000.0 / lay.n;
    let tr = |q: scene::P2| Pos2::new(r.left() + ((q[0] - origin[0]) * mm) as f32 * s, r.bottom() - ((q[1] - origin[1]) * mm) as f32 * s);
    let col = |c: scene::Rgb| Color32::from_rgb(c[0], c[1], c[2]);
    for it in &sc.items {
        match it {
            scene::Item::Line { rgb, w_mm, pts, closed, .. } => {
                let v: Vec<Pos2> = pts.iter().map(|&q| tr(q)).collect();
                let st = Stroke::new((*w_mm as f32 * s).max(0.6), col(*rgb));
                painter.add(if *closed { Shape::closed_line(v, st) } else { Shape::line(v, st) });
            }
            scene::Item::Circle { rgb, c, r_mm, .. } => {
                painter.circle_filled(tr(*c), (*r_mm as f32 * s).max(1.0), col(*rgb));
            }
            scene::Item::Text { rgb, pos, h_mm, rot_deg, ax, ay, text, .. } => {
                let font = FontId::monospace((*h_mm as f32 * s / SCREEN_CAP).max(5.0));
                let at = tr(*pos);
                if rot_deg.abs() < 1.0 {
                    let h = if *ax < 0.25 { egui::Align::Min } else if *ax < 0.75 { egui::Align::Center } else { egui::Align::Max };
                    let v = if *ay < 0.25 { egui::Align::Max } else if *ay < 0.75 { egui::Align::Center } else { egui::Align::Min };
                    painter.text(at, Align2([h, v]), text, font, col(*rgb));
                } else {
                    // senkrecht nach oben, Anfang des Texts am Punkt, mittig zur Zeile
                    let galley = painter.layout_no_wrap(text.clone(), font, col(*rgb));
                    let top_left = at - Vec2::new(galley.size().y / 2.0, 0.0);
                    painter.add(Shape::Text(egui::epaint::TextShape::new(top_left, galley, col(*rgb)).with_angle(-std::f32::consts::FRAC_PI_2)));
                }
            }
            scene::Item::Mesh { .. } => {}
        }
    }
    painter.rect_stroke(r, 0.0, Stroke::new(1.0, Color32::BLACK), StrokeKind::Inside);
}

fn profile_plot(ui: &mut Ui, prof: &[ProfilePoint], exag: f32) {
    let size = ui.available_size().max(Vec2::new(300.0, 160.0));
    let (resp, painter) = ui.allocate_painter(size, Sense::hover());
    let r = resp.rect;
    painter.rect_filled(r, 2.0, Color32::WHITE);
    let zs: Vec<f64> = prof.iter().filter_map(|p| p.z).collect();
    if zs.len() < 2 {
        painter.text(r.center(), Align2::CENTER_CENTER, "Profil liegt außerhalb des Modells.", FontId::proportional(13.0), Color32::GRAY);
        return;
    }
    let zmin = zs.iter().cloned().fold(f64::MAX, f64::min);
    let zmax = zs.iter().cloned().fold(f64::MIN, f64::max);
    let smax = prof.last().map(|p| p.station).unwrap_or(1.0).max(1e-6);
    let inner = r.shrink2(Vec2::new(50.0, 24.0));
    let sx = inner.width() as f64 / smax;
    let sz_fit = inner.height() as f64 / (zmax - zmin).max(0.1);
    let sz = (sx * exag as f64).min(sz_fit);
    let z0 = (zmin + zmax) * 0.5;
    let to = |s: f64, z: f64| Pos2::new(inner.left() + (s * sx) as f32, inner.center().y - ((z - z0) * sz) as f32);
    let zstep = nice_step(30.0 / sz);
    let mut z = (zmin / zstep).floor() * zstep;
    while z <= zmax + zstep {
        let p = to(0.0, z);
        if p.y >= inner.top() - 10.0 && p.y <= inner.bottom() + 10.0 {
            painter.line_segment([Pos2::new(inner.left(), p.y), Pos2::new(inner.right(), p.y)], Stroke::new(0.5, Color32::from_gray(215)));
            painter.text(Pos2::new(inner.left() - 4.0, p.y), Align2::RIGHT_CENTER, format!("{:.1}", z), FontId::proportional(10.0), Color32::from_gray(90));
        }
        z += zstep;
    }
    let sstep = nice_step(80.0 / sx);
    let mut s = 0.0;
    while s <= smax {
        let p = to(s, zmin);
        painter.line_segment([Pos2::new(p.x, inner.top()), Pos2::new(p.x, inner.bottom())], Stroke::new(0.5, Color32::from_gray(225)));
        painter.text(Pos2::new(p.x, inner.bottom() + 4.0), Align2::CENTER_TOP, analysis::format_station(s), FontId::proportional(10.0), Color32::from_gray(90));
        s += sstep;
    }
    let mut fill = egui::Mesh::default();
    let bottom = inner.bottom();
    for w in prof.windows(2) {
        if let (Some(a), Some(b)) = (w[0].z, w[1].z) {
            let (pa, pb) = (to(w[0].station, a), to(w[1].station, b));
            let i = fill.vertices.len() as u32;
            let c = Color32::from_rgba_unmultiplied(170, 140, 90, 90);
            fill.colored_vertex(pa, c);
            fill.colored_vertex(pb, c);
            fill.colored_vertex(Pos2::new(pb.x, bottom), c);
            fill.colored_vertex(Pos2::new(pa.x, bottom), c);
            fill.add_triangle(i, i + 1, i + 2);
            fill.add_triangle(i, i + 2, i + 3);
        }
    }
    painter.add(Shape::mesh(fill));
    let mut run: Vec<Pos2> = Vec::new();
    for p in prof {
        match p.z {
            Some(z) => run.push(to(p.station, z)),
            None => {
                if run.len() >= 2 {
                    painter.add(Shape::line(std::mem::take(&mut run), Stroke::new(2.0, Color32::from_rgb(110, 70, 20))));
                }
                run.clear();
            }
        }
    }
    if run.len() >= 2 {
        painter.add(Shape::line(run, Stroke::new(2.0, Color32::from_rgb(110, 70, 20))));
    }
    if let Some(h) = resp.hover_pos() {
        if inner.contains(h) {
            let s = (h.x - inner.left()) as f64 / sx;
            if let Some(q) = prof.iter().min_by(|a, b| (a.station - s).abs().partial_cmp(&(b.station - s).abs()).unwrap_or(std::cmp::Ordering::Equal)) {
                if let Some(z) = q.z {
                    let p = to(q.station, z);
                    painter.circle_filled(p, 4.0, Color32::from_rgb(200, 0, 120));
                    render::halo_text(&painter, p + Vec2::new(6.0, -6.0), Align2::LEFT_BOTTOM, &format!("{}  {:.2} m", analysis::format_station(q.station), z), 12.0, Color32::BLACK, Color32::WHITE);
                }
            }
        }
    }
}

impl eframe::App for DgmApp {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let now = Instant::now();
        let dt = (now - self.last_frame).as_secs_f32().min(0.1);
        self.last_frame = now;
        if self.view.step(dt) || self.blink.map_or(false, |b| b.1.elapsed().as_secs() < 3) {
            ctx.request_repaint();
        }
        self.poll_job(&ctx);
        // shortcuts (während einer Hintergrundarbeit gesperrt)
        let busy = self.job.is_some();
        let (save, undo, redo) = ctx.input(|i| {
            (
                !busy && i.modifiers.command && i.key_pressed(Key::S),
                !busy && i.modifiers.command && !i.modifiers.shift && i.key_pressed(Key::Z),
                !busy && i.modifiers.command && (i.key_pressed(Key::Y) || (i.modifiers.shift && i.key_pressed(Key::Z))),
            )
        });
        if save {
            self.dirty = true;
            if self.autosave() {
                self.status = "Projektstand gespeichert.".into();
            }
        }
        if undo {
            self.undo();
        }
        if redo {
            self.redo();
        }
        // autosave
        if !busy && self.dirty && self.settings.autosave_secs > 0 && self.last_save.elapsed() >= Duration::from_secs(self.settings.autosave_secs) {
            self.autosave();
        }
        if self.dirty {
            ctx.request_repaint_after(Duration::from_secs(1));
        }

        // Analyse: bei Änderung der Modelle oder Einstellungen neu rechnen
        if self.cmp.is_loaded() {
            let params = self.project.as_ref().map(|p| p.st.params.clone()).unwrap_or_default();
            self.cmp.update(self.project.as_ref(), self.surf_id, &params);
        }
        egui::Panel::top("menu").show(ui, |ui| {
            self.menu_bar(ui);
            self.toolbar(ui);
        });
        egui::Panel::bottom("status").show(ui, |ui| self.status_bar(ui));
        egui::Panel::left("side").resizable(true).default_size(230.0).show(ui, |ui| self.side_panel(ui));
        egui::CentralPanel::no_frame().show(ui, |ui| self.map_ui(ui));
        self.dialogs(&ctx);
        self.notice_window(&ctx);
        self.dxfz_window(&ctx);
        self.job_window(&ctx);
        if let Some(pr) = &self.project {
            let info = format!("{} Punkte, {} Dreiecke", pr.st.points.len(), self.surface.as_ref().map_or(0, |s| s.tris.len()));
            self.cmp.window(&ctx, &pr.st.name, &info, &pr.st.params);
        }
        if std::mem::take(&mut self.cmp.want_load) {
            self.load_second();
        }
        if std::mem::take(&mut self.cmp.want_gm1) {
            self.save_diff_gm1();
        }
        if std::mem::take(&mut self.cmp.want_pdf) {
            self.print.open = true;
            self.print.scale_edit = 0.0;
        }
        if std::mem::take(&mut self.cmp.want_dxf) {
            self.dxf.open = true;
        }
        if std::mem::take(&mut self.cmp.want_params) {
            if let Some(pr) = &self.project {
                self.params_edit = pr.st.params.clone();
                self.show_params = true;
            }
        }

        if self.frame == 20 && self.print.open && self.auto_export.is_none() {
            self.fit_print_area();
        }
        // Testmodus: Dateien ohne Dialog schreiben, dann beenden
        if let Some((pdf, dxf, gm1)) = self.auto_export.clone() {
            ctx.request_repaint();
            if self.frame == 20 && pdf.is_some() {
                self.print.open = true;
                self.fit_print_area();
            }
            if self.frame == 120 {
                if let Some(p) = pdf {
                    self.export_pdf(&p);
                    println!("{}", self.status);
                }
                if let Some(p) = dxf {
                    self.export_dxf_screen(&p);
                    println!("{}", self.status);
                }
                if let Some(p) = gm1 {
                    let pts = self.cmp.gm1_points();
                    let r = files::write_gm1(&p, &pts);
                    println!("GM1: {} Punkte {:?}", pts.len(), r.is_ok());
                }
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }

        // screenshot mode / PNG export
        self.frame += 1;
        if self.screenshot.is_some() {
            ctx.request_repaint();
            if self.frame == 120 && !self.shot_requested {
                self.shot_requested = true;
                let shot = self.shot.clone();
                ctx.request_screenshot(move |img| *shot.lock().unwrap() = Some(img));
            }
            if self.frame > 600 {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
        let img = self.shot.lock().unwrap().take();
        if let Some(img) = img {
            let target = self.png_target.take().or_else(|| self.screenshot.clone());
            if let Some(path) = target {
                let ok = save_png(&img, &path);
                self.status = if ok { format!("Bild gespeichert: {}", path.display()) } else { "Bild konnte nicht gespeichert werden.".into() };
            }
            if self.screenshot.is_some() {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }

    fn on_exit(&mut self) {
        if self.dirty {
            self.autosave();
        }
        self.save_settings();
    }
}

fn save_png(img: &egui::ColorImage, path: &Path) -> bool {
    let [w, h] = img.size;
    let mut buf = Vec::with_capacity(w * h * 4);
    for c in &img.pixels {
        buf.extend_from_slice(&[c.r(), c.g(), c.b(), 255]);
    }
    if let Some(d) = path.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    image::save_buffer(path, &buf, w as u32, h as u32, image::ColorType::Rgba8).is_ok()
}

fn sorted(v: [usize; 3]) -> [usize; 3] {
    let mut v = v;
    v.sort();
    v
}

fn seg_dist(p: (f64, f64), a: (f64, f64), b: (f64, f64)) -> f64 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let l2 = dx * dx + dy * dy;
    let t = if l2 > 0.0 { (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / l2).clamp(0.0, 1.0) } else { 0.0 };
    (p.0 - a.0 - dx * t).hypot(p.1 - a.1 - dy * t)
}

fn clock() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let s = secs % 86400;
    format!("{:02}:{:02}:{:02} UTC", s / 3600, (s / 60) % 60, s % 60)
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

fn round_scale(m: f64) -> u64 {
    let s = nice_step(m / 5.0) / 2.0;
    ((m / s).round() * s).max(1.0) as u64
}

fn group_thousands(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push('.');
        }
        out.push(c);
    }
    out
}
