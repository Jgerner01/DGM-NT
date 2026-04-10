using System.Windows;
using System.Windows.Controls;
using System.ComponentModel;
using System.Windows.Media;
using System.Windows.Shapes;
using System.Windows.Data;
using DGM_NT.Models;
using DGM_NT.Services;
using Microsoft.Win32;
using System.IO;
using System.Globalization;
using IOPath = System.IO.Path;

namespace DGM_NT.Views;

/// <summary>
/// Interaktionslogik für MainWindow.xaml
/// Hauptfenster mit Canvas-basierter 2D-Darstellung
/// </summary>
public partial class MainWindow : Window
{
    // Canvas-Transformationsparameter (nach DXF-Viewer Technik)
    public double Scale = 1.0;    // Pixel pro Welteinheit
    public double PanX = 0.0;     // Weltkoordinate im Canvas-Mittelpunkt (X)
    public double PanY = 0.0;     // Weltkoordinate im Canvas-Mittelpunkt (Y)
    private bool _needsRedraw = true;
    // Geometrie-Cache: wird nur bei Datenänderung oder Scale-Drift > 2x neu aufgebaut
    private bool   _geometryCacheValid = false;
    private double _cacheScale         = 1.0;

    // Maus-Interaktion
    private bool _isPanning = false;
    private Point _lastMousePos;
    
    // Daten
    private DgmProjekt? _aktuellesProjekt;
    private List<Punkt> _punkte = new();
    private List<Dreieck> _dreiecke = new();
    private List<Bruchkante> _bruchkanten = new();
    private List<Hoehenlinie> _hoeheNlinien = new();
    
    // Services
    private TinService _tinService = new();
    private HoehenlinienService _hoehenlinienService = new();
    private ProjectService _projectService = new();
    private ProfilService _profilService = new();
    private VolumenService _volumenService = new();
    private DxfExportService _dxfExportService = new();
    private AppSettings _appSettings = null!;
    
    // Anzeige-Optionen (wird jetzt über Checkboxen gesteuert)
    private double _zoomFaktor = 1.0;
    
    public MainWindow(AppSettings appSettings)
    {
        _appSettings = appSettings;
        InitializeComponent();
        
        // Canvas-Event für Größenänderung und Mausrad
        DrawPanel.SizeChanged += (s, e) => { _needsRedraw = true; ReDrawPanel(); };
        DrawPanel.MouseWheel += DrawPanel_MouseWheel;
        DrawPanel.MouseLeftButtonDown += DrawPanel_MouseLeftButtonDown;
        DrawPanel.MouseMove += DrawPanel_MouseMove;
        DrawPanel.MouseLeftButtonUp += DrawPanel_MouseLeftButtonUp;
        
        // Fenster-Wiederherstellung
        try
        {
            this.Left = _appSettings.FensterLinks;
            this.Top = _appSettings.FensterOben;
            this.Width = _appSettings.FensterBreite;
            this.Height = _appSettings.FensterHoehe;
            this.WindowState = _appSettings.FensterMaximiert ? WindowState.Maximized : WindowState.Normal;
        }
        catch { }
    }

    protected override void OnClosing(System.ComponentModel.CancelEventArgs e)
    {
        // Einstellungen speichern
        if (_appSettings != null)
        {
            _appSettings.FensterLinks = this.RestoreBounds.Left;
            _appSettings.FensterOben = this.RestoreBounds.Top;
            _appSettings.FensterBreite = this.RestoreBounds.Width;
            _appSettings.FensterHoehe = this.RestoreBounds.Height;
            _appSettings.FensterMaximiert = this.WindowState == WindowState.Maximized;
            _appSettings.Speichern();
        }
        base.OnClosing(e);
    }

    #region Canvas-Rendering (nach DXF-Viewer Technik)

    #region Transformation: Welt <-> Pixel

    /// <summary>
    /// Welt-Koordinaten → Canvas-Pixel
    /// </summary>
    private Point ToScreen(double wx, double wy)
    {
        double canvasW = DrawPanel.ActualWidth;
        double canvasH = DrawPanel.ActualHeight;
        return new Point(
            (wx - PanX) * Scale + canvasW / 2.0,
            -(wy - PanY) * Scale + canvasH / 2.0);
    }

    /// <summary>
    /// Canvas-Pixel → Welt-Koordinaten
    /// </summary>
    private (double x, double y) ToWorld(double sx, double sy)
    {
        double canvasW = DrawPanel.ActualWidth;
        double canvasH = DrawPanel.ActualHeight;
        return (
            (sx - canvasW / 2.0) / Scale + PanX,
            -(sy - canvasH / 2.0) / Scale + PanY);
    }

    #endregion

    #region Maus-Interaktion

    private void DrawPanel_MouseWheel(object sender, System.Windows.Input.MouseWheelEventArgs e)
    {
        double factor = e.Delta > 0 ? 1.25 : 1.0 / 1.25;
        ApplyZoom(factor, e.GetPosition(DrawPanel).X, e.GetPosition(DrawPanel).Y);
    }

    public void ApplyZoom(double factor, double sx, double sy)
    {
        var (wx, wy) = ToWorld(sx, sy);
        Scale = Math.Clamp(Scale * factor, 1e-9, 1e9);
        double canvasW = DrawPanel.ActualWidth;
        double canvasH = DrawPanel.ActualHeight;
        PanX = wx - (sx - canvasW / 2.0) / Scale;
        PanY = wy + (sy - canvasH / 2.0) / Scale;
        _needsRedraw = true;
        ReDrawPanel();
    }

    private void DrawPanel_MouseLeftButtonDown(object sender, System.Windows.Input.MouseButtonEventArgs e)
    {
        _isPanning = true;
        _lastMousePos = e.GetPosition(DrawPanel);
        DrawPanel.CaptureMouse();
        DrawPanel.Cursor = System.Windows.Input.Cursors.SizeAll;
    }

    private void DrawPanel_MouseMove(object sender, System.Windows.Input.MouseEventArgs e)
    {
        if (!_isPanning) return;
        
        Point currentPos = e.GetPosition(DrawPanel);
        double dx = currentPos.X - _lastMousePos.X;
        double dy = currentPos.Y - _lastMousePos.Y;
        
        PanX -= dx / Scale;
        PanY += dy / Scale;
        
        _lastMousePos = currentPos;
        _needsRedraw = true;
        ReDrawPanel();
    }

    private void DrawPanel_MouseLeftButtonUp(object sender, System.Windows.Input.MouseButtonEventArgs e)
    {
        if (_isPanning)
        {
            _isPanning = false;
            DrawPanel.ReleaseMouseCapture();
            DrawPanel.Cursor = null;
        }
    }

    #endregion

