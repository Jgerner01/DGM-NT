//! Höhen für DXF-Dateien: Punkte ohne Z-Koordinate (oder mit Z = 0) erhalten ihre Höhe durch lineare
//! Interpolation im Dreiecksmodell. Die Datei wird Gruppe für Gruppe umgeschrieben, alle übrigen Inhalte
//! (Tabellen, Blöcke, Texte, Umlaute) bleiben Byte für Byte erhalten.
//! Unterstützt: POINT, LINE, CIRCLE, ARC, TEXT, MTEXT, ATTRIB, INSERT, 3DFACE, SOLID, TRACE, POLYLINE
//! (wird zur 3D-Polylinie) und LWPOLYLINE (wird zur 3D-Polylinie, nur ohne Bogensegmente).
//! Hardware: keine. Autor: Johann Gerner.

use std::collections::HashMap;

use crate::model::Project;

/// Kleinste Rasterzelle der Dreieckssuche (m)
const MIN_CELL: f64 = 1e-3;
/// Rasterzelle = Mittel der Dreiecksgröße mal diesen Faktor
const CELL_FACTOR: f64 = 2.0;
/// Toleranz der Schwerpunktkoordinaten (Punkte auf einer Dreieckskante)
const BARY_TOL: f64 = 1e-9;
/// Ein Z-Wert gilt als null unterhalb dieser Grenze
const ZERO_TOL: f64 = 1e-12;
const FLAG_CLOSED: i32 = 1;
const FLAG_3D_POLYLINE: i32 = 8;
const FLAG_MESH: i32 = 16;
const FLAG_POLYFACE: i32 = 64;
const FLAG_3D_VERTEX: i32 = 32;

/// Dreiecksmodell zur linearen Höhenabfrage.
pub struct Tin {
    ox: f64,
    oy: f64,
    cell: f64,
    pts: Vec<[f64; 3]>,
    tris: Vec<[u32; 3]>,
    grid: HashMap<(i32, i32), Vec<u32>>,
}

impl Tin {
    /// Aus Punkten (x, y, z) und Dreiecken; `None` ohne Dreiecke.
    pub fn new(points: &[[f64; 3]], triangles: &[[usize; 3]]) -> Option<Tin> {
        if triangles.is_empty() {
            return None;
        }
        let mut b = [f64::MAX, f64::MAX, f64::MIN, f64::MIN];
        for t in triangles {
            for &i in t {
                b[0] = b[0].min(points[i][0]);
                b[1] = b[1].min(points[i][1]);
                b[2] = b[2].max(points[i][0]);
                b[3] = b[3].max(points[i][1]);
            }
        }
        let area = ((b[2] - b[0]) * (b[3] - b[1])).max(0.0);
        let cell = ((area / triangles.len() as f64).sqrt() * CELL_FACTOR).max(MIN_CELL);
        let pts: Vec<[f64; 3]> = points.iter().map(|p| [p[0] - b[0], p[1] - b[1], p[2]]).collect();
        let tris: Vec<[u32; 3]> = triangles.iter().map(|t| [t[0] as u32, t[1] as u32, t[2] as u32]).collect();
        let mut grid: HashMap<(i32, i32), Vec<u32>> = HashMap::new();
        for (ti, t) in tris.iter().enumerate() {
            let xs = t.iter().map(|&i| pts[i as usize][0]);
            let ys = t.iter().map(|&i| pts[i as usize][1]);
            let (x0, x1) = (xs.clone().fold(f64::MAX, f64::min), xs.fold(f64::MIN, f64::max));
            let (y0, y1) = (ys.clone().fold(f64::MAX, f64::min), ys.fold(f64::MIN, f64::max));
            for cx in (x0 / cell).floor() as i32..=(x1 / cell).floor() as i32 {
                for cy in (y0 / cell).floor() as i32..=(y1 / cell).floor() as i32 {
                    grid.entry((cx, cy)).or_default().push(ti as u32);
                }
            }
        }
        Some(Tin { ox: b[0], oy: b[1], cell, pts, tris, grid })
    }

