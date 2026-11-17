use std::fs::File;
use std::io::{self, BufReader, Write};
use std::path::Path;

use tiff::decoder::{Decoder, DecodingResult, Limits};
use tiff::tags::Tag;
use tiff::ColorType;

use crate::files;
use crate::job::ProgressFn;

/// Fortschrittsmeldung beim Schreiben alle so viele Rasterwerte.
const WRITE_PROGRESS_SAMPLES: usize = 1 << 18;
/// Relative Toleranz beim Vergleich mit dem NoData-Wert (Float32-Werte stimmen nur auf ~7 Stellen überein).
const NODATA_REL_TOL: f64 = 1e-6;
/// Betrag, ab dem ein Wert als NoData-Platzhalter gilt (z. B. ±3,4e38 bei Float32-Rastern).
const NODATA_HUGE: f64 = 1e30;
/// GeoKey-Nummer der vertikalen Einheit (VerticalUnitsGeoKey)
const KEY_VERTICAL_UNITS: u16 = 4099;
/// GeoKey-Nummer der Längeneinheit projizierter Koordinaten (ProjLinearUnitsGeoKey)
const KEY_LINEAR_UNITS: u16 = 3076;

#[derive(Clone, Copy)]
struct GeoTransform {
    xx: f64,
    xy: f64,
    x0: f64,
    yx: f64,
    yy: f64,
    y0: f64,
    unit_to_meters: f64,
}

impl GeoTransform {
    fn from_decoder<R: io::Read + io::Seek>(decoder: &mut Decoder<R>) -> Result<Self, String> {
        let unit_to_meters = unit_factor(decoder, KEY_LINEAR_UNITS)?;

        if let Some(keys) = geo_keys(decoder) {
            if let Some(model_type) = geo_key_value(&keys, 1024) {
                if model_type != 1 {
                    return Err(
                        "GeoTIFF-Koordinaten müssen projiziert sein (geografische Gradwerte werden nicht umgerechnet)."
                            .into(),
                    );
                }
            }
        }

        if let Ok(matrix) = decoder.get_tag_f64_vec(Tag::ModelTransformationTag) {
            if matrix.len() != 16 {
                return Err("Ungültige GeoTIFF-Transformationsmatrix.".into());
            }
            if matrix[2].abs() > 1e-12
                || matrix[6].abs() > 1e-12
                || matrix[12].abs() > 1e-12
                || matrix[13].abs() > 1e-12
                || matrix[14].abs() > 1e-12
                || matrix[15].abs() < 1e-12
            {
                return Err("Nicht affine GeoTIFF-Transformationsmatrix wird nicht unterstützt.".into());
            }
            return Ok(Self {
                xx: matrix[0] / matrix[15],
                xy: matrix[1] / matrix[15],
                x0: matrix[3] / matrix[15],
                yx: matrix[4] / matrix[15],
                yy: matrix[5] / matrix[15],
                y0: matrix[7] / matrix[15],
                unit_to_meters,
            });
        }

        let scale = decoder
            .get_tag_f64_vec(Tag::ModelPixelScaleTag)
            .map_err(|_| "GeoTIFF enthält keine Pixelmaßstabsangabe.".to_string())?;
        let tie = decoder
            .get_tag_f64_vec(Tag::ModelTiepointTag)
            .map_err(|_| "GeoTIFF enthält keine Referenzkoordinate (Tiepoint).".to_string())?;
        if scale.len() < 2 || tie.len() < 6 || scale[0] == 0.0 || scale[1] == 0.0 {
            return Err("Ungültige GeoTIFF-Pixelmaßstabs- oder Tiepoint-Angabe.".into());
        }
        Ok(Self {
            xx: scale[0],
            xy: 0.0,
            x0: tie[3] - tie[0] * scale[0],
            yx: 0.0,
            yy: -scale[1],
            y0: tie[4] + tie[1] * scale[1],
            unit_to_meters,
        })
    }