    /// <summary>
    /// Passt Scale und Pan so an dass alle Daten sichtbar sind (FitToView)
    /// </summary>
    public void FitToView()
    {
        if (_punkte.Count == 0) return;
        
        double minX = _punkte.Min(p => p.X);
        double maxX = _punkte.Max(p => p.X);
        double minY = _punkte.Min(p => p.Y);
        double maxY = _punkte.Max(p => p.Y);
        
        double ww = maxX - minX;
        double wh = maxY - minY;
        double cx = (minX + maxX) / 2;
        double cy = (minY + maxY) / 2;
        
        double canvasW = DrawPanel.ActualWidth;
        double canvasH = DrawPanel.ActualHeight;
        
        if (canvasW > 0 && canvasH > 0 && ww > 0 && wh > 0)
        {
            Scale = Math.Min(canvasW / ww, canvasH / wh) * 0.9;
        }
        
        PanX = cx;
        PanY = cy;
        _needsRedraw = true;
        ReDrawPanel();
    }

    /// <summary>
    /// Haupt-Zeichenmethode: trennt Geometrie-Cache-Aufbau von Pan/Zoom-Transform.
    /// Pan/Zoom → nur MatrixTransform aktualisieren (kein Neuzeichnen der Geometrie).
    /// Datenänderung oder Scale-Drift &gt; 2x → einmaliger Cache-Neuaufbau.
    /// </summary>
    private void ReDrawPanel()
    {
        if (!_needsRedraw) return;
        _needsRedraw = false;

        double w = DrawPanel.ActualWidth;
        double h = DrawPanel.ActualHeight;
        if (w <= 0 || h <= 0) return;

        // Hintergrund (billig, immer neu)
        DrawPanel.RenderBackground(dc =>
            dc.DrawRectangle(Brushes.White, null, new Rect(0, 0, w, h)));

        if (_punkte.Count == 0) return;

        // Geometrie-Cache neu aufbauen wenn: Daten geändert ODER Scale > 2x vom Cache-Zeitpunkt
        double scaleRatio = _cacheScale > 0 ? Scale / _cacheScale : 0;
        if (!_geometryCacheValid || scaleRatio < 0.5 || scaleRatio > 2.0)
            BuildGeometryCache();

        // Pan/Zoom als reine Matrix-Transformation — null CPU, GPU-beschleunigt
        // Weltkoord → Screen:  sx = wx*Scale - PanX*Scale + w/2
        //                      sy = wy*(-Scale) + PanY*Scale + h/2
        var m = new Matrix(Scale, 0, 0, -Scale,
                           -PanX * Scale + w / 2.0,
                            PanY * Scale + h / 2.0);
        DrawPanel.SetContentTransform(m);
    }

    /// <summary>
    /// Invalidiert den Geometrie-Cache und zeichnet neu.
    /// Aufrufen nach Datenänderungen (Import, Vermaschen, Checkbox-Toggle).
    /// </summary>
    private void InvalidateGeometryCache()
    {
        _geometryCacheValid = false;
        _needsRedraw = true;
        ReDrawPanel();
    }

    /// <summary>
    /// Baut die gesamte Geometrie einmalig in Weltkoordinaten auf.
    /// Alle Dreiecke/Punkte/Kanten/Höhenlinien landen in je EINER StreamGeometry
    /// → ein DrawGeometry-Call pro Ebene statt N Calls pro Frame.
    /// Pen-Dicken werden in Welteinheiten gespeichert: PenPx / CacheScale.
    /// Bei Scale-Drift ≤ 2x weichen Liniendicken höchstens um Faktor 2 ab.
    /// </summary>
    private void BuildGeometryCache()
    {
        _cacheScale = Scale;
        _geometryCacheValid = true;
        
        // Parameter laden für Glättungsinfo
        var appParams = AppParameter.Laden();
        System.Diagnostics.Debug.WriteLine($"[BuildGeometryCache] HlGlaettungTyp={appParams.HlGlaettungTyp}, Faktor={appParams.HlGlaettungFaktor}");

        double inv = 1.0 / _cacheScale;   // Welteininheit pro Pixel bei Cache-Scale

        // Pens in Welteinheiten, gefroren (WPF-Optimierung)
        var penDreieck  = MkPen(Brushes.Green,   0.5 * inv);
        var penBK       = MkPen(Brushes.Blue,     1.5 * inv);
        var penHlDuenn  = MkPen(Brushes.Brown,    1.0 * inv);
        var penHlDick   = MkPen(Brushes.Brown,    2.0 * inv);
        var penPunkt    = MkPen(Brushes.DarkRed,  0.5 * inv);
        double pRadius  = 2.0 * inv;   // Punkt-Symbol: 2 Pixel Radius (halbiert)

        DrawPanel.RenderContent(dc =>
        {
            // ── Dreiecke: ALLE in einer einzigen StreamGeometry ────────────────
            if (ChkDreiecke.IsChecked == true && _dreiecke.Count > 0)
            {
                var geo = new StreamGeometry();
                using (var ctx = geo.Open())
                {
                    foreach (var d in _dreiecke)
                    {
                        if (d.Code == 0) continue;
                        var p1 = _punkte[d.P1]; var p2 = _punkte[d.P2]; var p3 = _punkte[d.P3];
                        ctx.BeginFigure(new Point(p1.X, p1.Y), false, true);
                        ctx.LineTo(new Point(p2.X, p2.Y), true, false);
                        ctx.LineTo(new Point(p3.X, p3.Y), true, false);
                    }
                }
                geo.Freeze();
                dc.DrawGeometry(null, penDreieck, geo);
            }

            // ── Punkte: ALLE als KREISE ───────────────────────────────────────
            if (ChkPunkte.IsChecked == true && _punkte.Count > 0)
            {
                double r = pRadius;
                // Kreis als 8-Segment-Polygon in StreamGeometry
                foreach (var p in _punkte)
                {
                    var circleGeo = CreateCircleGeometry(p.X, p.Y, r, 8);
                    circleGeo.Freeze();
                    dc.DrawGeometry(Brushes.Red, penPunkt, circleGeo);
                }
            }

            // ── Bruchkanten: ALLE in einer StreamGeometry ──────────────────────
            if (ChkBruchkanten.IsChecked == true && _bruchkanten.Count > 0)
            {
                var geo = new StreamGeometry();
                using (var ctx = geo.Open())
                {
                    foreach (var bk in _bruchkanten)
                    {
                        if (!bk.Sichtbar || bk.Punkte.Count < 2) continue;
                        bool first = true;
                        foreach (var idx in bk.Punkte)
                        {
                            var p = _punkte[idx];
                            if (first) { ctx.BeginFigure(new Point(p.X, p.Y), false, false); first = false; }
                            else ctx.LineTo(new Point(p.X, p.Y), true, false);
                        }
                    }
                }
                geo.Freeze();
                dc.DrawGeometry(null, penBK, geo);
            }

            // ── Höhenlinien: IMMER Original (grau) + Geglättet (braun) ─────────
            if (ChkHoehenlinien.IsChecked == true && _hoeheNlinien.Count > 0)
            {
                var penHlOrig = MkPen(Brushes.LightGray, 0.5 * inv);   // Original in hellgrau
                var penHlOrigDick = MkPen(Brushes.LightGray, 1.0 * inv);

                // 1. IMMER Original-Linien in hellgrau zeichnen
                var geoOrigDuenn = new StreamGeometry();
                var geoOrigDick = new StreamGeometry();
                bool hatOrig = false;
                using (var ctxD = geoOrigDuenn.Open())
                using (var ctxK = geoOrigDick.Open())
                {
                    foreach (var hl in _hoeheNlinien)
                    {
                        var pts = hl.OriginalPunkte.Count > 0 ? hl.OriginalPunkte : hl.Punkte;
                        if (pts.Count < 2) continue;
                        hatOrig = true;
                        var ctx = hl.Typ == 1 ? ctxK : ctxD;
                        bool first = true;
                        foreach (var pt in pts)
                        {
                            double px = pt.Item1, py = pt.Item2;
                            if (first) { ctx.BeginFigure(new Point(px, py), false, hl.Geschlossen); first = false; }
                            else ctx.LineTo(new Point(px, py), true, false);
                        }
                    }
                }
                if (hatOrig)
                {
                    geoOrigDuenn.Freeze(); geoOrigDick.Freeze();
                    dc.DrawGeometry(null, penHlOrig, geoOrigDuenn);
                    dc.DrawGeometry(null, penHlOrigDick, geoOrigDick);
                }

                // 2. Geglättete Linien in braun zeichnen
                var geoDuenn = new StreamGeometry();
                var geoDick  = new StreamGeometry();
                bool hatGeglaettet = false;
                using (var ctxD = geoDuenn.Open())
                using (var ctxK = geoDick.Open())
                {
                    foreach (var hl in _hoeheNlinien)
                    {
                        if (hl.Punkte.Count < 2) continue;
                        hatGeglaettet = true;
                        var ctx = hl.Typ == 1 ? ctxK : ctxD;
                        bool first = true;
                        foreach (var pt in hl.Punkte)
                        {
                            if (first) { ctx.BeginFigure(new Point(pt.X, pt.Y), false, hl.Geschlossen); first = false; }
                            else ctx.LineTo(new Point(pt.X, pt.Y), true, false);
                        }
                    }
                }
                if (hatGeglaettet)
                {
                    geoDuenn.Freeze(); geoDick.Freeze();
                    dc.DrawGeometry(null, penHlDuenn, geoDuenn);
                    dc.DrawGeometry(null, penHlDick,  geoDick);
                }

                // 3. Höhenwerte auf Zählkurven zeichnen
                var appParams2 = AppParameter.Laden();
                if (appParams2.HlBeschriftung)
                {
                    var textBrush = Brushes.Brown;
                    var textFormat = new System.Windows.Media.Typeface("Segoe UI");
                    double fontSize = appParams2.HlSchriftgroesse * inv * 12; // Convert to points
                    
                    foreach (var hl in _hoeheNlinien)
                    {
                        if (hl.Typ != 1 || hl.Punkte.Count < 2) continue;
                        
                        // Text in der Mitte der Linie platzieren
                        int midIdx = hl.Punkte.Count / 2;
                        var midPt = hl.Punkte[midIdx];
                        var screenPt = ToScreen(midPt.X, midPt.Y);
                        
                        string text = $"{hl.Hoehe:F0}";
                        var formattedText = new System.Windows.Media.FormattedText(
                            text,
                            System.Globalization.CultureInfo.CurrentCulture,
                            System.Windows.FlowDirection.LeftToRight,
                            textFormat,
                            fontSize,
                            textBrush,
                            VisualTreeHelper.GetDpi(DrawPanel).PixelsPerDip);
                        
                        // Hintergrund-Rechteck für bessere Lesbarkeit
                        double pad = 2;
                        var bgRect = new Rect(screenPt.X - formattedText.Width / 2 - pad, 
                                             screenPt.Y - formattedText.Height / 2 - pad,
                                             formattedText.Width + pad * 2, 
                                             formattedText.Height + pad * 2);
                        dc.DrawRectangle(Brushes.White, null, bgRect);
                        
                        // Text zeichnen
                        dc.DrawText(formattedText, new Point(screenPt.X - formattedText.Width / 2, screenPt.Y - formattedText.Height / 2));
                    }
                }
                
                System.Diagnostics.Debug.WriteLine($"[Render] HL: {hatOrig} Orig, {hatGeglaettet} Geglättet, {_hoeheNlinien.Count} Linien");
            }
        });
    }

