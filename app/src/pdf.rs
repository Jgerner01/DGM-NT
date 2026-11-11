//! Minimaler PDF-Schreiber für die Druckausgabe: reine Vektorgrafik (Pfade, Text in Helvetica,
//! Gouraud-Dreiecksnetze für Farbflächen). Alle Maße in mm, Ursprung links unten.

use std::fmt::Write as _;

use crate::files::to_cp1252;
use crate::scene::Rgb;

const PT: f64 = 72.0 / 25.4;
/// Höhe der Großbuchstaben von Helvetica (1/1000 der Schriftgröße)
const CAP: f64 = 0.718;

#[rustfmt::skip]
const W_REG: [u16; 95] = [
    278, 278, 355, 556, 556, 889, 667, 191, 333, 333, 389, 584, 278, 333, 278, 278,
    556, 556, 556, 556, 556, 556, 556, 556, 556, 556,
    278, 278, 584, 584, 584, 556, 1015,
    667, 667, 722, 722, 667, 611, 778, 722, 278, 500, 667, 556, 833, 722, 778, 667, 778, 722, 667, 611, 722, 667, 944, 667, 667, 611,
    278, 278, 278, 469, 556, 333,
    556, 556, 500, 556, 556, 278, 556, 556, 222, 222, 500, 222, 833, 556, 556, 556, 556, 333, 500, 278, 556, 500, 722, 500, 500, 500,
    334, 260, 334, 584,
];

#[rustfmt::skip]
const W_BOLD: [u16; 95] = [
    278, 333, 474, 556, 556, 889, 722, 238, 333, 333, 389, 584, 278, 333, 278, 278,
    556, 556, 556, 556, 556, 556, 556, 556, 556, 556,
    333, 333, 584, 584, 584, 611, 975,
    722, 722, 722, 722, 667, 611, 778, 722, 278, 556, 722, 611, 833, 722, 778, 667, 778, 722, 667, 611, 722, 667, 944, 667, 667, 611,
    333, 278, 333, 584, 556, 333,
    556, 611, 556, 611, 556, 333, 611, 611, 278, 278, 556, 278, 889, 611, 611, 611, 611, 389, 556, 333, 611, 556, 778, 556, 556, 500,
    389, 280, 389, 584,
];

fn glyph_width(c: char, bold: bool) -> f64 {
    let t = if bold { &W_BOLD } else { &W_REG };
    let w = match c {
        ' '..='~' => t[c as usize - 32],
        'ä' | 'ö' | 'ü' => t[b'a' as usize - 32],
        'Ä' => t[b'A' as usize - 32],
        'Ö' => t[b'O' as usize - 32],
        'Ü' => t[b'U' as usize - 32],
        'ß' => 611,
        '²' | '³' => 333,
        '°' => 400,
        '±' => 584,
        _ => 556,
    };
    w as f64 / 1000.0
}

/// Breite eines Textes bei Schriftgröße `size` (mm).
pub fn text_width(s: &str, size: f64, bold: bool) -> f64 {
    s.chars().map(|c| glyph_width(c, bold)).sum::<f64>() * size
}

/// Schriftgröße für eine Großbuchstabenhöhe `cap_h`.
pub fn font_size_for_cap(cap_h: f64) -> f64 {
    cap_h / CAP
}

fn n(v: f64) -> String {
    let s = format!("{:.3}", v);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() || s == "-" || s == "-0" {
        "0".into()
    } else {
        s.to_string()
    }
}

fn rgb_ops(c: Rgb) -> String {
    format!("{} {} {}", n(c[0] as f64 / 255.0), n(c[1] as f64 / 255.0), n(c[2] as f64 / 255.0))
}

fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2 + 2);
    s.push('<');
    for b in bytes {
        let _ = write!(s, "{:02X}", b);
    }
    s.push('>');
    s
}

struct Shading {
    data: Vec<u8>,
    bbox: [f64; 4],
}

#[derive(Default)]
pub struct Content {
    s: String,
    shadings: Vec<Shading>,
}

