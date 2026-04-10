using System;
using System.Collections.Generic;
using System.IO;
using System.Text;
using System.Globalization;
using System.Linq;
using DGM_NT.Models;

namespace DGM_NT.Services;

/// <summary>
/// Erkennt und importiert verschiedene Punkt-Dateiformate
/// </summary>
public class FileFormatService
{
    /// <summary>Ergebnis eines Imports</summary>
    public class ImportErgebnis
    {
        public List<Punkt> Punkte { get; set; } = new();
        public string FormatName { get; set; } = "";
        public string DateiPfad { get; set; } = "";
        public int AnzahlZeilen { get; set; }
        public int AnzahlPunkte { get; set; }
        public int AnzahlFehler { get; set; }
        public int AnzahlUebersprungen { get; set; } // Null-Höhen, 0/0-Koordinaten
        public int AnzahlAusreisser { get; set; }
        public List<string> Warnungen { get; set; } = new();
        public List<string> AusreisserPunkte { get; set; } = new(); // Punktnummern der Ausreißer
        public double MinX, MaxX, MinY, MaxY, MinH, MaxH;
        public bool Erfolg => AnzahlPunkte > 0;
    }

    /// <summary>Alle unterstützten Formate</summary>
    private static readonly Dictionary<string, FormatInfo> Formate = new()
    {
        { ".csv",  new FormatInfo("CSV (Semikolon/Komma)", "CSV-Textdatei", ImportCsv) },
        { ".txt",  new FormatInfo("TXT (Tab/getrennt)", "Tab-getrennte Textdatei", ImportTxt) },
        { ".xyz",  new FormatInfo("XYZ (ASCII)", "ASCII XYZ-Format", ImportXyz) },
        { ".dat",  new FormatInfo("DAT (Festformat)", "Feld-basierte DAT-Datei", ImportDat) },
        { ".asc",  new FormatInfo("ASC (ArcInfo)", "ESRI ASCII Grid Punkte", ImportAsc) },
        { ".land", new FormatInfo("LAND (FEST)", "FEST-LandXML Punkte", ImportLand) },
        { ".xml",  new FormatInfo("XML", "XML-Punktedaten", ImportXml) },
        { ".kor",  new FormatInfo("KOR (Koordinaten)", "ASCII Koordinaten-Datei", ImportKor) },
    };

    private class FormatInfo
    {
        public string Name;
        public string Beschreibung;
        public Func<string, ImportErgebnis> ImportFunc;
        public FormatInfo(string name, string beschreibung, Func<string, ImportErgebnis> func)
        {
            Name = name; Beschreibung = beschreibung; ImportFunc = func;
        }
    }

    /// <summary>Erkennt das Format anhand der Endung</summary>
    public static string? ErkenneFormat(string dateiPfad)
    {
        string ext = Path.GetExtension(dateiPfad).ToLowerInvariant();
        return Formate.ContainsKey(ext) ? Formate[ext].Name : null;
    }

    /// <summary>Importiert Punkte aus beliebiger Datei</summary>
    public static ImportErgebnis ImportierePunkte(string dateiPfad)
    {
        var ergebnis = new ImportErgebnis { DateiPfad = dateiPfad };
        string ext = Path.GetExtension(dateiPfad).ToLowerInvariant();

        if (!Formate.ContainsKey(ext))
        {
            ergebnis.Warnungen.Add($"Unbekanntes Format: {ext}");
            // Versuche CSV als Fallback
            return ImportCsv(dateiPfad);
        }

        var format = Formate[ext];
        ergebnis.FormatName = format.Name;

        try
        {
            return format.ImportFunc(dateiPfad);
        }
        catch (Exception ex)
        {
            ergebnis.Warnungen.Add($"Fehler beim Import: {ex.Message}");
            return ergebnis;
        }
    }

    #region Import-Methoden