    private static Pen MkPen(Brush brush, double thickness)
    {
        var pen = new Pen(brush, thickness);
        pen.Freeze();
        return pen;
    }

    /// <summary>
    /// Speichert Höhenlinien automatisch im Projektverzeichnis
    /// </summary>
    private void HlSpeichern()
    {
        if (_aktuellesProjekt == null || string.IsNullOrEmpty(_aktuellesProjekt.GmPfad) || _hoeheNlinien.Count == 0)
            return;

        try
        {
            string hlDatei = IOPath.Combine(_aktuellesProjekt.GmPfad, "hoehenlinien.dgm");
            using var writer = new BinaryWriter(File.Open(hlDatei, FileMode.Create));
            
            writer.Write("DGM-NT-HL");  // Magic
            writer.Write(1);             // Version
            writer.Write(_hoeheNlinien.Count);
            
            foreach (var hl in _hoeheNlinien)
            {
                writer.Write(hl.Hoehe);
                writer.Write(hl.Typ);
                writer.Write(hl.AnfangsCode);
                writer.Write(hl.EndeCode);
                writer.Write(hl.Geschlossen);
                
                // Geglättete Punkte
                writer.Write(hl.Punkte.Count);
                foreach (var pt in hl.Punkte)
                {
                    writer.Write(pt.X);
                    writer.Write(pt.Y);
                }
                
                // Original-Punkte (immer speichern, auch wenn nicht geglättet)
                bool hatOriginal = hl.OriginalPunkte != null && hl.OriginalPunkte.Count > 0;
                writer.Write(hatOriginal);
                if (hatOriginal)
                {
                    writer.Write(hl.OriginalPunkte.Count);
                    foreach (var pt in hl.OriginalPunkte)
                    {
                        writer.Write(pt.x);
                        writer.Write(pt.y);
                    }
                }
            }
        }
        catch { }
    }

    /// <summary>
    /// Lädt Höhenlinien aus dem Projektverzeichnis falls vorhanden
    /// </summary>
    private bool HlLaden()
    {
        if (_aktuellesProjekt == null || string.IsNullOrEmpty(_aktuellesProjekt.GmPfad))
            return false;

        string hlDatei = IOPath.Combine(_aktuellesProjekt.GmPfad, "hoehenlinien.dgm");
        if (!File.Exists(hlDatei))
            return false;

        try
        {
            using var reader = new BinaryReader(File.Open(hlDatei, FileMode.Open));
            
            string magic = reader.ReadString();
            if (magic != "DGM-NT-HL") return false;
            
            int version = reader.ReadInt32();
            int anzHl = reader.ReadInt32();
            
            _hoeheNlinien.Clear();
            for (int i = 0; i < anzHl; i++)
            {
                double hoehe = reader.ReadDouble();
                byte typ = reader.ReadByte();
                byte anfangsCode = reader.ReadByte();
                byte endeCode = reader.ReadByte();
                bool geschlossen = reader.ReadBoolean();
                int anzPunkte = reader.ReadInt32();
                
                var hl = new Hoehenlinie(hoehe, typ);
                hl.AnfangsCode = anfangsCode;
                hl.EndeCode = endeCode;
                
                for (int j = 0; j < anzPunkte; j++)
                {
                    double px = reader.ReadDouble();
                    double py = reader.ReadDouble();
                    hl.Punkte.Add((px, py));
                }
                
                // Original-Punkte lesen (falls vorhanden)
                bool hatOriginal = reader.ReadBoolean();
                if (hatOriginal)
                {
                    int anzOriginal = reader.ReadInt32();
                    for (int j = 0; j < anzOriginal; j++)
                    {
                        double px = reader.ReadDouble();
                        double py = reader.ReadDouble();
                        hl.OriginalPunkte.Add((px, py));
                    }
                }

                _hoeheNlinien.Add(hl);
            }
            
            return true;
        }
        catch
        {
            return false;
        }
    }

