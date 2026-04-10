namespace DGM_NT.Models;

/// <summary>
/// Repräsentiert ein Profil (Querprofil, Längsprofil, Schnittprofil)
/// Basierend auf dem Profilsystem im Delphi-Code (TqprofInfo, TlprofInfo, TsprofInfo)
/// </summary>
public class Profil
{
    /// <summary>Name des Profils</summary>
    public string Name { get; set; } = "";
    
    /// <summary>Dateiname des Profils</summary>
    public string Dateiname { get; set; } = "";
    
    /// <summary>Profiltyp: 1=Querprofil, 2=Längsprofil, 3=Schnittprofil</summary>
    public int Typ { get; set; }
    
    /// <summary>Profilpunkte</summary>
    public List<ProfilPunkt> Punkte { get; set; } = new();
    
    /// <summary>Maximaler Abstand für Interpolation</summary>
    public double MaxAbstand { get; set; } = 10.0;
    
    /// <summary>Standard-Abstand</summary>
    public double StandardAbstand { get; set; } = 1.0;
    
    /// <summary>Kennzeichnung für Profilpunkte</summary>
    public string Kennzeichnung { get; set; } = "P";
    
    /// <summary>Glättungsfaktor (0=keine, 1=leicht, 2=stark)</summary>
    public int Glaettung { get; set; }
    
    /// <summary>Start-Station (für Längsprofile)</summary>
    public double StartStation { get; set; }
    
    /// <summary>Ende X-Koordinate (für Schnittprofile)</summary>
    public double EndeX { get; set; }
    
    /// <summary>Ende Y-Koordinate (für Schnittprofile)</summary>
    public double EndeY { get; set; }
    
    /// <summary>Anfang X-Koordinate (für Schnittprofile)</summary>
    public double AnfangX { get; set; }
    
    /// <summary>Anfang Y-Koordinate (für Schnittprofile)</summary>
    public double AnfangY { get; set; }
    
    /// <summary>Splines verwenden</summary>
    public bool MitSpline { get; set; }
    
    /// <summary>Nur Bruchkanten</summary>
    public bool NurBruchkanten { get; set; }
    
    /// <summary>Kotierte Ausgabe</summary>
    public bool MitKote { get; set; }
    
    /// <summary>Horizontalbezug (für Profilbeschriftung)</summary>
    public string HorizontalBezug { get; set; } = "";
    
    /// <summary>Profil-Korrektur-Dateiname</summary>
    public string KorrekturDateiname { get; set; } = "";
}

/// <summary>
/// Einzelner Punkt im Profil
/// </summary>
public class ProfilPunkt
{
    /// <summary>Punktnummer</summary>
    public string Nr { get; set; } = "";
    
    /// <summary>X-Koordinate</summary>
    public double X { get; set; }
    
    /// <summary>Y-Koordinate</summary>
    public double Y { get; set; }
    
    /// <summary>Station (Abstand vom Profilbeginn)</summary>
    public double Station { get; set; }
    
    /// <summary>Höhe</summary>
    public double H { get; set; }
    
    /// <summary>Steigung zum nächsten Punkt</summary>
    public double Steigung { get; set; }
    
    /// <summary>Code</summary>
    public byte Code { get; set; }
    
    /// <summary>Punkt-Index im Original-Geländemodell</summary>
    public int PunktIndex { get; set; }
    
    public ProfilPunkt() { }
    
    public ProfilPunkt(double station, double h)
    {
        Station = station;
        H = h;
    }
    
    public override string ToString() => $"Station {Station:F2}, H={H:F3}";
}