    private static ImportErgebnis ImportCsv(string dateiPfad)
    {
        var ergebnis = new ImportErgebnis { FormatName = "CSV", DateiPfad = dateiPfad };
        var lines = File.ReadAllLines(dateiPfad);
        ergebnis.AnzahlZeilen = lines.Length;

        bool headerUebersprungen = false;
        char trennzeichen = ';';

        foreach (var rawLine in lines)
        {
            string line = rawLine.Trim();
            if (string.IsNullOrEmpty(line)) continue;

            // Header erkennen
            if (!headerUebersprungen)
            {
                if (line.StartsWith("PNr", StringComparison.OrdinalIgnoreCase) ||
                    line.StartsWith("Punkt", StringComparison.OrdinalIgnoreCase) ||
                    line.StartsWith("Nr", StringComparison.OrdinalIgnoreCase) ||
                    line.StartsWith("#", StringComparison.OrdinalIgnoreCase))
                {
                    headerUebersprungen = true;
                    // Trennzeichen erkennen
                    if (line.Contains('\t')) trennzeichen = '\t';
                    else if (line.Contains(',')) trennzeichen = ',';
                    else if (line.Contains(';')) trennzeichen = ';';
                    continue;
                }
                headerUebersprungen = true;
            }

            var parts = line.Split(trennzeichen);
            if (TryParsePunkt(parts, ergebnis))
                ergebnis.AnzahlPunkte++;
            else
                ergebnis.AnzahlFehler++;
        }

        return ergebnis;
    }

    private static ImportErgebnis ImportTxt(string dateiPfad)
    {
        var ergebnis = new ImportErgebnis { FormatName = "TXT (Tab)", DateiPfad = dateiPfad };
        var lines = File.ReadAllLines(dateiPfad);
        ergebnis.AnzahlZeilen = lines.Length;

        foreach (var rawLine in lines)
        {
            string line = rawLine.Trim();
            if (string.IsNullOrEmpty(line) || line.StartsWith("#")) continue;

            var parts = line.Split('\t');
            if (TryParsePunkt(parts, ergebnis))
                ergebnis.AnzahlPunkte++;
            else
            {
                // Fallback: Semikolon oder Komma
                parts = line.Split(';', ',');
                if (TryParsePunkt(parts, ergebnis))
                    ergebnis.AnzahlPunkte++;
                else
                    ergebnis.AnzahlFehler++;
            }
        }

        return ergebnis;
    }

    private static ImportErgebnis ImportXyz(string dateiPfad)
    {
        var ergebnis = new ImportErgebnis { FormatName = "XYZ", DateiPfad = dateiPfad };
        var lines = File.ReadAllLines(dateiPfad);
        ergebnis.AnzahlZeilen = lines.Length;

        int nr = 1;
        foreach (var rawLine in lines)
        {
            string line = rawLine.Trim();
            if (string.IsNullOrEmpty(line) || line.StartsWith("#")) continue;

            // XYZ: X Y H (space-separated)
            var parts = line.Split(new[] { ' ', '\t', ';', ',' }, StringSplitOptions.RemoveEmptyEntries);
            if (parts.Length >= 2)
            {
                var p = new Punkt { PNr = nr.ToString() };
                if (TryParseDouble(parts[0], out double x) && TryParseDouble(parts[1], out double y))
                {
                    p.X = x;
                    p.Y = y;
                    p.H = parts.Length > 2 ? (TryParseDouble(parts[2], out double h) ? h : 0) : 0;
                    if (parts.Length > 3) p.Code = parts[3].Trim();

                    ergebnis.Punkte.Add(p);
                    ergebnis.AnzahlPunkte++;
                    UpdateBounds(ergebnis, p);
                }
                else ergebnis.AnzahlFehler++;
                nr++;
            }
            else ergebnis.AnzahlFehler++;
        }

        return ergebnis;
    }

    private static ImportErgebnis ImportDat(string dateiPfad)
    {
        var ergebnis = new ImportErgebnis { FormatName = "DAT (Festformat)", DateiPfad = dateiPfad };
        var lines = File.ReadAllLines(dateiPfad);
        ergebnis.AnzahlZeilen = lines.Length;

        foreach (var rawLine in lines)
        {
            string line = rawLine.Trim();
            if (string.IsNullOrEmpty(line) || line.StartsWith("#") || line.StartsWith("//")) continue;

            // Festformat: PNr(15) X(14) Y(14) H(10) Code(15)
            try
            {
                var p = new Punkt();
                if (line.Length >= 5)
                {
                    // Flexible Festformat-Erkennung
                    var parts = new List<string>();
                    int pos = 0;
                    while (pos < line.Length)
                    {
                        // Springe zu nächstem Zahlenwert oder Text
                        while (pos < line.Length && line[pos] == ' ') pos++;
                        int start = pos;
                        while (pos < line.Length && line[pos] != ' ') pos++;
                        if (start < line.Length) parts.Add(line.Substring(start, pos - start));
                    }

                    if (parts.Count >= 3 && TryParsePunkt(parts.ToArray(), ergebnis))
                        ergebnis.AnzahlPunkte++;
                    else
                        ergebnis.AnzahlFehler++;
                }
                else ergebnis.AnzahlFehler++;
            }
            catch { ergebnis.AnzahlFehler++; }
        }

        return ergebnis;
    }

