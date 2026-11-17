//! Reading and writing the project files of the original program
//! (GM0 parameters, GM1 points, GM2 breaklines, GM9 rings, GMG plan lines)
//! plus the exports (DXF, KOR, profile and volume lists).
//! Layouts are documented in MECHANICS.md.

use std::fs;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use crate::job::ProgressFn;
use crate::model::{Params, Point};

/// Fortschrittsmeldung beim Einlesen alle so viele Zeilen.
const READ_PROGRESS_LINES: usize = 2048;

/// Read a text file the way the old program wrote it (Windows-1252, CR/LF).
pub fn read_text(path: &Path) -> std::io::Result<String> {
    let bytes = fs::read(path)?;
    Ok(bytes
        .iter()
        .filter(|b| **b != 0x1a)
        .map(|&b| cp1252(b))
        .collect())
}

fn cp1252(b: u8) -> char {
    match b {
        0x80 => '€',
        0x8a => 'Š',
        0x9a => 'š',
        0x8e => 'Ž',
        0x9e => 'ž',
        0x96 => '–',
        0x97 => '—',
        _ => b as char, // Latin-1 range matches Windows-1252 for umlauts
    }
}

pub fn to_cp1252(s: &str) -> Vec<u8> {
    s.chars()
        .map(|c| if (c as u32) < 256 { c as u8 } else { b'?' })
        .collect()
}

pub fn write_text(path: &Path, text: &str) -> std::io::Result<()> {
    let text = text.replace("\r\n", "\n").replace('\n', "\r\n");
    let tmp = path.with_extension("tmp~");
    {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(&to_cp1252(&text))?;
        f.sync_all()?;
    }
    fs::rename(&tmp, path)
}

/// Find a file next to `base` with the given extension, ignoring case
/// (the old data folders mix `X.GM1`, `x.gm1` …).
pub fn find_ext(dir: &Path, stem: &str, ext: &str) -> Option<PathBuf> {
    let want = format!("{}.{}", stem, ext).to_lowercase();
    let rd = fs::read_dir(dir).ok()?;
    for e in rd.flatten() {
        let n = e.file_name().to_string_lossy().to_lowercase();
        if n == want {
            return Some(e.path());
        }
    }
    None
}

/// All projects (= GM1 point files) in a data folder.
pub fn list_projects(dir: &Path) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    if let Ok(rd) = fs::read_dir(dir) {
        for e in rd.flatten() {
            let n = e.file_name().to_string_lossy().to_string();
            let l = n.to_lowercase();
            if l.ends_with(".gm1") && !l.contains(".prj") {
                let size = e.metadata().map(|m| m.len()).unwrap_or(0);
                if size > 0 {
                    let stem = n[..n.len() - 4].to_string();
                    if !out.iter().any(|o| o.eq_ignore_ascii_case(&stem)) {
                        out.push(stem);
                    }
                }
            }
        }
    }
    out.sort_by_key(|s| s.to_lowercase());
    out
}

// ---------------------------------------------------------------- GM1

/// GM1: `Punktbezeichnung Rechtswert Hochwert Höhe [Code]`, separated by blanks/tabs.
#[allow(dead_code)]
pub fn read_gm1(path: &Path, p: &Params) -> std::io::Result<Vec<Point>> {
    read_gm1_progress(path, p, &mut |_, _| {})
}

/// Wie `read_gm1`, meldet aber den Fortschritt (gelesene Bytes / Dateigröße).
pub fn read_gm1_progress(path: &Path, p: &Params, progress: ProgressFn) -> std::io::Result<Vec<Point>> {
    const STAGE: &str = "Punkte einlesen";
    progress(0.0, STAGE);
    let text = read_text(path)?;
    let total = text.len().max(1) as f32;
    let mut done = 0usize;
    let mut pts = Vec::new();
    for (n, line) in text.lines().enumerate() {
        done += line.len() + 1;
        if n % READ_PROGRESS_LINES == 0 {
            progress(done as f32 / total, STAGE);
        }
        let t: Vec<&str> = line.split_whitespace().collect();
        if t.len() < 4 {
            continue;
        }
        let (Ok(x), Ok(y), Ok(z)) = (
            t[1].replace(',', ".").parse::<f64>(),
            t[2].replace(',', ".").parse::<f64>(),
            t[3].replace(',', ".").parse::<f64>(),
        ) else {
            continue;
        };
        pts.push(Point {
            name: t[0].to_string(),
            x: x * p.scale_xy,
            y: y * p.scale_xy,
            z: z * p.scale_h,
            code: t.get(4).map(|s| s.to_string()).unwrap_or_default(),
            active: true,
            label_pos: 0,
            added: false,
        });
    }
    progress(1.0, STAGE);
    Ok(pts)
}

