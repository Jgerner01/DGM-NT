# DGM-NT – Digitales Geländemodell Neuer Generation

Eine moderne Windows-Desktop-Anwendung zur Erstellung, Visualisierung und Analyse digitaler Geländemodelle.

## Überblick

DGM-NT ist eine Neuimplementierung des klassischen Delphi-DGM-Programms (von Fritz Wimbauer, 1996) in modernem C# / WPF mit OpenGL-basierter 3D-Visualisierung.

## ✅ Features (v0.2.0 – April 2026)

### 3D-Visualisierung
- **OpenTK 3D-View** mit OpenGL 3.3
- Interaktive Kamerasteuerung (Rotation, Zoom)
- Farbcodierte Geländedarstellung nach Neigung
- Koordinatenachsen als Orientierung
- Echtzeit-Overlay mit Projektinfo und Statistiken

### Datenmodelle
- `Punkt` – Geländepunkt mit Koordinaten (X, Y, H), Code und Ableitungen
- `Dreieck` – TIN-Dreieck mit Normalvektoren, Neigung und Flächeninhalt
- `Bruchkante` – Geländekanten (hart/weich) mit Punktfolgen
- `Hoehenlinie` – Höhenlinien mit Zählkurven, geschlossen/offen
- `Profil` – Quer-, Längs- und Schnittprofile mit Stationierung
- `VolumenErgebnis` – Berechnungsergebnisse mit Einschnitt/Auftrag
- `DgmProjekt` – Projektverwaltung mit Layern, Farben und Einstellungen

### TIN-Vermaschung
- **Delaunay-Triangulation** (inkrementeller Algorithmus)
- Automatische Normalvektor- und Neigungsberechnung
- Bruchkantenerkennung nach Neigungsunterschieden (>15°)

### Höhenlinien
- Automatische Schnittpunktberechnung pro Dreieck
- Verbindung zusammenhängender Segmente
- Erkennung geschlossener Höhenlinien
- Zählkurven (jede 5. Äquidistanz)
- Konfigurierbare Äquidistanz

### Profilansichten
- **Querprofile** entlang einer beliebigen Linie
- **Längsprofile** entlang einer Trasse
- **Schnittprofile** mit frei definierbaren Punkten
- Baryzentrische TIN-Interpolation
- Automatische Stationierung und Steigungsberechnung

### Volumenberechnung
- **DGM gegen Ebene** (Bezugshöhe)
- **DGM gegen DGM** (zwei Geländemodelle)
- **Baugruben-Volumen** mit polygonalem Umring
- Auflockerungsfaktor und Materialdichte
- Einschnitt, Aufschüttung, Differenz

### Daten-Import/Export
- **CSV-Import** (Format: `PNr;X;Y;H;Code`)
- **CSV-Export** von Punktedaten
- **DXF-Export** (AutoCAD R12-kompatibel)
  - Punkte als Kreise + Text
  - Dreiecke als 3DFACE
  - Bruchkanten als LINE
  - Höhenlinien als POLYLINE
- **JSON-Projekt** (komplettes Speichern/Laden)
  - Alle Daten (Punkte, Dreiecke, BK, HL)
  - Projekt-Metadaten

### Benutzeroberfläche
- **Menüleiste** (6 Menüs, nach Delphi-Vorbild)
  - Datei, Bearbeiten, Verarbeiten, Ansicht, Ausgabe, Werkzeuge, Hilfe
- **Toolbar** mit 24+ Funktionsschaltflächen
  - Projekt: Laden, Speichern
  - Daten: Punkte einlesen
  - Verarbeitung: Vermaschen, Bruchkanten, Höhenlinien, HL-Plan
  - Ausgabe: Höhen, Profile, Volumen, Gitter
  - Ansicht: Zoom +, Zoom -, Original, Ausschnitt
  - Anzeige: Punkte, Dreiecke, Bruchk., Höhenl. (Toggle)
- **Statusbar** mit Koordinaten, Zoom, Punktestand
- **Seitenpanel** mit Projektliste und Info

## 🚧 Geplante Features
- Punktedialog (Eingabe, Bearbeitung, löschen)
- Gittermodell erzeugen und visualisieren
- CAD-Höhenlinienplan mit Beschriftung
- Druckausgabe / WMF-Export
- Punkte-fangen (Snap-Funktion)
- Ausschnitt per Maus wählen
- Undo/Redo
- Mehrere Projekte gleichzeitig

## Technologie-Stack

| Komponente | Technologie |
|------------|-------------|
| Framework  | .NET 8.0 WPF (Windows) |
| 3D-Engine  | OpenTK 4.8 (OpenGL 3.3) |
| MVVM       | CommunityToolkit.Mvvm |
| JSON       | Newtonsoft.Json |
| CSV        | CsvHelper |