    private static ImportErgebnis ImportAsc(string dateiPfad)
    {
        var ergebnis = new ImportErgebnis { FormatName = "ASC (ESRI)", DateiPfad = dateiPfad };
        var lines = File.ReadAllLines(dateiPfad);
        ergebnis.AnzahlZeilen = lines.Length;

        // ESRI ASCII Grid Header parsen
        double xll = 0, yll = 0, zellgroesse = 1, nodata = -9999;
        int ncols = 0, nrows = 0;
        int headerLines = 0;
        bool headerParsed = false;

        foreach (var rawLine in lines)
        {
            string line = rawLine.Trim();
            if (string.IsNullOrEmpty(line)) continue;

            if (!headerParsed)
            {
                if (line.StartsWith("ncols", StringComparison.OrdinalIgnoreCase))
                    ncols = int.Parse(line.Split(new[] { ' ', '\t' }, StringSplitOptions.RemoveEmptyEntries)[1]);
                else if (line.StartsWith("nrows", StringComparison.OrdinalIgnoreCase))
                    nrows = int.Parse(line.Split(new[] { ' ', '\t' }, StringSplitOptions.RemoveEmptyEntries)[1]);
                else if (line.StartsWith("xllcorner", StringComparison.OrdinalIgnoreCase))
                    xll = double.Parse(line.Split(new[] { ' ', '\t' }, StringSplitOptions.RemoveEmptyEntries)[1], CultureInfo.InvariantCulture);
                else if (line.StartsWith("yllcorner", StringComparison.OrdinalIgnoreCase))
                    yll = double.Parse(line.Split(new[] { ' ', '\t' }, StringSplitOptions.RemoveEmptyEntries)[1], CultureInfo.InvariantCulture);
                else if (line.StartsWith("cellsize", StringComparison.OrdinalIgnoreCase))
                    zellgroesse = double.Parse(line.Split(new[] { ' ', '\t' }, StringSplitOptions.RemoveEmptyEntries)[1], CultureInfo.InvariantCulture);
                else if (line.StartsWith("NODATA", StringComparison.OrdinalIgnoreCase))
                    nodata = double.Parse(line.Split(new[] { ' ', '\t' }, StringSplitOptions.RemoveEmptyEntries)[1], CultureInfo.InvariantCulture);
                else if (ncols > 0 && nrows > 0) headerParsed = true;

                headerLines++;
                if (headerLines >= 6) headerParsed = true;
                continue;
            }

            // Datenzeilen: Zellenwerte pro Zeile
            var parts = line.Split(new[] { ' ', '\t' }, StringSplitOptions.RemoveEmptyEntries);
            for (int col = 0; col < parts.Length; col++)
            {
                if (TryParseDouble(parts[col], out double h) && Math.Abs(h - nodata) > 0.001)
                {
                    double x = xll + col * zellgroesse;
                    double y = yll + (nrows - 1 - (ergebnis.Punkte.Count / ncols)) * zellgroesse;

                    var p = new Punkt
                    {
                        PNr = (ergebnis.Punkte.Count + 1).ToString(),
                        X = x,
                        Y = y,
                        H = h,
                        Code = "ASC"
                    };
                    ergebnis.Punkte.Add(p);
                    ergebnis.AnzahlPunkte++;
                    UpdateBounds(ergebnis, p);
                }
            }
        }

        return ergebnis;
    }

    private static ImportErgebnis ImportLand(string dateiPfad)
    {
        // FEST/LAND Format: vereinfachte Version
        return ImportCsv(dateiPfad); // Fallback auf CSV
    }