    /// Aus den aktiven Dreiecken eines Projekts.
    pub fn from_project(pr: &Project) -> Option<Tin> {
        let points: Vec<[f64; 3]> = pr.st.points.iter().map(|p| [p.x, p.y, p.z]).collect();
        let tris: Vec<[usize; 3]> = pr.st.triangles.iter().filter(|t| t.active).map(|t| t.v).collect();
        Tin::new(&points, &tris)
    }

    /// Höhe an der Stelle (x, y) durch lineare Interpolation im Dreieck; `None` außerhalb des Modells.
    pub fn height(&self, x: f64, y: f64) -> Option<f64> {
        let (px, py) = (x - self.ox, y - self.oy);
        let key = ((px / self.cell).floor() as i32, (py / self.cell).floor() as i32);
        for &ti in self.grid.get(&key)? {
            let t = &self.tris[ti as usize];
            let (a, b, c) = (self.pts[t[0] as usize], self.pts[t[1] as usize], self.pts[t[2] as usize]);
            let det = (b[1] - c[1]) * (a[0] - c[0]) + (c[0] - b[0]) * (a[1] - c[1]);
            if det.abs() < ZERO_TOL {
                continue;
            }
            let l1 = ((b[1] - c[1]) * (px - c[0]) + (c[0] - b[0]) * (py - c[1])) / det;
            let l2 = ((c[1] - a[1]) * (px - c[0]) + (a[0] - c[0]) * (py - c[1])) / det;
            let l3 = 1.0 - l1 - l2;
            if l1 >= -BARY_TOL && l2 >= -BARY_TOL && l3 >= -BARY_TOL {
                return Some(l1 * a[2] + l2 * b[2] + l3 * c[2]);
            }
        }
        None
    }
}

/// Ergebnis der Auswertung
#[derive(Clone, Default, Debug)]
pub struct ZReport {
    /// Elemente, deren Lage Höhen braucht oder schon hat
    pub seen: usize,
    /// Elemente, denen Höhen zugewiesen wurden
    pub changed: usize,
    /// zugewiesene Z-Werte
    pub points_set: usize,
    /// Elemente, die bereits Z-Werte haben (unverändert)
    pub kept_with_z: usize,
    /// Elemente mit einem Punkt außerhalb des Modells (unverändert)
    pub outside: usize,
    /// Polylinien mit Bogensegmenten (unverändert)
    pub arcs_skipped: usize,
}

impl ZReport {
    pub fn summary(&self) -> String {
        format!(
            "{} Elemente geprüft: {} erhielten Höhen ({} Z-Werte), {} hatten schon Höhen, {} liegen außerhalb des Modells, {} Polylinien mit Bögen unverändert.",
            self.seen, self.changed, self.points_set, self.kept_with_z, self.outside, self.arcs_skipped
        )
    }
}

// ---------------------------------------------------------------- Gruppen lesen und schreiben

#[derive(Clone)]
struct Pair {
    code: i32,
    code_line: String,
    value: String,
}

impl Pair {
    fn new(code: i32, value: impl Into<String>) -> Pair {
        Pair { code, code_line: format!("{:>3}", code), value: value.into() }
    }
    fn is(&self, code: i32, value: &str) -> bool {
        self.code == code && self.value.trim() == value
    }
    fn num(&self) -> Option<f64> {
        self.value.trim().parse::<f64>().ok()
    }
}

fn parse(text: &str) -> Result<(Vec<Pair>, &'static str), String> {
    let eol = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let mut lines: Vec<&str> = text.split('\n').map(|l| l.strip_suffix('\r').unwrap_or(l)).collect();
    if lines.last().map_or(false, |l| l.is_empty()) {
        lines.pop();
    }
    if lines.len() % 2 != 0 {
        return Err("Keine gültige DXF-Datei (ungerade Zeilenzahl).".into());
    }
    let mut pairs = Vec::with_capacity(lines.len() / 2);
    for (n, ch) in lines.chunks_exact(2).enumerate() {
        let code = ch[0].trim().parse::<i32>().map_err(|_| format!("Keine gültige DXF-Datei (Gruppencode in Zeile {}).", n * 2 + 1))?;
        pairs.push(Pair { code, code_line: ch[0].to_string(), value: ch[1].to_string() });
    }
    Ok((pairs, eol))
}