    /// <summary>
    /// Erzeugt eine StreamGeometry für einen Kreis in Weltkoordinaten
    /// </summary>
    private static StreamGeometry CreateCircleGeometry(double cx, double cy, double radius, int segments)
    {
        var geo = new StreamGeometry();
        using var ctx = geo.Open();
        ctx.BeginFigure(new Point(cx + radius, cy), true, true);
        for (int i = 1; i <= segments; i++)
        {
            double angle = 2.0 * Math.PI * i / segments;
            ctx.LineTo(new Point(cx + Math.Cos(angle) * radius, cy + Math.Sin(angle) * radius), true, false);
        }
        return geo;
    }

    #endregion
    
    #region Menü-Events
    
    /// <summary>
    /// Lädt ein Projekt ohne Dialog (für Auto-Load beim Start)
    /// </summary>
    public void LetztesProjektLaden(string dateiPfad)
    {
        try
        {
            var daten = _projectService.LoadProjekt(dateiPfad);
            if (daten != null)
            {
                _aktuellesProjekt = daten.Projekt;
                _punkte = daten.Punkte;
                _dreiecke = daten.Dreiecke;
                _bruchkanten = daten.Bruchkanten;
                _hoeheNlinien = daten.Hoehenlinien;
                
                // Falls keine Dreiecke im Projekt, versuche TIN-Datei zu laden
                if (_dreiecke.Count == 0)
                {
                    bool tinGeladen = TinLaden();
                    if (tinGeladen)
                    {
                        StatusMeldung.Text = $"Projekt geladen + TIN geladen: {_dreiecke.Count} Dreiecke";
                    }
                    else
                    {
                        StatusMeldung.Text = $"Projekt geladen: {dateiPfad} ({_punkte.Count} Punkte)";
                    }
                }
                else
                {
                    StatusMeldung.Text = $"Projekt geladen: {dateiPfad} ({_punkte.Count} Punkte, {_dreiecke.Count} Dreiecke)";
                }
                
                // TIN automatisch laden
                if (_dreiecke.Count == 0)
                {
                    bool tinGeladen = TinLaden();
                    if (tinGeladen)
                        StatusMeldung.Text = $"Projekt geladen + TIN geladen: {_dreiecke.Count} Dreiecke";
                }
                
                // Höhenlinien automatisch laden falls vorhanden
                if (_hoeheNlinien.Count == 0)
                {
                    bool hlGeladen = HlLaden();
                    if (hlGeladen)
                    {
                        StatusMeldung.Text += $", {_hoeheNlinien.Count} Höhenlinien geladen";
                    }
                }
                
                UpdateUI();
                InvalidateGeometryCache();
                PruefeDateienUndAktualisiereCheckboxen();
            }
        }
        catch (Exception ex)
        {
            StatusMeldung.Text = $"Letztes Projekt konnte nicht geladen werden: {ex.Message}";
        }
    }
    
    private void MenuProjektNeu_Click(object sender, RoutedEventArgs e)
    {
        var dialog = new ProjektNeuDialog { Owner = this };
        
        if (dialog.ShowDialog() == true)
        {
            try
            {
                // Neues Projekt erstellen
                _aktuellesProjekt = new DgmProjekt
                {
                    Name = dialog.ProjektName,
                    Ort = dialog.Ort,
                    Bearbeiter = dialog.Bearbeiter,
                    GmPfad = dialog.ProjektPfad
                };
                
                _punkte.Clear();
                _dreiecke.Clear();
                _bruchkanten.Clear();
                _hoeheNlinien.Clear();
                
                // Leere Projektdatei im Verzeichnis speichern
                string projektDatei = IOPath.Combine(dialog.ProjektPfad, dialog.ProjektName + ".dgmnt");
                _projectService.SaveProjekt(_aktuellesProjekt, _punkte, _dreiecke, _bruchkanten, _hoeheNlinien, projektDatei);

                UpdateUI();
                StatusMeldung.Text = $"Neues Projekt erstellt: {dialog.ProjektPfad}";
                
                // Zuletzt verwendetes Projekt speichern
                _appSettings.LetztesProjekt = projektDatei;
                _appSettings.Speichern();
            }
            catch (Exception ex)
            {
                MessageBox.Show($"Fehler beim Erstellen des Projekts:\n{ex.Message}", "Fehler", MessageBoxButton.OK, MessageBoxImage.Error);
            }
        }
    }
    
    private void MenuProjektLaden_Click(object sender, RoutedEventArgs e)
    {
        // Erstes Projekt: Basisverzeichnis auswählen
        var ordnerDialog = new Microsoft.Win32.OpenFolderDialog
        {
            Title = "Projektverzeichnis auswählen"
        };
        
        if (ordnerDialog.ShowDialog() != true)
            return;
        
        // Im ausgewählten Verzeichnis nach .dgmnt-Dateien suchen
        var dgmntDateien = Directory.GetFiles(ordnerDialog.FolderName, "*.dgmnt");
        
        if (dgmntDateien.Length == 0)
        {
            MessageBox.Show(
                $"Kein DGM-NT-Projekt gefunden in:\n{ordnerDialog.FolderName}\n\n" +
                $"Es wurde keine .dgmnt-Datei im Verzeichnis gefunden.",
                "Kein Projekt gefunden",
                MessageBoxButton.OK,
                MessageBoxImage.Information);
            return;
        }
        
        // Wenn mehrere Projekte, Auswahl-Dialog zeigen
        string ausgewaehlteDatei;
        if (dgmntDateien.Length == 1)
        {
            ausgewaehlteDatei = dgmntDateien[0];
        }
        else
        {
            var auswahlDialog = new ProjektAuswahlDialog(dgmntDateien, ordnerDialog.FolderName) { Owner = this };
            if (auswahlDialog.ShowDialog() != true)
                return;
            ausgewaehlteDatei = auswahlDialog.AusgewaehlteDatei;
        }
        
        // Projekt laden
        try
        {
            var daten = _projectService.LoadProjekt(ausgewaehlteDatei);
            if (daten != null)
            {
                _aktuellesProjekt = daten.Projekt;
                _punkte = daten.Punkte;
                _dreiecke = daten.Dreiecke;
                _bruchkanten = daten.Bruchkanten;
                _hoeheNlinien = daten.Hoehenlinien;
                
                UpdateUI();
                StatusMeldung.Text = $"Projekt geladen: {ausgewaehlteDatei} ({_punkte.Count} Punkte, {_dreiecke.Count} Dreiecke)";
                
                // Zuletzt verwendetes Projekt speichern
                _appSettings.LetztesProjekt = ausgewaehlteDatei;
                _appSettings.Speichern();
            }
            else
            {
                MessageBox.Show("Projekt konnte nicht geladen werden.", "Fehler", MessageBoxButton.OK, MessageBoxImage.Error);
            }
        }
        catch (Exception ex)
        {
            MessageBox.Show($"Fehler beim Laden:\n{ex.Message}", "Fehler", MessageBoxButton.OK, MessageBoxImage.Error);
        }
    }
    