// ---------------------------------------------------------------- GM0

/// GM0 line layout as read by the original at 0x51322c (see MECHANICS.md).
pub fn read_gm0(path: &Path) -> Option<Params> {
    let text = read_text(path).ok()?;
    let lines: Vec<&str> = text.lines().collect();
    let f = |i: usize, d: f64| -> f64 {
        lines
            .get(i)
            .and_then(|l| l.trim().replace(',', ".").parse::<f64>().ok())
            .unwrap_or(d)
    };
    let mut p = Params::default();
    p.contour_interval = f(0, p.contour_interval);
    p.max_side = f(1, p.max_side);
    p.min_angle_gon = f(2, p.min_angle_gon);
    p.subdivision = f(3, p.subdivision as f64).clamp(0.0, 15.0) as u32;
    p.scale_xy = f(4, 1.0);
    p.scale_h = f(5, 1.0);
    p.win = [f(6, 0.0), f(7, 0.0), f(8, 10.0), f(9, 10.0)];
    p.h_low = f(10, 0.0);
    p.h_high = f(11, 0.0);
    p.decimals_h = f(12, 2.0) as u32;
    p.decimals_index = f(13, 0.0) as u32;
    p.scale = f(14, 1000.0);
    p.v16 = f(15, 2.0);
    p.v17 = f(16, 3.0);
    p.border_mm = f(17, 20.0);
    p.grid_mm = f(18, 100.0);
    p.thin_mm = f(19, 0.05);
    p.min_dist_mm = f(20, 2.0);
    p.separator = lines.get(21).map(|s| s.trim().to_string()).unwrap_or_default();
    p.smoothing = lines
        .get(22)
        .map(|s| {
            let s = s.trim();
            if s.eq_ignore_ascii_case("A") {
                10
            } else {
                s.parse::<u32>().unwrap_or(0).min(10)
            }
        })
        .unwrap_or(0);
    p.path_line = lines.get(23).map(|s| s.trim().to_string()).unwrap_or_default();
    for i in 0..13 {
        if let Some(v) = lines.get(24 + i).and_then(|l| l.trim().parse::<i32>().ok()) {
            p.layers[i] = v;
        }
    }
    // Erweiterung (neu): Export-Parameter in den Zeilen 38.. – das alte Programm liest nur 37 Zeilen
    p.exp_circle_mm = f(37, p.exp_circle_mm).max(0.0);
    p.exp_text_nr_mm = f(38, p.exp_text_nr_mm).max(0.0);
    p.exp_text_h_mm = f(39, p.exp_text_h_mm).max(0.0);
    p.exp_text_index_mm = f(40, p.exp_text_index_mm).max(0.0);
    p.exp_text_grid_mm = f(41, p.exp_text_grid_mm).max(0.0);
    for i in 0..p.exp_colors.len() {
        if let Some(v) = lines.get(42 + i).and_then(|l| l.trim().parse::<i32>().ok()) {
            p.exp_colors[i] = v;
        }
    }
    // Rechengrenze (Zeile 56); fehlt sie, gilt die Vorgabe
    p.max_calc_points = f(55, p.max_calc_points as f64).clamp(1.0, u32::MAX as f64) as u32;
    Some(p)
}