impl Content {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn save(&mut self) {
        self.s += "q\n";
    }
    pub fn restore(&mut self) {
        self.s += "Q\n";
    }
    pub fn clip_rect(&mut self, x: f64, y: f64, w: f64, h: f64) {
        let _ = writeln!(self.s, "{} {} {} {} re W n", n(x), n(y), n(w), n(h));
    }
    pub fn stroke_color(&mut self, c: Rgb) {
        let _ = writeln!(self.s, "{} RG", rgb_ops(c));
    }
    pub fn fill_color(&mut self, c: Rgb) {
        let _ = writeln!(self.s, "{} rg", rgb_ops(c));
    }
    pub fn line_width(&mut self, w: f64) {
        let _ = writeln!(self.s, "{} w", n(w.max(0.01)));
    }
    pub fn dash(&mut self, d: Option<(f64, f64)>) {
        match d {
            Some((a, b)) => {
                let _ = writeln!(self.s, "[{} {}] 0 d", n(a), n(b));
            }
            None => self.s += "[] 0 d\n",
        }
    }
    pub fn round_joins(&mut self) {
        self.s += "1 j 1 J\n";
    }
    fn path(&mut self, pts: &[(f64, f64)]) {
        for (i, p) in pts.iter().enumerate() {
            let _ = writeln!(self.s, "{} {} {}", n(p.0), n(p.1), if i == 0 { "m" } else { "l" });
        }
    }
    pub fn polyline(&mut self, pts: &[(f64, f64)], closed: bool) {
        if pts.len() < 2 {
            return;
        }
        self.path(pts);
        self.s += if closed { "s\n" } else { "S\n" };
    }
    pub fn fill_poly(&mut self, pts: &[(f64, f64)]) {
        if pts.len() < 3 {
            return;
        }
        self.path(pts);
        self.s += "f\n";
    }
    pub fn rect(&mut self, x: f64, y: f64, w: f64, h: f64, fill: bool, stroke: bool) {
        let _ = writeln!(self.s, "{} {} {} {} re {}", n(x), n(y), n(w), n(h), match (fill, stroke) {
            (true, true) => "B",
            (true, false) => "f",
            (false, true) => "S",
            _ => "n",
        });
    }
    pub fn circle(&mut self, cx: f64, cy: f64, r: f64, fill: bool) {
        let k = 0.552_284_75 * r;
        let _ = writeln!(self.s, "{} {} m", n(cx + r), n(cy));
        for (a, b, c, d, e, f) in [
            (cx + r, cy + k, cx + k, cy + r, cx, cy + r),
            (cx - k, cy + r, cx - r, cy + k, cx - r, cy),
            (cx - r, cy - k, cx - k, cy - r, cx, cy - r),
            (cx + k, cy - r, cx + r, cy - k, cx + r, cy),
        ] {
            let _ = writeln!(self.s, "{} {} {} {} {} {} c", n(a), n(b), n(c), n(d), n(e), n(f));
        }
        self.s += if fill { "f\n" } else { "S\n" };
    }

    /// Text mit Großbuchstabenhöhe `cap_h` (mm). ax: 0 links … 1 rechts; ay: 0 Grundlinie, 0.5 Mitte, 1 oben.
    /// `halo`: weiße Kontur unter dem Text (Lesbarkeit auf Linien).
    #[allow(clippy::too_many_arguments)]
    pub fn text(&mut self, x: f64, y: f64, cap_h: f64, bold: bool, rgb: Rgb, rot_deg: f64, ax: f64, ay: f64, text: &str, halo: bool) {
        if text.is_empty() {
            return;
        }
        let size = font_size_for_cap(cap_h);
        let w = text_width(text, size, bold);
        let (x0, y0) = (-ax * w, -ay * cap_h);
        let (sn, cs) = rot_deg.to_radians().sin_cos();
        let font = if bold { "F2" } else { "F1" };
        let str_hex = hex(&to_cp1252(text));
        let tm = format!("{} {} {} {} {} {} Tm {} {} Td", n(cs), n(sn), n(-sn), n(cs), n(x), n(y), n(x0), n(y0));
        if halo {
            let _ = writeln!(self.s, "q 1 1 1 RG 1 1 1 rg {} w 1 j BT /{} {} Tf 2 Tr {} {} Tj ET Q", n(cap_h * 0.35), font, n(size), tm, str_hex);
        }
        let _ = writeln!(self.s, "q {} rg BT /{} {} Tf 0 Tr {} {} Tj ET Q", rgb_ops(rgb), font, n(size), tm, str_hex);
    }

