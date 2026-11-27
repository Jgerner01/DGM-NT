//! Zweck:    Update-Dienst: sucht neue Versionen in den GitHub-Releases, lädt Paket und Prüfsumme,
//!           prüft, entpackt, tauscht die Programmdateien aus und startet neu. Enthält das Fenster
//!           „Programm-Update“ (Menü Hilfe → Nach Updates suchen).
//! Hardware: Windows-PC
//! Autor:    Johann Gerner
//!
//! Gemeinsamer Standard aller Rust-Programme (Vorlage: GNSS-NT). Projektabhängig sind nur die
//! Konstanten in `version.rs`. Ein Release braucht zwei Dateien (siehe tools/Release-Paket.ps1):
//!   <PAKET>-v1.2.3-win-x64.zip  und  <PAKET>-v1.2.3-win-x64.zip.sha256
//! Die Prüfsumme erkennt beschädigte Downloads, ersetzt aber keine Signatur.
//!
//! Windows erlaubt das Umbenennen einer laufenden EXE. Die alten Dateien wandern deshalb in den
//! Unterordner `_vorherige_Version`, die neuen werden an ihre Stelle kopiert, dann Neustart.

use crate::version;
use anyhow::{Context, Result, bail};
use eframe::egui;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

pub const BACKUP_DIR: &str = "_vorherige_Version";
const MAX_SIZE: u64 = 300 * 1024 * 1024;
const SEARCH_TIMEOUT: Duration = Duration::from_secs(15);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const CHECKSUM_LENGTH: usize = 64;
const BUFFER: usize = 64 * 1024;
const PERCENT: u64 = 100;
const BYTES_PER_MB: f64 = 1e6;
const REPAINT_MS: u64 = 100;
const WINDOW_WIDTH: f32 = 420.0;
const COLOR_GOOD: egui::Color32 = egui::Color32::from_rgb(40, 150, 60);
const COLOR_NEW: egui::Color32 = egui::Color32::from_rgb(50, 110, 200);
const COLOR_BAD: egui::Color32 = egui::Color32::from_rgb(200, 50, 50);

fn api_url() -> String {
    format!("https://api.github.com/repos/{}/releases/latest", version::REPOSITORY)
}

/// Eine gefundene neuere Version.
#[derive(Debug, Clone, PartialEq)]
pub struct Info {
    pub tag: String,
    pub notes: String,
    pub zip_url: String,
    pub checksum_url: String,
    pub size: u64,
}

/// Version „1.2.3“ oder „v1.2.3“ als vergleichbare Zahlen.
pub fn parse_version(text: &str) -> Option<[u64; 3]> {
    let parts: Vec<u64> = text
        .trim()
        .trim_start_matches('v')
        .split('.')
        .map(|p| p.parse().ok())
        .collect::<Option<_>>()?;
    parts.try_into().ok()
}

pub fn package_name(tag: &str) -> String {
    format!("{}-{tag}-win-x64.zip", version::PAKET)
}

/// Nur https-Adressen auf github.com ohne Benutzerangabe und Port.
fn is_github_url(url: &str) -> bool {
    url.strip_prefix("https://")
        .is_some_and(|r| r.split(['/', '?', '#']).next() == Some("github.com"))
}

/// Wertet die Antwort aus; `None`, wenn nichts Neueres oder kein vollständiges Paket vorliegt.
pub fn read_release(json: &str, current: &str) -> Result<Option<Info>> {
    let root: serde_json::Value = serde_json::from_str(json).context("ungültige Antwort")?;
    if root["draft"].as_bool() == Some(true) || root["prerelease"].as_bool() == Some(true) {
        return Ok(None);
    }
    let tag = root["tag_name"]
        .as_str()
        .context("ungültige Antwort: tag_name fehlt")?
        .to_string();
    let current = parse_version(current).context("eigene Version nicht lesbar")?;
    if parse_version(&tag).is_none_or(|v| v <= current) {
        return Ok(None);
    }
    let zip = package_name(&tag);
    let assets = root["assets"]
        .as_array()
        .context("ungültige Antwort: assets fehlt")?;
    let find = |name: &str| assets.iter().find(|a| a["name"].as_str() == Some(name));
    let (Some(package), Some(checksum)) = (find(&zip), find(&format!("{zip}.sha256"))) else {
        return Ok(None);
    };
    let url = |a: &serde_json::Value| a["browser_download_url"].as_str().unwrap_or("").to_string();
    let info = Info {
        notes: root["body"].as_str().unwrap_or("").to_string(),
        zip_url: url(package),
        checksum_url: url(checksum),
        size: package["size"].as_u64().unwrap_or(0),
        tag,
    };
    let safe = is_github_url(&info.zip_url) && is_github_url(&info.checksum_url);
    Ok((safe && info.size > 0 && info.size <= MAX_SIZE).then_some(info))
}

