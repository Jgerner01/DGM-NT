namespace DGM_NT.Models;

/// <summary>
/// Repräsentiert einen Geländepunkt mit Koordinaten und Höhe
/// Entspricht dem 'punkt' Record im Delphi-Code (uvar.pas)
/// </summary>
public class Punkt
{
    /// <summary>Punktnummer/Bezeichnung</summary>
    public string PNr { get; set; } = "";
    
    /// <summary>X-Koordinate (Rechtswert)</summary>
    public double X { get; set; }
    
    /// <summary>Y-Koordinate (Hochwert)</summary>
    public double Y { get; set; }
    
    /// <summary>Höhe (Z-Koordinate)</summary>
    public double H { get; set; }
    
    /// <summary>Code/Punktcode (z.B. für Symbolzuordnung)</summary>
    public string Code { get; set; } = "";
    
    /// <summary>1. Ableitung X (für Oberflächenberechnung)</summary>
    public double Ableitung1X { get; set; }
    
    /// <summary>1. Ableitung Y</summary>
    public double Ableitung1Y { get; set; }
    
    /// <summary>2. Ableitung X</summary>
    public double Ableitung2X { get; set; }
    
    /// <summary>2. Ableitung XY</summary>
    public double Ableitung2XY { get; set; }
    
    /// <summary>2. Ableitung Y</summary>
    public double Ableitung2Y { get; set; }
    
    /// <summary>Am Punkt anliegende Dreiecke (Zähler)</summary>
    public byte Z { get; set; }
    
    /// <summary>Am Punkt anliegende Dreiecke (Code)</summary>
    public byte C { get; set; }
    
    /// <summary>Dreiecksindex (max 15 anliegende Dreiecke)</summary>
    public List<int> Dreiecke { get; set; } = new();
    
    /// <summary>Farbe für Darstellung</summary>
    public int Farbe { get; set; }
    
    public Punkt() { }
    
    public Punkt(string pnr, double x, double y, double h, string code = "")
    {
        PNr = pnr;
        X = x;
        Y = y;
        H = h;
        Code = code;
    }
    
    public override string ToString() => $"{PNr}: X={X:F3}, Y={Y:F3}, H={H:F3}";
}