    fn point(self, column: f64, row: f64) -> (f64, f64) {
        (
            (self.xx * column + self.xy * row + self.x0) * self.unit_to_meters,
            (self.yx * column + self.yy * row + self.y0) * self.unit_to_meters,
        )
    }
}

/// Umrechnungsfaktor einer GeoTIFF-Längeneinheit (Schlüssel `key`) in Meter; ohne Angabe 1.
fn unit_factor<R: io::Read + io::Seek>(decoder: &mut Decoder<R>, key: u16) -> Result<f64, String> {
    geo_keys(decoder)
        .and_then(|keys| geo_key_value(&keys, key))
        .map(|unit| match unit {
            9001 => Ok(1.0),
            9002 => Ok(0.3048),
            9003 => Ok(1200.0 / 3937.0),
            _ => Err(format!("Nicht unterstützte GeoTIFF-Längeneinheit: {}", unit)),
        })
        .transpose()
        .map(|f| f.unwrap_or(1.0))
}

fn geo_keys<R: io::Read + io::Seek>(decoder: &mut Decoder<R>) -> Option<Vec<u16>> {
    decoder.get_tag_u16_vec(Tag::GeoKeyDirectoryTag).ok()
}

fn geo_key_value(keys: &[u16], wanted: u16) -> Option<u16> {
    let count = *keys.get(3)? as usize;
    for entry in keys.get(4..)?.chunks_exact(4).take(count) {
        if entry[0] == wanted && entry[1] == 0 && entry[2] == 1 {
            return Some(entry[3]);
        }
    }
    None
}

/// Anzahl der Rasterwerte und Zugriff auf den i-ten Wert als f64 – ohne das ganze Raster zu kopieren.
fn sample_accessor(image: &DecodingResult) -> (usize, Box<dyn Fn(usize) -> f64 + '_>) {
    match image {
        DecodingResult::U8(v) => (v.len(), Box::new(move |i| f64::from(v[i]))),
        DecodingResult::U16(v) => (v.len(), Box::new(move |i| f64::from(v[i]))),
        DecodingResult::U32(v) => (v.len(), Box::new(move |i| f64::from(v[i]))),
        DecodingResult::U64(v) => (v.len(), Box::new(move |i| v[i] as f64)),
        DecodingResult::F16(v) => (v.len(), Box::new(move |i| f64::from(v[i].to_f32()))),
        DecodingResult::F32(v) => (v.len(), Box::new(move |i| f64::from(v[i]))),
        DecodingResult::F64(v) => (v.len(), Box::new(move |i| v[i])),
        DecodingResult::I8(v) => (v.len(), Box::new(move |i| f64::from(v[i]))),
        DecodingResult::I16(v) => (v.len(), Box::new(move |i| f64::from(v[i]))),
        DecodingResult::I32(v) => (v.len(), Box::new(move |i| f64::from(v[i]))),
        DecodingResult::I64(v) => (v.len(), Box::new(move |i| v[i] as f64)),
    }
}

/// Ist `z` der NoData-Wert des Rasters (oder ein riesiger Platzhalterwert)?
fn is_nodata(z: f64, nodata: Option<f64>) -> bool {
    z.abs() >= NODATA_HUGE || nodata.map_or(false, |n| (z - n).abs() <= n.abs() * NODATA_REL_TOL)
}

fn nodata_value<R: io::Read + io::Seek>(decoder: &mut Decoder<R>) -> Result<Option<f64>, String> {
    for tag in decoder.tag_iter() {
        let (tag, value) = tag.map_err(|e| format!("GeoTIFF-Metadaten: {}", e))?;
        if tag == Tag::GdalNodata {
            let value = value
                .into_string()
                .map_err(|e| format!("Ungültiger GeoTIFF-NoData-Wert: {}", e))?;
            let value = value
                .trim_matches('\0')
                .trim()
                .parse::<f64>()
                .map_err(|e| format!("Ungültiger GeoTIFF-NoData-Wert: {}", e))?;
            return Ok(Some(value));
        }
    }
    Ok(None)
}