fn agent(timeout: Option<Duration>) -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(timeout)
        .timeout_connect(Some(CONNECT_TIMEOUT))
        .http_status_as_error(false)
        .user_agent(format!("{}/{}", version::PAKET, version::VERSION))
        .build()
        .into()
}

fn get(agent: &ureq::Agent, url: &str) -> Result<ureq::http::Response<ureq::Body>> {
    let response = agent.get(url).call().context("Verbindung fehlgeschlagen")?;
    if !response.status().is_success() {
        bail!("Der Server antwortete mit Status {}.", response.status().as_u16());
    }
    Ok(response)
}

/// Sucht auf GitHub nach einer neueren Version; ohne Release ergibt sich `None`.
pub fn search(current: &str) -> Result<Option<Info>> {
    let mut response = agent(Some(SEARCH_TIMEOUT))
        .get(&api_url())
        .call()
        .context("Verbindung fehlgeschlagen")?;
    match response.status().as_u16() {
        404 => Ok(None),
        200..300 => read_release(
            &response.body_mut().read_to_string().context("Antwort nicht lesbar")?,
            current,
        ),
        s => bail!("GitHub antwortete mit Status {s}."),
    }
}

/// Ordner für heruntergeladene Updates.
pub fn download_dir() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(version::PAKET)
        .join("update")
}

pub fn sha256_hex(data: &[u8]) -> String {
    Sha256::digest(data).iter().map(|b| format!("{b:02X}")).collect()
}

/// Lädt und prüft das Paket und liefert den entpackten Ordner. `progress` in Prozent.
pub fn download(info: &Info, dir: &Path, progress: &AtomicU64) -> Result<PathBuf> {
    let agent = agent(None);
    let text = get(&agent, &info.checksum_url)?
        .body_mut()
        .read_to_string()
        .context("Prüfsumme nicht lesbar")?;
    let expected = text.split_whitespace().next().unwrap_or("").to_string();
    if expected.len() != CHECKSUM_LENGTH {
        bail!("Die Prüfsummendatei des Updates ist ungültig.");
    }
    let mut source = get(&agent, &info.zip_url)?.into_body().into_reader();
    let mut data = Vec::new();
    let mut buffer = vec![0u8; BUFFER];
    loop {
        let n = source.read(&mut buffer).context("Download unterbrochen")?;
        if n == 0 {
            break;
        }
        data.extend_from_slice(&buffer[..n]);
        if data.len() as u64 > info.size {
            bail!("Das Update ist größer als angekündigt.");
        }
        progress.store(data.len() as u64 * PERCENT / info.size, Ordering::Relaxed);
    }
    if data.len() as u64 != info.size {
        bail!("Das Update wurde nicht vollständig geladen.");
    }
    if !sha256_hex(&data).eq_ignore_ascii_case(&expected) {
        bail!("Die Prüfsumme des Updates stimmt nicht.");
    }
    let target = dir.join(&info.tag);
    if target.exists() {
        fs::remove_dir_all(&target).context("alter Update-Ordner nicht löschbar")?;
    }
    unpack(&data, &target)?;
    Ok(target)
}