    /// Gouraud-gefärbte Dreiecke (je 3 aufeinanderfolgende Ecken); als PDF-Shading Typ 4 (Vektor).
    pub fn mesh(&mut self, verts: &[((f64, f64), Rgb)]) {
        if verts.len() < 3 {
            return;
        }
        let mut bb = [f64::MAX, f64::MAX, f64::MIN, f64::MIN];
        for ((x, y), _) in verts {
            bb = [bb[0].min(*x), bb[1].min(*y), bb[2].max(*x), bb[3].max(*y)];
        }
        if bb[2] - bb[0] < 1e-6 {
            bb[2] = bb[0] + 1e-6;
        }
        if bb[3] - bb[1] < 1e-6 {
            bb[3] = bb[1] + 1e-6;
        }
        let q = |v: f64, lo: f64, hi: f64| (((v - lo) / (hi - lo)).clamp(0.0, 1.0) * 4_294_967_295.0).round() as u32;
        let mut data = Vec::with_capacity(verts.len() * 12);
        for ((x, y), c) in verts {
            data.push(0u8);
            data.extend_from_slice(&q(*x, bb[0], bb[2]).to_be_bytes());
            data.extend_from_slice(&q(*y, bb[1], bb[3]).to_be_bytes());
            data.extend_from_slice(c);
        }
        self.shadings.push(Shading { data, bbox: bb });
        let _ = writeln!(self.s, "q /Sh{} sh Q", self.shadings.len());
    }
}

fn flate(data: &[u8]) -> Vec<u8> {
    miniz_oxide::deflate::compress_to_vec_zlib(data, 6)
}

fn utf16_hex(s: &str) -> String {
    let mut b = vec![0xFE, 0xFF];
    for u in s.encode_utf16() {
        b.extend_from_slice(&u.to_be_bytes());
    }
    hex(&b)
}

/// (Jahr, Monat, Tag, Stunde, Minute, Sekunde) in UTC
fn utc_now() -> (i64, u32, u32, u32, u32, u32) {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    let (days, rem) = (secs.div_euclid(86400), secs.rem_euclid(86400));
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    (y, m, d, (rem / 3600) as u32, (rem % 3600 / 60) as u32, (rem % 60) as u32)
}