    private static ImportErgebnis ImportXml(string dateiPfad)
    {
        var ergebnis = new ImportErgebnis { FormatName = "XML", DateiPfad = dateiPfad };
        var lines = File.ReadAllLines(dateiPfad);
        ergebnis.AnzahlZeilen = lines.Length;

        // Einfacher XML-Punkte-Parser
        int nr = 1;
        foreach (var rawLine in lines)
        {
            string line = rawLine.Trim();
            // <Punkt X="..." Y="..." H="..." Nr="..."/>
            if (line.Contains("<Punkt") || line.Contains("<point") || line.Contains("<POINT"))
            {
                try
                {
                    double x = ExtractXmlValue(line, "X");
                    double y = ExtractXmlValue(line, "Y");
                    double h = ExtractXmlValue(line, "H", 0);
                    string pnr = ExtractXmlValue(line, "Nr", nr).ToString();

                    var p = new Punkt { PNr = pnr, X = x, Y = y, H = h };
                    ergebnis.Punkte.Add(p);
                    ergebnis.AnzahlPunkte++;
                    UpdateBounds(ergebnis, p);
                    nr++;
                }
                catch { ergebnis.AnzahlFehler++; }
            }
        }

        return ergebnis;
    }

    /// <summary>
    /// Importiert .kor-Datei (ASCII: Punktnummer Rechtswert Hochwert Höhe Punktcode)
    /// Leerzeichen-getrennt, robust gegen leere Zeilen und Kommentare
    /// Filtert: Höhe=0, Koordinaten 0/0
    /// Erkennt Ausreißer
    /// </summary>
    private static ImportErgebnis ImportKor(string dateiPfad)
    {
        var ergebnis = new ImportErgebnis { FormatName = "KOR", DateiPfad = dateiPfad };
        var lines = File.ReadAllLines(dateiPfad);
        ergebnis.AnzahlZeilen = lines.Length;

        // Phase 1: Alle gültigen Punkte einlesen
        var allePunkte = new List<Punkt>();
        int nr = 1;

        foreach (var rawLine in lines)
        {
            string line = rawLine.Trim();
            if (string.IsNullOrEmpty(line) || line.StartsWith("#") || line.StartsWith("//") || line.StartsWith("*"))
                continue;

            // Leerzeichen als Trenner (ein oder mehrere)
            var parts = line.Split(new[] { ' ', '\t' }, StringSplitOptions.RemoveEmptyEntries);

            // Erwartet: PNr X Y H [Code]
            if (parts.Length < 4)
            {
                ergebnis.AnzahlFehler++;
                continue;
            }

            try
            {
                string pnr = parts[0].Trim();
                
                // Versuche double zu parsen (mit Punkt oder Komma)
                double x = ParseDoubleFlexible(parts[1]);
                double y = ParseDoubleFlexible(parts[2]);
                double h = ParseDoubleFlexible(parts[3]);

                if (double.IsNaN(x) || double.IsNaN(y) || double.IsNaN(h))
                {
                    ergebnis.AnzahlFehler++;
                    continue;
                }

                string code = parts.Length > 4 ? parts[4].Trim() : "";

                // === VALIDIERUNG ===
                
                // 1. Koordinaten 0/0 entfernen
                if (Math.Abs(x) < 0.001 && Math.Abs(y) < 0.001)
                {
                    ergebnis.AnzahlUebersprungen++;
                    ergebnis.Warnungen.Add($"Punkt {pnr} entfernt: Koordinaten 0/0");
                    continue;
                }

                // 2. Höhe = 0 → überspringen (keine Höhenangabe)
                if (Math.Abs(h) < 0.001)
                {
                    ergebnis.AnzahlUebersprungen++;
                    ergebnis.Warnungen.Add($"Punkt {pnr} übersprungen: Höhe = 0");
                    continue;
                }

                // 3. Unplausible Koordinaten (> 10 Millionen)
                if (Math.Abs(x) > 10000000 || Math.Abs(y) > 10000000)
                {
                    ergebnis.AnzahlFehler++;
                    ergebnis.Warnungen.Add($"Punkt {pnr} übersprungen: unplausible Koordinaten X={x:F2}, Y={y:F2}");
                    continue;
                }

                // 4. Unplausible Höhe (unter -500 oder über 9000)
                if (h < -500 || h > 9000)
                {
                    ergebnis.AnzahlFehler++;
                    ergebnis.Warnungen.Add($"Punkt {pnr} übersprungen: unplausible Höhe H={h:F2}");
                    continue;
                }

                // Wenn keine PNr, generiere eine
                if (string.IsNullOrEmpty(pnr) || pnr == "-")
                    pnr = nr.ToString();

                var p = new Punkt { PNr = pnr, X = x, Y = y, H = h, Code = code };
                allePunkte.Add(p);
                nr++;
            }
            catch
            {
                ergebnis.AnzahlFehler++;
            }
        }

        // Phase 2: Ausreißer-Erkennung
        if (allePunkte.Count >= 10)
        {
            var (ausreisser, normalePunkte) = ErkennungAusreisser(allePunkte);
            
            ergebnis.AnzahlAusreisser = ausreisser.Count;
            ergebnis.AusreisserPunkte = ausreisser.Select(p => p.PNr).ToList();
            
            // Warnung hinzufügen
            if (ausreisser.Count > 0)
            {
                ergebnis.Warnungen.Add(
                    $"⚠ {ausreisser.Count} Ausreißer erkannt: {string.Join(", ", ausreisser.Take(10).Select(p => p.PNr))}" +
                    (ausreisser.Count > 10 ? $" (+{ausreisser.Count - 10} weitere)" : ""));
                
                foreach (var ap in ausreisser)
                {
                    // Ausreißer trotzdem einlesen, aber markieren
                    ap.Code = string.IsNullOrEmpty(ap.Code) ? "AUSREISSER" : ap.Code + "/AUSREISSER";
                }
            }
            
            ergebnis.Punkte.AddRange(normalePunkte);
            ergebnis.Punkte.AddRange(ausreisser);
        }
        else
        {
            // Zu wenige Punkte für Ausreißer-Erkennung
            ergebnis.Punkte.AddRange(allePunkte);
        }

        ergebnis.AnzahlPunkte = ergebnis.Punkte.Count;
        
        // Bounds aktualisieren
        foreach (var p in ergebnis.Punkte)
            UpdateBounds(ergebnis, p);

        return ergebnis;
    }