fn serialize(pairs: &[Pair], eol: &str) -> Vec<u8> {
    let mut out = String::new();
    for p in pairs {
        out += &p.code_line;
        out += eol;
        out += &p.value;
        out += eol;
    }
    out.chars().map(|c| c as u32 as u8).collect()
}

/// Neue Objekthandles oberhalb aller vorhandenen vergeben (für die aus LWPOLYLINE erzeugten Elemente).
struct HandleAlloc {
    next: u64,
    used: bool,
}

impl HandleAlloc {
    fn scan(pairs: &[Pair]) -> HandleAlloc {
        let mut max = 0u64;
        for p in pairs {
            if p.code == 5 || p.code == 105 {
                if let Ok(v) = u64::from_str_radix(p.value.trim(), 16) {
                    max = max.max(v);
                }
            }
        }
        HandleAlloc { next: max + 1, used: false }
    }
    fn take(&mut self) -> String {
        self.used = true;
        let h = format!("{:X}", self.next);
        self.next += 1;
        h
    }
    /// $HANDSEED im Kopf der Datei anheben
    fn patch_seed(&self, pairs: &mut [Pair]) {
        if !self.used {
            return;
        }
        for i in 0..pairs.len().saturating_sub(1) {
            if pairs[i].is(9, "$HANDSEED") {
                pairs[i + 1].value = format!("{:X}", self.next);
                return;
            }
        }
    }
}

// ---------------------------------------------------------------- Punkte eines Elements

#[derive(Clone, Copy)]
struct PointRef {
    xi: usize,
    yi: usize,
    zi: Option<usize>,
}

fn find_point(ent: &[Pair], base: i32) -> Option<PointRef> {
    let xi = ent.iter().position(|p| p.code == base)?;
    let yi = ent.iter().skip(xi + 1).position(|p| p.code == base + 10)? + xi + 1;
    let zi = ent.iter().position(|p| p.code == base + 20);
    Some(PointRef { xi, yi, zi })
}

fn needs_z(ent: &[Pair], p: &PointRef, replace_zero: bool) -> bool {
    match p.zi {
        None => true,
        Some(zi) => replace_zero && ent[zi].num().map_or(false, |z| z.abs() < ZERO_TOL),
    }
}

fn xy(ent: &[Pair], p: &PointRef) -> Option<(f64, f64)> {
    Some((ent[p.xi].num()?, ent[p.yi].num()?))
}

fn fmt_z(z: f64) -> String {
    format!("{:.3}", z)
}

/// Z-Werte eintragen: vorhandene Gruppen ersetzen, fehlende hinter der Y-Gruppe einfügen.
fn apply_z(ent: &mut Vec<Pair>, zs: &[(PointRef, f64)]) {
    let mut inserts: Vec<(usize, i32, f64)> = Vec::new();
    for (p, z) in zs {
        match p.zi {
            Some(zi) => ent[zi].value = fmt_z(*z),
            None => inserts.push((p.yi, ent[p.xi].code + 20, *z)),
        }
    }
    inserts.sort_by(|a, b| b.0.cmp(&a.0));
    for (yi, code, z) in inserts {
        ent.insert(yi + 1, Pair::new(code, fmt_z(z)));
    }
}

/// Punktartiges Element (POINT, LINE, TEXT …): Gruppen `groups` sind die X-Codes der Punkte.
fn fix_point_entity(ent: &mut Vec<Pair>, groups: &[i32], tin: &Tin, replace_zero: bool, rep: &mut ZReport) {
    let refs: Vec<PointRef> = groups.iter().filter_map(|&g| find_point(ent, g)).collect();
    if refs.is_empty() {
        return;
    }
    rep.seen += 1;
    let need: Vec<PointRef> = refs.into_iter().filter(|p| needs_z(ent, p, replace_zero)).collect();
    if need.is_empty() {
        rep.kept_with_z += 1;
        return;
    }
    let mut zs = Vec::with_capacity(need.len());
    for p in need {
        let Some((x, y)) = xy(ent, &p) else { return };
        match tin.height(x, y) {
            Some(z) => zs.push((p, z)),
            None => {
                rep.outside += 1;
                return;
            }
        }
    }
    apply_z(ent, &zs);
    rep.changed += 1;
    rep.points_set += zs.len();
}

