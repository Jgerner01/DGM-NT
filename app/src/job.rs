//! Fortschrittsanzeige für Hintergrundarbeiten (Punkte einlesen, Vermaschen, GEO-TIFF umwandeln).
//! Die Arbeit läuft in einem eigenen Thread und meldet Anteil und Arbeitsschritt über `Progress`;
//! die Oberfläche liest beides in jedem Bild und zeichnet den Fortschrittsbalken.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

/// Anteil in Promille; dieser Wert bedeutet „Dauer unbekannt“ (Anzeige ohne Prozentangabe).
const INDETERMINATE: u32 = u32::MAX;
const PERMILLE: f32 = 1000.0;

/// Rückmeldefunktion der Arbeitsschritte: Anteil 0..1 und Name des Arbeitsschritts.
pub type ProgressFn<'a> = &'a mut dyn FnMut(f32, &str);

pub struct Progress {
    permille: AtomicU32,
    stage: Mutex<String>,
}

impl Progress {
    pub fn new() -> Arc<Progress> {
        Arc::new(Progress { permille: AtomicU32::new(0), stage: Mutex::new(String::new()) })
    }

    /// Anteil (0..1) und Arbeitsschritt melden.
    pub fn set(&self, fraction: f32, stage: &str) {
        let pm = (fraction.clamp(0.0, 1.0) * PERMILLE) as u32;
        self.permille.store(pm, Ordering::Relaxed);
        self.set_stage(stage);
    }

    /// Arbeitsschritt ohne bekannten Fortschritt melden.
    pub fn set_indeterminate(&self, stage: &str) {
        self.permille.store(INDETERMINATE, Ordering::Relaxed);
        self.set_stage(stage);
    }

    fn set_stage(&self, stage: &str) {
        if let Ok(mut s) = self.stage.lock() {
            if s.as_str() != stage {
                *s = stage.to_string();
            }
        }
    }

    /// Anteil 0..1 oder `None`, wenn die Dauer unbekannt ist.
    pub fn fraction(&self) -> Option<f32> {
        match self.permille.load(Ordering::Relaxed) {
            INDETERMINATE => None,
            pm => Some(pm as f32 / PERMILLE),
        }
    }

    pub fn stage(&self) -> String {
        self.stage.lock().map(|s| s.clone()).unwrap_or_default()
    }
}