#[allow(clippy::too_many_arguments)]
fn write_raster_points(
    writer: &mut dyn Write,
    width: u32,
    height: u32,
    transform: GeoTransform,
    pixel_is_point: bool,
    nodata: Option<f64>,
    z_to_meters: f64,
    len: usize,
    value: &dyn Fn(usize) -> f64,
    tile: Option<usize>,
    progress: &mut dyn FnMut(f32),
) -> io::Result<usize> {
    let expected = width as usize * height as usize;
    if len != expected {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Die Anzahl der TIFF-Höhenwerte passt nicht zur Rastergröße.",
        ));
    }
    let pixel_offset = if pixel_is_point { 0.0 } else { 0.5 };
    let mut point_number = 0usize;
    for index in 0..len {
        if index % WRITE_PROGRESS_SAMPLES == 0 {
            progress(index as f32 / len.max(1) as f32);
        }
        let raw = value(index);
        if !raw.is_finite() || is_nodata(raw, nodata) {
            continue;
        }
        let z = raw * z_to_meters;
        point_number += 1;
        let column = index % width as usize;
        let row = index / width as usize;
        let (x, y) = transform.point(column as f64 + pixel_offset, row as f64 + pixel_offset);
        if !x.is_finite() || !y.is_finite() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Die GeoTIFF-Transformation erzeugt ungültige Koordinaten.",
            ));
        }
        let name = match tile {
            Some(tile_number) => format!("{}_{}", tile_number, point_number),
            None => point_number.to_string(),
        };
        // Zeilenende CR/LF wie in allen GM1-Dateien des alten Programms
        write!(writer, "{:>12} {:>15.3} {:>15.3} {:>10.3} \r\n", name, x, y, z)?;
    }
    progress(1.0);
    Ok(point_number)
}

fn import_one(
    path: &Path,
    writer: &mut dyn Write,
    tile: Option<usize>,
    progress: &mut dyn FnMut(f32),
) -> io::Result<usize> {
    let file = File::open(path)?;
    // Die Standardgrenze der tiff-Bibliothek (256 MB je Bild) würde große Höhenraster ablehnen.
    let mut decoder = Decoder::new(BufReader::new(file))
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?
        .with_limits(Limits::unlimited());
    let (width, height) = decoder
        .dimensions()
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
    let color_type = decoder
        .colortype()
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
    if !matches!(color_type, ColorType::Gray(_)) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Es werden nur einbandige GeoTIFF-Höhenraster unterstützt.",
        ));
    }
    if decoder
        .get_tag_u32(Tag::Orientation)
        .map_or(false, |orientation| orientation != 1)
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "GeoTIFFs mit abweichender TIFF-Bildorientierung werden nicht unterstützt.",
        ));
    }
    let transform = GeoTransform::from_decoder(&mut decoder)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let pixel_is_point = geo_keys(&mut decoder)
        .and_then(|keys| geo_key_value(&keys, 1025))
        .map_or(false, |raster_type| raster_type == 2);
    let nodata = nodata_value(&mut decoder)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    // Höheneinheit: nur wenn das GeoTIFF sie angibt, sonst gelten Meter
    let z_to_meters = unit_factor(&mut decoder, KEY_VERTICAL_UNITS)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let image = decoder
        .read_image()
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
    let (len, value) = sample_accessor(&image);
    write_raster_points(
        writer,
        width,
        height,
        transform,
        pixel_is_point,
        nodata,
        z_to_meters,
        len,
        &*value,
        tile,
        progress,
    )
}

