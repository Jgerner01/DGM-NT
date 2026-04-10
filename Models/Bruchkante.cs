using System.Collections.ObjectModel;

namespace DGM_NT.Models;

/// <summary>
/// Repräsentiert eine Bruchkante im Gelände
/// Entspricht dem Bruchkantensystem im Delphi-Code (bkfa, bkfe)
/// </summary>
public class Bruchkante
{
    /// <summary>ID der Bruchkante</summary>
    public int Id { get; set; }
    
    /// <summary>Punkte der Bruchkante (laufende Nummern)</summary>
    public ObservableCollection<int> Punkte { get; set; } = new();
    
    /// <summary>0 = hart (Geländekante), 1 = weich (Übergang)</summary>
    public int Typ { get; set; }
    
    /// <summary>Bruchkanten-Code</summary>
    public string Code { get; set; } = "";
    
    /// <summary>Farbe für Darstellung</summary>
    public int Farbe { get; set; }
    
    /// <summary>Liniestärke</summary>
    public double Strichstaerke { get; set; } = 1.0;
    
    /// <summary>Sichtbar in der Darstellung</summary>
    public bool Sichtbar { get; set; } = true;
    
    public Bruchkante() { }
    
    public Bruchkante(int id, int typ = 0)
    {
        Id = id;
        Typ = typ;
    }
    
    /// <summary>Länge der Bruchkante berechnen</summary>
    public double LaengeBerechnen(List<Punkt> punkte)
    {
        double laenge = 0;
        for (int i = 0; i < Punkte.Count - 1; i++)
        {
            var p1 = punkte[Punkte[i]];
            var p2 = punkte[Punkte[i + 1]];
            double dx = p2.X - p1.X;
            double dy = p2.Y - p1.Y;
            laenge += Math.Sqrt(dx * dx + dy * dy);
        }
        return laenge;
    }
    
    public override string ToString() => $"BK {Id}: {Punkte.Count} Punkte, Typ={(Typ == 0 ? "hart" : "weich")}";
}
