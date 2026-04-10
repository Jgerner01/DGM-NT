using System;
using System.IO;
using System.Windows;
using System.Windows.Threading;
using System.Diagnostics;
using DGM_NT.Views;

namespace DGM_NT;

/// <summary>
/// Eigener Einstiegspunkt mit Debug-Logging
/// </summary>
public static class Program
{
    [STAThread]
    public static void Main()
    {
        string logPfad = Path.Combine(AppDomain.CurrentDomain.BaseDirectory, "dgm-nt-debug.log");
        void Log(string msg)
        {
            var zeile = $"[{DateTime.Now:HH:mm:ss.fff}] {msg}";
            try { File.AppendAllText(logPfad, zeile + Environment.NewLine); } catch { }
            Debug.WriteLine(zeile);
        }

        try
        {
            // Log starten
            File.WriteAllText(logPfad, 
                $"=== DGM-NT Debug Log ===\n" +
                $"Gestartet: {DateTime.Now}\n" +
                $"PID: {Process.GetCurrentProcess().Id}\n" +
                $".NET: {Environment.Version}\n" +
                $"OS: {Environment.OSVersion}\n" +
                $"64-Bit: {Environment.Is64BitProcess}\n" +
                $"Pfad: {AppDomain.CurrentDomain.BaseDirectory}\n\n");

            Log("Anwendung gestartet");

            // App initialisieren
            var app = new App();
            // Kein InitializeComponent() - kann bei leerer App.hängen
            Log("App erstellt");

            // Debug-Fenster
            var debugWin = new DebugWindow();
            debugWin.AddLog("DGM-NT startet...");
            debugWin.AddLog($".NET: {Environment.Version}");
            debugWin.AddLog($"OS: {Environment.OSVersion}");
            debugWin.AddLog($"Pfad: {AppDomain.CurrentDomain.BaseDirectory}");
            debugWin.Show();
            debugWin.UpdateLayout();  // Force rendering
            Log("DebugWindow: OK");

            // MainWindow erstellen
            debugWin.AddLog("Erstelle MainWindow...");
            debugWin.SetStatus("Erstelle MainWindow...", "Orange");
            debugWin.UpdateLayout();  // Force rendering
            Log("Erstelle MainWindow...");

            MainWindow? mainWindow = null;
            var settings = DGM_NT.Services.AppSettings.Laden();
            Log($"Settings geladen");
            
            try
            {
                debugWin.AddLog("Lade Settings...");
                debugWin.UpdateLayout();
                debugWin.AddLog($"Settings OK, Letztes: '{settings.LetztesProjekt}'");
                debugWin.UpdateLayout();
                
                debugWin.AddLog("Erstelle MainWindow Instanz...");
                debugWin.UpdateLayout();
                mainWindow = new MainWindow(settings);
                debugWin.AddLog("MainWindow erstellt: OK");
                Log("MainWindow erstellt: OK");
            }
            catch (Exception ex)
            {
                debugWin.AddLog($"FEHLER: {ex.GetType().Name}");
                debugWin.AddLog($"  Meldung: {ex.Message}");
                if (ex.InnerException != null)
                    debugWin.AddLog($"  Inner: {ex.InnerException.Message}");
                debugWin.AddLog($"  Stack: {ex.StackTrace?.Split('\n')[0]}");
                debugWin.SetStatus("MainWindow erstellen fehlgeschlagen!", "Red");
                Log($"MainWindow FEHLER: {ex}");
                debugWin.UpdateLayout();

                MessageBox.Show(
                    $"Fehler beim Erstellen:\n\n{ex.GetType().Name}\n{ex.Message}",
                    "DGM-NT Startfehler",
                    MessageBoxButton.OK, MessageBoxImage.Error);

                app.Shutdown(1);
                return;
            }

            // Letztes Projekt automatisch laden
            if (!string.IsNullOrEmpty(settings.LetztesProjekt) && File.Exists(settings.LetztesProjekt))
            {
                debugWin.AddLog($"Lade letztes Projekt: {settings.LetztesProjekt}");
                Log($"Auto-load letztes Projekt: {settings.LetztesProjekt}");

                try
                {
                    mainWindow.LetztesProjektLaden(settings.LetztesProjekt);
                    debugWin.AddLog("Letztes Projekt geladen: OK");
                }
                catch (Exception ex)
                {
                    debugWin.AddLog($"Letztes Projekt laden FEHLER: {ex.Message}");
                    Log($"Letztes Projekt laden FEHLER: {ex.Message}");
                }
            }

            // MainWindow anzeigen
            debugWin.AddLog("Zeige MainWindow...");
            debugWin.SetStatus("Zeige MainWindow...", "Orange");
            Log("Zeige MainWindow...");

            try
            {
                mainWindow.Show();
                debugWin.AddLog("MainWindow angezeigt: OK");
                debugWin.SetStatus("Erfolgreich gestartet!", "Green");
                Log("MainWindow angezeigt: OK");

                // Debug nach 3s schließen
                var timer = new DispatcherTimer { Interval = TimeSpan.FromSeconds(3) };
                timer.Tick += (s, e) => { timer.Stop(); debugWin.Close(); };
                timer.Start();
            }
            catch (Exception ex)
            {
                debugWin.AddLog($"MainWindow anzeigen FEHLER: {ex.GetType().Name}");
                debugWin.AddLog($"  Meldung: {ex.Message}");
                debugWin.SetStatus("MainWindow anzeigen fehlgeschlagen!", "Red");
                Log($"MainWindow anzeigen FEHLER: {ex}");

                MessageBox.Show(
                    $"Fehler beim Anzeigen:\n\n{ex.GetType().Name}\n{ex.Message}",
                    "DGM-NT Startfehler",
                    MessageBoxButton.OK, MessageBoxImage.Error);

                app.Shutdown(1);
                return;
            }

            app.Run();
        }
        catch (Exception ex)
        {
            // Crash vor allem
            string crashLog = Path.Combine(AppDomain.CurrentDomain.BaseDirectory, "dgm-nt-crash.log");
            File.WriteAllText(crashLog,
                $"CRASH\n{DateTime.Now}\n\n" +
                $"Typ: {ex.GetType().Name}\n" +
                $"Msg: {ex.Message}\n" +
                $"Inner: {ex.InnerException?.Message}\n" +
                $"Stack:\n{ex.StackTrace}");

            MessageBox.Show(
                $"DGM-NT CRASH:\n\n{ex.GetType().Name}\n{ex.Message}\n\nCrash-Log: {crashLog}",
                "DGM-NT CRASH",
                MessageBoxButton.OK, MessageBoxImage.Error);
        }
    }
}
