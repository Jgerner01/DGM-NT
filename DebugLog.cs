using System;
using System.IO;
using System.Windows;
using System.Reflection;
using System.Diagnostics;

namespace DGM_NT;

/// <summary>
/// Debug-Loger schreibt in Datei und Konsole
/// </summary>
public static class DebugLog
{
    private static string _logFile = Path.Combine(AppDomain.CurrentDomain.BaseDirectory, "dgm-nt-debug.log");
    private static StreamWriter? _writer;
    
    public static void Init()
    {
        try
        {
            _writer = new StreamWriter(_logFile, append: false) { AutoFlush = true };
            Log("=== DGM-NT Debug Log gestartet ===");
            Log($"Zeit: {DateTime.Now:yyyy-MM-dd HH:mm:ss}");
            Log($"Pfad: {AppDomain.CurrentDomain.BaseDirectory}");
            Log($"EXE: {Process.GetCurrentProcess().MainModule?.FileName}");
            Log($"PID: {Process.GetCurrentProcess().Id}");
            Log($"CLR: {Environment.Version}");
            Log($"OS: {Environment.OSVersion}");
            Log($"64-Bit: {Environment.Is64BitProcess}");
        }
        catch (Exception ex)
        {
            // Falls Log-Datei nicht erstellt werden kann
            Console.WriteLine($"Log-Init Fehler: {ex.Message}");
        }
    }
    
    public static void Log(string meldung)
    {
        string zeile = $"[{DateTime.Now:HH:mm:ss.fff}] {meldung}";
        try
        {
            _writer?.WriteLine(zeile);
        }
        catch { }
        Debug.WriteLine(zeile);
        Console.WriteLine(zeile);
    }
    
    public static void LogException(Exception ex, string kontext = "")
    {
        Log($"*** FEHLER {kontext} ***");
        Log($"Typ: {ex.GetType().Name}");
        Log($"Msg: {ex.Message}");
        Log($"Source: {ex.Source}");
        Log($"Stack: {ex.StackTrace}");
        
        if (ex.InnerException != null)
        {
            Log($"--> Inner: {ex.InnerException.GetType().Name}: {ex.InnerException.Message}");
            Log($"--> Inner Stack: {ex.InnerException.StackTrace}");
        }
    }
    
    public static void Close()
    {
        try { _writer?.Close(); } catch { }
    }
}