    /// <summary>
    /// Erkennt Ausreißer anhand der Entfernung zum Schwerpunkt
    /// Ein Punkt gilt als Ausreißer wenn er > 3 * Standardabweichung vom Mittelwert entfernt ist
    /// </summary>
    private static (List<Punkt> ausreisser, List<Punkt> normal) ErkennungAusreisser(List<Punkt> punkte)
    {
        if (punkte.Count < 10) return (new List<Punkt>(), punkte);

        // Schwerpunkt berechnen
        double avgX = punkte.Average(p => p.X);
        double avgY = punkte.Average(p => p.Y);
        double avgH = punkte.Average(p => p.H);

        // Entfernung jedes Punktes zum Schwerpunkt
        var distanzen = punkte.Select(p =>
        {
            double dx = p.X - avgX;
            double dy = p.Y - avgY;
            double dh = (p.H - avgH) * 0.1; // Höhe mit Faktor 0.1 gewichten
            return Math.Sqrt(dx * dx + dy * dy + dh * dh);
        }).ToList();

        double mittelwert = distanzen.Average();
        double stdAbw = Math.Sqrt(distanzen.Average(d => (d - mittelwert) * (d - mittelwert)));
        
        // Schwellwert: 3 * Standardabweichung (oder mind. 50m)
        double schwellwert = Math.Max(3.0 * stdAbw, 50.0);

        var ausreisser = new List<Punkt>();
        var normal = new List<Punkt>();

        for (int i = 0; i < punkte.Count; i++)
        {
            if (distanzen[i] > schwellwert)
                ausreisser.Add(punkte[i]);
            else
                normal.Add(punkte[i]);
        }

        return (ausreisser, normal);
    }

    #endregion

    #region Helper

    /// <summary>
    /// Parst Double-Werte flexibel mit Punkt oder Komma als Dezimaltrenner
    /// </summary>
    private static double ParseDoubleFlexible(string value)
    {
        if (string.IsNullOrWhiteSpace(value)) return double.NaN;
        
        value = value.Trim();
        
        // Versuche mit InvariantCulture (Punkt als Trenner)
        if (double.TryParse(value, NumberStyles.Float, CultureInfo.InvariantCulture, out double result))
            return result;
        
        // Versuche mit CurrentCulture (Komma als Trenner)
        if (double.TryParse(value, NumberStyles.Float, CultureInfo.CurrentCulture, out result))
            return result;
        
        // Ersetze Komma durch Punkt und versuche nochmal
        string replaced = value.Replace(',', '.');
        if (double.TryParse(replaced, NumberStyles.Float, CultureInfo.InvariantCulture, out result))
            return result;
        
        return double.NaN;
    }