fn group_value(ent: &[Pair], code: i32) -> Option<&Pair> {
    ent.iter().find(|p| p.code == code)
}

/// Flag-Gruppe (70) setzen; fehlt sie, wird sie am Ende angefügt.
fn or_flag(ent: &mut Vec<Pair>, flag: i32) {
    match ent.iter_mut().find(|p| p.code == 70) {
        Some(p) => p.value = (p.value.trim().parse::<i32>().unwrap_or(0) | flag).to_string(),
        None => ent.push(Pair::new(70, flag.to_string())),
    }
}

/// POLYLINE mit ihren VERTEX-Elementen: alle Stützpunkte erhalten Höhen, die Linie wird zur 3D-Polylinie.
fn fix_polyline(header: &mut Vec<Pair>, vertices: &mut [Vec<Pair>], tin: &Tin, replace_zero: bool, rep: &mut ZReport) {
    let flag = group_value(header, 70).and_then(|p| p.value.trim().parse::<i32>().ok()).unwrap_or(0);
    if flag & (FLAG_MESH | FLAG_POLYFACE) != 0 || vertices.is_empty() {
        return;
    }
    rep.seen += 1;
    let mut work: Vec<(usize, PointRef, f64)> = Vec::new();
    for (vi, v) in vertices.iter().enumerate() {
        let Some(p) = find_point(v, 10) else { continue };
        if !needs_z(v, &p, replace_zero) {
            continue;
        }
        let Some((x, y)) = xy(v, &p) else { return };
        match tin.height(x, y) {
            Some(z) => work.push((vi, p, z)),
            None => {
                rep.outside += 1;
                return;
            }
        }
    }
    if work.is_empty() {
        rep.kept_with_z += 1;
        return;
    }
    for (vi, p, z) in &work {
        apply_z(&mut vertices[*vi], &[(*p, *z)]);
        or_flag(&mut vertices[*vi], FLAG_3D_VERTEX);
    }
    or_flag(header, FLAG_3D_POLYLINE);
    rep.changed += 1;
    rep.points_set += work.len();
}

