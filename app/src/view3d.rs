//! Gittermodell: shaded 3D wire model of the surface, freely rotatable.

use egui::{Align2, Color32, FontId, Mesh, PointerButton, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, Ui, Vec2};

use crate::export::Sheet;
use crate::render::height_color;
use crate::tin::Surface;
use crate::water::PlaneFit;

pub struct Grid3d {
    pub step: f64,
    pub nx: usize,
    pub ny: usize,
    pub x0: f64,
    pub y0: f64,
    pub z: Vec<Option<f64>>,
}

pub struct View3d {
    pub azimuth: f32,
    pub elevation: f32,
    pub exaggeration: f32,
    pub zoom: f32,
    pub mesh_width: f64,
    pub shaded: bool,
    pub wire: bool,
    pub grid: Option<Grid3d>,
    pub grid_key: (f64, usize),
    /// Verschiebung der Ansicht (m in Bildrichtung, u nach rechts, v nach unten)
    pub pan: (f64, f64),
    /// Größe der Zeichenfläche im Fenster (Bildschirmpunkte), vom letzten Zeichnen
    pub view_size: Vec2,
    /// Blatt, dessen Druckrahmen eingeblendet wird (solange der PDF-Dialog offen ist)
    pub frame: Option<Sheet>,
    /// Wasserflächen, die im Modell mit gezeichnet werden
    pub waters: Vec<WaterShape>,
    pub water_rgb: [u8; 3],
}

/// Wasserfläche für die 3D-Darstellung: Umriss (x, y) und Ausgleichungsebene
pub struct WaterShape {
    pub poly: Vec<(f64, f64)>,
    pub plane: PlaneFit,
}

/// Teilung der Rasterweite für die Wasserfelder (feinerer Uferverlauf)
const WATER_SUBDIV: f64 = 2.0;
/// Höchstzahl der Wasserfelder je Richtung
const WATER_MAX_CELLS: f64 = 300.0;