    private static bool TryParsePunkt(string[] parts, ImportErgebnis ergebnis)
    {
        if (parts.Length < 2) return false;

        try
        {
            var p = new Punkt();

            // Erste Spalte = Punktnummer (wenn string) oder X
            if (double.TryParse(parts[0].Replace(',', '.'), NumberStyles.Any, CultureInfo.InvariantCulture, out _))
            {
                // parts[0] ist X
                p.PNr = (ergebnis.Punkte.Count + 1).ToString();
                p.X = double.Parse(parts[0].Replace(',', '.'), NumberStyles.Any, CultureInfo.InvariantCulture);
                if (parts.Length > 1)
                    p.Y = double.Parse(parts[1].Replace(',', '.'), NumberStyles.Any, CultureInfo.InvariantCulture);
                if (parts.Length > 2)
                    p.H = double.Parse(parts[2].Replace(',', '.'), NumberStyles.Any, CultureInfo.InvariantCulture);
                if (parts.Length > 3)
                    p.Code = parts[3].Trim();
            }
            else
            {
                // parts[0] = PNr
                p.PNr = parts[0].Trim();
                if (parts.Length > 1)
                    p.X = double.Parse(parts[1].Replace(',', '.'), NumberStyles.Any, CultureInfo.InvariantCulture);
                if (parts.Length > 2)
                    p.Y = double.Parse(parts[2].Replace(',', '.'), NumberStyles.Any, CultureInfo.InvariantCulture);
                if (parts.Length > 3)
                    p.H = double.Parse(parts[3].Replace(',', '.'), NumberStyles.Any, CultureInfo.InvariantCulture);
                if (parts.Length > 4)
                    p.Code = parts[4].Trim();
            }

            ergebnis.Punkte.Add(p);
            UpdateBounds(ergebnis, p);
            return true;
        }
        catch
        {
            return false;
        }
    }

    private static bool TryParseDouble(string value, out double result)
    {
        return double.TryParse(value.Replace(',', '.'), NumberStyles.Any, CultureInfo.InvariantCulture, out result);
    }

    private static void UpdateBounds(ImportErgebnis ergebnis, Punkt p)
    {
        if (ergebnis.Punkte.Count == 1)
        {
            ergebnis.MinX = ergebnis.MaxX = p.X;
            ergebnis.MinY = ergebnis.MaxY = p.Y;
            ergebnis.MinH = ergebnis.MaxH = p.H;
        }
        else
        {
            if (p.X < ergebnis.MinX) ergebnis.MinX = p.X;
            if (p.X > ergebnis.MaxX) ergebnis.MaxX = p.X;
            if (p.Y < ergebnis.MinY) ergebnis.MinY = p.Y;
            if (p.Y > ergebnis.MaxY) ergebnis.MaxY = p.Y;
            if (p.H < ergebnis.MinH) ergebnis.MinH = p.H;
            if (p.H > ergebnis.MaxH) ergebnis.MaxH = p.H;
        }
    }

    private static double ExtractXmlValue(string line, string attrName, double defaultValue = 0)
    {
        int start = line.IndexOf($"{attrName}=\"", StringComparison.OrdinalIgnoreCase);
        if (start < 0) return defaultValue;
        start += attrName.Length + 2;
        int end = line.IndexOf('"', start);
        if (end < 0) return defaultValue;
        string val = line.Substring(start, end - start).Replace(',', '.');
        return double.TryParse(val, NumberStyles.Any, CultureInfo.InvariantCulture, out double result) ? result : defaultValue;
    }

    private static object ExtractXmlValue(string line, string attrName, object defaultValue)
    {
        int start = line.IndexOf($"{attrName}=\"", StringComparison.OrdinalIgnoreCase);
        if (start < 0) return defaultValue;
        start += attrName.Length + 2;
        int end = line.IndexOf('"', start);
        if (end < 0) return defaultValue;
        return line.Substring(start, end - start);
    }

    #endregion
}