/// LWPOLYLINE (2D, ohne Höhe je Stützpunkt) in eine 3D-POLYLINE mit Höhen umwandeln.
fn lwpolyline_to_3d(ent: &[Pair], tin: &Tin, alloc: &mut HandleAlloc, rep: &mut ZReport) -> Option<Vec<Pair>> {
    let marker = ent.iter().position(|p| p.is(100, "AcDbPolyline"));
    let body_start = marker.map_or_else(|| ent.iter().position(|p| matches!(p.code, 90 | 70 | 43 | 38 | 39 | 10)).unwrap_or(ent.len()), |m| m + 1);
    let common = &ent[1..marker.unwrap_or(body_start)];
    let closed = ent[body_start..].iter().find(|p| p.code == 70).and_then(|p| p.value.trim().parse::<i32>().ok()).unwrap_or(0) & FLAG_CLOSED != 0;
    let mut verts: Vec<(f64, f64)> = Vec::new();
    let mut pending_x: Option<f64> = None;
    for p in &ent[body_start..] {
        match p.code {
            10 => pending_x = p.num(),
            20 => {
                if let (Some(x), Some(y)) = (pending_x.take(), p.num()) {
                    verts.push((x, y));
                }
            }
            42 if p.num().map_or(false, |b| b.abs() > ZERO_TOL) => {
                rep.seen += 1;
                rep.arcs_skipped += 1;
                return None;
            }
            _ => {}
        }
    }
    if verts.len() < 2 {
        return None;
    }
    rep.seen += 1;
    let mut zs = Vec::with_capacity(verts.len());
    for &(x, y) in &verts {
        match tin.height(x, y) {
            Some(z) => zs.push(z),
            None => {
                rep.outside += 1;
                return None;
            }
        }
    }
    let has_handle = common.iter().any(|p| p.code == 5);
    let has_markers = marker.is_some();
    let owner = common.iter().find(|p| p.code == 330).map(|p| p.value.clone());
    let layer = common.iter().find(|p| p.code == 8).map(|p| p.value.clone()).unwrap_or_else(|| "0".into());
    let poly_handle = common.iter().find(|p| p.code == 5).map(|p| p.value.clone());
    let mut out: Vec<Pair> = vec![Pair::new(0, "POLYLINE")];
    out.extend(common.iter().cloned());
    if has_markers {
        out.push(Pair::new(100, "AcDb3dPolyline"));
    }
    out.push(Pair::new(66, "1"));
    out.push(Pair::new(10, "0.0"));
    out.push(Pair::new(20, "0.0"));
    out.push(Pair::new(30, "0.0"));
    out.push(Pair::new(70, (FLAG_3D_POLYLINE | if closed { FLAG_CLOSED } else { 0 }).to_string()));
    for (&(x, y), z) in verts.iter().zip(&zs) {
        out.push(Pair::new(0, "VERTEX"));
        if has_handle {
            out.push(Pair::new(5, alloc.take()));
            if let Some(h) = &poly_handle {
                out.push(Pair::new(330, h.clone()));
            }
        }
        if has_markers {
            out.push(Pair::new(100, "AcDbEntity"));
        }
        out.push(Pair::new(8, layer.clone()));
        if has_markers {
            out.push(Pair::new(100, "AcDbVertex"));
            out.push(Pair::new(100, "AcDb3dPolylineVertex"));
        }
        out.push(Pair::new(10, format!("{}", x)));
        out.push(Pair::new(20, format!("{}", y)));
        out.push(Pair::new(30, fmt_z(*z)));
        out.push(Pair::new(70, FLAG_3D_VERTEX.to_string()));
    }
    out.push(Pair::new(0, "SEQEND"));
    if has_handle {
        out.push(Pair::new(5, alloc.take()));
        if let Some(o) = &owner {
            out.push(Pair::new(330, o.clone()));
        }
    }
    if has_markers {
        out.push(Pair::new(100, "AcDbEntity"));
    }
    out.push(Pair::new(8, layer));
    rep.changed += 1;
    rep.points_set += verts.len();
    Some(out)
}

/// Elemente mit ihren X-Codes der Punkte, die eine Höhe bekommen können.
fn point_groups(ty: &str) -> Option<&'static [i32]> {
    Some(match ty {
        "POINT" | "CIRCLE" | "ARC" | "MTEXT" | "INSERT" => &[10],
        "LINE" | "TEXT" | "ATTRIB" => &[10, 11],
        "3DFACE" | "SOLID" | "TRACE" => &[10, 11, 12, 13],
        _ => return None,
    })
}

