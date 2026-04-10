using System;
using System.IO;
using Newtonsoft.Json;

namespace DGM_NT.Services;

/// <summary>
/// Einfache Anwendungseinstellungen
/// </summary>
public class AppSettings
{
    /// <summary>Pfad des zuletzt geöffneten Projekts</summary>
    public string LetztesProjekt { get; set; } = "";

    /// <summary>Fensterposition/-größe</summary>
    public double FensterLinks { get; set; } = 100;
    public double FensterOben { get; set; } = 100;
    public double FensterBreite { get; set; } = 1400;
    public double FensterHoehe { get; set; } = 900;
    public bool FensterMaximiert { get; set; } = true;

    private static string SettingsPfad => Path.Combine(
        Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData),
        "DGM-NT", "settings.json");

    /// <summary>Lädt die Einstellungen oder erstellt neue</summary>
    public static AppSettings Laden()
    {
        try
        {
            if (File.Exists(SettingsPfad))
            {
                var json = File.ReadAllText(SettingsPfad);
                var settings = JsonConvert.DeserializeObject<AppSettings>(json);
                return settings ?? new AppSettings();
            }
        }
        catch
        {
            // Falls Datei korrupt, neue erstellen
        }
        return new AppSettings();
    }

    /// <summary>Speichert die Einstellungen</summary>
    public void Speichern()
    {
        try
        {
            string verzeichnis = Path.GetDirectoryName(SettingsPfad)!;
            Directory.CreateDirectory(verzeichnis);
            var json = JsonConvert.SerializeObject(this, Formatting.Indented);
            File.WriteAllText(SettingsPfad, json);
        }
        catch
        {
            // Silent fail – nicht kritisch
        }
    }
}
