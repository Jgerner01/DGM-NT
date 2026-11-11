//! Map view: world (Rechtswert/Hochwert, metres) <-> screen, with smooth animated zoom.

use egui::{Pos2, Rect, Vec2};

#[derive(Clone, Copy, Debug)]
pub struct View {
    /// world coordinates of the view centre
    pub cx: f64,
    pub cy: f64,
    /// screen points per metre
    pub ppm: f64,
    target: Option<(f64, f64, f64)>,
    /// world point kept under a screen offset while a zoom animates
    anchor: Option<(f64, f64, f64, f64)>,
}

impl Default for View {
    fn default() -> Self {
        View { cx: 0.0, cy: 0.0, ppm: 1.0, target: None, anchor: None }
    }
}

/// Zoom limits: from a whole country down to millimetres.
pub const MIN_PPM: f64 = 1e-4;
pub const MAX_PPM: f64 = 20000.0;

impl View {
    pub fn to_screen(&self, rect: Rect, x: f64, y: f64) -> Pos2 {
        let c = rect.center();
        Pos2::new(c.x + ((x - self.cx) * self.ppm) as f32, c.y - ((y - self.cy) * self.ppm) as f32)
    }
    pub fn to_world(&self, rect: Rect, p: Pos2) -> (f64, f64) {
        let c = rect.center();
        (self.cx + (p.x - c.x) as f64 / self.ppm, self.cy - (p.y - c.y) as f64 / self.ppm)
    }
    /// visible world rectangle [x0, y0, x1, y1]
    pub fn world_rect(&self, rect: Rect) -> [f64; 4] {
        let (x0, y1) = self.to_world(rect, rect.left_top());
        let (x1, y0) = self.to_world(rect, rect.right_bottom());
        [x0, y0, x1, y1]
    }

    fn current_target(&self) -> (f64, f64, f64) {
        self.target.unwrap_or((self.cx, self.cy, self.ppm))
    }

    /// Zoom by `factor` keeping the world point under `anchor` fixed (animated).
    pub fn zoom_at(&mut self, rect: Rect, anchor: Pos2, factor: f64) {
        let (tcx, tcy, tppm) = self.current_target();
        let nppm = (tppm * factor).clamp(MIN_PPM, MAX_PPM);
        let c = rect.center();
        let dx = (anchor.x - c.x) as f64;
        let dy = (anchor.y - c.y) as f64;
        // world point under the anchor at the target state
        let wx = tcx + dx / tppm;
        let wy = tcy - dy / tppm;
        self.target = Some((wx - dx / nppm, wy + dy / nppm, nppm));
        self.anchor = Some((wx, wy, dx, dy));
    }

    pub fn pan_pixels(&mut self, d: Vec2) {
        self.cx -= d.x as f64 / self.ppm;
        self.cy += d.y as f64 / self.ppm;
        if let Some((tx, ty, tp)) = self.target {
            self.target = Some((tx - d.x as f64 / tp, ty + d.y as f64 / tp, tp));
        }
        if let Some((wx, wy, dx, dy)) = self.anchor {
            self.anchor = Some((wx, wy, dx + d.x as f64, dy + d.y as f64));
        }
    }

    pub fn animate_to(&mut self, cx: f64, cy: f64, ppm: f64) {
        self.target = Some((cx, cy, ppm.clamp(MIN_PPM, MAX_PPM)));
        self.anchor = None;
    }

    pub fn fit(&mut self, rect: Rect, b: [f64; 4], margin: f32, animate: bool) {
        let w = (b[2] - b[0]).max(1e-3);
        let h = (b[3] - b[1]).max(1e-3);
        let ppm = ((rect.width() - 2.0 * margin).max(50.0) as f64 / w).min((rect.height() - 2.0 * margin).max(50.0) as f64 / h);
        let (cx, cy) = ((b[0] + b[2]) * 0.5, (b[1] + b[3]) * 0.5);
        if animate {
            self.animate_to(cx, cy, ppm);
        } else {
            self.cx = cx;
            self.cy = cy;
            self.ppm = ppm.clamp(MIN_PPM, MAX_PPM);
            self.target = None;
            self.anchor = None;
        }
    }

    /// Advance the animation; returns true while still moving.
    pub fn step(&mut self, dt: f32) -> bool {
        let Some((tx, ty, tp)) = self.target else { return false };
        let k = 1.0 - (-(dt as f64) * 14.0).exp();
        // interpolate zoom in log space so it feels even
        let lp = self.ppm.ln() + (tp.ln() - self.ppm.ln()) * k;
        // keep the anchor consistent: interpolate centre linearly
        self.ppm = lp.exp();
        if let Some((wx, wy, dx, dy)) = self.anchor {
            self.cx = wx - dx / self.ppm;
            self.cy = wy + dy / self.ppm;
        } else {
            self.cx += (tx - self.cx) * k;
            self.cy += (ty - self.cy) * k;
        }
        if (self.ppm / tp - 1.0).abs() < 1e-3 && ((self.cx - tx) * tp).abs() < 0.3 && ((self.cy - ty) * tp).abs() < 0.3 {
            self.cx = tx;
            self.cy = ty;
            self.ppm = tp;
            self.target = None;
            self.anchor = None;
            return false;
        }
        true
    }

    pub fn is_animating(&self) -> bool {
        self.target.is_some()
    }
}

/// A "nice" step (1, 2, 5 × 10^n) not smaller than `v`.
pub fn nice_step(v: f64) -> f64 {
    if v <= 0.0 || !v.is_finite() {
        return 1.0;
    }
    let e = v.log10().floor();
    let b = 10f64.powf(e);
    for m in [1.0, 2.0, 5.0, 10.0] {
        if m * b >= v {
            return m * b;
        }
    }
    10.0 * b
}
