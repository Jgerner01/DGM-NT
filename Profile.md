# Bemaßte Profile – Analyse und Vorschlag

Stand: 17.11.2026 · Status: **Konzept, noch kein Code**

## 0. Entscheidungen (Stand 17.11.2026)

| Thema | Festlegung |
|---|---|
| Layout | orientiert sich an der Anleitung des LBB Rheinland-Pfalz (siehe 3.1) |
| Anfangsstation | frei wählbar, Standard 0 |
| Querprofile | aus einer Achse im festen Abstand **und** einzeln |
| Beliebiges Querprofil | beliebige Schnittlinie im Gelände, die wie ein Querprofil dargestellt wird (siehe 4.9) |
| Blattformate | A4, A3, dazu Rolle: A3-Breite mit frei wählbarer Länge **bis 4000 mm** |
| DXF | **überhöht**, wie der Plot |
| Maßstab | wird **automatisch** vorgeschlagen (siehe 4.8), jederzeit überschreibbar |
| Schriftgrößen (Papier) | normale Schrift **5 mm**, Zeichnungsbeschriftung **3,5 mm**, Maßzahlen **2,5 mm** (Höhe der Großbuchstaben) |
| Schriftart | Normschrift (DIN EN ISO 3098), sonst Festbreitenschrift (Courier) (siehe 4.4a) |

## 1. Ziel

Aus dem Geländemodell sollen **bemaßte Profile** entstehen, die man als Vektor-PDF und als DXF ausgibt:

- **Längsprofil** entlang einer Achse (Polygonzug),
- **Querprofile** senkrecht zu einer Achse in festem Stationsabstand oder einzeln,
- mit Höhenraster, Überhöhung, Beschriftungsband (Höhe, Station, Abstand, Neigung) und Maßangaben.

## 2. Was es schon gibt