fn point_in_polygon(p: (f64, f64), poly: &[(f64, f64)]) -> bool {
    let mut inside = false;
    let mut j = poly.len() - 1;
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[j]);
        if (a.1 > p.1) != (b.1 > p.1) && p.0 < (b.0 - a.0) * (p.1 - a.1) / (b.1 - a.1) + a.0 {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// Kleinste zulässige Maschenweite (m)
pub const MESH_WIDTH_MIN: f64 = 0.05;
/// Höchstzahl der Rasterfelder des Gittermodells
const MAX_GRID_CELLS: f64 = 1_000_000.0;

/// Anteil der Zeichenfläche des Blatts, den das Modell beim Einpassen höchstens füllt
const FIT_FILL: f64 = 0.96;
/// Anteil der Fensterfläche, den das Modell bei Zoom 1 einnimmt
const BASE_FILL: f64 = 0.85;
/// Abstand des Blatts zum Fensterrand (Bildschirmpunkte)
const PAGE_MARGIN: f32 = 22.0;
/// Grenzen des Zooms
const ZOOM_MIN: f32 = 0.05;
const ZOOM_MAX: f32 = 200.0;

/// Lage des Blatts im Fenster: Blattrechteck, Zeichenfläche und Bildschirmpunkte je mm.
pub struct FrameGeom {
    pub page: Rect,
    pub area: Rect,
    pub pxmm: f32,
}

pub fn frame_geom(rect: Rect, sheet: &Sheet) -> FrameGeom {
    let (w, h) = sheet.size();
    let avail = rect.shrink(PAGE_MARGIN);
    let s = (avail.width() / w as f32).min(avail.height() / h as f32).max(0.05);
    let page = Rect::from_center_size(rect.center(), Vec2::new(w as f32 * s, h as f32 * s));
    let (ax, ay, aw, ah) = sheet.area();
    let min = Pos2::new(page.left() + ax as f32 * s, page.top() + (h - ay - ah) as f32 * s);
    FrameGeom { page, area: Rect::from_min_size(min, Vec2::new(aw as f32 * s, ah as f32 * s)), pxmm: s }
}

impl Default for View3d {
    fn default() -> Self {
        View3d { azimuth: 0.6, elevation: 0.55, exaggeration: 2.0, zoom: 1.0, mesh_width: 0.0, shaded: true, wire: true, grid: None, grid_key: (0.0, 0), pan: (0.0, 0.0), view_size: Vec2::ZERO, frame: None, waters: Vec::new(), water_rgb: crate::water::DEFAULT_WATER_RGB }
    }
}

impl View3d {
    pub fn build(&mut self, s: &Surface, area: [f64; 4], surf_id: usize) {
        let w = (area[2] - area[0]).max(1.0);
        let h = (area[3] - area[1]).max(1.0);
        if self.mesh_width <= 0.0 {
            self.mesh_width = crate::view::nice_step(w.max(h) / 70.0);
        }
        // Rasterfelder insgesamt begrenzen (Speicher, Zeichnen); kleinere Maschenweiten sind sonst frei wählbar
        let step = self.mesh_width.max(MESH_WIDTH_MIN).max((w * h / MAX_GRID_CELLS).sqrt());
        if self.grid.is_some() && self.grid_key == (step, surf_id) {
            return;
        }
        let nx = (w / step).ceil() as usize + 1;
        let ny = (h / step).ceil() as usize + 1;
        let mut z = Vec::with_capacity(nx * ny);
        for j in 0..ny {
            for i in 0..nx {
                z.push(s.height(area[0] + i as f64 * step, area[1] + j as f64 * step));
            }
        }
        self.grid = Some(Grid3d { step, nx, ny, x0: area[0], y0: area[1], z });
        self.grid_key = (step, surf_id);
    }

    pub fn ui(&mut self, ui: &mut Ui, zr: (f64, f64)) {
        let Some(g) = &self.grid else {
            ui.label("Noch kein Modell vorhanden.");
            return;
        };
        let size = ui.available_size().max(egui::vec2(200.0, 200.0));
        let (resp, painter) = ui.allocate_painter(size, Sense::click_and_drag());
        let rect = resp.rect;
        painter.rect_filled(rect, 0.0, Color32::from_rgb(236, 240, 245));
        self.view_size = rect.size();
        let base = Self::base_scale(rect.size(), g);
        if resp.dragged_by(PointerButton::Primary) {
            let d = resp.drag_delta();
            self.azimuth -= d.x * 0.01;
            self.elevation = (self.elevation + d.y * 0.01).clamp(0.05, 1.5);
        } else if resp.dragged_by(PointerButton::Secondary) || resp.dragged_by(PointerButton::Middle) {
            let d = resp.drag_delta();
            let sc = base * self.zoom as f64;
            self.pan = (self.pan.0 + d.x as f64 / sc, self.pan.1 + d.y as f64 / sc);
        }
        if resp.hovered() {
            let sc = ui.input(|i| i.smooth_scroll_delta.y);
            if sc != 0.0 {
                self.zoom = (self.zoom * (1.0 + sc * 0.002)).clamp(ZOOM_MIN, ZOOM_MAX);
            }
        }
        let scale = base * self.zoom as f64;
        let c = rect.center() + Vec2::new((self.pan.0 * scale) as f32, (self.pan.1 * scale) as f32);
        let wire = Stroke::new(0.7, Color32::from_rgba_unmultiplied(30, 40, 50, 170));
        let mut mesh = Mesh::default();
        let mut lines: Vec<Shape> = Vec::new();
        for q in self.quads(zr) {
            let sp: Vec<Pos2> = q.p.iter().map(|&(u, v)| Pos2::new(c.x + (u * scale) as f32, c.y + (v * scale) as f32)).collect();
            let col = Color32::from_rgb(q.rgb[0], q.rgb[1], q.rgb[2]);
            let idx = mesh.vertices.len() as u32;
            for p in &sp {
                mesh.colored_vertex(*p, col);
            }
            mesh.add_triangle(idx, idx + 1, idx + 2);
            mesh.add_triangle(idx, idx + 2, idx + 3);
            let wired = self.wire && !q.water;
            if wired && !self.shaded {
                lines.push(Shape::closed_line(sp, wire));
            } else if wired {
                // keep the painter order: flush the mesh before the wire of this quad
                let m = std::mem::take(&mut mesh);
                lines.push(Shape::mesh(m));
                lines.push(Shape::closed_line(sp, wire));
            }
        }
        if !mesh.vertices.is_empty() {
            lines.push(Shape::mesh(mesh));
        }
        let painter = painter.with_clip_rect(rect);
        painter.extend(lines);
        if let Some(sheet) = &self.frame {
            Self::draw_frame(&painter, rect, sheet, 1000.0 * frame_geom(rect, sheet).pxmm as f64 / scale);
        }
        painter.text(
            rect.left_top() + egui::vec2(8.0, 6.0),
            egui::Align2::LEFT_TOP,
            format!("Maschenweite {:.2} m · Überhöhung {:.1}× · ziehen = drehen, rechte Maustaste = verschieben, Mausrad = Zoom", g.step, self.exaggeration),
            egui::FontId::proportional(12.0),
            Color32::from_gray(60),
        );
    }
}

impl View3d {
    /// Bildschirmpunkte je m bei Zoom 1.
    fn base_scale(size: Vec2, g: &Grid3d) -> f64 {
        let w = (g.nx - 1) as f64 * g.step;
        let h = (g.ny - 1) as f64 * g.step;
        size.x.min(size.y) as f64 * BASE_FILL / w.max(h)
    }

    /// Maßstabszahl N (1:N) der Ausgabe auf dem Blatt bei der jetzigen Ansicht.
    pub fn print_n(&self, sheet: &Sheet) -> Option<f64> {
        let g = self.grid.as_ref()?;
        if self.view_size.x < 1.0 {
            return None;
        }
        let pxmm = frame_geom(Rect::from_min_size(Pos2::ZERO, self.view_size), sheet).pxmm as f64;
        Some(1000.0 * pxmm / (Self::base_scale(self.view_size, g) * self.zoom as f64))
    }

    /// Zoom so setzen, dass der Druckmaßstab 1:`n` entsteht.
    pub fn set_print_n(&mut self, sheet: &Sheet, n: f64) {
        let Some(g) = self.grid.as_ref() else { return };
        let pxmm = frame_geom(Rect::from_min_size(Pos2::ZERO, self.view_size), sheet).pxmm as f64;
        let zoom = 1000.0 * pxmm / n.max(1.0) / Self::base_scale(self.view_size, g);
        self.zoom = (zoom as f32).clamp(ZOOM_MIN, ZOOM_MAX);
    }

    /// Modell mittig in die Zeichenfläche des Blatts einpassen: Normmaßstab und Verschiebung.
    pub fn fit_print(&mut self, sheet: &Sheet, zr: (f64, f64), standard: impl Fn(f64) -> f64) {
        let Some(e) = self.extent(zr) else { return };
        let (_, _, aw, ah) = sheet.area();
        let n = standard((((e[2] - e[0]) / aw).max((e[3] - e[1]) / ah) * 1000.0 / FIT_FILL).max(1.0));
        self.set_print_n(sheet, n);
        // Mitte der Zeichenfläche relativ zur Blattmitte (mm, y nach unten)
        let (w, h) = sheet.size();
        let (ax, ay, aw, ah) = sheet.area();
        let off = (ax + aw * 0.5 - w * 0.5, (h - ay - ah * 0.5) - h * 0.5);
        let mm = 1000.0 / n;
        self.pan = (off.0 / mm - (e[0] + e[2]) * 0.5, off.1 / mm - (e[1] + e[3]) * 0.5);
    }

    /// Druckrahmen wie bei der Druckausgabe: Blatt, Zeichenfläche (nur sie wird gedruckt), Legendenstreifen.
    fn draw_frame(painter: &egui::Painter, rect: Rect, sheet: &Sheet, n: f64) {
        let FrameGeom { page, area, .. } = frame_geom(rect, sheet);
        let dim = Color32::from_rgba_unmultiplied(205, 207, 212, 200);
        let paper = Color32::from_rgba_unmultiplied(255, 255, 255, 228);
        for r in [
            Rect::from_min_max(rect.left_top(), Pos2::new(rect.right(), page.top())),
            Rect::from_min_max(Pos2::new(rect.left(), page.bottom()), rect.right_bottom()),
            Rect::from_min_max(Pos2::new(rect.left(), page.top()), Pos2::new(page.left(), page.bottom())),
            Rect::from_min_max(Pos2::new(page.right(), page.top()), Pos2::new(rect.right(), page.bottom())),
        ] {
            painter.rect_filled(r, 0.0, dim);
        }
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
        let strip = if sheet.landscape {
            Rect::from_min_max(Pos2::new(area.right(), area.top()), page.right_bottom())
        } else {
            Rect::from_min_max(Pos2::new(area.left(), area.bottom()), page.right_bottom())
        };
        let c = strip.center();
        let grey = Color32::from_gray(110);
        painter.text(c - Vec2::new(0.0, 12.0), Align2::CENTER_CENTER, "Legende", FontId::proportional(14.0), grey);
        painter.text(c + Vec2::new(0.0, 6.0), Align2::CENTER_CENTER, format!("Maßstab 1:{}", n.round().max(1.0)), FontId::proportional(15.0), Color32::from_rgb(160, 0, 100));
        painter.text(c + Vec2::new(0.0, 24.0), Align2::CENTER_CENTER, "Maßstabsleiste · Nordpfeil", FontId::proportional(11.0), grey);
        painter.text(Pos2::new(page.center().x, page.top() - 6.0), Align2::CENTER_BOTTOM, "Druckvorschau A4 – Ansicht drehen, zoomen, verschieben; gedruckt wird der Bereich im schwarzen Rahmen", FontId::proportional(12.0), Color32::from_gray(50));
    }
}

/// Ein Rasterfeld in Blattkoordinaten: Ecken (u nach rechts, v nach unten, in m, um die Modellmitte) und Farbe.
pub struct ProjQuad {
    pub p: [(f64, f64); 4],
    pub rgb: [u8; 3],
    /// Feld gehört zu einer Wasserfläche (ohne Gitterlinien)
    pub water: bool,
}

impl View3d {
    /// Parallelprojektion der Rasterpunkte: (u, v, Tiefe), u nach rechts, v nach unten (m), Tiefe größer = weiter weg.
    fn projector(&self, g: &Grid3d, zr: (f64, f64)) -> impl Fn(f64, f64, f64) -> (f64, f64, f64) {
        let w = (g.nx - 1) as f64 * g.step;
        let h = (g.ny - 1) as f64 * g.step;
        let (cx, cy) = (g.x0 + w * 0.5, g.y0 + h * 0.5);
        let zc = (zr.0 + zr.1) * 0.5;
        let ex = self.exaggeration as f64;
        let (sa, ca) = (self.azimuth.sin() as f64, self.azimuth.cos() as f64);
        let (se, ce) = (self.elevation.sin() as f64, self.elevation.cos() as f64);
        move |x, y, z| {
            let (dx, dy, dz) = (x - cx, y - cy, (z - zc) * ex);
            let rx = dx * ca - dy * sa;
            let ry = dx * sa + dy * ca;
            (rx, ry * se - dz * ce, -(ry * ce + dz * se))
        }
    }

    /// Umgrenzung (u_min, v_min, u_max, v_max) des projizierten Modells in m; `None` ohne Modell.
    pub fn extent(&self, zr: (f64, f64)) -> Option<[f64; 4]> {
        let g = self.grid.as_ref()?;
        let proj = self.projector(g, zr);
        let mut b = [f64::MAX, f64::MAX, f64::MIN, f64::MIN];
        for j in 0..g.ny {
            for i in 0..g.nx {
                if let Some(z) = g.z[j * g.nx + i] {
                    let (u, v, _) = proj(g.x0 + i as f64 * g.step, g.y0 + j as f64 * g.step, z);
                    b = [b[0].min(u), b[1].min(v), b[2].max(u), b[3].max(v)];
                }
            }
        }
        (b[0] <= b[2]).then_some(b)
    }

    /// Richtung nach Norden in der Ansicht (x nach rechts, y nach oben) für den Nordpfeil.
    pub fn north_dir(&self) -> (f64, f64) {
        let (sa, ca) = (self.azimuth.sin() as f64, self.azimuth.cos() as f64);
        (-sa, -ca * self.elevation.sin() as f64)
    }

    /// Alle Rasterfelder mit Schattierung, von hinten nach vorn sortiert (Malerverfahren).
    pub fn quads(&self, zr: (f64, f64)) -> Vec<ProjQuad> {
        let Some(g) = &self.grid else { return Vec::new() };
        let proj = self.projector(g, zr);
        let ex = self.exaggeration as f64;
        let at = |i: usize, j: usize| g.z[j * g.nx + i];
        let light = [-0.5f64, 0.5, 0.7];
        let ll = (light[0] * light[0] + light[1] * light[1] + light[2] * light[2]).sqrt();
        // Schattierung aus der Flächenneigung (nx, ny: Gefälle je Meter, schon überhöht)
        let shade_of = |nx: f64, ny: f64| {
            let nl = (nx * nx + ny * ny + 1.0).sqrt();
            ((nx * light[0] + ny * light[1] + light[2]) / (nl * ll)).clamp(0.0, 1.0)
        };
        let lit = |base: Color32, shade: f64| {
            let k = (0.45 + 0.55 * shade) as f32;
            [(base.r() as f32 * k) as u8, (base.g() as f32 * k) as u8, (base.b() as f32 * k) as u8]
        };
        let mut items: Vec<(f64, ProjQuad)> = Vec::new();
        for j in 0..g.ny - 1 {
            for i in 0..g.nx - 1 {
                let (Some(a), Some(b), Some(cc), Some(d)) = (at(i, j), at(i + 1, j), at(i + 1, j + 1), at(i, j + 1)) else { continue };
                let zs = [a, b, cc, d];
                let xs = [g.x0 + i as f64 * g.step, g.x0 + (i + 1) as f64 * g.step];
                let ys = [g.y0 + j as f64 * g.step, g.y0 + (j + 1) as f64 * g.step];
                let corners = [(xs[0], ys[0], zs[0]), (xs[1], ys[0], zs[1]), (xs[1], ys[1], zs[2]), (xs[0], ys[1], zs[3])];
                let p = corners.map(|(x, y, z)| {
                    let q = proj(x, y, z);
                    (q.0, q.1)
                });
                let dep = proj((xs[0] + xs[1]) * 0.5, (ys[0] + ys[1]) * 0.5, zs.iter().sum::<f64>() * 0.25).2;
                let shade = shade_of(-((zs[1] - zs[0]) + (zs[2] - zs[3])) * 0.5 * ex / g.step, -((zs[3] - zs[0]) + (zs[2] - zs[1])) * 0.5 * ex / g.step);
                let base = if self.shaded { height_color((zs.iter().sum::<f64>() * 0.25 - zr.0) / (zr.1 - zr.0).max(1e-9)) } else { Color32::WHITE };
                items.push((dep, ProjQuad { p, rgb: lit(base, shade), water: false }));
            }
        }
        // Wasserflächen: kleine Felder auf der Ausgleichungsebene, mit den Geländefeldern nach Tiefe sortiert
        // (Gelände über dem Wasser verdeckt es, Wasser über dem Gelände verdeckt dieses)
        for ws in &self.waters {
            let (bx0, by0, bx1, by1) = ws.poly.iter().fold((f64::MAX, f64::MAX, f64::MIN, f64::MIN), |b, q| (b.0.min(q.0), b.1.min(q.1), b.2.max(q.0), b.3.max(q.1)));
            let cs = (g.step / WATER_SUBDIV).max((bx1 - bx0).max(by1 - by0) / WATER_MAX_CELLS);
            if !(cs > 0.0) || ws.poly.len() < 3 {
                continue;
            }
            let shade = shade_of(-ws.plane.a * ex, -ws.plane.b * ex);
            let rgb = lit(Color32::from_rgb(self.water_rgb[0], self.water_rgb[1], self.water_rgb[2]), shade);
            let (nx, ny) = (((bx1 - bx0) / cs).ceil() as usize, ((by1 - by0) / cs).ceil() as usize);
            for j in 0..ny {
                for i in 0..nx {
                    let (x0, y0) = (bx0 + i as f64 * cs, by0 + j as f64 * cs);
                    if !point_in_polygon((x0 + cs * 0.5, y0 + cs * 0.5), &ws.poly) {
                        continue;
                    }
                    let c = [(x0, y0), (x0 + cs, y0), (x0 + cs, y0 + cs), (x0, y0 + cs)];
                    let p = c.map(|(x, y)| {
                        let q = proj(x, y, ws.plane.z_at(x, y));
                        (q.0, q.1)
                    });
                    let dep = proj(x0 + cs * 0.5, y0 + cs * 0.5, ws.plane.z_at(x0 + cs * 0.5, y0 + cs * 0.5)).2;
                    items.push((dep, ProjQuad { p, rgb, water: true }));
                }
            }
        }
        items.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        let out: Vec<ProjQuad> = items.into_iter().map(|(_, q)| q).collect();
        out
    }
}