pub fn write_gm0(path: &Path, p: &Params) -> std::io::Result<()> {
    let mut s = String::new();
    s += &format!("{:12.2}\n", p.contour_interval);
    s += &format!("{:12.0}\n", p.max_side);
    s += &format!("{:12.1}\n", p.min_angle_gon);
    s += &format!("{}\n", p.subdivision);
    s += &format!("{:12.5}\n", p.scale_xy);
    s += &format!("{:12.5}\n", p.scale_h);
    for v in p.win {
        s += &format!("{:12.2}\n", v);
    }
    s += &format!("{:12.2}\n", p.h_low);
    s += &format!("{:12.2}\n", p.h_high);
    s += &format!("{}\n", p.decimals_h);
    s += &format!("{}\n", p.decimals_index);
    s += &format!("{:8.1}\n", p.scale);
    s += &format!("{:8.1}\n", p.v16);
    s += &format!("{:8.1}\n", p.v17);
    s += &format!("{:8.1}\n", p.border_mm);
    s += &format!("{:8.1}\n", p.grid_mm);
    s += &format!("{:8.2}\n", p.thin_mm);
    s += &format!("{:8.2}\n", p.min_dist_mm);
    s += &format!("{}\n", p.separator);
    s += &format!("{}\n", if p.smoothing == 10 { "A".to_string() } else { p.smoothing.to_string() });
    s += &format!("{}\n", p.path_line);
    for l in p.layers {
        s += &format!("{}\n", l);
    }
    // Erweiterung: Radius Punktkreis, Schrifthöhen (Nummer, Höhe, Zähllinien, Gitter) in mm, Farbnummern
    for v in [p.exp_circle_mm, p.exp_text_nr_mm, p.exp_text_h_mm, p.exp_text_index_mm, p.exp_text_grid_mm] {
        s += &format!("{:8.2}\n", v);
    }
    for c in p.exp_colors {
        s += &format!("{}\n", c);
    }
    write_text(path, &s)
}

/// Punktdatei GM1: `Punktbezeichnung Rechtswert Hochwert Höhe [Code]`.
pub fn write_gm1(path: &Path, pts: &[(String, f64, f64, f64, String)]) -> std::io::Result<()> {
    let mut s = String::with_capacity(pts.len() * 64);
    for (name, x, y, z, code) in pts {
        s += &format!("{:>12} {:>15.3} {:>15.3} {:>10.3} {}\n", name, x, y, z, code);
    }
    write_text(path, &s)
}

