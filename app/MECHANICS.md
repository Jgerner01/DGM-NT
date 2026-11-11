# DGM-NT – Werte und Regeln aus dem Original (dgmx.exe)

Quellen: Ghidra-Dekompilierung von `C:\dgm\dgmx.exe` (Delphi, 32 bit), das Handbuch
`Handbuch DGM.pdf`, die Steuerdatei `dgm.sys` und die Projektdateien in `C:\dgm\daten`.
Werte mit „estimated" wurden im Code gesucht, aber nicht eindeutig gefunden.

## Parameterdatei GM0 (gelesen in 0x51322c)

Zeile für Zeile; Vorgaben, falls die Datei fehlt (aus demselben Code):

| Zeile | Bedeutung | Vorgabe |
|---|---|---|
| 1 | Abstand Höhenlinien (m) | 10.0 |
| 2 | max. Dreieckseite (m) – im Code quadriert verwendet | 100.0 |
| 3 | min. Dreieckswinkel (gon) | 5.0 |
| 4 | Dreiecksunterteilung (0..15, Ganzzahl) | 10 |
| 5 | Skalierung X,Y | 1.0 |
| 6 | Skalierung H | 1.0 |
| 7–10 | Fenster: Rechts-/Hochwert links unten, rechts oben | 0, 0, 10, 10 |
| 11–12 | Höhe unten / oben | 0, 0 |
| 13 | Nachkomma Höhen | 2 |
| 14 | Nachkomma Zähllinien | 0 |
| 15 | Maßstabszahl Plott | 1000 |
| 16, 17 | (Plotwerte, Bedeutung offen) | 2.0, 3.0 |
| 18 | Rand Zeichnung (mm) | 20.0 |
| 19 | Abstand Gitterkreuze (mm) | 100.0 |
| 20 | Ausdünnung (mm) | 0.05 |
| 21 | Mindestabstand Höhenlinienpunkte (mm) | 2.0 |
| 22 | Trennzeichen Punktnummer (Text) | leer |
| 23 | Glättung (ein Zeichen, '0'..'9', 10 = Akima) | '0' |
| 24 | Pfad | – |
| 25–37 | CAD-Ebenen (Layer) 11..23 | 11..23 |

## Weitere Dateien

- **GM1**: `Punktbezeichnung Rechtswert Hochwert Höhe [Code]`, durch Leerzeichen/Tabs getrennt (Handbuch).
- **GM2** Bruchkanten: ein Punktname pro Zeile, jedes Polygon endet mit `#`, weiche mit `##` (Handbuch).
- **GM9** Umring/Inringe (Leseroutine 0x50793c): Punktnamen je Zeile, eine Gruppe endet mit `#`
  (Konstante 0x507e60) oder Dateiende. Erste Gruppe = Umring (max. 8000 Punkte), jede weitere = Inring
  (max. 100 Punkte, höchstens 16). Ein wiederholter Schlusspunkt wird verworfen.
  Laut Anwender: Umring im Uhrzeigersinn, Inring gegen den Uhrzeigersinn – der Neubau ordnet neu
  gezeichnete Ringe nach dieser Drehrichtung zu und schreibt sie so.
- **GMG** Grundriss: wie GM2.
- **PRJ**: Delphi-ShortStrings (Name, Nummer, Beschreibung, Ort, Straße, Bearbeiter, Pfade, Datum).

## dgm.sys (Voreinstellungen)

- identische Punkte: Abstand < **0.05 m** (Zeile 14) → nicht verwendet.
- Abstand zu Bruchkante 0.10 m (Zeile 15).
- Farben (CADdy-Nummern): Punktsymbole 1 rot, Punktnummern 1 rot, Höhe 6 cyan, Umring 5 magenta,
  Höhenlinien 2 grün, Zähllinien 3 gelb, Bruchkanten 11 braun, Gitter 4 blau, Dreiecke 6 cyan.
  Der Neubau nimmt für Höhenlinien Brauntöne (besser lesbar auf der Höhenfarbe).

## Zoom im Original (0x51ef48 „größer")

- Vergrößert nur das Bildschirm-Bitmap: Faktor +1.0 je Klick (Konstante 0x51f134 = 1.0),
  bis der Faktor 2.9 erreicht (Konstante 0x51f128) → höchstens 3-fach, unscharf (Bitmap).
- „Ausschnitt": Fenster neu zeichnen; Cursortasten schieben um die halbe Ausdehnung, Strg+Klick zentriert.