    private void MenuProjektSpeichern_Click(object sender, RoutedEventArgs e)
    {
        if (_aktuellesProjekt == null)
        {
            MessageBox.Show("Kein Projekt zum Speichern vorhanden.", "Hinweis", MessageBoxButton.OK, MessageBoxImage.Information);
            return;
        }
        
        try
        {
            // Projektdatei im Projektverzeichnis speichern
            string projektDatei = IOPath.Combine(_aktuellesProjekt.GmPfad, _aktuellesProjekt.Name + ".dgmnt");
            _projectService.SaveProjekt(_aktuellesProjekt, _punkte, _dreiecke, _bruchkanten, _hoeheNlinien, projektDatei);
            StatusMeldung.Text = $"Projekt gespeichert: {projektDatei}";
        }
        catch (Exception ex)
        {
            MessageBox.Show($"Fehler beim Speichern:\n{ex.Message}", "Fehler", MessageBoxButton.OK, MessageBoxImage.Error);
        }
    }
    
    private void MenuBeenden_Click(object sender, RoutedEventArgs e)
    {
        Close();
    }
    
    private void MenuPunkteEingeben_Click(object sender, RoutedEventArgs e)
    {
        // Punkte-Eingabedialog öffnen
        StatusMeldung.Text = "Punkte eingeben - Dialog folgt";
    }
    
    private void MenuPunkteImport_Click(object sender, RoutedEventArgs e)
    {
        var dialog = new OpenFileDialog
        {
            Filter = "Alle Punkt-Dateien|*.csv;*.txt;*.xyz;*.dat;*.asc;*.land;*.xml;*.kor|" +
                     "CSV-Dateien (*.csv)|*.csv|" +
                     "TXT-Dateien (*.txt)|*.txt|" +
                     "XYZ-Dateien (*.xyz)|*.xyz|" +
                     "DAT-Dateien (*.dat)|*.dat|" +
                     "ASC-Dateien (*.asc)|*.asc|" +
                     "LAND-Dateien (*.land)|*.land|" +
                     "XML-Dateien (*.xml)|*.xml|" +
                     "KOR-Dateien (*.kor)|*.kor|" +
                     "Alle Dateien (*.*)|*.*",
            Title = "Punkte importieren"
        };

        if (dialog.ShowDialog() == true)
        {
            ImportPunkte(dialog.FileName);
        }
    }
    
    private void MenuPunkteExport_Click(object sender, RoutedEventArgs e)
    {
        var dialog = new SaveFileDialog
        {
            Filter = "CSV-Dateien (*.csv)|*.csv|Alle Dateien (*.*)|*.*",
            Title = "Punkte exportieren"
        };
        
        if (dialog.ShowDialog() == true)
        {
            ExportPunkte(dialog.FileName);
        }
    }
    
    private void MenuVermaschen_Click(object sender, RoutedEventArgs e)
    {
        Vermaschen();
    }
    
    private void MenuBruchkantenBerechnen_Click(object sender, RoutedEventArgs e)
    {
        BruchkantenBerechnen();
    }
    
    private void MenuHoeheNlinienErzeugen_Click(object sender, RoutedEventArgs e)
    {
        HoehenlinienErzeugen();
    }
    
    private void MenuHoeheNlinienPlan_Click(object sender, RoutedEventArgs e)
    {
        StatusMeldung.Text = "Höhenlinienplan zeichnen - Dialog folgt";
    }
    
    private void MenuProfileZeichnen_Click(object sender, RoutedEventArgs e)
    {
        StatusMeldung.Text = "Profile zeichnen - Dialog folgt";
    }
    
    private void MenuVolumen_Click(object sender, RoutedEventArgs e)
    {
        StatusMeldung.Text = "Volumenberechnung - Dialog folgt";
    }
    
    private void MenuUeber_Click(object sender, RoutedEventArgs e)
    {
        MessageBox.Show(
            "DGM-NT v0.2.0\nDigitales Geländemodell - Neuer Generation\n\n" +
            "Basierend auf dem Delphi-DGM von Fritz Wimbauer (1996)\n" +
            "Neuimplementierung in C# / WPF mit OpenTK 3D\n\n" +
            "Features:\n" +
            "• TIN-Vermaschung (Delaunay)\n" +
            "• Höhenlinienberechnung\n" +
            "• Bruchkantenerkennung\n" +
            "• Profilansichten\n" +
            "• Volumenberechnung\n" +
            "• DXF-Export\n" +
            "• JSON Projekt-Speicherung",
            "Über DGM-NT",
            MessageBoxButton.OK,
            MessageBoxImage.Information);
    }
    
    private void MenuCadExport_Click(object sender, RoutedEventArgs e)
    {
        var dialog = new SaveFileDialog
        {
            Filter = "DXF-Dateien (*.dxf)|*.dxf|Alle Dateien (*.*)|*.*",
            Title = "DXF-Export",
            DefaultExt = "dxf"
        };
        
        if (dialog.ShowDialog() == true)
        {
            try
            {
                StatusMeldung.Text = "Exportiere nach DXF...";
                _dxfExportService.ExportDxf(dialog.FileName, _punkte, _dreiecke, _bruchkanten, _hoeheNlinien, _aktuellesProjekt);
                StatusMeldung.Text = $"DXF exportiert: {dialog.FileName}";
            }
            catch (Exception ex)
            {
                MessageBox.Show($"Fehler beim DXF-Export:\n{ex.Message}", "Fehler", MessageBoxButton.OK, MessageBoxImage.Error);
            }
        }
    }
    
    #endregion
    
    #region Toolbar-Events
    
    private void BtnProjektLaden_Click(object sender, RoutedEventArgs e) => MenuProjektLaden_Click(sender, e);
    private void BtnProjektSpeichern_Click(object sender, RoutedEventArgs e) => MenuProjektSpeichern_Click(sender, e);
    private void BtnPunkteEinlesen_Click(object sender, RoutedEventArgs e) => MenuPunkteImport_Click(sender, e);
    private void BtnVermaschen_Click(object sender, RoutedEventArgs e) => Vermaschen();
    private void BtnBruchkanten_Click(object sender, RoutedEventArgs e) => BruchkantenBerechnen();
    private void BtnHoeheNlinien_Click(object sender, RoutedEventArgs e) => HoehenlinienErzeugen();
    private void BtnHoeheNlinienPlan_Click(object sender, RoutedEventArgs e) => MenuHoeheNlinienPlan_Click(sender, e);
    private void BtnProfile_Click(object sender, RoutedEventArgs e) => MenuProfileZeichnen_Click(sender, e);
    private void BtnVolumen_Click(object sender, RoutedEventArgs e) => MenuVolumen_Click(sender, e);
    