/// Fertige PDF-Datei mit einer Seite `w` × `h` mm.
pub fn build(c: &Content, w: f64, h: f64, title: &str) -> Vec<u8> {
    let mut objs: Vec<Vec<u8>> = Vec::new(); // Objekt i+1 steht an Stelle i
    let nsh = c.shadings.len();
    let first_sh = 7;
    let info_id = first_sh + nsh;
    objs.push(b"<< /Type /Catalog /Pages 2 0 R >>".to_vec());
    objs.push(b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec());
    let mut shres = String::new();
    for i in 0..nsh {
        let _ = write!(shres, "/Sh{} {} 0 R ", i + 1, first_sh + i);
    }
    objs.push(
        format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {} {}] /Contents 4 0 R /Resources << /Font << /F1 5 0 R /F2 6 0 R >> /Shading << {}>> >> >>",
            n(w * PT),
            n(h * PT),
            shres
        )
        .into_bytes(),
    );
    let body = format!("{} 0 0 {} 0 0 cm\n{}", n(PT), n(PT), c.s);
    let z = flate(body.as_bytes());
    let mut st = format!("<< /Filter /FlateDecode /Length {} >>\nstream\n", z.len()).into_bytes();
    st.extend_from_slice(&z);
    st.extend_from_slice(b"\nendstream");
    objs.push(st);
    objs.push(b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>".to_vec());
    objs.push(b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold /Encoding /WinAnsiEncoding >>".to_vec());
    for s in &c.shadings {
        let z = flate(&s.data);
        let mut o = format!(
            "<< /ShadingType 4 /ColorSpace /DeviceRGB /BitsPerCoordinate 32 /BitsPerComponent 8 /BitsPerFlag 8 /AntiAlias true /Decode [{} {} {} {} 0 1 0 1 0 1] /Filter /FlateDecode /Length {} >>\nstream\n",
            n(s.bbox[0]),
            n(s.bbox[2]),
            n(s.bbox[1]),
            n(s.bbox[3]),
            z.len()
        )
        .into_bytes();
        o.extend_from_slice(&z);
        o.extend_from_slice(b"\nendstream");
        objs.push(o);
    }
    let (y, mo, d, hh, mi, ss) = utc_now();
    objs.push(format!("<< /Title {} /Creator {} /Producer (DGM-NT) /CreationDate (D:{:04}{:02}{:02}{:02}{:02}{:02}Z) >>", utf16_hex(title), utf16_hex("DGM-NT"), y, mo, d, hh, mi, ss).into_bytes());
    debug_assert_eq!(objs.len(), info_id);

    let mut out: Vec<u8> = b"%PDF-1.5\n%\xE2\xE3\xCF\xD3\n".to_vec();
    let mut offs = Vec::with_capacity(objs.len());
    for (i, o) in objs.iter().enumerate() {
        offs.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
        out.extend_from_slice(o);
        out.extend_from_slice(b"\nendobj\n");
    }
    let xref = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for o in &offs {
        out.extend_from_slice(format!("{:010} 00000 n \n", o).as_bytes());
    }
    out.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R /Info {} 0 R >>\nstartxref\n{}\n%%EOF\n", objs.len() + 1, info_id, xref).as_bytes());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimal_pdf_is_well_formed() {
        let mut c = Content::new();
        c.stroke_color([200, 0, 0]);
        c.line_width(0.3);
        c.polyline(&[(10.0, 10.0), (100.0, 50.0)], false);
        c.text(20.0, 20.0, 3.0, false, [0, 0, 0], 30.0, 0.5, 0.5, "Höhe 12,5 m² (ä)", true);
        c.mesh(&[((10.0, 10.0), [255, 0, 0]), ((50.0, 10.0), [0, 255, 0]), ((30.0, 40.0), [0, 0, 255])]);
        let pdf = build(&c, 210.0, 297.0, "Prüfung");
        assert!(pdf.starts_with(b"%PDF-1.5"));
        assert!(pdf.ends_with(b"%%EOF\n"));
        // xref-Offsets zeigen auf "n 0 obj" (byteweise, die Streams sind binär)
        let find = |hay: &[u8], needle: &[u8], from: usize| hay[from..].windows(needle.len()).position(|w| w == needle).map(|p| p + from);
        let mut xr = 0;
        while let Some(p) = find(&pdf, b"startxref\n", xr) {
            xr = p + 1;
        }
        let xr = xr - 1;
        let line_end = find(&pdf, b"\n", xr + 10).unwrap();
        let start: usize = std::str::from_utf8(&pdf[xr + 10..line_end]).unwrap().trim().parse().unwrap();
        assert!(pdf[start..].starts_with(b"xref"));
        let table = std::str::from_utf8(&pdf[start..xr]).unwrap();
        for (i, l) in table.lines().skip(3).take(8).enumerate() {
            let off: usize = l[..10].parse().unwrap();
            assert!(pdf[off..].starts_with(format!("{} 0 obj", i + 1).as_bytes()), "Objekt {}", i + 1);
        }
        assert!(text_width("AAA", 10.0, false) > 0.0);
    }
}

/// Datum (UTC) als TT.MM.JJJJ.
pub fn today() -> String {
    let (y, m, d, ..) = utc_now();
    format!("{:02}.{:02}.{:04}", d, m, y)
}
