# DGM-NT – Digitales Geländemodell (Rust)

Neubau des Programms DGM (Delphi, `dgmx.exe`) in Rust mit egui. Dieselben Dateien (GM0, GM1, GM2, GM9, GMG),
dazu stufenloser Vektor-Zoom, Autospeichern und ein Modellvergleich.

## Funktionen

- **Dateien:** liest und schreibt die Projektdateien des Originals (GM0/GM1/GM2/GM9/GMG); Autospeichern alle 60 s
  (`<Projekt>.dgmnt.json`), Rückgängig/Wiederholen.
- **Modell:** Vermaschung mit Bruchkanten, Umring und Inringen, glatte Modellfläche ohne Überschwingen, Höhenlinien
  mit Zähllinien und Beschriftung, Höhenfarben mit Schummerung, 3D-Ansicht, Profil, Volumen über Bezugshöhe.
- **Analyse – Modellvergleich:** zweites Modell einlesen, gemeinsamer Umring, Auftrags- und Abtragsvolumen aus
  Dreiecksprismen (mit Auflockerungsfaktoren, Standard 1,1 und 1,5), Höhenunterschied als Farbschema und Isolinien,
  Differenzmodell als GM1 speichern.
- **Ausgabe:** vektorbasierte PDF-Druckausgabe auf A4 (hoch oder quer, Rahmen, Legende, Maßstab, Nordpfeil) und
  DXF-Export der Bildschirmdarstellung; Schriften, Kreise und Farben stehen in der Parameterdatei (GM0, Zeilen 38–55).

Die Regeln und Werte aus dem Original sowie die Beschreibung aller neuen Verfahren stehen in
[app/MECHANICS.md](app/MECHANICS.md).

## Bauen

```
cd app
cargo build --release
cargo test --release
```

Die Programmdatei liegt danach in `app/target/release/dgm-nt.exe`. Die Release-Datei auf GitHub ist unsigniert;
Windows-Systeme mit Smart App Control oder SmartScreen können sie blockieren oder warnen.

## Hinweis

Auf dem Branch `master` dieses Repositorys liegt ein älteres C#/WPF-Projekt (Tag `v0.01`). Dieser Branch (`rust`)
enthält die Rust-Neuentwicklung.