    private void BtnZeigeAlle_Click(object sender, RoutedEventArgs e)
    {
        FitToView();
    }
    
    private void BtnZoomGross_Click(object sender, RoutedEventArgs e)
    {
        double canvasW = DrawPanel.ActualWidth;
        double canvasH = DrawPanel.ActualHeight;
        ApplyZoom(1.5, canvasW / 2, canvasH / 2);
    }
    
    private void BtnZoomKlein_Click(object sender, RoutedEventArgs e)
    {
        double canvasW = DrawPanel.ActualWidth;
        double canvasH = DrawPanel.ActualHeight;
        ApplyZoom(1.0 / 1.5, canvasW / 2, canvasH / 2);
    }
    
    private void BtnAusschnitt_Click(object sender, RoutedEventArgs e)
    {
        StatusMeldung.Text = "Ausschnitt mit Maus wählen - Implementierung folgt";
    }
    
    /// <summary>
    /// Checkbox geändert → Anzeige aktualisieren
    /// </summary>
    private void ChkAnzeige_Changed(object sender, RoutedEventArgs e)
    {
        // Checkbox geändert → Geometrie-Cache ungültig, neu aufbauen
        InvalidateGeometryCache();
    }
    
    // Menü-Checkboxen sync mit Overlay-Checkboxen
    private void ViewDreiecke_Click(object sender, RoutedEventArgs e) { ChkDreiecke.IsChecked = ViewDreiecke.IsChecked; }
    private void ViewBruchkanten_Click(object sender, RoutedEventArgs e) { ChkBruchkanten.IsChecked = ViewBruchkanten.IsChecked; }
    private void ViewHoehenlinien_Click(object sender, RoutedEventArgs e) { ChkHoehenlinien.IsChecked = ViewHoeheNlinien.IsChecked; }

    private void ViewZoomGross_Click(object sender, RoutedEventArgs e) => BtnZoomGross_Click(sender, e);
    private void ViewZoomKlein_Click(object sender, RoutedEventArgs e) => BtnZoomKlein_Click(sender, e);
    private void ViewZeigeAlle_Click(object sender, RoutedEventArgs e) => BtnZeigeAlle_Click(sender, e);
    private void ViewAusschnitt_Click(object sender, RoutedEventArgs e) => BtnAusschnitt_Click(sender, e);
    
    private void ListProjekte_SelectionChanged(object sender, SelectionChangedEventArgs e)
    {
        // Projekt aus Liste laden
    }
    
    #endregion
    
    #region Verarbeitungsmethoden

    private void BerechneDreieckNeigung(Dreieck d)
    {
        if (_punkte.Count <= d.P3) return;
        var p1 = _punkte[d.P1];
        var p2 = _punkte[d.P2];
        var p3 = _punkte[d.P3];

        double ax = p2.X - p1.X, ay = p2.Y - p1.Y, az = p2.H - p1.H;
        double bx = p3.X - p1.X, by = p3.Y - p1.Y, bz = p3.H - p1.H;

        d.NormalX = ay * bz - az * by;
        d.NormalY = az * bx - ax * bz;
        d.NormalZ = ax * by - ay * bx;

        double len = Math.Sqrt(d.NormalX * d.NormalX + d.NormalY * d.NormalY + d.NormalZ * d.NormalZ);
        if (len > 0) { d.NormalX /= len; d.NormalY /= len; d.NormalZ /= len; }
        d.Neigung = Math.Acos(Math.Abs(d.NormalZ)) * 180.0 / Math.PI;
        d.Flaeche = 0.5 * len;
        d.SchwerpunktX = (p1.X + p2.X + p3.X) / 3.0;
        d.SchwerpunktY = (p1.Y + p2.Y + p3.Y) / 3.0;
        d.SchwerpunktH = (p1.H + p2.H + p3.H) / 3.0;
    }

    /// <summary>
    /// Speichert TIN-Ergebnis automatisch im Projektverzeichnis
    /// </summary>
    private void TinSpeichern()
    {
        if (_aktuellesProjekt == null || string.IsNullOrEmpty(_aktuellesProjekt.GmPfad)) return;
        try {
            var appParams = AppParameter.Laden();
            string tinDatei = IOPath.Combine(_aktuellesProjekt.GmPfad, "tin.dgm");
            using var w = new BinaryWriter(File.Open(tinDatei, FileMode.Create));
            w.Write("DGM-NT-TIN"); w.Write(1); w.Write(_punkte.Count); w.Write(_dreiecke.Count);
            w.Write(appParams.TinSuchradius); w.Write(appParams.TinToleranz); w.Write(appParams.TinDichte); w.Write(appParams.TinMinFlaeche);
            for (int i = 0; i < _punkte.Count; i++) { w.Write(i); w.Write(_punkte[i].X); w.Write(_punkte[i].Y); w.Write(_punkte[i].H); }
            foreach (var d in _dreiecke) { w.Write(d.P1); w.Write(d.P2); w.Write(d.P3); w.Write(d.Code); }
        } catch { }
    }

    /// <summary>
    /// Lädt TIN-Ergebnis aus dem Projektverzeichnis falls vorhanden
    /// </summary>
    private bool TinLaden()
    {
        if (_aktuellesProjekt == null || string.IsNullOrEmpty(_aktuellesProjekt.GmPfad))
            return false;

        string tinDatei = IOPath.Combine(_aktuellesProjekt.GmPfad, "tin.dgm");
        if (!File.Exists(tinDatei))
            return false;

        try
        {
            using var reader = new BinaryReader(File.Open(tinDatei, FileMode.Open));
            
            string magic = reader.ReadString();
            if (magic != "DGM-NT-TIN") return false;
            
            int version = reader.ReadInt32();
            int anzPunkte = reader.ReadInt32();
            int anzDreiecke = reader.ReadInt32();
            
            // Punkte lesen (wenn noch keine vorhanden)
            if (_punkte.Count == 0)
            {
                for (int i = 0; i < anzPunkte; i++)
                {
                    int idx = reader.ReadInt32();
                    double x = reader.ReadDouble();
                    double y = reader.ReadDouble();
                    double h = reader.ReadDouble();
                    
                    var p = new Punkt
                    {
                        PNr = idx.ToString(),
                        X = x,
                        Y = y,
                        H = h
                    };
                    _punkte.Add(p);
                }
            }
            else
            {
                // Punkte überspringen
                for (int i = 0; i < anzPunkte; i++)
                {
                    reader.ReadInt32();
                    reader.ReadDouble();
                    reader.ReadDouble();
                    reader.ReadDouble();
                }
            }
            
            // Dreiecke lesen
            for (int i = 0; i < anzDreiecke; i++)
            {
                var d = new Dreieck(
                    reader.ReadInt32(),
                    reader.ReadInt32(),
                    reader.ReadInt32());
                d.Code = reader.ReadByte();
                _dreiecke.Add(d);
            }
            
            // Neigung berechnen (nur einmal)
            foreach (var d in _dreiecke)
            {
                BerechneDreieckNeigung(d);
            }

            return true;
        }
        catch
        {
            return false;
        }
    }