## Projektstruktur

```
DGM-NT/
├── Models/                    # Datenmodelle
│   ├── Punkt.cs               # Geländepunkt
│   ├── Dreieck.cs             # TIN-Dreieck
│   ├── Bruchkante.cs          # Bruchkante
│   ├── Hoehenlinie.cs         # Höhenlinie
│   ├── Profil.cs              # Profile (Quer/Längs/Schnitt)
│   ├── VolumenErgebnis.cs     # Volumen-Ergebnis
│   └── DgmProjekt.cs          # Projekt (Layer, Farben)
├── Views/                     # UI-Komponenten
│   └── MainWindow.xaml        # Hauptfenster
├── Services/                  # Geschäftslogik
│   ├── TinService.cs          # TIN-Vermaschung (Delaunay)
│   ├── HoehenlinienService.cs # Höhenlinien
│   ├── ProfilService.cs       # Profilberechnung
│   ├── VolumenService.cs      # Volumenberechnung
│   ├── ProjectService.cs      # JSON Speichern/Laden
│   └── DxfExportService.cs    # DXF-Export
├── Resources/                 # Ressourcen
│   └── Styles.xaml            # Styles & Farben
├── App.xaml / App.xaml.cs     # Einstiegspunkt
├── DGM-NT.csproj              # Projektdatei
├── DGM-NT.sln                 # Solution
└── README.md                  # Diese Datei
```

## Installation & Start

### Voraussetzungen
- .NET 8.0 SDK oder höher
- Windows 10/11
- OpenGL 3.3-kompatible Grafikkarte

### Bauen
```bash
cd DGM-NT
dotnet restore
dotnet build
dotnet run
```

## Bedienung

### 1. Punkte importieren
1. Toolbar → **"Punkte"** klicken
2. CSV-Datei auswählen (Format: `PNr;X;Y;H;Code`)
3. Punkte erscheinen als rote Punkte im 3D-View

### 2. TIN erstellen
1. Toolbar → **"Vermaschen"** klicken
2. Dreiecksnetz wird berechnet und als Drahtgitter angezeigt

### 3. Höhenlinien erzeugen
1. Toolbar → **"Höhenlinien"** klicken
2. Höhenlinien werden basierend auf der Äquidistanz erzeugt

### 4. Bruchkanten erkennen
1. Toolbar → **"Bruchkanten"** klicken
2. Kanten mit Neigungsunterschied >15° werden markiert

### 5. Volumen berechnen
1. Menü → **Ausgabe → Volumenberechnung**
2. Bezugshöhe eingeben
3. Ergebnis: Einschnitt, Auftrag, Differenz

### 6. DXF-Export
1. Menü → **Ausgabe → CAD-Export (DXF)**
2. Datei speichern
3. In AutoCAD, QGIS oder anderem CAD-Programm öffnen

### 7. Projekt speichern/laden
- **Speichern:** Menü → Datei → Projekt speichern (oder Toolbar)
- **Laden:** Menü → Datei → Projekt laden

## Datenformate

### CSV-Import/Export
```csv
PNr;X;Y;H;Code
1;500000.123;6000000.456;123.456;
2;500010.456;6000005.789;124.567;
3;500020.789;6000010.012;125.678;
```

### JSON-Projekt
```json
{
  "Version": "1.0",
  "ErstelltAm": "2026-04-10T...",
  "Projekt": {
    "Name": "Mein Projekt",
    "Ort": "München",
    "Bearbeiter": "Max Mustermann",
    "HoeheAequidistanz": 1.0
  },
  "Punkte": [...],
  "Dreiecke": [...],
  "Bruchkanten": [...],
  "Hoehenlinien": [...]
}
```

### DXF-Export
- **Layer:** Punkte, Punktnr, Dreiecke, Bruchkanten_hart, Bruchkanten_weich, Hoehenlinien, Zaehllinien, Hl-Schrift
- **Entities:** CIRCLE, TEXT, 3DFACE, LINE, POLYLINE
- **Format:** AutoCAD R12 (AC1009)

## Vergleich zum Delphi-Original

| Feature | Delphi (1996) | DGM-NT (2026) |
|---------|---------------|---------------|
| UI | OWL/Windows 3.1 | WPF/Windows 10+ |
| 3D | 2D Canvas | OpenGL 3.3 |
| TIN | Max 4000P/8000DR | Unlimited (RAM) |
| Speichern | Binär (.GM1-.GM8) | JSON |
| Export | Caddy, DXF | DXF (R12) |
| Profile | Ja | Ja |
| Volumen | Ja | Ja |

## Lizenz

Basierend auf dem Delphi-DGM-Programm von Fritz Wimbauer (1996).

## Version

**v0.2.0** – April 2026 – Alle Kernfunktionen implementiert