/// Entpackt das Paket; Pfade außerhalb des Zielordners werden abgelehnt.
fn unpack(data: &[u8], target: &Path) -> Result<()> {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(data))
        .context("Das Update-Paket ist ungültig.")?;
    fs::create_dir_all(target)?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).context("Das Update-Paket ist ungültig.")?;
        let Some(relative) = entry.enclosed_name() else {
            bail!("Das Update-Paket enthält einen unzulässigen Pfad.");
        };
        let path = target.join(relative);
        if entry.is_dir() {
            fs::create_dir_all(&path)?;
            continue;
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut file = fs::File::create(&path)?;
        std::io::copy(&mut entry, &mut file)?;
        file.flush()?;
    }
    if !target.join(version::GUI_EXE).is_file() {
        bail!("Das Update enthält keine Programmdatei.");
    }
    Ok(())
}

/// Austauschbar sind nur ausgelieferte Programme, keine Entwicklungsbuilds im Zielordner von cargo.
pub fn is_installed(exe: &Path) -> bool {
    exe.file_name().is_some_and(|n| n.eq_ignore_ascii_case(version::GUI_EXE))
        && exe.parent().is_some_and(|d| !d.join(".cargo-lock").exists())
}

/// Ersetzt die Dateien im Zielordner durch die des Pakets (nur oberste Ebene).
/// Bei einem Fehler wird der vorherige Stand wiederhergestellt.
pub fn apply(source: &Path, target: &Path) -> Result<()> {
    if !source.join(version::GUI_EXE).is_file() {
        bail!("Das Update enthält keine Programmdatei.");
    }
    let backup = target.join(BACKUP_DIR);
    if backup.exists() {
        fs::remove_dir_all(&backup)
            .context("alte Sicherung nicht löschbar (läuft die alte Version noch?)")?;
    }
    fs::create_dir_all(&backup).context("Programmordner ist nicht beschreibbar")?;
    let mut saved: Vec<(PathBuf, PathBuf)> = Vec::new();
    let mut copied: Vec<PathBuf> = Vec::new();
    let result = (|| -> Result<()> {
        for entry in fs::read_dir(source)? {
            let new = entry?.path();
            let Some(name) = new.file_name().filter(|_| new.is_file()) else {
                continue;
            };
            let old = target.join(name);
            if old.exists() {
                let copy = backup.join(name);
                fs::rename(&old, &copy)
                    .with_context(|| format!("{} nicht verschiebbar", old.display()))?;
                saved.push((old.clone(), copy));
            }
            copied.push(old.clone());
            fs::copy(&new, &old).with_context(|| format!("{} nicht schreibbar", old.display()))?;
        }
        Ok(())
    })();
    if result.is_err() {
        for file in &copied {
            let _ = fs::remove_file(file);
        }
        for (original, copy) in &saved {
            let _ = fs::rename(copy, original);
        }
    }
    result
}

// ---------------------------------------------------------------------------------------------
// Oberfläche
// ---------------------------------------------------------------------------------------------

enum Answer {
    Searched(Result<Option<Info>, String>),
    Downloaded(Result<PathBuf, String>),
}

#[derive(Default)]
enum Status {
    #[default]
    Idle,
    Searching,
    Current,
    Available(Info),
    Loading(Info),
    /// geladen und geprüft, Ordner mit den neuen Dateien
    Ready(Info, PathBuf),
    Failed(String),
}

/// Zustand und Fenster der Update-Funktion. Einbindung:
///   beim Start `updater.search_quietly(ctx)`, im Menü Hilfe `updater.menu_entry(ui)`,
///   in jedem Frame `updater.show(ctx)`.
#[derive(Default)]
pub struct Updater {
    status: Status,
    progress: Arc<AtomicU64>,
    answers: Option<Receiver<Answer>>,
    open: bool,
}