    private void Vermaschen()
    {
        if (_punkte.Count < 3)
        {
            MessageBox.Show($"Nur {_punkte.Count} Punkte vorhanden. Mindestens 3 Punkte erforderlich!", "Fehler", MessageBoxButton.OK, MessageBoxImage.Warning);
            return;
        }

        try
        {
            // Parameter-Dialog zeigen
            var dlg = new TinParameterDialog(_punkte.Count) { Owner = this };
            if (dlg.ShowDialog() != true)
                return;

            System.Diagnostics.Debug.WriteLine($"[Vermaschen] Start mit {_punkte.Count} Punkten, Suchradius={dlg.Suchradius}, Tol={dlg.Toleranz}");
            StatusMeldung.Text = $"Vermasche {_punkte.Count} Punkte zu TIN...";
            
            // Vermaschung mit Parametern
            _dreiecke = _tinService.DelaunayTriangulation(_punkte, dlg.Suchradius, dlg.Toleranz, dlg.MinFlaeche, dlg.DegenerierteEntfernen);
            
            System.Diagnostics.Debug.WriteLine($"[Vermaschen] Ergebnis: {_dreiecke.Count} Dreiecke");

            if (_dreiecke.Count == 0)
            {
                MessageBox.Show("Keine Dreiecke erzeugt! Prüfen Sie die Punkte und Parameter.", "Fehler", MessageBoxButton.OK, MessageBoxImage.Error);
                return;
            }

            // Parameter speichern
            TinSpeichern();

            StatusMeldung.Text = $"TIN erstellt: {_dreiecke.Count} Dreiecke → tin.dgm gespeichert";
            UpdateUI();
            InvalidateGeometryCache();
            PruefeDateienUndAktualisiereCheckboxen();
        }
        catch (Exception ex)
        {
            MessageBox.Show($"Fehler beim Vermaschen:\n{ex.Message}", "Fehler", MessageBoxButton.OK, MessageBoxImage.Error);
        }
    }

    private void BruchkantenBerechnen()
    {
        if (_punkte.Count < 2 || _dreiecke.Count == 0)
        {
            MessageBox.Show("Zuerst Punkte einlesen und vermaschen!", "Hinweis", MessageBoxButton.OK, MessageBoxImage.Information);
            return;
        }
        
        try
        {
            StatusMeldung.Text = "Berechne Bruchkanten...";
            _bruchkanten = _tinService.ErkennungBruchkanten(_punkte, _dreiecke);
            StatusMeldung.Text = $"Bruchkanten erkannt: {_bruchkanten.Count} Kanten";
            UpdateUI();
        }
        catch (Exception ex)
        {
            MessageBox.Show($"Fehler bei der Bruchkantenerkennung:\n{ex.Message}", "Fehler", MessageBoxButton.OK, MessageBoxImage.Error);
        }
    }
    
    private void HoehenlinienErzeugen()
    {
        if (_punkte.Count < 3 || _dreiecke.Count == 0)
        {
            MessageBox.Show("Zuerst Punkte einlesen und vermaschen!", "Hinweis", MessageBoxButton.OK, MessageBoxImage.Information);
            return;
        }

        try
        {
            // Parameter aus XML-Datei laden
            var appParams = AppParameter.Laden();
            
            // Höhenbereich ermitteln
            double minH = _punkte.Min(p => p.H);
            double maxH = _punkte.Max(p => p.H);
            double aktuelleAeq = appParams.HlAequidistanz;
            int zkIntervall = appParams.HlZaehlkurvenIntervall;
            
            // Parameter-Dialog zeigen
            var dlg = new HlParameterDialog(minH, maxH, aktuelleAeq, zkIntervall) { Owner = this };
            
            if (dlg.ShowDialog() != true)
                return;
            
            // Einstellungen übernehmen
            double aequidistanz = dlg.Aequidistanz;
            
            // Projekt aktualisieren
            if (_aktuellesProjekt != null)
            {
                _aktuellesProjekt.HoeheAequidistanz = aequidistanz;
                _aktuellesProjekt.ZaehlkurvenIntervall = dlg.ZaehlkurvenIntervall;
            }
            
            StatusMeldung.Text = $"Erzeuge Höhenlinien (Äquidistanz: {aequidistanz}m, Zählkurven: jede {dlg.ZaehlkurvenIntervall}.)...";
            _hoeheNlinien = _hoehenlinienService.BerechneHoeheNlinien(_punkte, _dreiecke, aequidistanz, 
                appParams.HlGlaettungTyp, appParams.HlGlaettungFaktor, appParams.HlGlaettungKanten);
            
            // Automatisch speichern
            HlSpeichern();
            
            StatusMeldung.Text = $"Höhenlinien erzeugt: {_hoeheNlinien.Count} Linien → hoehenlinien.dgm gespeichert";
            UpdateUI();
            InvalidateGeometryCache();
            PruefeDateienUndAktualisiereCheckboxen();
        }
        catch (Exception ex)
        {
            MessageBox.Show($"Fehler bei der Höhenlinienerzeugung:\n{ex.Message}", "Fehler", MessageBoxButton.OK, MessageBoxImage.Error);
        }
    }
    