pub fn write_gm1(inputs: &[std::path::PathBuf], output: &Path, progress: ProgressFn) -> io::Result<usize> {
    if inputs.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Es wurden keine GeoTIFF-Dateien ausgewählt.",
        ));
    }
    let multiple = inputs.len() > 1;
    let mut total = 0usize;
    files::write_gm1_stream(output, |writer| {
        let mut reference_crs: Option<Option<u16>> = None;
        for (index, path) in inputs.iter().enumerate() {
            let file = File::open(path)?;
            let mut decoder = Decoder::new(BufReader::new(file))
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
            let crs = geo_keys(&mut decoder).and_then(|keys| geo_key_value(&keys, 3072));
            if let Some(reference) = reference_crs {
                if reference != crs {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "Die ausgewählten GeoTIFFs verwenden unterschiedliche oder nicht durchgängig angegebene projizierte Koordinatensysteme.",
                    ));
                }
            } else {
                reference_crs = Some(crs);
            }
            let tile = multiple.then_some(index + 1);
            let count = inputs.len() as f32;
            let stage = format!("GEO-TIFF {} von {}: Punkte schreiben", index + 1, inputs.len());
            progress(index as f32 / count, &format!("GEO-TIFF {} von {}: Raster lesen", index + 1, inputs.len()));
            let imported = import_one(path, writer, tile, &mut |f| progress((index as f32 + f) / count, &stage))
                .map_err(|e| io::Error::new(e.kind(), format!("{}: {}", path.display(), e)))?;
            total = total.checked_add(imported).ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidData, "Zu viele Rasterpunkte für eine GM1-Datei.")
            })?;
        }
        progress(1.0, "GEO-TIFF: fertig");
        if total == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Die ausgewählten GeoTIFFs enthalten keine gültigen Höhenwerte.",
            ));
        }
        Ok(())
    })?;
    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scale_tiepoint_places_pixel_centers() {
        let transform = GeoTransform {
            xx: 2.0,
            xy: 0.0,
            x0: 100.0,
            yx: 0.0,
            yy: -3.0,
            y0: 200.0,
            unit_to_meters: 1.0,
        };
        assert_eq!(transform.point(0.5, 0.5), (101.0, 198.5));
    }

    #[test]
    fn multiple_tiles_prefix_point_names_and_skip_nodata() {
        let transform = GeoTransform {
            xx: 1.0,
            xy: 0.0,
            x0: 0.0,
            yx: 0.0,
            yy: -1.0,
            y0: 2.0,
            unit_to_meters: 1.0,
        };
        let mut output = Vec::new();
        let count = write_raster_points(
            &mut output,
            2,
            2,
            transform,
            false,
            Some(-9999.0),
            1.0,
            4,
            &|i| [10.0, -9999.0, 12.0, f64::NAN][i],
            Some(2),
            &mut |_| {},
        )
        .unwrap();
        assert_eq!(count, 2);
        let text = String::from_utf8(output).unwrap();
        assert!(text.contains("2_1"));
        assert!(text.contains("2_2"));
        assert_eq!(text.lines().count(), 2);
    }

    #[test]
    fn nodata_matches_float32_and_huge_values() {
        // NoData-Wert als gerundeter Text, Rasterwert als Float32
        let nodata = Some(-3.40282e38);
        assert!(is_nodata(f64::from(f32::MIN), nodata));
        assert!(is_nodata(-9999.0, Some(-9999.0)));
        assert!(is_nodata(f64::from(f32::MAX), None));
        assert!(!is_nodata(412.37, Some(-9999.0)));
        assert!(!is_nodata(0.0, Some(0.001)));
    }

    #[test]
    fn height_unit_is_converted_and_lines_end_with_crlf() {
        let transform = GeoTransform { xx: 1.0, xy: 0.0, x0: 0.0, yx: 0.0, yy: -1.0, y0: 1.0, unit_to_meters: 1.0 };
        let mut output = Vec::new();
        write_raster_points(&mut output, 1, 1, transform, false, None, 0.3048, 1, &|_| 100.0, None, &mut |_| {}).unwrap();
        let text = String::from_utf8(output).unwrap();
        assert!(text.contains("30.480"));
        assert!(text.ends_with("\r\n"));
    }

    #[test]
    fn geo_key_value_reads_inline_key() {
        let keys = [1, 1, 0, 2, 1024, 0, 1, 1, 3072, 0, 1, 32632];
        assert_eq!(geo_key_value(&keys, 3072), Some(32632));
    }
}
