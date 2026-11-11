//! Gittermodell: shaded 3D wire model of the surface, freely rotatable.

use egui::{Color32, Mesh, Pos2, Rect, Sense, Shape, Stroke, Ui};

use crate::render::height_color;
use crate::tin::Surface;

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
}

impl Default for View3d {
    fn default() -> Self {
        View3d { azimuth: 0.6, elevation: 0.55, exaggeration: 2.0, zoom: 1.0, mesh_width: 0.0, shaded: true, wire: true, grid: None, grid_key: (0.0, 0) }
    }
}

impl View3d {
    pub fn build(&mut self, s: &Surface, area: [f64; 4], surf_id: usize) {
        let w = (area[2] - area[0]).max(1.0);
        let h = (area[3] - area[1]).max(1.0);
        if self.mesh_width <= 0.0 {
            self.mesh_width = crate::view::nice_step(w.max(h) / 70.0);
        }
        let step = self.mesh_width.max(w.max(h) / 400.0);
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
        if resp.dragged() {
            let d = resp.drag_delta();
            self.azimuth -= d.x * 0.01;
            self.elevation = (self.elevation + d.y * 0.01).clamp(0.05, 1.5);
        }
        if resp.hovered() {
            let sc = ui.input(|i| i.smooth_scroll_delta.y);
            if sc != 0.0 {
                self.zoom = (self.zoom * (1.0 + sc * 0.002)).clamp(0.2, 20.0);
            }
        }
        let w = (g.nx - 1) as f64 * g.step;
        let h = (g.ny - 1) as f64 * g.step;
        let (cx, cy) = (g.x0 + w * 0.5, g.y0 + h * 0.5);
        let zc = (zr.0 + zr.1) * 0.5;
        let ex = self.exaggeration as f64;
        let (sa, ca) = (self.azimuth.sin() as f64, self.azimuth.cos() as f64);
        let (se, ce) = (self.elevation.sin() as f64, self.elevation.cos() as f64);
        let scale = (rect.width().min(rect.height()) as f64 * 0.85 / w.max(h)) * self.zoom as f64;
        let c = rect.center();
        // project: returns screen pos and depth (bigger = farther)
        let proj = |x: f64, y: f64, z: f64| -> (Pos2, f64) {
            let (dx, dy, dz) = (x - cx, y - cy, (z - zc) * ex);
            let rx = dx * ca - dy * sa;
            let ry = dx * sa + dy * ca;
            let sy = ry * se - dz * ce;
            let depth = ry * ce + dz * se;
            (Pos2::new(c.x + (rx * scale) as f32, c.y + (sy * scale) as f32 + (0.0) as f32), -depth)
        };
        let at = |i: usize, j: usize| g.z[j * g.nx + i];
        // quads sorted back to front
        let mut quads: Vec<(f64, usize, usize)> = Vec::new();
        for j in 0..g.ny - 1 {
            for i in 0..g.nx - 1 {
                if let (Some(a), Some(b), Some(cc), Some(d)) = (at(i, j), at(i + 1, j), at(i + 1, j + 1), at(i, j + 1)) {
                    let x = g.x0 + (i as f64 + 0.5) * g.step;
                    let y = g.y0 + (j as f64 + 0.5) * g.step;
                    let (_, dep) = proj(x, y, (a + b + cc + d) * 0.25);
                    quads.push((dep, i, j));
                }
            }
        }
        quads.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        let light = [-0.5f64, 0.5, 0.7];
        let ll = (light[0] * light[0] + light[1] * light[1] + light[2] * light[2]).sqrt();
        let mut mesh = Mesh::default();
        let mut lines: Vec<Shape> = Vec::new();
        let wire = Stroke::new(0.7, Color32::from_rgba_unmultiplied(30, 40, 50, 170));
        for (_, i, j) in quads {
            let zs = [at(i, j).unwrap(), at(i + 1, j).unwrap(), at(i + 1, j + 1).unwrap(), at(i, j + 1).unwrap()];
            let xs = [g.x0 + i as f64 * g.step, g.x0 + (i + 1) as f64 * g.step];
            let ys = [g.y0 + j as f64 * g.step, g.y0 + (j + 1) as f64 * g.step];
            let corners = [(xs[0], ys[0], zs[0]), (xs[1], ys[0], zs[1]), (xs[1], ys[1], zs[2]), (xs[0], ys[1], zs[3])];
            let sp: Vec<Pos2> = corners.iter().map(|&(x, y, z)| proj(x, y, z).0).collect();
            // normal for shading
            let nx = -((zs[1] - zs[0]) + (zs[2] - zs[3])) * 0.5 * ex / g.step;
            let ny = -((zs[3] - zs[0]) + (zs[2] - zs[1])) * 0.5 * ex / g.step;
            let nl = (nx * nx + ny * ny + 1.0).sqrt();
            let shade = ((nx * light[0] + ny * light[1] + light[2]) / (nl * ll)).clamp(0.0, 1.0);
            let base = if self.shaded {
                height_color((zs.iter().sum::<f64>() * 0.25 - zr.0) / (zr.1 - zr.0).max(1e-9))
            } else {
                Color32::WHITE
            };
            let k = (0.45 + 0.55 * shade) as f32;
            let col = Color32::from_rgb((base.r() as f32 * k) as u8, (base.g() as f32 * k) as u8, (base.b() as f32 * k) as u8);
            let idx = mesh.vertices.len() as u32;
            for p in &sp {
                mesh.colored_vertex(*p, col);
            }
            mesh.add_triangle(idx, idx + 1, idx + 2);
            mesh.add_triangle(idx, idx + 2, idx + 3);
            if self.wire && !self.shaded {
                lines.push(Shape::closed_line(sp, wire));
            } else if self.wire {
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
        let _ = Rect::NOTHING;
        painter.text(
            rect.left_top() + egui::vec2(8.0, 6.0),
            egui::Align2::LEFT_TOP,
            format!("Maschenweite {:.2} m · Überhöhung {:.1}× · ziehen = drehen, Mausrad = Zoom", g.step, self.exaggeration),
            egui::FontId::proportional(12.0),
            Color32::from_gray(60),
        );
    }
}
