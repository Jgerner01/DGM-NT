namespace DGM_NT.Models;

/// <summary>
/// Repräsentiert ein DGM-Projekt
/// Entspricht dem 'projrec' Record im Delphi-Code
/// </summary>
public class DgmProjekt
{
    /// <summary>Projektname</summary>
    public string Name { get; set; } = "";
    
    /// <summary>Projektnummer</summary>
    public string Nummer { get; set; } = "";
    
    /// <summary>Beschreibung</summary>
    public string Beschreibung { get; set; } = "";
    
    /// <summary>Ort</summary>
    public string Ort { get; set; } = "";
    
    /// <summary>Ortsteil</summary>
    public string Ort1 { get; set; } = "";
    
    /// <summary>Bearbeiter</summary>
    public string Bearbeiter { get; set; } = "";
    
    /// <summary>Pfad für Geländemodell-Dateien</summary>
    public string GmPfad { get; set; } = "";
    
    /// <summary>Pfad für CAD-Dateien (DXF, etc.)</summary>
    public string CadPfad { get; set; } = "";
    
    /// <summary>Letztes Bearbeitungsdatum</summary>
    public string Datum { get; set; } = "";
    
    /// <summary>Dateistatus (welche Dateien vorhanden)</summary>
    public bool[] DateiStatus { get; set; } = new bool[20];
    
    /// <summary>Parameter für Höhenlinien</summary>
    public double HoeheAequidistanz { get; set; } = 1.0;
    
    /// <summary>Zählkurven-Intervall (jede 5.)</summary>
    public int ZaehlkurvenIntervall { get; set; } = 5;
    
    /// <summary>Bruchkanten verwenden</summary>
    public bool MitBruchkanten { get; set; } = true;
    
    /// <summary>Punktnummern anzeigen</summary>
    public bool MitPunktnummern { get; set; }
    
    /// <summary>Höhenlinien anzeigen</summary>
    public bool MitHoeheNlinien { get; set; }
    
    /// <summary>Dreiecke anzeigen</summary>
    public bool MitDreiecke { get; set; }
    
    /// <summary>Umring anzeigen</summary>
    public bool MitUmring { get; set; }
    
    /// <summary>Gittermodell anzeigen</summary>
    public bool MitGittermodell { get; set; }
    
    /// <summary>Farben für Layer</summary>
    public Dictionary<int, int> LayerFarben { get; set; } = new();
    
    /// <summary>Layer-Namen (DXF-kompatibel)</summary>
    public Dictionary<int, string> LayerNamen { get; set; } = new();
    
    /// <summary>Schriftgröße</summary>
    public int Schriftgroesse { get; set; } = 10;
    
    /// <summary>Zoom-Faktor</summary>
    public double ZoomFaktor { get; set; } = 1.0;
    
    /// <summary>Fensterkoordinaten (Ausschnitt)</summary>
    public double XMin { get; set; }
    public double YMin { get; set; }
    public double XMax { get; set; }
    public double YMax { get; set; }
    
    public DgmProjekt()
    {
        // Standard-Layer initialisieren (wie im Delphi-Code)
        LayerNamen[1] = "Punkte";
        LayerNamen[2] = "Linien";
        LayerNamen[3] = "Punktnr";
        LayerNamen[4] = "Höhen";
        LayerNamen[5] = "Umring";
        LayerNamen[6] = "Höhenlinien";
        LayerNamen[7] = "Zähllinien";
        LayerNamen[8] = "Hl-Schrift";
        LayerNamen[9] = "Zl-Schrift";
        LayerNamen[10] = "Bruchkanten";
        LayerNamen[11] = "Gitter";
        LayerNamen[12] = "Dreiecke";
        LayerNamen[13] = "Dreiecknr";
        LayerNamen[14] = "Grundriss";
        LayerNamen[15] = "Rahmen";
        LayerNamen[16] = "Schriftfeld";
        
        // Standard-Farben
        LayerFarben[1] = 0x0000FF;    // Punkte: Rot
        LayerFarben[2] = 0x000000;    // Linien: Schwarz
        LayerFarben[5] = 0x0000FF;    // Umring: Blau
        LayerFarben[6] = 0x8B4513;    // Höhenlinien: Braun
        LayerFarben[10] = 0xFF0000;   // Bruchkanten: Rot
        LayerFarben[12] = 0xCCCCCC;   // Dreiecke: Grau
    }
    
    public override string ToString() => $"Projekt: {Name} ({Ort})";
}