    private void ImportPunkte(string dateiname)
    {
        try
        {
            // Format erkennen
            string? formatName = FileFormatService.ErkenneFormat(dateiname);
            
            if (formatName == null)
            {
                var result = MessageBox.Show(
                    $"Dateiendung nicht bekannt: {IOPath.GetExtension(dateiname)}\n\n" +
                    $"Soll trotzdem versucht werden, die Datei als CSV zu importieren?",
                    "Unbekanntes Format",
                    MessageBoxButton.YesNo,
                    MessageBoxImage.Question);
                    
                if (result != MessageBoxResult.Yes)
                    return;
            }
            
            StatusMeldung.Text = $"Analysiere Datei: {IOPath.GetFileName(dateiname)}...";
            
            // Import analysieren
            var importErgebnis = FileFormatService.ImportierePunkte(dateiname);
            
            if (!importErgebnis.Erfolg)
            {
                MessageBox.Show(
                    $"Keine Punkte konnten importiert werden.\n\n" +
                    $"Format: {importErgebnis.FormatName}\n" +
                    $"Datei: {dateiname}\n" +
                    $"Fehler: {importErgebnis.AnzahlFehler}\n\n" +
                    $"Bitte überprüfen Sie das Dateiformat.",
                    "Import fehlgeschlagen",
                    MessageBoxButton.OK,
                    MessageBoxImage.Warning);
                StatusMeldung.Text = "Import fehlgeschlagen";
                return;
            }
            
            // Bestätigungs-Dialog zeigen
            var bestaetigungDialog = new ImportBestaetigungDialog(importErgebnis) { Owner = this };
            
            if (bestaetigungDialog.ShowDialog() != true || !bestaetigungDialog.Importieren)
            {
                StatusMeldung.Text = "Import abgebrochen";
                return;
            }
            
            // Punkte übernehmen
            int vorher = _punkte.Count;
            _punkte.AddRange(importErgebnis.Punkte);
            int neuHinzugefuegt = _punkte.Count - vorher;

            // Debug: Erste 3 Punkte ausgeben
            string debugInfo = "";
            for (int i = Math.Max(0, vorher); i < Math.Min(vorher + 3, _punkte.Count); i++)
            {
                var p = _punkte[i];
                debugInfo += $" | P{p.PNr} X={p.X:F3} Y={p.Y:F3} H={p.H:F3}";
            }
            System.Diagnostics.Debug.WriteLine($"Importierte Punkte: {debugInfo}");

            // Automatisch CSV im Projektverzeichnis speichern
            if (_aktuellesProjekt != null)
            {
                string csvPfad = IOPath.Combine(_aktuellesProjekt.GmPfad, "punkte.csv");
                try
                {
                    // Schreibe NUR die neu importierten Punkte (ohne alte)
                    var neuePunkte = _punkte.Skip(vorher).Take(neuHinzugefuegt).ToList();
                    
                    using var writer = new StreamWriter(csvPfad, false); // false = überschreiben
                    writer.WriteLine("PNr;X;Y;H;Code");
                    
                    foreach (var p in neuePunkte)
                    {
                        string xStr = p.X.ToString("F3", CultureInfo.InvariantCulture);
                        string yStr = p.Y.ToString("F3", CultureInfo.InvariantCulture);
                        string hStr = p.H.ToString("F3", CultureInfo.InvariantCulture);
                        writer.WriteLine($"{p.PNr};{xStr};{yStr};{hStr};{p.Code}");
                    }
                    
                    StatusMeldung.Text = $"{neuHinzugefuegt} Punkte importiert ({importErgebnis.FormatName}) – CSV: {csvPfad}{debugInfo}";
                }
                catch (Exception csvEx)
                {
                    StatusMeldung.Text = $"{neuHinzugefuegt} Punkte importiert – CSV-Fehler: {csvEx.Message}";
                }
            }
            else
            {
                StatusMeldung.Text = $"{neuHinzugefuegt} Punkte importiert ({importErgebnis.FormatName}) – Kein Projekt geöffnet{debugInfo}";
            }

            UpdateUI(); // ruft intern InvalidateGeometryCache() auf
        }
        catch (Exception ex)
        {
            MessageBox.Show($"Fehler beim Import:\n{ex.Message}\n\n{ex.StackTrace}", "Fehler", MessageBoxButton.OK, MessageBoxImage.Error);
            StatusMeldung.Text = "Import fehlgeschlagen";
        }
    }
    
    private void ExportPunkte(string dateiname)
    {
        try
        {
            using var writer = new StreamWriter(dateiname);
            writer.WriteLine("PNr;X;Y;H;Code");
            
            foreach (var p in _punkte)
            {
                writer.WriteLine($"{p.PNr};{p.X:F3};{p.Y:F3};{p.H:F3};{p.Code}");
            }
            
            StatusMeldung.Text = $"{_punkte.Count} Punkte exportiert nach {IOPath.GetFileName(dateiname)}";
        }
        catch (Exception ex)
        {
            MessageBox.Show($"Fehler beim Export:\n{ex.Message}", "Fehler", MessageBoxButton.OK, MessageBoxImage.Error);
        }
    }
    
    #endregion
    
    #region UI-Updates
    
    private void UpdateUI()
    {
        LblProjekt.Text = _aktuellesProjekt?.Name ?? "Kein Projekt";
        LblPunkte.Text = $"Punkte: {_punkte.Count}";
        LblDreiecke.Text = $"Dreiecke: {_dreiecke.Count}";

        LblInfoName.Text = "Name:";
        LblInfoWert.Text = _aktuellesProjekt?.Name ?? "-";
        LblInfoOrtWert.Text = _aktuellesProjekt?.Ort ?? "-";
        LblInfoBearbeiterWert.Text = _aktuellesProjekt?.Bearbeiter ?? "-";

        StatusProjekt.Text = _aktuellesProjekt?.Name ?? "Kein Projekt";
        StatusPunkte.Text = $"P: {_punkte.Count}  D: {_dreiecke.Count}";

        // Kamera automatisch auf Daten zentrieren
        KameraAnpassen();

        // Projektliste aktualisieren
        ListProjekte.Items.Clear();
        if (_aktuellesProjekt != null)
        {
            ListProjekte.Items.Add(_aktuellesProjekt.Name);
        }

        // Geometrie-Cache nach jeder Datenänderung ungültig machen
        InvalidateGeometryCache();
    }

    /// <summary>
    /// Passt den Viewport an die aktuellen Daten an (FitToView)
    /// </summary>
    private void KameraAnpassen()
    {
        FitToView();
    }

    /// <summary>
    /// Prüft ob TIN/Bruchkanten/Höhenlinien-Dateien existieren und aktualisiert die Checkboxen
    /// </summary>
    private void PruefeDateienUndAktualisiereCheckboxen()
    {
        if (_aktuellesProjekt == null || string.IsNullOrEmpty(_aktuellesProjekt.GmPfad))
            return;

        string tinDatei = IOPath.Combine(_aktuellesProjekt.GmPfad, "tin.dgm");
        string bkDatei = IOPath.Combine(_aktuellesProjekt.GmPfad, "bruchkanten.dgm");
        string hlDatei = IOPath.Combine(_aktuellesProjekt.GmPfad, "hoehenlinien.dgm");
        string hlplanDatei = IOPath.Combine(_aktuellesProjekt.GmPfad, "hoehenplan.dgm");

        // Dreiecke
        ChkDreiecke.IsEnabled = _dreiecke.Count > 0 || File.Exists(tinDatei);
        if (_dreiecke.Count > 0 && ChkDreiecke.IsChecked != true)
            ChkDreiecke.IsChecked = true;

        // Bruchkanten
        ChkBruchkanten.IsEnabled = _bruchkanten.Count > 0 || File.Exists(bkDatei);
        if (_bruchkanten.Count > 0 && ChkBruchkanten.IsChecked != true)
            ChkBruchkanten.IsChecked = true;

        // Höhenlinien
        ChkHoehenlinien.IsEnabled = _hoeheNlinien.Count > 0 || File.Exists(hlDatei);
        if (_hoeheNlinien.Count > 0 && ChkHoehenlinien.IsChecked != true)
            ChkHoehenlinien.IsChecked = true;

        // Höhenplan
        ChkHoehenplan.IsEnabled = File.Exists(hlplanDatei);
    }
    
    private void UpdateZoomAnzeige()
    {
        LblZoom.Text = $"Zoom: {_zoomFaktor:F1}x";
        StatusZoom.Text = $"Zoom: {_zoomFaktor:F1}x";
    }
    
    private void UpdateStatus()
    {
        StatusPunkte.Text = $"P: {_punkte.Count}  D: {_dreiecke.Count}";
    }
    
    #endregion
}
