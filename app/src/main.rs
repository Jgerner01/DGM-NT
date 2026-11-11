#![windows_subsystem = "windows"]

mod analysis;
mod compare;
mod export;
mod pdf;
mod scene;
mod app;
mod contours;
mod files;
mod model;
mod render;
mod tin;
mod view;
mod view3d;

use std::path::PathBuf;

/// Programmicon für Fenster und Taskleiste (256 px PNG, zur Compile-Zeit eingebettet).
fn app_icon() -> Option<egui::IconData> {
    let img = image::load_from_memory_with_format(
        include_bytes!("../assets/dgm-nt.png"),
        image::ImageFormat::Png,
    )
    .ok()?
    .into_rgba8();
    let (width, height) = img.dimensions();
    Some(egui::IconData { rgba: img.into_raw(), width, height })
}

fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let mut screenshot: Option<PathBuf> = None;
    let mut open: Option<PathBuf> = None;
    let mut zoom: Option<f64> = None;
    let show_3d = args.iter().any(|a| a == "--3d");
    // Test/Screenshot: --second <gm1> [--analysis volume|diff|iso]
    let mut second: Option<PathBuf> = None;
    let mut analysis: Option<String> = None;
    let (mut export_pdf, mut export_dxf, mut export_gm1): (Option<PathBuf>, Option<PathBuf>, Option<PathBuf>) = (None, None, None);
    let portrait = args.iter().any(|a| a == "--portrait");
    let args_print = args.iter().any(|a| a == "--print");
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--export-pdf" if i + 1 < args.len() => {
                export_pdf = Some(PathBuf::from(&args[i + 1]));
                i += 1;
            }
            "--export-dxf" if i + 1 < args.len() => {
                export_dxf = Some(PathBuf::from(&args[i + 1]));
                i += 1;
            }
            "--export-gm1" if i + 1 < args.len() => {
                export_gm1 = Some(PathBuf::from(&args[i + 1]));
                i += 1;
            }
            "--second" if i + 1 < args.len() => {
                second = Some(PathBuf::from(&args[i + 1]));
                i += 1;
            }
            "--analysis" if i + 1 < args.len() => {
                analysis = Some(args[i + 1].clone());
                i += 1;
            }
            "--screenshot" if i + 1 < args.len() => {
                screenshot = Some(PathBuf::from(&args[i + 1]));
                i += 1;
            }
            "--zoom" if i + 1 < args.len() => {
                zoom = args[i + 1].parse().ok();
                i += 1;
            }
            a if !a.starts_with("--") => open = Some(PathBuf::from(a)),
            _ => {}
        }
        i += 1;
    }
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("DGM-NT – Digitales Geländemodell")
        .with_inner_size([1366.0, 860.0])
        .with_min_inner_size([800.0, 520.0]);
    if let Some(icon) = app_icon() {
        viewport = viewport.with_icon(std::sync::Arc::new(icon));
    }
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    eframe::run_native(
        "DGM-NT",
        options,
        Box::new(move |cc| {
            let mut a = app::DgmApp::new(cc, screenshot, open, zoom);
            a.set_show_3d(show_3d);
            if args_print {
                a.start_print(portrait);
            }
            if export_pdf.is_some() || export_dxf.is_some() || export_gm1.is_some() {
                a.start_exports(export_pdf.clone(), export_dxf.clone(), export_gm1.clone(), portrait);
            }
            if let Some(s) = &second {
                a.start_analysis(s, analysis.as_deref());
            }
            Ok(Box::new(a))
        }),
    )
}
