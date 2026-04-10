using System;
using System.IO;
using System.Xml.Serialization;

namespace DGM_NT.Services;

/// <summary>
/// Allgemeine Anwendungsparameter – werden als XML gespeichert
/// Enthält Parameter für: Punkte-Import, TIN, Höhenlinien, CAD-Export, etc.
/// </summary>
[XmlRoot("DgmParameter")]
public class AppParameter
{
    // ─── Punkte-Import ─────────────────────────────────────────────
    public string ImportTrennzeichen { get; set; } = ";";          // Semikolon, Komma, Tab
    public bool ImportPunkteAlsGanzzahl { get; set; } = false;      // Punktnummer als Ganzzahl interpretieren
    public double ImportKoordinatenToleranz { get; set; } = 0.001;  // Toleranz für 0/0-Prüfung
    public bool ImportAusreisserErkennen { get; set; } = true;
    public double ImportAusreisserFaktor { get; set; } = 3.0;       // Standardabweichung-Faktor

    // ─── TIN / Vermaschung ─────────────────────────────────────────
    public double TinSuchradius { get; set; } = 0;                // sm: Suchradius (0 = unbegrenzt)
    public double TinToleranz { get; set; } = 0.0001;             // tol: Umkreis-Toleranz
    public int TinDichte { get; set; } = 1;                       // dichte: Unterteilungsgrad (1..15)
    public double TinMinFlaeche { get; set; } = 1e-10;            // Minimale Dreiecksfläche
    public bool TinDegenerierteEntfernen { get; set; } = true;

    // ─── Höhenlinien ───────────────────────────────────────────────
    public double HlAequidistanz { get; set; } = 1.0;
    public int HlZaehlkurvenIntervall { get; set; } = 5;
    public int HlGlaettungTyp { get; set; } = 0;                    // 0=Keine, 1=Linear, 2=B-Spline, 3=Bezier
    public double HlGlaettungFaktor { get; set; } = 1.0;
    public bool HlGlaettungKanten { get; set; } = true;
    public bool HlBeschriftung { get; set; } = false;
    public double HlSchriftgroesse { get; set; } = 2.5;

    // ─── CAD / DXF-Export ──────────────────────────────────────────
    public string CadExportFormat { get; set; } = "DXF-R12";        // DXF-R12, DXF-2000, DWG
    public bool CadLayerTrennen { get; set; } = true;
    public bool CadPunkteExportieren { get; set; } = true;
    public bool CadDreieckeExportieren { get; set; } = false;
    public bool CadHoeheNlinienExportieren { get; set; } = true;
    public bool CadBruchkantenExportieren { get; set; } = true;
    public string CadKoordinatensystem { get; set; } = "Gauss-Krueger";

    // ─── Anzeige / Darstellung ─────────────────────────────────────
    public bool AnzPunkte { get; set; } = true;
    public bool AnzDreiecke { get; set; } = false;
    public bool AnzBruchkanten { get; set; } = true;
    public bool AnzHoeheNlinien { get; set; } = true;
    public double AnzPunktgroesse { get; set; } = 2.0;              // Pixel-Radius
    public string AnzFarbSchema { get; set; } = "Standard";          // Standard, Dunkel, Hochkontrast

    // ─── Projektverwaltung ─────────────────────────────────────────
    public string LetztesProjektPfad { get; set; } = "";
    public bool AutoLoadLetztesProjekt { get; set; } = true;
    public string ProjektBasisPfad { get; set; } = "";

    // ─── Ausgabe / Druck ───────────────────────────────────────────
    public int DruckDPI { get; set; } = 300;
    public string DruckFormat { get; set; } = "A4";                 // A4, A3, A2, A1, A0
    public bool DruckenQuerformat { get; set; } = true;
    public bool DruckenRahmen { get; set; } = true;
    public bool DruckenLegende { get; set; } = true;

    // ─── Sonstiges ─────────────────────────────────────────────────
    public double IdToleranz { get; set; } = 0.01;                  // Toleranz für identische Punkte
    public int UndoMaxSchritte { get; set; } = 50;

    // ─── Pfad zur Parameterdatei ───────────────────────────────────
    public static string ParameterPfad => Path.Combine(
        Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData),
        "DGM-NT", "parameter.xml");

    /// <summary>
    /// Lädt die Parameter aus der XML-Datei oder erstellt neue
    /// </summary>
    public static AppParameter Laden()
    {
        try
        {
            if (File.Exists(ParameterPfad))
            {
                var serializer = new XmlSerializer(typeof(AppParameter));
                using var reader = new StreamReader(ParameterPfad);
                var param = (AppParameter?)serializer.Deserialize(reader);
                return param ?? new AppParameter();
            }
        }
        catch
        {
            // Falls Datei korrupt, neue erstellen
        }
        return new AppParameter();
    }

    /// <summary>
    /// Speichert die Parameter als XML-Datei
    /// </summary>
    public void Speichern()
    {
        try
        {
            string verzeichnis = Path.GetDirectoryName(ParameterPfad)!;
            Directory.CreateDirectory(verzeichnis);
            
            var serializer = new XmlSerializer(typeof(AppParameter));
            using var writer = new StreamWriter(ParameterPfad);
            serializer.Serialize(writer, this);
        }
        catch
        {
            // Silent fail – nicht kritisch
        }
    }
}