| Baustein | Fundstelle | Zustand |
|---|---|---|
| Werkzeug „Profil“ (Achspunkte klicken, rechte Maustaste = rechnen) | `Tool::Profile`, [app.rs](app/src/app.rs) | vorhanden |
| Profilberechnung: Punkt alle `step` m plus alle Achsknicke | `analysis::profile`, [analysis.rs:117](app/src/analysis.rs#L117) | tastet nur ab; die Dreieckskanten des Netzes werden nicht geschnitten |
| Stationsformat `km+mmm.mm` | `analysis::format_station` | vorhanden |
| Bildschirmdarstellung mit Überhöhung 1–20, Höhen- und Stationsraster | `profile_plot`, [app.rs:4078](app/src/app.rs#L4078) | nur Bildschirm, ohne Bemaßung, nicht exportierbar |
| Export nur als LNG-Textliste | Fenster „Längsprofil“ | vorhanden |
| Gemeinsame Zeichnungsbeschreibung für PDF und DXF (`Item::Line/Text/Circle/Mesh`, `Legend`) | [scene.rs](app/src/scene.rs) | direkt wiederverwendbar |
| PDF-Ausgabe: A4-Blatt, Rand, Legendenstreifen, Maßstab 1:N, Maßstabsbalken | `Sheet`, `render_pdf`, `draw_strip`, [export.rs](app/src/export.rs) | wiederverwendbar |
| DXF-Ausgabe mit Ebenen und Farben aus den Parametern | `to_dxf` | wiederverwendbar |
| Text mit Halo, beliebiger Drehung und Ankerpunkt | `Item::Text` | vorhanden |
| Wasserflächen mit Ausgleichungsebene | [water.rs](app/src/water.rs) | für Wasserspiegel im Profil nutzbar |
| Zweite Oberfläche (Modellvergleich A/B) | [compare.rs](app/src/compare.rs) | für ein zweites Geländeband nutzbar |
| Projektdatei `.dgmnt.json` mit `#[serde(default)]` und Undo über `checkpoint()` | `ProjectState`, [model.rs](app/src/model.rs) | Profile lassen sich dort ablegen |

**Lücken:**

1. Das Profil ist flüchtig: Es wird nicht im Projekt gespeichert und nicht in der Lageplan-Ansicht gezeigt.
2. Es gibt keine Querprofile.
3. Das Profil wird abgetastet. Dreiecksgrenzen, Bruchkanten und Spitzen fallen zwischen die Stützstellen.
4. Es gibt keine Bemaßung, keinen Rahmen, kein Beschriftungsband und keinen Plot-Maßstab.
5. Es gibt weder PDF- noch DXF-Ausgabe der Profile.

## 3. Beispiele aus der Praxis (Recherche)

Die Recherche ging über Herstellerhilfen und Behördenanleitungen. Die RAS-Verm selbst habe ich nicht im Volltext gelesen. Konkrete Zeichenvorschriften müssen vor der Umsetzung noch geprüft werden.

- **LBB Rheinland-Pfalz** (Leitfaden mit Civil 3D, siehe 3.1).
- **Regelwerk.** Die RAS-Verm (FGSV) hat laut Sächsischem Katalog im Anhang 3 eine Zeichenvorschrift für CAD. Das Land Sachsen-Anhalt veröffentlicht dazu Planmuster ([Katalog Grundpläne Sachsen](https://www.lasuv.sachsen.de/download/2023-12-18_Katalog_Grundplaene.pdf), [LSBB Sachsen-Anhalt](https://lsbb.sachsen-anhalt.de/fileadmin/Bibliothek/Politik_und_Verwaltung/Landesbetriebe/LSBB/Service/Vermessung/Hauptdokument_35.pdf)).
- **Überhöhung.** Sie ist das Verhältnis von Längenmaßstab zu Höhenmaßstab, 1 heißt keine Überhöhung ([Novapoint](https://novapointhelp.trimble.com/Novapoint-CAD-Tools/Terrain-Information/Draw-Cross-Section-from-Quadri), [Carlson](https://files.carlsonsw.com/mirror/manuals/Carlson_2022/source/Site_Road_Design/Sections/Draw_Section_File/Draw_Section_File.htm)).
- **Querprofile aus dem Geländemodell.** Novapoint zeichnet sie in festem Intervall und interpoliert die Höhen im Modell. Carlson bietet getrennte Maßstäbe, Gitter oben und unten und die Höhe bei Abstand 0 ([Novapoint](https://novapointhelp.trimble.com/Road/Road-CAD-tools/Draw/Draw-Cross-Sections/Draw-Terrain-Cross-Sections)).
- **Beschriftung und Tabellen.** Civil 3D setzt Stations-Höhen-Beschriftungen und -Tabellen über Beschriftungswerkzeuge ([Autodesk](https://help.autodesk.com/cloudhelp/2020/ENU/Civil3D-ProjExplorer/files/GUID-BE29F254-BF3F-4D5A-8B1E-20669CC40B74.htm)). Civilgeo kombiniert eine Wertetabelle (Station/Höhe) mit einem Anzeiger beim Überfahren ([Civilgeo](https://knowledge.civilgeo.com/elevation-profile-command)).
- **Deutsche CAD-Pakete.** dataflor legt den Geländeschnitt aus einem Dreiecksnetz an, Größe und Farbe der Maßtexte sind einstellbar ([dataflor](https://en.wiki.dataflor.de/doku.php/de/produkte/cad_2017/gelaendeschnitt?s[]=layout)). Pythagoras bietet ein „Längsprofil“ im Geländemodell ([Pythagoras](https://pythagorasbv.freshdesk.com/en/support/solutions/articles/77000574296-how-to-create-a-terrain-profile)).
- **Höhenbezug.** Die DB verlangt im Regelfall den Anschluss an das staatliche Höhennetz, bei Glasfaserprojekten gilt DHHN2016 ([RI Vermessung](https://www.bbsoft.de/pdf/BBSoftInfo_ohnePreis.pdf)). Im Profil sollte das Höhensystem angegeben werden können.

### 3.1 Was die LBB-Anleitung festlegt

Ich habe den Text der Anleitung (Version 4.0, Stand 18.10.2018, 26 Seiten) ausgelesen. Die Bilder und Dialoge darin konnte ich nicht auswerten. Zahlen zu Schriftgrößen, Linienstärken oder Blattmaßen stehen im Text nicht. Das Dokument beschreibt das Erstellen mit Civil 3D und der Vorlage „_AutoCAD Civil 3D 2012 Deutschland_LBB_V_4-0.dwt“. Daraus übernehme ich:

- **Zwei Arten von Längsschnitt:**
  - **Einfacher Längsschnitt:** Bänder Geländehöhe und Stationierung im Nebenintervall **5 m**, einstellbar (Beispiel 10 m).
  - **Genauer Höhenplan:** Stationsbeschriftung mit Höhe alle **20 m** (Grundeinstellung), Intervall einstellbar (Beispiel 100 m).
- **Geländeknickpunkte:** Es gilt eine **Bereinigung** von 5 (Standard). Dann werden nicht alle Knickpunkte beschriftet. Mit 0 werden **alle** beschriftet. Das entspricht meiner Textausdünnung in 4.4.
- **Führungslinien** (Lote) oben im Längsschnitt: Intervall der Führungslinien und der Bänder müssen **immer denselben Wert** haben, sonst ist die Darstellung nicht synchron. Bei mir gibt es deshalb nur **ein** Intervall für beide.
- **Bandsätze** mit 3 oder 4 Bändern, je nach Stil. Bei Gradientenplanung kommen Planungshöhe und die **Abstände zwischen Querprofilen** hinzu. Die Beschriftung kann „versetzt“ stehen (Text wechselt zwischen zwei Zeilen, damit er nicht überlappt). Punktsymbole stehen ausgerichtet an der Station.
- **Querprofilpläne** zeigen „Planung und Bestand“ (zwei Gelände/Oberflächen) mit Datenbändern. Mehrere Pläne werden in einem Arbeitsgang erzeugt.
- **Schrift:** Die Stile nennen Arial. Zweisprachige Beschriftung (Deutsch/Englisch) gibt es dort zusätzlich. Das übernehme ich **nicht**.

**Gemeinsamer Aufbau** (der „klassische Planaufbau“ der Beispiele):

```
 Höhe ü. NHN
  ┌────────────────────────────────────────────────┐
  │          ╱╲___            Geländelinie          │   Raster, Höhenskala links
  │    ____╱      ╲___╱╲                           │   Wasserspiegel als Linie
  │___╱                  ╲___                       │
  ├────────────────────────────────────────────────┤  ← Bezugshorizont (z. B. 412,00 m)
  │ Gelände-Höhe  │ 415,23 │ 415,87 │ 416,10 │ … │   Beschriftungsband
  │ Abstand       │   20,00 │  20,00 │  20,00 │ … │
  │ Neigung       │   +3,2% │  +1,1% │  -0,6% │ … │
  │ Station       │ 0+000,00│ 0+020,00│ 0+040,00│… │
  └────────────────────────────────────────────────┘
```

## 4. Vorschlag zur Umsetzung

### 4.1 Kerngedanke: Profil als `Scene`, Überhöhung im Maßstab

Die vorhandene Zeichnungsbeschreibung kennt Weltkoordinaten in Metern, dazu Größen in Millimetern auf dem Papier, und `render_pdf` rechnet mit dem Maßstab 1:N um. Das passt für Profile direkt, wenn man das Profil in einer **Profilebene** zeichnet:

- x = Station (m)
- y = (Höhe − Bezugshöhe) × Überhöhung (m)

Beim Druckmaßstab 1:N erscheinen die Längen dann im Maßstab 1:N, die Höhen im Maßstab 1:(N/Überhöhung). Beispiel 1:500 mit Überhöhung 10 ergibt 1:500 und 1:50. Es gibt nur **einen** Maßstab im Dialog und einen Überhöhungsfaktor.

So bleiben PDF-Ausgabe, DXF-Ausgabe, Legendenstreifen, Maßstabsbalken und Parameterfarben unverändert nutzbar. Es braucht nur eine neue Funktion, die die Profil-`Scene` baut (`profile_scene`, in einer neuen Datei `profile.rs`).

Zu beachten: Das Beschriftungsband liegt unter der Zeichnung. Seine Zeilenhöhe wird in Millimetern angegeben und intern mit N/1000 in Meter umgerechnet, damit sie im Plot gleich bleibt.

### 4.2 Datenmodell

```rust
// profile.rs (neu)
/// Wie das Profil gezeichnet wird (unabhängig davon, wie die Schnittlinie entstand)
pub enum ProfileView { Laengs, Quer }

/// Woher die Schnittlinie kommt
pub enum ProfileSource {
    Axis,                              // Längsprofil entlang einer Achse
    CrossSections { step: f64, half_width: f64 }, // Querprofile aus einer Achse
    Single { zero: ZeroPoint },        // einzelner, beliebiger Schnitt (4.9)
}

pub struct ProfileDef {
    pub name: String,
    pub view: ProfileView,
    pub source: ProfileSource,
    pub line: Vec<[f64; 2]>,          // Schnittlinie bzw. Achse in Weltkoordinaten
    pub start_station: f64,           // Anfangsstation (Standard 0)
}

pub struct ProfileStyle {              // in Params (serde default)
    pub scale_n: Option<f64>,         // Maßstab 1:N; None = automatisch (4.8)
    pub exaggeration: Option<f64>,    // Überhöhung; None = automatisch
    pub label_step: f64,              // Intervall für Bänder UND Führungslinien (m)
    pub kink_clean: f64,              // „Bereinigung“ der Knickpunktbeschriftung (m), 0 = alle
    pub datum: Option<f64>,           // Bezugshöhe; None = automatisch
    pub bands: ProfileBands,          // Höhe / Abstand / Neigung / Station / Punktnummer
    pub sheet: ProfileSheet,          // A4, A3 oder Rolle (4.5)
    pub font: ProfileFont,            // Normschrift oder Courier (4.4a)
    pub colors: [Rgb; …],             // Gelände, Raster, Text, Wasser, Maßkette
}
```

- Ablage: `ProjectState.profiles: Vec<ProfileDef>` mit `#[serde(default)]`. Alte Projektdateien bleiben lesbar.
- Das berechnete Profil wird nicht gespeichert, sondern bei Bedarf neu aus Achse und Modell berechnet.

### 4.3 Berechnung (`profile.rs`)

1. **Exakter Schnitt mit dem Dreiecksnetz.** Statt abzutasten, wird die Achse mit den Dreieckskanten geschnitten. Jeder Schnitt ist eine Stützstelle des Profils, Bruchkanten werden dadurch genau getroffen. Zusätzlich gibt es Stützstellen an den Achsknicken und an den Beschriftungsstationen.
2. **Lücken.** Wo die Achse außerhalb des Modells oder in einem Loch liegt (`height` = `None`), wird das Profil unterbrochen, nicht verbunden.
3. **Querprofile.** Senkrecht zur Achsrichtung an jeder Station, links/rechts bis `half_width`. An Knickpunkten der Achse wird die Winkelhalbierende verwendet.
4. **Kennwerte** je Stützstelle: Station, Höhe, Teilabstand, Neigung (%). Zusätzlich Hoch-/Tiefpunkte.
5. **Wasserspiegel.** Schneidet die Achse eine Wasserfläche, wird der Schnitt der Ausgleichungsebene mit dem Profil als Linie hinzugefügt.
6. **Zweites Gelände.** Optional wird die Oberfläche B des Modellvergleichs als zweite Linie gezeichnet (z. B. Bestand und Planung).

### 4.4 Darstellung und Bemaßung

Alle Elemente entstehen als `Item`:

- **Rahmen und Raster:** Höhenraster mit „schönen“ Schritten (wie `nice_step`), Stationsraster, Höhenskala links mit Höhenzahlen.
- **Gelände:** `Item::Line` in der Geländefarbe, bei Wunsch mit Füllung bis zur Bezugslinie (`Item::Mesh`).
- **Bezugshorizont:** Linie mit Beschriftung „Bezugshöhe 412,00 m ü. NHN“.
- **Beschriftungsband:** Zeilen Höhe, Abstand, Neigung, Station, optional Punktnummer/Code. Die Zeilenkopfzellen stehen links. Bei Querprofilen entfällt die Zeile Station und es wird der Abstand von der Achse (links/rechts) geschrieben.
- **Maßangaben im Profil:**
  - Höhenkoten an Knick-, Hoch- und Tiefpunkten, mit Lotlinie bis ins Band,
  - Maßkette mit Teilabständen unter dem Profil,
  - Gefällepfeile mit Prozentangabe an längeren Abschnitten,
  - Höhenunterschied zwischen zwei gewählten Stationen (Maßlinie mit Pfeilen).
- **Überschneidungen.** Texte, die sich überdecken würden, werden ausgedünnt (Priorität: Extrempunkte vor Intervallpunkten). Die vorhandenen Halo-Texte bleiben lesbar.
- **Legendenstreifen:** Maßstab (Länge/Höhe), Überhöhung, Bezugshöhe, Profilname, Modellname, Datum, Anzahl der Profile. Das Nordpfeil-Symbol von `draw_strip` muss dafür **optional** werden, weil es im Profil nichts bedeutet. Das ist die einzige Änderung am bestehenden Ausgabecode.

### 4.4a Schriftgrößen und Schriftart

**Größen** (Höhe der Großbuchstaben auf dem Papier, unabhängig vom Maßstab; im DXF werden sie mit N/1000 in Meter umgerechnet, das macht `to_dxf` schon):

| Verwendung | Größe |
|---|---|
| normale Schrift: Profilname, Überschriften, Texte im Legendenstreifen | **5,0 mm** |
| Zeichnungsbeschriftung: Zeilenköpfe des Bandes, Achsenbeschriftung, Bezugshöhe, Stations-Beschriftung | **3,5 mm** |
| Maßzahlen: Höhen, Abstände, Neigungen, Koten, Maßketten | **2,5 mm** |

Das sind genau die Größenreihen der Normschrift (2,5 – 3,5 – 5 – 7 – 10 mm). Bei der Prüfung auf Textüberschneidung wird mit diesen Größen gerechnet.

**Schriftart, drei Stufen:**

1. **DXF:** Textstil mit der CAD-Normschrift (`ISOCPEUR` bzw. `isocp.shx`, DIN EN ISO 3098). Ist sie am Zielrechner nicht vorhanden, ersetzt das CAD-Programm sie selbst. Dazu schreibt `to_dxf` eine STYLE-Tabelle. Das ist heute nicht der Fall.
2. **PDF, sofort verfügbar:** **Courier** als Festbreitenschrift. Sie ist in jedem PDF-Betrachter enthalten und muss nicht eingebettet werden. Der PDF-Schreiber kennt bisher nur Helvetica. Die Zeichenbreite von Courier ist fest (0,6 × Schriftgröße), die Textbreitenberechnung wird dadurch einfacher.
3. **PDF, später:** Normschrift als eingebettete TrueType-Datei, wenn eine frei nutzbare Schrift gefunden ist (z. B. osifont, Lizenz noch zu prüfen). Dafür muss der PDF-Schreiber Schriften einbetten können. Das ist ein eigener, größerer Schritt (siehe Schritt 8).

Die Schriftart wird in `ProfileStyle.font` gewählt: Normschrift (wo möglich) oder Courier.

### 4.5 Ausgabe

- **Blattformate:** A4 (hoch/quer), A3 (quer), **Rolle**: A3-Breite 297 mm, Länge frei von 420 bis **4000 mm**. Die Länge wird automatisch nach dem Plan gewählt (aufgerundet auf volle 10 mm) und ist änderbar. Der Streifen mit Legende und Maßstab steht bei der Rolle am rechten Ende. `Sheet` ist bisher fest auf A4 und braucht dafür Breite und Höhe als Parameter.
- **PDF:** über `render_pdf`. Ist das Profil auch bei kleinstem zulässigem Maßstab länger als 4000 mm, wird es auf mehrere Blätter umbrochen, mit Überlappung und Stationsangabe am Rand (zweiter Schritt).
- **Querprofile:** mehrere Profile je Blatt, Raster aus Zeilen und Spalten, jedes mit Stationsüberschrift. Alle Querprofile eines Blattes haben denselben Maßstab, damit sie sich vergleichen lassen.
- **DXF:** über `to_dxf`, **überhöht** wie der Plot. Ergebnis ist ein maßstäblicher Plan in Metern. Die Höhenzahlen im Band und an den Koten zeigen immer die **wahren** Höhen, nicht die überhöhten.
- **Liste:** die vorhandene LNG-Datei bleibt, dazu eine CSV mit Station/Rechtswert/Hochwert/Höhe/Neigung.

### 4.6 Oberfläche

- Neuer Eintrag im Menü (Vorschlag **Auswerten → Profile…**), zusätzlich das vorhandene Werkzeug „Profil“ für das Klicken der Achse.
- **Fenster „Profile“** mit editierbarer Tabelle (wie bei den Wasserflächen): Name, Art, Anfangsstation, Querprofilabstand, Breite. Knöpfe: Neu (Achse klicken), Löschen, Achse auf Linie/Bruchkante übernehmen.
- **Vorschau** im Fenster: dieselbe `Scene` wird mit dem Painter gezeichnet. Dann ist die Vorschau deckungsgleich mit PDF und DXF, und `profile_plot` kann entfallen.
- Einstellungen in **Parameter → Profile:** Überhöhung, Beschriftungsintervall, Bezugshöhe, Bänder, Farben und Ebenen.
- Achsen erscheinen im Lageplan als Linie mit Stationsmarken und Name. Optional kommen sie in die Druckausgabe.

### 4.7 Ebenen und Farben

`Params.layers` hat 13 Ebenen aus der Parameterdatei. Vorschlag: neue Ebenen (Profil-Gelände, -Raster, -Text, -Wasser, -Bemaßung) anhängen und mit `#[serde(default)]` füllen. Die ersten 13 und damit die Kompatibilität bleiben unverändert.

### 4.8 Automatischer Maßstab und Überhöhung

Beim Anlegen oder Ändern eines Profils schlägt das Programm Maßstab, Überhöhung und Blatt vor. Alles bleibt von Hand überschreibbar („auto“ wieder wählbar).

**Eingaben:** Profillänge L (m), Höhenspanne ΔH (m, vom niedrigsten bis höchsten Punkt), Art (Längs/Quer), gewünschtes Blatt oder „auto“.

**Maßstabsreihe:** 1:50, 1:100, 1:200, 1:250, 1:500, 1:1000, 1:2000, 1:2500, 1:5000, 1:10000.

**Ablauf:**

1. **Längenmaßstab N:** der größte Maßstab, bei dem die Zeichnung auf das kleinste passende Blatt passt, in der Reihenfolge A4 quer, A3 quer, Rolle (bis 4000 mm). Plotlänge = L / N × 1000 mm + Rand links (Zeilenköpfe des Bandes, etwa 35 mm) + Rand rechts + Legendenstreifen.
2. **Überhöhung ü:** aus {1, 2, 5, 10, 20, 25, 50}. Gewählt wird die größte, bei der Profilhöhe + Band noch in die Blatthöhe passt. Profilhöhe = ΔH × ü / N × 1000 mm. Für Längsprofile liegt die Obergrenze bei 25, damit das Gelände nicht verzerrt wirkt. Querprofile bekommen bevorzugt **ü = 1**, nur wenn sie sonst zu flach würden, mehr.
3. **Beschriftungsintervall** (Bänder und Führungslinien gemeinsam): der kleinste Wert aus {5, 10, 20, 25, 50, 100, 200, 500} m, bei dem zwischen zwei Beschriftungen mindestens die Textbreite der Maßzahlen (2,5 mm Schrift) plus 2 mm Abstand liegt. Als Vorgabe wird 20 m angenommen, wenn es passt (wie im LBB-Leitfaden).
4. **Höhenraster:** schöner Schritt aus {0,1; 0,2; 0,5; 1; 2; 5; 10} m, so dass die Rasterlinien mindestens 8 mm auseinander liegen.
5. **Bezugshöhe:** der niedrigste Geländepunkt abzüglich ΔH/10, abgerundet auf ein Vielfaches des Höhenrasters.

Die Wahl wird in einem Satz in der Legende ausgewiesen, zum Beispiel „Maßstab 1:500 / 1:50, Überhöhung 10fach“. Es gibt eine Prüfung der Plausibilität: Passt auch das kleinste Blatt der Reihe nicht, erscheint eine Meldung mit der nächstmöglichen Lösung (Mehrblatt).

### 4.9 Beliebiges Querprofil

Jede Schnittlinie lässt sich als **Querprofil** darstellen, auch wenn sie nicht senkrecht zu einer Achse liegt. Der Unterschied zum Längsprofil liegt nur in der Darstellung:

- **Nullpunkt:** Bei `ProfileSource::Single` wird ein Nullpunkt gewählt (Anfang, Mitte oder ein angeklickter Punkt auf der Linie). Von dort aus zählt der Abstand **links negativ** und **rechts positiv**, in Blickrichtung der Linie vom Anfang zum Ende.
- **Beschriftung:** statt der Station steht der **Abstand vom Nullpunkt** (z. B. „-12,50“ / „+8,00“). Dazu die Überschrift mit Name und Station des Nullpunkts, zum Beispiel „Querprofil 0+120,00“. Bei Querprofilen aus einer Achse ist die Station die der Achse. Bei einem einzelnen Schnitt kann sie **frei vergeben** werden (Standard: Name oder Zähler).
- **Maßstab:** automatisch wie in 4.8, vorzugsweise ohne Überhöhung. Alle Querprofile einer Reihe laufen gemeinsam auf einem Blatt.
- **Bedienung:** Werkzeug „Profil“ bekommt zwei Betriebsarten, **Längsprofil** und **Querprofil**. Im Querprofilmodus klickt man zwei Punkte (oder einen Linienzug) und wählt den Nullpunkt. Zusätzlich kann man **eine Achse anklicken** und im Dialog Abstand und halbe Breite eingeben, dann entsteht die ganze Reihe.
- **Wiederverwendung:** Rechnung, Raster, Band, Bemaßung und Ausgabe sind dieselben wie beim Längsprofil. Es ändern sich nur die Zeilenbeschriftung und die Vorzeichen der Abstände.

## 5. Umsetzungsschritte

| Schritt | Inhalt | Prüfung |
|---|---|---|
| 1 | `profile.rs`: exakter Schnitt Linie × Dreiecksnetz, Kennwerte, Anfangsstation | Unit-Tests: Ebene, Bruchkante, Loch, Linie außerhalb |
| 2 | `ProfileDef` im Projekt, Fenster mit Tabelle, Speichern/Laden, Undo | Rundlauftest der Projektdatei |
| 3 | `profile_scene` mit Rahmen, Raster, Gelände, Bezugslinie, Band, Schriftgrößen 5 / 3,5 / 2,5 mm, Courier. Blattformate A4, A3, Rolle bis 4000 mm. PDF und DXF überhöht | PDF per `lopdf` lesbar, DXF-Test, Test der Blattgrößen |
| 4 | Automatischer Maßstab und Überhöhung (4.8) | Tests mit flachem, steilem, sehr langem Profil |
| 5 | Bemaßung: Koten, Maßkette, Gefälle, Textausdünnung („Bereinigung“) | Test auf überlappungsfreie Texte |
| 6 | Querprofile: aus Achse im Abstand, einzeln, beliebiger Schnitt (4.9) | Test mit symmetrischem Gelände, Vorzeichen links/rechts |
| 7 | Wasserspiegel, zweites Gelände („Planung und Bestand“) | Test mit Wasserfläche |
| 8 | Mehrblatt, CSV, DXF-Textstil mit Normschrift, später eingebettete Normschrift im PDF, Menü, Hilfe/README | Sichttest in der Oberfläche |

**Stand 17.11.2026:** Alle Schritte sind umgesetzt (`app/src/profile.rs`, `pdf.rs`): exakter Schnitt mit dem Dreiecksnetz, Profile im Projekt gespeichert, Fenster „Profile“ mit Vorschau, Längs- und Querprofil (auch beliebige Schnittlinie), automatischer Maßstab, Blattformate A4/A3/Rolle bis 4000 mm, überhöhtes DXF (Textstil ISO3098/`isocp.shx`), PDF mit eingebetteter Normschrift (automatisch aus dem Windows-Schriftenordner, z. B. ISOCPEUR; einstellbar in „Parameter → Profile (PDF)“, „-“ = Courier), Schriftgrößen 5 / 3,5 / 2,5 mm, Koten mit Bereinigung (Vorgabe 5 m), Bandzeilen Höhe, Höhe 2, Neigung, Abstand, Kette, Station, Maßkette, Höhenunterschied-Maßlinie, Wasserspiegel, zweites Gelände aus dem Modellvergleich (gestrichelt), CSV, Querprofile im Abstand aus einer Achse mit gemeinsamem Maßstab, mehrere Querprofile je Blatt (A4/A3/Rolle) oder je Profil eine Datei, Umbruch zu langer Längsprofile im PDF auf mehrere Rollenblätter. Hinweise: Der Umbruch gilt nur fürs PDF, das DXF bleibt in einem Stück. Die Rolle ist 297 mm breit angenommen.

Jeder Schritt ist für sich nutzbar. Schritt 1 bis 3 ergeben schon ein druckbares Längsprofil im Rollenformat.

## 6. Risiken und offene Fragen

Geklärt sind Layout (LBB), Anfangsstation, Querprofile, Blattformate, DXF und Schriftgrößen (Abschnitt 0). Offen bleibt nur:

1. **Normschrift im PDF.** Courier ist sofort möglich. Eine eingebettete Normschrift braucht eine Schriftdatei mit passender Lizenz und eine Erweiterung des PDF-Schreibers. Reicht dir Courier im PDF zunächst, und die Normschrift kommt im DXF und später im PDF?
2. **Rollenbreite.** Ich nehme A3-Rolle mit 297 mm Breite an, Länge bis 4000 mm. Meinst du dagegen 420 mm Breite?
3. **Bereinigung der Knickpunkte.** Als Vorgabe 5 m wie im LBB-Leitfaden? Ich lese „5“ dort als Meter. Das steht nur im Text, nicht in einer Einheit.
4. **LBB-Bilder.** Das Aussehen der Bänder (Zeilenhöhe, Reihenfolge) steht nur in den Bildern der Anleitung, die ich nicht auswerten konnte. Hast du ein fertiges Beispielprofil (PDF/DXF) nach LBB-Vorlage? Daran würde ich die Bandzeilen und Maße ausrichten.
5. **Leistung.** Der exakte Schnitt läuft über Dreiecke. Bei Modellen mit vielen 100 000 Dreiecken braucht es ein Raster oder eine Kantensuche entlang der Linie statt einer Prüfung aller Dreiecke.

## 7. Quellen

- [LBB Rheinland-Pfalz: Anleitung Längsschnitte, Höhen- und Querprofilpläne](https://lbb.rlp.de/fileadmin/lbb/4_Auftragnehmer/CAD-Plancode-Bauingieurwesen/AnleitungLaengsschnitteHoehen-undQuerprofilplaene-V_4-0.pdf)
- [Sachsen: Katalog Grundpläne (RAS-Verm Anhang 3)](https://www.lasuv.sachsen.de/download/2023-12-18_Katalog_Grundplaene.pdf)
- [LSBB Sachsen-Anhalt: Planmuster zur RAS-Verm](https://lsbb.sachsen-anhalt.de/fileadmin/Bibliothek/Politik_und_Verwaltung/Landesbetriebe/LSBB/Service/Vermessung/Hauptdokument_35.pdf)
- [Trimble Novapoint: Querprofile aus dem Gelände](https://novapointhelp.trimble.com/Road/Road-CAD-tools/Draw/Draw-Cross-Sections/Draw-Terrain-Cross-Sections)
- [Carlson: Draw Section File](https://files.carlsonsw.com/mirror/manuals/Carlson_2022/source/Site_Road_Design/Sections/Draw_Section_File/Draw_Section_File.htm)
- [Autodesk Civil 3D: Profil- und Schnittansichten](https://help.autodesk.com/cloudhelp/2020/ENU/Civil3D-ProjExplorer/files/GUID-BE29F254-BF3F-4D5A-8B1E-20669CC40B74.htm)
- [Civilgeo: Elevation Profile Command](https://knowledge.civilgeo.com/elevation-profile-command)
- [dataflor: Geländeschnitt](https://en.wiki.dataflor.de/doku.php/de/produkte/cad_2017/gelaendeschnitt?s[]=layout)
- [Pythagoras: Terrain profile](https://pythagorasbv.freshdesk.com/en/support/solutions/articles/77000574296-how-to-create-a-terrain-profile)
- [Kubla Cubed: Cross Section Element](https://www.kublasoftware.com/support/documentation/kubla-cubed/2024/7.4/user-manual/measure-elements/cross-section.html)