impl Updater {
    fn run(&mut self, ctx: &egui::Context, job: impl FnOnce() -> Answer + Send + 'static) {
        let (tx, rx) = mpsc::channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(job());
            ctx.request_repaint();
        });
        self.answers = Some(rx);
    }

    fn search(&mut self, ctx: &egui::Context) {
        if self.answers.is_some() {
            return;
        }
        self.status = Status::Searching;
        self.run(ctx, || {
            Answer::Searched(search(version::VERSION).map_err(|e| format!("{e:#}")))
        });
    }

    /// Suche beim Programmstart: das Fenster öffnet sich nur, wenn eine neue Version vorliegt.
    /// Entwicklungsbuilds suchen nicht.
    pub fn search_quietly(&mut self, ctx: &egui::Context) {
        if std::env::current_exe().is_ok_and(|p| is_installed(&p)) {
            self.search(ctx);
        }
    }

    /// Menüeintrag „Nach Updates suchen …“
    pub fn menu_entry(&mut self, ui: &mut egui::Ui) {
        if ui.button("Nach Updates suchen …").clicked() {
            self.open = true;
            self.search(ui.ctx());
            ui.close();
        }
    }

    fn download(&mut self, ctx: &egui::Context) {
        let Status::Available(info) = &self.status else {
            return;
        };
        let info = info.clone();
        self.status = Status::Loading(info.clone());
        self.progress.store(0, Ordering::Relaxed);
        let progress = self.progress.clone();
        self.run(ctx, move || {
            Answer::Downloaded(
                download(&info, &download_dir(), &progress).map_err(|e| format!("{e:#}")),
            )
        });
    }

    fn poll(&mut self) {
        let Some(answer) = self.answers.as_ref().and_then(|r| r.try_recv().ok()) else {
            return;
        };
        self.answers = None;
        self.status = match (answer, std::mem::take(&mut self.status)) {
            (Answer::Searched(Ok(Some(info))), _) => {
                self.open = true;
                Status::Available(info)
            }
            (Answer::Searched(Ok(None)), _) => Status::Current,
            (Answer::Downloaded(Ok(dir)), Status::Loading(info)) => Status::Ready(info, dir),
            (Answer::Searched(Err(e)) | Answer::Downloaded(Err(e)), _) => Status::Failed(e),
            (Answer::Downloaded(Ok(_)), other) => other,
        };
    }

    /// Tauscht die Programmdateien aus, startet die neue Version und beendet diese.
    fn install(&mut self, ctx: &egui::Context, source: PathBuf) {
        let exe = match std::env::current_exe() {
            Ok(p) if is_installed(&p) => p,
            _ => {
                self.status = Status::Failed(
                    "Dieser Programmstand ist ein Entwicklungsbuild und wird nicht ersetzt.".into(),
                );
                return;
            }
        };
        let Some(dir) = exe.parent() else { return };
        if let Err(e) = apply(&source, dir) {
            self.status =
                Status::Failed(format!("Update fehlgeschlagen, bisherige Version bleibt: {e:#}"));
            return;
        }
        match std::process::Command::new(dir.join(version::GUI_EXE)).current_dir(dir).spawn() {
            Ok(_) => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            Err(e) => {
                self.status =
                    Status::Failed(format!("Update installiert, Neustart fehlgeschlagen: {e}"))
            }
        }
    }

    /// Fenster „Programm-Update“; in jedem Frame aufrufen.
    pub fn show(&mut self, ctx: &egui::Context) {
        self.poll();
        if !self.open {
            return;
        }
        let mut open = self.open;
        let mut search = false;
        let mut load = false;
        let mut install = None;
        egui::Window::new("Programm-Update")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(WINDOW_WIDTH)
            .show(ctx, |ui| {
                ui.label(format!("{} {}", version::PROGRAMM, version::FULL));
                ui.separator();
                match &self.status {
                    Status::Idle => {}
                    Status::Searching => {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label("Suche auf GitHub …");
                        });
                    }
                    Status::Current => {
                        ui.colored_label(COLOR_GOOD, format!("Version {} ist aktuell.", version::VERSION));
                    }
                    Status::Available(info) => {
                        ui.colored_label(
                            COLOR_NEW,
                            format!("Neue Version {} ({:.1} MB)", info.tag, info.size as f64 / BYTES_PER_MB),
                        );
                        egui::ScrollArea::vertical().max_height(240.0).show(ui, |ui| {
                            ui.label(&info.notes);
                        });
                        load = ui.button("Herunterladen").clicked();
                    }
                    Status::Loading(info) => {
                        let p = self.progress.load(Ordering::Relaxed);
                        ui.add(
                            egui::ProgressBar::new(p as f32 / PERCENT as f32)
                                .text(format!("{} wird geladen: {p} %", info.tag)),
                        );
                        ui.ctx().request_repaint_after(Duration::from_millis(REPAINT_MS));
                    }
                    Status::Ready(info, dir) => {
                        ui.colored_label(COLOR_GOOD, format!("{} ist geladen und geprüft.", info.tag));
                        ui.label("Vorher offene Arbeiten speichern.");
                        if ui.button("Installieren und neu starten").clicked() {
                            install = Some(dir.clone());
                        }
                    }
                    Status::Failed(e) => {
                        ui.colored_label(COLOR_BAD, e);
                    }
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    search = ui
                        .add_enabled(self.answers.is_none(), egui::Button::new("Erneut suchen"))
                        .clicked();
                    ui.hyperlink_to(
                        "Alle Versionen auf GitHub",
                        format!("https://github.com/{}/releases", version::REPOSITORY),
                    );
                });
            });
        self.open = open;
        if search {
            self.search(ctx);
        }
        if load {
            self.download(ctx);
        }
        if let Some(dir) = install {
            self.install(ctx, dir);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const URL: &str = "https://github.com/Jgerner01/x/releases/download/v1.2.3/x.zip";

    fn release(tag: &str, url: &str, size: u64) -> String {
        let zip = package_name(tag);
        format!(
            r#"{{"tag_name":"{tag}","body":"Neu","draft":false,"prerelease":false,"assets":[
            {{"name":"{zip}","browser_download_url":"{url}","size":{size}}},
            {{"name":"{zip}.sha256","browser_download_url":"{url}.sha256","size":90}}]}}"#
        )
    }

    #[test]
    fn finds_only_newer_safe_releases() {
        let json = release("v1.2.3", URL, 100);
        assert_eq!(read_release(&json, "0.3.0").unwrap().unwrap().tag, "v1.2.3");
        assert!(read_release(&json, "1.2.3").unwrap().is_none());
        assert!(read_release(&release("v1.10.0", URL, 100), "1.9.9").unwrap().is_some());
        assert!(
            read_release(&json.replace("\"draft\":false", "\"draft\":true"), "0.1.0")
                .unwrap()
                .is_none()
        );
        for url in [
            "http://github.com/x.zip",
            "https://github.com.evil.example/x",
            "https://github.com@evil.example/x",
        ] {
            assert!(read_release(&release("v1.2.3", url, 100), "0.1.0").unwrap().is_none(), "{url}");
        }
        assert!(read_release(&release("v1.2.3", URL, 0), "0.1.0").unwrap().is_none());
        assert!(read_release("kein json", "0.1.0").is_err());
    }

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("{}-update-{name}-{}", version::PAKET, std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn apply_keeps_backup_and_foreign_files() {
        let root = temp("apply");
        let (new, old) = (root.join("neu"), root.join("prog"));
        fs::create_dir_all(&new).unwrap();
        fs::create_dir_all(&old).unwrap();
        fs::write(new.join(version::GUI_EXE), "neu").unwrap();
        fs::write(old.join(version::GUI_EXE), "alt").unwrap();
        fs::write(old.join("eigene.txt"), "bleibt").unwrap();
        apply(&new, &old).unwrap();
        assert_eq!(fs::read_to_string(old.join(version::GUI_EXE)).unwrap(), "neu");
        assert_eq!(fs::read_to_string(old.join(BACKUP_DIR).join(version::GUI_EXE)).unwrap(), "alt");
        assert_eq!(fs::read_to_string(old.join("eigene.txt")).unwrap(), "bleibt");
        assert!(apply(&root.join("leer"), &old).is_err());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn development_build_is_not_replaced() {
        let d = temp("dev");
        assert!(is_installed(&d.join(version::GUI_EXE)));
        fs::write(d.join(".cargo-lock"), "").unwrap();
        assert!(!is_installed(&d.join(version::GUI_EXE)));
        let _ = fs::remove_dir_all(d);
    }
}
