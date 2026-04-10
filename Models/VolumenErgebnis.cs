namespace DGM_NT.Models;

/// <summary>
/// Repräsentiert die Ergebnisse der Volumenberechnung
/// Basierend auf dem Volumen-Modul im Delphi-Code
/// </summary>
public class VolumenErgebnis
{
    /// <summary>Name des Bereichs</summary>
    public string Name { get; set; } = "";
    
    /// <summary>Grundhöhe (Bezugshöhe)</summary>
    public double GrundHoehe { get; set; }
    
    /// <summary>Auflockerungsfaktor</summary>
    public double Auflockerung { get; set; } = 1.0;
    
    /// <summary>Dichte des Materials (t/m³)</summary>
    public double Dichte { get; set; } = 1.5;
    
    /// <summary>Einschnitt-Volumen (Abtrag)</summary>
    public double Einschnitt { get; set; }
    
    /// <summary>Aufschüttung-Volumen (Antrag)</summary>
    public double Aufschuettung { get; set; }
    
    /// <summary>Differenzvolumen</summary>
    public double Differenz => Einschnitt - Aufschuettung;
    
    /// <summary>Grundfläche</summary>
    public double Grundflaeche { get; set; }
    
    /// <summary>Umgriff (Umfang des Bereichs)</summary>
    public double Umgriff { get; set; }
    
    /// <summary>Durchschnittshöhe</summary>
    public double DurchschnittHoehe { get; set; }
    
    /// <summary>Maximale Höhe im Bereich</summary>
    public double MaxHoehe { get; set; }
    
    /// <summary>Minimale Höhe im Bereich</summary>
    public double MinHoehe { get; set; }
    
    /// <summary>Anzahl der vermessenen Punkte</summary>
    public int PunkteZahl { get; set; }
    
    /// <summary>Anzahl der Dreiecke</summary>
    public int DreieckeZahl { get; set; }
    
    /// <summary>Berechnungsdatum</summary>
    public DateTime Datum { get; set; } = DateTime.Now;
    
    /// <summary>Dateiname für Ausgabe</summary>
    public string AusgabeDateiname { get; set; } = "";
    
    /// <summary>Berechnungstyp: 1=DGM gegen Ebene, 2=DGM gegen DGM</summary>
    public int Typ { get; set; }
    
    public VolumenErgebnis() { }
    
    public override string ToString() => 
        $"Volumen: Einschnitt={Einschnitt:F2}m³, Auftrag={Aufschuettung:F2}m³, Diff={Differenz:F2}m³";
}