/// Punkte einer Punktdatei GM1 schreiben (Ausschnitt). Koordinaten und Höhen werden durch die beim
/// Einlesen angewandten Maßstäbe geteilt, damit die Datei beim erneuten Einlesen dieselben Werte ergibt.
pub fn write_gm1_points<'a>(path: &Path, pts: impl Iterator<Item = &'a Point>, scale_xy: f64, scale_h: f64) -> std::io::Result<usize> {
    let nonzero = |s: f64| if s == 0.0 || !s.is_finite() { 1.0 } else { s };
    let (sxy, sh) = (nonzero(scale_xy), nonzero(scale_h));
    let mut count = 0usize;
    write_gm1_stream(path, |w| {
        for p in pts {
            let line = format!("{:>12} {:>15.3} {:>15.3} {:>10.3} {}
", p.name, p.x / sxy, p.y / sxy, p.z / sh, p.code);
            w.write_all(&to_cp1252(&line))?;
            count += 1;
        }
        Ok(())
    })?;
    Ok(count)
}

/// Write a GM1 file incrementally without keeping all points in memory.
pub fn write_gm1_stream<F>(path: &Path, write_points: F) -> std::io::Result<()>
where
    F: FnOnce(&mut dyn Write) -> std::io::Result<()>,
{
    let tmp = path.with_extension("tmp~");
    let result = (|| {
        let file = fs::File::create(&tmp)?;
        let mut writer = BufWriter::new(file);
        write_points(&mut writer)?;
        writer.flush()?;
        let file = writer.into_inner().map_err(|e| e.into_error())?;
        file.sync_all()?;
        drop(file);
        fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

// ---------------------------------------------------------------- GM2 / GM9 / GMG

/// Polylines of point names; each closed by `#`, soft breaklines by `##`.
pub fn read_polylines(path: &Path) -> Vec<(Vec<String>, bool)> {
    let Ok(text) = read_text(path) else { return vec![] };
    let mut out = Vec::new();
    let mut cur: Vec<String> = Vec::new();
    for line in text.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if t.starts_with('#') {
            let soft = t.starts_with("##");
            if !cur.is_empty() {
                out.push((std::mem::take(&mut cur), soft));
            }
            continue;
        }
        if let Some(name) = t.split_whitespace().next() {
            cur.push(name.to_string());
        }
    }
    if !cur.is_empty() {
        out.push((cur, false));
    }
    out
}

pub fn write_polylines(path: &Path, lines: &[(Vec<String>, bool)], terminate_last: bool) -> std::io::Result<()> {
    let mut s = String::new();
    for (i, (names, soft)) in lines.iter().enumerate() {
        for n in names {
            s += n;
            s.push('\n');
        }
        if terminate_last || i + 1 < lines.len() {
            s += if *soft { "##\n" } else { "#\n" };
        }
    }
    write_text(path, &s)
}

// ---------------------------------------------------------------- exports

pub struct DxfWriter {
    s: String,
    /// Textstil, der allen Texten zugewiesen wird (Name)
    style: Option<String>,
}

impl DxfWriter {
    pub fn new() -> Self {
        DxfWriter { s: String::from("0\nSECTION\n2\nENTITIES\n"), style: None }
    }
    /// Mit Linientyp- und Ebenentabelle: alle Ebenen (Name, ACI-Farbe) stehen in der Ebenenliste des
    /// CAD-Programms, auch wenn sie noch keine Elemente enthalten.
    pub fn with_layers(layers: &[(String, i32)]) -> Self {
        let mut s = String::from("0\nSECTION\n2\nTABLES\n");
        s += "0\nTABLE\n2\nLTYPE\n70\n1\n0\nLTYPE\n2\nCONTINUOUS\n70\n0\n3\nSolid line\n72\n65\n73\n0\n40\n0.0\n0\nENDTAB\n";
        s += &format!("0\nTABLE\n2\nLAYER\n70\n{}\n", layers.len());
        for (name, aci) in layers {
            s += &format!("0\nLAYER\n2\n{}\n70\n0\n62\n{}\n6\nCONTINUOUS\n", name, aci);
        }
        s += "0\nENDTAB\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n";
        DxfWriter { s, style: None }
    }
    /// Legt einen Textstil (Name, Schriftdatei, z. B. die Normschrift `isocp.shx`) an und weist ihn allen Texten zu.
    /// Fehlt die Schrift am Zielrechner, ersetzt das CAD-Programm sie selbst. Nur mit `with_layers` benutzbar.
    pub fn with_text_style(mut self, name: &str, font_file: &str) -> Self {
        let tail = "0\nENDSEC\n0\nSECTION\n2\nENTITIES\n";
        if let Some(i) = self.s.rfind(tail) {
            let table = format!("0\nTABLE\n2\nSTYLE\n70\n1\n0\nSTYLE\n2\n{}\n70\n0\n40\n0.0\n41\n1.0\n50\n0.0\n71\n0\n42\n2.5\n3\n{}\n4\n\n0\nENDTAB\n", name, font_file);
            self.s.insert_str(i, &table);
            self.style = Some(name.to_string());
        }
        self
    }
    pub fn polyline(&mut self, layer: &str, color: i32, pts: &[[f64; 3]], closed: bool) {
        if pts.len() < 2 {
            return;
        }
        self.s += &format!(
            "0\nPOLYLINE\n8\n{}\n62\n{}\n66\n1\n70\n{}\n",
            layer,
            color,
            if closed { 9 } else { 8 }
        );
        for p in pts {
            self.s += &format!("0\nVERTEX\n8\n{}\n10\n{:.3}\n20\n{:.3}\n30\n{:.3}\n70\n32\n", layer, p[0], p[1], p[2]);
        }
        self.s += &format!("0\nSEQEND\n8\n{}\n", layer);
    }
    pub fn point(&mut self, layer: &str, color: i32, p: [f64; 3]) {
        self.s += &format!("0\nPOINT\n8\n{}\n62\n{}\n10\n{:.3}\n20\n{:.3}\n30\n{:.3}\n", layer, color, p[0], p[1], p[2]);
    }
    pub fn text(&mut self, layer: &str, color: i32, p: [f64; 2], h: f64, rot_deg: f64, t: &str) {
        self.s += &format!(
            "0\nTEXT\n8\n{}\n62\n{}\n10\n{:.3}\n20\n{:.3}\n30\n0.0\n40\n{:.3}\n50\n{:.2}\n1\n{}\n",
            layer, color, p[0], p[1], h, rot_deg, t
        );
    }
    pub fn face(&mut self, layer: &str, color: i32, a: [f64; 3], b: [f64; 3], c: [f64; 3]) {
        self.s += &format!("0\n3DFACE\n8\n{}\n62\n{}\n", layer, color);
        for (i, p) in [a, b, c, c].iter().enumerate() {
            self.s += &format!("1{}\n{:.3}\n2{}\n{:.3}\n3{}\n{:.3}\n", i, p[0], i, p[1], i, p[2]);
        }
    }
    /// Farbangabe: ACI-Nummer (Rückfall) und Echtfarbe (Gruppe 420).
    fn color(&mut self, aci: i32, rgb: [u8; 3]) {
        self.s += &format!("62\n{}\n420\n{}\n", aci, ((rgb[0] as u32) << 16) | ((rgb[1] as u32) << 8) | rgb[2] as u32);
    }
    pub fn polyline_c(&mut self, layer: &str, aci: i32, rgb: [u8; 3], pts: &[[f64; 3]], closed: bool) {
        if pts.len() < 2 {
            return;
        }
        self.s += &format!("0\nPOLYLINE\n8\n{}\n", layer);
        self.color(aci, rgb);
        self.s += &format!("66\n1\n70\n{}\n", if closed { 9 } else { 8 });
        for p in pts {
            self.s += &format!("0\nVERTEX\n8\n{}\n10\n{:.3}\n20\n{:.3}\n30\n{:.3}\n70\n32\n", layer, p[0], p[1], p[2]);
        }
        self.s += &format!("0\nSEQEND\n8\n{}\n", layer);
    }
    pub fn circle_c(&mut self, layer: &str, aci: i32, rgb: [u8; 3], c: [f64; 2], r: f64) {
        self.s += &format!("0\nCIRCLE\n8\n{}\n", layer);
        self.color(aci, rgb);
        self.s += &format!("10\n{:.3}\n20\n{:.3}\n30\n0.0\n40\n{:.4}\n", c[0], c[1], r);
    }
    /// Text mit Ausrichtung: ax 0 links / 0.5 Mitte / 1 rechts, ay 0 Grundlinie / 0.5 Mitte / 1 oben.
    #[allow(clippy::too_many_arguments)]
    pub fn text_c(&mut self, layer: &str, aci: i32, rgb: [u8; 3], p: [f64; 2], h: f64, rot_deg: f64, ax: f64, ay: f64, t: &str) {
        self.s += &format!("0\nTEXT\n8\n{}\n", layer);
        self.color(aci, rgb);
        self.s += &format!("10\n{:.3}\n20\n{:.3}\n30\n0.0\n40\n{:.4}\n1\n{}\n50\n{:.2}\n", p[0], p[1], h, t, rot_deg);
        if let Some(st) = self.style.clone() {
            self.s += &format!("7\n{}\n", st);
        }
        let h72 = if ax < 0.25 { 0 } else if ax < 0.75 { 1 } else { 2 };
        let v73 = if ay < 0.25 { 0 } else if ay < 0.75 { 2 } else { 3 };
        if h72 != 0 || v73 != 0 {
            self.s += &format!("72\n{}\n11\n{:.3}\n21\n{:.3}\n31\n0.0\n73\n{}\n", h72, p[0], p[1], v73);
        }
    }
    /// Gefülltes Dreieck (SOLID) in Echtfarbe.
    pub fn solid_c(&mut self, layer: &str, aci: i32, rgb: [u8; 3], a: [f64; 2], b: [f64; 2], c: [f64; 2]) {
        self.s += &format!("0\nSOLID\n8\n{}\n", layer);
        self.color(aci, rgb);
        for (i, p) in [a, b, c, c].iter().enumerate() {
            self.s += &format!("1{}\n{:.3}\n2{}\n{:.3}\n3{}\n0.0\n", i, p[0], i, p[1], i);
        }
    }
    pub fn finish(mut self) -> String {
        self.s += "0\nENDSEC\n0\nEOF\n";
        self.s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gm0_export_parameters_roundtrip() {
        let dir = std::env::temp_dir().join("dgmnt-gm0-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("t.gm0");
        let mut p = Params::default();
        p.exp_circle_mm = 0.9;
        p.exp_text_nr_mm = 1.8;
        p.exp_text_h_mm = 1.2;
        p.exp_text_index_mm = 2.5;
        p.exp_text_grid_mm = 2.2;
        p.exp_colors = [3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 1, 2, 3];
        p.layers[1] = 99;
        write_gm0(&path, &p).unwrap();
        let q = read_gm0(&path).unwrap();
        assert_eq!((q.exp_circle_mm, q.exp_text_nr_mm, q.exp_text_h_mm, q.exp_text_index_mm, q.exp_text_grid_mm), (0.9, 1.8, 1.2, 2.5, 2.2));
        assert_eq!(q.exp_colors, p.exp_colors);
        assert_eq!(q.layers, p.layers);
        // eine GM0 des alten Programms (nur 37 Zeilen) liefert die Vorgaben
        let old: String = std::fs::read_to_string(&path).unwrap().lines().take(37).collect::<Vec<_>>().join("\r\n");
        std::fs::write(&path, old).unwrap();
        let r = read_gm0(&path).unwrap();
        assert_eq!(r.exp_colors, Params::default().exp_colors);
        assert_eq!(r.exp_circle_mm, Params::default().exp_circle_mm);
        assert_eq!(r.layers[1], 99);
    }

    #[test]
    fn read_gm1_reports_progress_and_window_roundtrip() {
        let dir = std::env::temp_dir().join("dgmnt-gm1-window-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("w.gm1");
        let pts: Vec<(String, f64, f64, f64, String)> =
            (0..5000).map(|i| (format!("P{}", i), i as f64, (i % 50) as f64, 1.5, String::new())).collect();
        write_gm1(&path, &pts).unwrap();
        let mut seen: Vec<f32> = Vec::new();
        let all = read_gm1_progress(&path, &Params::default(), &mut |f, _| seen.push(f)).unwrap();
        assert_eq!(all.len(), 5000);
        assert!(seen.len() >= 3);
        assert!(seen.windows(2).all(|w| w[0] <= w[1]));
        assert_eq!(*seen.last().unwrap(), 1.0);
        // Ausschnitt schreiben und wieder lesen
        let inside: Vec<&Point> = all.iter().filter(|p| p.x >= 100.0 && p.x <= 199.0).collect();
        let out = dir.join("cut.gm1");
        assert_eq!(write_gm1_points(&out, inside.into_iter(), 1.0, 1.0).unwrap(), 100);
        let back = read_gm1(&out, &Params::default()).unwrap();
        assert_eq!(back.len(), 100);
        assert_eq!((back[0].name.as_str(), back[0].x, back[0].z), ("P100", 100.0, 1.5));
    }

    #[test]
    fn gm1_roundtrip() {
        let dir = std::env::temp_dir().join("dgmnt-gm1-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("t.gm1");
        let pts = vec![
            ("D1".to_string(), 729888.173, 215917.12, -0.304, String::new()),
            ("D2".to_string(), 729888.5, 215917.0, 0.0, "SK".to_string()),
            ("D3".to_string(), 0.0, 0.0, 1.25, String::new()),
        ];
        write_gm1(&path, &pts).unwrap();
        let back = read_gm1(&path, &Params::default()).unwrap();
        assert_eq!(back.len(), 3);
        assert_eq!((back[0].name.as_str(), back[0].x, back[0].y, back[0].z), ("D1", 729888.173, 215917.12, -0.304));
        assert_eq!(back[1].code, "SK");
        assert_eq!((back[2].name.as_str(), back[2].x, back[2].y, back[2].z), ("D3", 0.0, 0.0, 1.25));
    }
}