**Neu:** Vektorzeichnung bei jedem Bild; stufenloser Zoom am Mauszeiger (Mausrad, Faktor 1.25 je Raste,
weich animiert), 0.0001 … 20000 Bildpunkte je Meter; Ziehen verschiebt; Doppelklick ×2; Rechteck-Ausschnitt;
Cursortasten und Strg+Klick wie im Original; feste, scharfe Linienbreiten; Punkt- und Textdichte passt sich
dem Zoom an; Übersichtskarte, Maßstabsleiste, Nordpfeil; Höhenfarben mit Schummerung.

## Autospeichern (neu, im Original nicht vorhanden – Handbuch „Ende": Ungespeichertes geht verloren)

- Alle **60 s** (einstellbar 0–3600 s) nach einer Änderung und beim Beenden.
- Datei `<Projekt>.dgmnt.json` im Datenordner, atomar geschrieben, vorheriger Stand als `.bak`.
- Beim Öffnen wird dieser Stand geladen, wenn er neuer als die GM1 ist. Die GM-Dateien werden nur auf
  ausdrücklichen Befehl geschrieben. Rückgängig/Wiederholen bis 40 Schritte.

## Modellfläche (Neubau, nach Vorgabe des Anwenders)

### Was das Original rechnet (gelesen in 0x50b8c8, Aufruf aus 0x4d4358)

- Punkt-Höhe `0x50b8c8(Unterteilung, &Dreieck, &z, x, y)`: Dreieck suchen (0x50b4d4, zuletzt
  gefundenes Dreieck zuerst, sonst alle der Reihe nach, max. 50000), dann
  - Unterteilung = 0: eben, `z = z1 + (z2−z1)·u + (z3−z1)·v` (lokale Dreieckskoordinaten u, v).
  - Unterteilung ≠ 0: Polynom 5. Grades je Dreieck, `z = Σ p[i][j]·u^i·v^j` mit i+j ≤ 5,
    aus Höhe, 1. und 2. Ableitungen (zx, zy, zxx, zxy, zyy) an den drei Ecken; Koeffizienten
    mit den Konstanten 2, 1, 0.5, 20, 8, 12, 3, −30, 14, 30, 16, 6, 2.5, −2, 4, 5 (Gleitkommatabelle
    0x50cba8–0x50cbe4). Das ist die bivariate Quintik-Interpolation nach Akima (1978).
  - Ecken an einer Bruchkante (Kennung je Ecke): die 2. Ableitungen werden 0 gesetzt und die
    1. Ableitungen ersetzt (aus der Neigung des ebenen Dreiecks, abgeleitet, nicht Stelle für Stelle
    geprüft).
  - Die Koeffizienten werden je Dreieck zwischengespeichert (Tabelle hinter 0x53132c).
- Akimas Quintik ist C1-stetig über die Kanten, kann im Inneren aber überschwingen.

### Neubau (Vorgabe des Anwenders, geht vor)

Der Anwender wünscht eine hyperbolische Unterteilung: glatte Kanten, im Inneren kein Übersteuern,
besondere Behandlung von Bruchkanten und Rand. Umsetzung:

- Neigung je Punkt: gewichtete Ausgleichsebene durch die Nachbarn (Gewicht 1/d²).
- An harten Bruchkanten: Neigung je Seite nur aus den Dreiecken derselben Seite.
- Am Rand: Neigung × 0.5; Randkanten: Wölbungsgrenze × 0.5.
- Kante i–j: kubische Hermite-Wölbung gegenüber der Geraden, begrenzt durch
  `bound · tanh(Wölbung / bound)` mit `bound = 0.5·|Δz| + 0.05·Länge·min(|g_i|,|g_j|)`.
- Dreiecksinneres: rationale Mischung der drei Kantenwölbungen, Gewicht `(b_i·b_j)² / Σ` → im Inneren
  nur konvexe Mischung, kein Übersteuern. Harte Bruchkanten bleiben gerade (Knick im Modell).
- Unterteilung 0 = eben (lineare Interpolation), 1..15 = Teilung der Seiten (Handbuch).

## Höhenlinien

