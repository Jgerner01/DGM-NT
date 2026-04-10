namespace DGM_NT.Models;

/// <summary>
/// Repräsentiert eine Höhenlinie
/// Entspricht dem 'hlpolyt' Record im Delphi-Code
/// </summary>
public class Hoehenlinie
{
    /// <summary>Höhe der Höhenlinie</summary>
    public double Hoehe { get; set; }
    
    /// <summary>X-Koordinate Ursprung links</summary>
    public double XUL { get; set; }
    
    /// <summary>Y-Koordinate Ursprung links</summary>
    public double YUL { get; set; }
    
    /// <summary>Polygonpunkte der Höhenlinie</summary>
    public List<(double X, double Y)> Punkte { get; set; } = new();
    
    /// <summary>0 = normale Kurve, 1 = Zählkurve (jede 5.)</summary>
    public byte Typ { get; set; }
    
    /// <summary>Anfangscode (0=kein, 1=normale BK, 2=weiche BK, 3=geschlossen)</summary>
    public byte AnfangsCode { get; set; }
    
    /// <summary>Endcode</summary>
    public byte EndeCode { get; set; }
    
    /// <summary>Blatt-ID bei projektaufgeteilten Daten</summary>
    public short BlattId { get; set; }
    
    /// <summary>Geschlossene Höhenlinie</summary>
    public bool Geschlossen => AnfangsCode == 3 || EndeCode == 3;
    
    /// <summary>Original-Punkte vor der Glättung (für Vergleichsanzeige)</summary>
    public List<(double x, double y)> OriginalPunkte { get; set; } = new();
    
    public Hoehenlinie() { }
    
    public Hoehenlinie(double hoehe, byte typ = 0)
    {
        Hoehe = hoehe;
        Typ = typ;
    }
    
    /// <summary>Länge der Höhenlinie berechnen</summary>
    public double Laenge
    {
        get
        {
            double laenge = 0;
            for (int i = 0; i < Punkte.Count - 1; i++)
            {
                double dx = Punkte[i + 1].X - Punkte[i].X;
                double dy = Punkte[i + 1].Y - Punkte[i].Y;
                laenge += Math.Sqrt(dx * dx + dy * dy);
            }
            if (Geschlossen && Punkte.Count > 2)
            {
                double dx = Punkte[0].X - Punkte[^1].X;
                double dy = Punkte[0].Y - Punkte[^1].Y;
                laenge += Math.Sqrt(dx * dx + dy * dy);
            }
            return laenge;
        }
    }
    
    public override string ToString() => $"HL {Hoehe:F2}m ({Punkte.Count} Punkte, {(Geschlossen ? "geschlossen" : "offen")})";
}