/// DXF-Datei (ASCII) lesen, fehlende Höhen aus dem Modell ergänzen und die neue Datei zurückgeben.
/// `replace_zero`: auch Z = 0 gilt als fehlend (viele CAD-Programme schreiben für 2D-Zeichnungen Z = 0).
pub fn add_heights(bytes: &[u8], tin: &Tin, replace_zero: bool) -> Result<(Vec<u8>, ZReport), String> {
    if bytes.starts_with(b"AutoCAD Binary DXF") {
        return Err("Binäre DXF-Dateien werden nicht unterstützt – bitte als ASCII-DXF speichern.".into());
    }
    // Byte für Byte als Zeichen 0–255 lesen: so bleibt jede Kodierung beim Zurückschreiben erhalten
    let text: String = bytes.iter().map(|&b| b as char).collect();
    let (pairs, eol) = parse(&text)?;
    let mut start = None;
    let mut end = None;
    for i in 0..pairs.len().saturating_sub(1) {
        match start {
            None if pairs[i].is(0, "SECTION") && pairs[i + 1].is(2, "ENTITIES") => start = Some(i + 2),
            Some(_) if pairs[i].is(0, "ENDSEC") => {
                end = Some(i);
                break;
            }
            _ => {}
        }
    }
    let (Some(s), Some(e)) = (start, end) else {
        return Err("Die Datei enthält keinen ENTITIES-Abschnitt.".into());
    };
    let mut alloc = HandleAlloc::scan(&pairs);
    let ents = &pairs[s..e];
    let mut ranges: Vec<std::ops::Range<usize>> = Vec::new();
    let mut k = 0;
    while k < ents.len() {
        let st = k;
        k += 1;
        while k < ents.len() && ents[k].code != 0 {
            k += 1;
        }
        ranges.push(st..k);
    }
    let ty_of = |r: &std::ops::Range<usize>| ents[r.start].value.trim().to_ascii_uppercase();
    let mut rep = ZReport::default();
    let mut out: Vec<Pair> = pairs[..s].to_vec();
    let mut i = 0;
    while i < ranges.len() {
        let mut ent: Vec<Pair> = ents[ranges[i].clone()].to_vec();
        let ty = if ent[0].code == 0 { ty_of(&ranges[i]) } else { String::new() };
        if ty == "POLYLINE" {
            let mut j = i + 1;
            let mut verts: Vec<Vec<Pair>> = Vec::new();
            while j < ranges.len() && ty_of(&ranges[j]) == "VERTEX" {
                verts.push(ents[ranges[j].clone()].to_vec());
                j += 1;
            }
            fix_polyline(&mut ent, &mut verts, tin, replace_zero, &mut rep);
            out.extend(ent);
            for v in verts {
                out.extend(v);
            }
            i = j;
            continue;
        }
        if ty == "LWPOLYLINE" {
            match lwpolyline_to_3d(&ent, tin, &mut alloc, &mut rep) {
                Some(new) => out.extend(new),
                None => out.extend(ent),
            }
        } else if let Some(groups) = point_groups(&ty) {
            fix_point_entity(&mut ent, groups, tin, replace_zero, &mut rep);
            out.extend(ent);
        } else {
            out.extend(ent);
        }
        i += 1;
    }
    out.extend(pairs[e..].iter().cloned());
    alloc.patch_seed(&mut out);
    Ok((serialize(&out, eol), rep))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ebene z = 100 + x/10 über dem Quadrat 0..100
    fn tin() -> Tin {
        let pts = [[0.0, 0.0, 100.0], [100.0, 0.0, 110.0], [100.0, 100.0, 110.0], [0.0, 100.0, 100.0]];
        Tin::new(&pts, &[[0, 1, 2], [0, 2, 3]]).unwrap()
    }

    fn dxf(entities: &str) -> Vec<u8> {
        format!("  0\r\nSECTION\r\n  2\r\nHEADER\r\n  9\r\n$HANDSEED\r\n  5\r\n20\r\n  0\r\nENDSEC\r\n  0\r\nSECTION\r\n  2\r\nENTITIES\r\n{}  0\r\nENDSEC\r\n  0\r\nEOF\r\n", entities.replace('\n', "\r\n"))
            .into_bytes()
    }

    #[test]
    fn linear_interpolation_and_outside() {
        let t = tin();
        assert!((t.height(50.0, 30.0).unwrap() - 105.0).abs() < 1e-9);
        assert!((t.height(100.0, 100.0).unwrap() - 110.0).abs() < 1e-9);
        assert!(t.height(150.0, 30.0).is_none());
    }

    #[test]
    fn point_without_z_and_with_zero_z() {
        let src = dxf("  0\nPOINT\n  8\nA\n 10\n50.0\n 20\n30.0\n  0\nPOINT\n  8\nA\n 10\n20.0\n 20\n30.0\n 30\n0.0\n  0\nPOINT\n  8\nA\n 10\n20.0\n 20\n30.0\n 30\n7.5\n  0\nPOINT\n 10\n500.0\n 20\n30.0\n");
        let (out, rep) = add_heights(&src, &tin(), true).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains(" 30\r\n105.000\r\n"));
        assert!(text.contains(" 30\r\n102.000\r\n"));
        assert!(text.contains(" 30\r\n7.5\r\n"));
        assert_eq!((rep.seen, rep.changed, rep.kept_with_z, rep.outside), (4, 2, 1, 1));
        // ohne „Z = 0 ersetzen“ bleibt der zweite Punkt unverändert
        let (_, rep) = add_heights(&src, &tin(), false).unwrap();
        assert_eq!((rep.changed, rep.kept_with_z), (1, 2));
    }

    #[test]
    fn line_gets_both_endpoints() {
        let src = dxf("  0\nLINE\n 10\n0.0\n 20\n0.0\n 11\n100.0\n 21\n50.0\n");
        let (out, rep) = add_heights(&src, &tin(), true).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains(" 30\r\n100.000\r\n") && text.contains(" 31\r\n110.000\r\n"));
        assert_eq!(rep.points_set, 2);
    }

    #[test]
    fn polyline_becomes_3d() {
        let src = dxf("  0\nPOLYLINE\n  8\nA\n 66\n1\n 70\n0\n  0\nVERTEX\n  8\nA\n 10\n0.0\n 20\n0.0\n  0\nVERTEX\n  8\nA\n 10\n100.0\n 20\n0.0\n  0\nSEQEND\n  8\nA\n");
        let (out, rep) = add_heights(&src, &tin(), true).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains(" 70\r\n8\r\n"));
        assert_eq!(text.matches(" 70\r\n32\r\n").count(), 2);
        assert!(text.contains(" 30\r\n110.000\r\n"));
        assert_eq!(rep.changed, 1);
    }

    #[test]
    fn lwpolyline_is_converted_with_new_handles() {
        let src = dxf("  0\nLWPOLYLINE\n  5\n3DC\n330\n1F\n100\nAcDbEntity\n  8\nL\n100\nAcDbPolyline\n 90\n3\n 70\n1\n 10\n0.0\n 20\n0.0\n 10\n100.0\n 20\n0.0\n 10\n100.0\n 20\n100.0\n");
        let (out, rep) = add_heights(&src, &tin(), true).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("POLYLINE") && !text.contains("LWPOLYLINE"));
        assert!(text.contains("AcDb3dPolylineVertex") && text.contains("SEQEND"));
        assert!(text.contains(" 70\r\n9\r\n"));
        // $HANDSEED wurde angehoben (vier neue Handles 3DD bis 3E0)
        assert!(text.contains("$HANDSEED\r\n  5\r\n3E1\r\n"));
        assert_eq!((rep.changed, rep.points_set), (1, 3));
    }

    #[test]
    fn lwpolyline_with_bulge_and_binary_are_refused() {
        let src = dxf("  0\nLWPOLYLINE\n100\nAcDbPolyline\n 90\n2\n 10\n0.0\n 20\n0.0\n 42\n0.5\n 10\n10.0\n 20\n0.0\n");
        let (out, rep) = add_heights(&src, &tin(), true).unwrap();
        assert!(String::from_utf8(out).unwrap().contains("LWPOLYLINE"));
        assert_eq!(rep.arcs_skipped, 1);
        assert!(add_heights(b"AutoCAD Binary DXF\r\n", &tin(), true).is_err());
    }

    #[test]
    fn untouched_parts_stay_byte_identical() {
        let mut src = dxf("  0\nTEXT\n  1\nStuetzmauer\n 10\n10.0\n 20\n10.0\n");
        let pos = src.windows(4).position(|w| w == b"Stue").unwrap();
        src.splice(pos + 2..pos + 4, [0xfcu8]); // „ü“ als Windows-1252-Byte
        let (out, _) = add_heights(&src, &tin(), true).unwrap();
        assert!(out.windows(11).any(|w| w == b"St\xfctzmauer\r"));
    }
}
