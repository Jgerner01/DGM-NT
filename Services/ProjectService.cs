using System.IO;
using Newtonsoft.Json;
using DGM_NT.Models;

namespace DGM_NT.Services;

/// <summary>
/// Service zum Speichern und Laden von DGM-Projekten im JSON-Format
/// </summary>
public class ProjectService
{
    /// <summary>
    /// Speichert das komplette Projekt als JSON-Datei
    /// </summary>
    public void SaveProjekt(DgmProjekt projekt, List<Punkt> punkte, 
        List<Dreieck> dreiecke, List<Bruchkante> bruchkanten, 
        List<Hoehenlinie> hoeheNlinien, string dateiPfad)
    {
        var projektDaten = new ProjektDaten
        {
            Version = "1.0",
            ErstelltAm = DateTime.Now,
            Projekt = projekt,
            Punkte = punkte,
            Dreiecke = dreiecke,
            Bruchkanten = bruchkanten,
            Hoehenlinien = hoeheNlinien
        };
        
        var json = JsonConvert.SerializeObject(projektDaten, Formatting.Indented);
        File.WriteAllText(dateiPfad, json);
    }
    
    /// <summary>
    /// Lädt ein Projekt aus einer JSON-Datei
    /// </summary>
    public ProjektDaten? LoadProjekt(string dateiPfad)
    {
        if (!File.Exists(dateiPfad))
            return null;
        
        var json = File.ReadAllText(dateiPfad);
        var daten = JsonConvert.DeserializeObject<ProjektDaten>(json);
        
        if (daten?.Projekt != null)
        {
            // Pfade aktualisieren
            daten.Projekt.GmPfad = Path.GetDirectoryName(dateiPfad) ?? "";
        }
        
        return daten;
    }
    
    /// <summary>
    /// Exportiert nur Punkte als JSON
    /// </summary>
    public void ExportPunkteJson(List<Punkt> punkte, string dateiPfad)
    {
        var json = JsonConvert.SerializeObject(punkte, Formatting.Indented);
        File.WriteAllText(dateiPfad, json);
    }
    
    /// <summary>
    /// Importiert Punkte aus JSON
    /// </summary>
    public List<Punkt> ImportPunkteJson(string dateiPfad)
    {
        var json = File.ReadAllText(dateiPfad);
        return JsonConvert.DeserializeObject<List<Punkt>>(json) ?? new List<Punkt>();
    }
}

/// <summary>
/// Container für alle Projektdaten
/// </summary>
public class ProjektDaten
{
    public string Version { get; set; } = "1.0";
    public DateTime ErstelltAm { get; set; }
    public DgmProjekt Projekt { get; set; } = null!;
    public List<Punkt> Punkte { get; set; } = new();
    public List<Dreieck> Dreiecke { get; set; } = new();
    public List<Bruchkante> Bruchkanten { get; set; } = new();
    public List<Hoehenlinie> Hoehenlinien { get; set; } = new();
}
