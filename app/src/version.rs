//! Zweck:    Programmname, Versions- und Buildnummer, GitHub-Repository (Grundlage für die Update-Suche)
//! Hardware: keine
//! Autor:    Johann Gerner

/// Anzeigename
pub const PROGRAMM: &str = "DGM-NT";
/// Präfix der Release-Pakete: <PAKET>-v<Version>-win-x64.zip
pub const PAKET: &str = "DGM-NT";
/// GitHub-Repository „Besitzer/Name“ (muss öffentlich sein, sonst findet die Update-Suche nichts)
pub const REPOSITORY: &str = "Jgerner01/DGM-NT";
/// Programmdatei, die im Paket enthalten sein muss und nach dem Update neu gestartet wird
pub const GUI_EXE: &str = "dgm-nt.exe";
/// Versionsnummer aus Cargo.toml
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
/// Buildnummer aus der Datei BUILDNUMMER (wird von tools/Release-Paket.ps1 erhöht)
#[allow(dead_code)]
pub const BUILD: &str = env!("APP_BUILD_NUMMER");
/// z. B. „0.5.0 (Build 3)“
pub const FULL: &str = concat!(env!("CARGO_PKG_VERSION"), " (Build ", env!("APP_BUILD_NUMMER"), ")");