- Lineare Interpolation in den Subdreiecken (Handbuch), Verkettung der Teilstücke.
- Ausdünnung: mittlerer Punkt fällt, wenn Dreieckshöhe < Ausdünnung (mm × Maßstab/1000);
  nicht, wenn die Sehne > 10 × Mindestabstand (estimated für „wesentlich überschritten").
- Glättung 0 keine, 1 kubischer Spline, 2..7 Spline mit abnehmender Ausrundung (Spannung (g−1)·0.14),
  8/9 ausgleichender Spline (2 bzw. 4 Eckenschnitte, 9 weiter von den Punkten), 10 Akima (Handbuch).
- Zähllinien: jede **5.** Linie – belegt durch die vom Original geschriebenen Beschriftungsdateien
  (da-dgm1.gms: 372.5, 377.5 … bei 0.5 m; Diplomarbeit.gms: 650/700 bei 10 m; METZKA.gms: 0.1-Schritte bei 0.02 m).
- Beschriftung der Zähllinien (Vorgabe des Anwenders): etwa alle **20 %** der größten Ausdehnung, jeweils an
  die am schwächsten gekrümmte Stelle in der Nähe verschoben (Suche ±40 % des Abstands), Linie unter der
  Zahl unterbrochen, Schrift immer lesbar aufgerichtet.

## Vermaschung

- Delaunay-Dreiecke mit Bruchkanten, Umring und Inringen als Zwangskanten (Original: „kleinste
  Dreieckseitensumme" – anderes Verfahren, ähnliches Ergebnis; estimated).
- Dreiecke mit Seite > max. Dreieckseite fallen weg (außer Zwangskanten); außerhalb Umring / innerhalb
  Inring fallen weg; schmale Randdreiecke mit Winkel < min. Dreieckswinkel (gon) werden wiederholt entfernt.

## Volumen

Prismenmethode gegen eine waagrechte Bezugshöhe (Handbuch), getrennt nach über / unter der Bezugshöhe,
mit Schnitt der Dreiecke an der Bezugsebene; Dreiecke werden dafür wie die Höhenlinien unterteilt (max. 4).

## Analyse – Modellvergleich (neu, nicht im Original; Menü „Analyse“, Modul `compare.rs`)

Vorgabe des Anwenders: zweites Geländemodell einlesen, Überschneidung in x/y prüfen, gemeinsamer Umring,
Höhen jeweils aus dem eigenen Modell interpoliert, Schnittkante, Volumen aus Dreiecksprismen, Auflockerung,
Höhenunterschied als Farbschema und Isolinienbild.

- **Einlesen:** GM1 des zweiten Modells (GM0/GM2/GM9 daneben werden mitgelesen; fehlt die GM0, gelten die
  Parameter des aktuellen Modells). Das zweite Modell wird mit den üblichen Regeln vermascht.
- **Rollen:** *Bestand* = aktuelles Modell, *Neu* = zweites Modell (nur Rollen; „tauschen“ kehrt das Vorzeichen um).
  `dz = Neu − Bestand`. **Auftrag** = dz > 0 (Neu über Bestand), **Abtrag** = dz < 0.
- **Interpolation:** linear in den Dreiecken (wie Unterteilung 0), unabhängig von der Dreiecksunterteilung.
- **Überlagerung:** jedes Dreieck des einen Modells wird mit jedem Dreieck des anderen geschnitten (konvexe
  Polygone). Jede Teilfläche liegt damit in genau einem Dreieck jedes Modells; die Punkte und Kanten des einen
  Modells sind im anderen eingerechnet, die Höhen kommen aus dem jeweiligen Modell. Bruchkanten sind Dreieckskanten
  und bleiben erhalten. Das entspricht einer gemeinsamen Vermaschung mit allen Kanten beider Modelle als Zwangskanten.
- **Gemeinsamer Umring:** Teile der Ränder beider Netze, die den gemeinsamen Bereich begrenzen.
- **Extrapolation** nur, wo die Ränder weniger als **0.10 m** auseinander liegen: das Randdreieck wird höchstens
  10 cm über den Rand fortgesetzt (Ebene des Randdreiecks), und nur dort, wo das andere Modell innerhalb dieser
  10 cm nach außen endet. Bei größerem Abstand wird der Umring auf das kleinere Modell zurückgenommen.
- **Schnittkante:** wo dz = 0 durch eine Teilfläche läuft, wird sie dort geteilt (Punkte auf der Schnittkante);
  jede Teilfläche ist danach ganz Auftrag oder ganz Abtrag.
- **Volumen:** Prisma je Teilfläche = Fläche × dz im Schwerpunkt (exakt, da dz linear). Summe der Prismen Neu
  minus Summe der Prismen Bestand über dem Umring = Auftrag − Abtrag (im Test geprüft).
  Prüfungen: versetzte Ebenen, sich kreuzende Ebenen (je 12 500 m³, Schnittkante 100 m) und Ränder 5 cm / 50 cm
  auseinander (mit / ohne Extrapolation) ergeben die exakten Werte.
- **Auflockerung:** DGM-Parameter `loosen_cut` (Abtrag, Vorgabe **1.1**) und `loosen_fill` (Auftrag, Vorgabe **1.5**);
  jede Volumenangabe zeigt das Originalvolumen (fest) und das aufgelockerte Volumen. Die Faktoren stehen im
  Parameterdialog und im Autospeicherstand; die GM0 des Originals bleibt unverändert (das alte Programm kennt sie nicht).
- **Darstellung** auf dem gemeinsamen Netz (Vektor, scharf bei jedem Zoom): Höhenunterschied als Farbschema
  (rot = Abtrag, blau = Auftrag, stufenlos oder gestuft) und/oder Isolinien (Abstand und Farbbereich automatisch
  oder einstellbar, Schnittkante schwarz, jede 5. Linie beschriftet); Auftrag/Abtrag als Klassenkarte mit Legende,
  Balkendiagramm und Tabelle (Fläche und Volumen je Dicke-Klasse).

## Ausgabe: Differenzmodell (GM1), Druckausgabe (PDF), DXF (neu; Menü „Analyse“, Module `export.rs`, `scene.rs`, `pdf.rs`)

### Differenzmodell als GM1
- Punkte = alle Eckpunkte der Teilflächen des gemeinsamen Netzes (Punkte beider Modelle im gemeinsamen Umring,
  Umringpunkte, Punkte auf der Schnittkante), auf 1 mm zusammengefasst; Höhe = `dz = Neu − Bestand` (3 Nachkommastellen).
  Punkte auf der Schnittkante (dz = 0) tragen den Code `SK`. Namen `D1`, `D2` …; Zeile wie die GM1 des Originals
  (`Name Rechtswert Hochwert Höhe [Code]`, cp1252).

### Gemeinsame Szene
PDF und DXF entstehen aus derselben Szene (`scene::Scene`), die aus der Bildschirmdarstellung aufgebaut wird:
sichtbarer Ausschnitt, eingeschaltete Ebenen (Farbflächen, Gitter, Dreiecke, Höhenlinien mit Beschriftung, Umring,
Grundriss, Bruchkanten, Punkte mit Nummer und Höhe) und – wenn gezeigt – die Analyse (Differenzfarben, Isolinien,
Schnittkante, gemeinsamer Umring). Größen (Linienbreite, Schrifthöhe, Kreisradius) sind mm auf dem Papier.
Höhenlinienbeschriftungen unterbrechen die Linie. Punktbeschriftungen erscheinen nur, wo sie auf dem Papier Platz
haben (mittlerer Punktabstand > 9 mm).

### Parameter für den Export (Parameterdatei GM0, Zeilen 38–55; neu)
Das alte Programm liest nur 37 Zeilen, die zusätzlichen Zeilen ändern an dessen Verhalten nichts.

| Zeile | Bedeutung | Vorgabe |
|---|---|---|
| 38 | Radius der Punktkreise (mm Papier) | 0.6 |
| 39 | Schrifthöhe Punktnummer (mm) | 1.5 |
| 40 | Schrifthöhe Punkthöhe (mm) | 1.5 |
| 41 | Schrifthöhe Zähllinien (mm) | 2.0 |
| 42 | Schrifthöhe Gitter/Koordinaten (mm) | 2.0 |
| 43–55 | CAD-Farbnummern (CADdy 1..12): Punktsymbole, Punktnummern, Punkthöhen, Umring, Höhenlinien, Zähllinien, Bruchkanten, Gitter, Dreiecke, Grundriss, Abtrag, Auftrag, Schnittkante | 1, 1, 6, 5, 2, 3, 11, 4, 6, 7, 9, 12, 7 |

Die Vorgaben der Farben entsprechen `dgm.sys` (siehe oben). Zusätzlich gelten aus der Parameterdatei die CAD-Ebenen
(Zeilen 25–37) und der Abstand der Gitterkreuze (Zeile 19, mm Papier). Die Bedeutung der Zeilen 16/17
(„Plotwerte“ 2.0 und 3.0) ist unbekannt und wird nicht verwendet.
Ebenenzuordnung (Index in Zeile 25–37): 1 Punktsymbole, 2 Gitter, 3 Punktnummern, 4 Punkthöhen, 5 Umring,
6 Höhenlinien, 7 Zähllinien, 8 Grundriss, 9 Höhenlinienbeschriftung, 10 Bruchkanten, 11 Differenz (Analyse),
12 Dreiecke, 13 Farbflächen.

### DXF-Export der Bildschirmdarstellung
- Ausschnitt = sichtbarer Kartenausschnitt; Maße in mm Papier × (N/1000) = Meter mit N = Maßstabszahl Plott der
  Parameterdatei (Zeile 15) oder dem im Dialog eingegebenen Wert.
- Farben aus den CAD-Farbnummern der Parameterdatei: ACI-Nummer (Gruppe 62, näherungsweise) und die Echtfarbe der
  CADdy-Farbe (Gruppe 420). Höhenlinien und Differenzlinien als 3D-Polylinien (Höhe bzw. dz als Z).
- Punkte als Kreise (Radius aus der Parameterdatei), Texte mit Schrifthöhen aus der Parameterdatei, Gitterkreuze
  mit Koordinatenbeschriftung. Farbflächen optional als SOLID-Dreiecke in Echtfarbe.

### Druckausgabe (PDF, A4)
- Vektorbasiert (Pfade, Text in Helvetica, Farbflächen als PDF-Gouraud-Netze), keine Rasterbilder.
- Ein Rahmen simuliert das Blatt auf dem Bildschirm; mit Zoom und Verschieben wird der Ausschnitt gewählt, gedruckt
  wird der Bereich im schwarzen Rahmen (WYSIWYG). Maßstab 1:N = 1000 · (Bildschirmpunkte je mm Blatt) / (Bildschirmpunkte je m);
  der Dialog zeigt ihn, kann ihn setzen („Normmaßstab“ rundet auf 1:500, 1:1000 …) und das Modell einpassen.
- Hoch- oder Querformat. Rand mindestens 10 mm rundum; zusätzlich ein Legendenstreifen (Vorgabe 45 mm unten bei
  Hochformat, 60 mm rechts bei Querformat, einstellbar).
- Im Streifen: Titel, Legende (Höhen- bzw. Neigungsfarben, Höhenlinienabstand; bei einer Analyse Differenzfarbleiste,
  Klassen und Volumen mit Original- und aufgelockertem Wert), Maßstab 1:N, Maßstabsleiste, Nordpfeil, Datum.
- Geprüft: Aufbau und Inhalt mit einem unabhängigen PDF-Parser (lopdf) und Tests (Seitengröße, Ränder, Beschnitt der
  Zeichenfläche, Legende, Maßstab, Nordpfeil). Die Darstellung selbst wurde in dieser Umgebung nicht von einem
  PDF-Betrachter gerendert.

### Nachtrag: Punktbeschriftung, Gitter, Menü „Datei“
- **Punktbeschriftung im DXF:** jeder Punkt bekommt Nummer und Höhe, je auf einer eigenen Ebene (Parameterdatei Zeile 27
  Punktnummern, Zeile 28 Punkthöhen; Kreise Zeile 25). Die Nummer steht rechts oberhalb des Punkts (0,9 mm rechts,
  Unterkante knapp über der Höhe), die Höhe rechts vom Punkt (0,9 mm, mittig in der Höhe des Punkts). Die vom Anwender
  gewählten Lagen (Punktnummer-Position) spiegeln das nach rechts unten, links unten und links oben. Die Dichteregel
  („nur wo Platz ist“) gilt nur für das PDF, nicht für das DXF.
- **Ebenentabelle:** das DXF enthält eine LAYER-Tabelle mit allen 13 CAD-Ebenen der Parameterdatei (Farbe nach Zweck),
  sie erscheinen im CAD-Programm auch, wenn sie leer sind.
- **Gitterabstand:** Abstand der Parameterdatei (Zeile 19, mm Papier) × Maßstab, höchstens ein Viertel des kürzeren
  Ausschnitts, auf 1-2-5 × 10^n gerundet. So enthält jede Richtung mindestens vier Teilungen und das Gitter füllt die
  ganze Zeichenfläche (bei A4 und 100 mm Abstand gab es sonst nur zwei Zeilen, die in einer Hälfte lagen).
- **PDF Hochformat:** der Legendenstreifen liegt zwischen dem Mindestrand unten (10 mm) und der Zeichenfläche (`Sheet::strip_rect`); die Legendenblöcke werden auf die zwei Textspalten nach Platzbedarf verteilt, Farbfelder nur bei Platz in zwei Spalten.
- **Menü Datei:** „DXF-Export der Bildschirmdarstellung…“ und „Druckausgabe (PDF, A4)…“ stehen bei den Exportfunktionen.
