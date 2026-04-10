using DGM_NT.Models;

namespace DGM_NT.Services;

/// <summary>
/// Service für Höhenlinienberechnung
/// Basierend auf dem Delphi-Code (ohlmach1.pas, hlspline.pas, ohlsort.pas)
/// </summary>
public class HoehenlinienService
{
    /// <summary>
    /// Berechnet Höhenlinien aus TIN oder Subdreiecken
    /// </summary>
    public List<Hoehenlinie> BerechneHoeheNlinien(List<Punkt> punkte, List<Dreieck> dreiecke, 
        double aequidistanz, int glaettungTyp = 0, double glaettungFaktor = 1.0, bool kantenErhalten = true)
    {
        if (punkte.Count < 3 || dreiecke.Count == 0)
            throw new ArgumentException("Punkte und Dreiecke erforderlich");
        
        System.Diagnostics.Debug.WriteLine($"[HL] Verwende {dreiecke.Count} Dreiecke");

        // Höhenbereich bestimmen
        double minH = punkte.Min(p => p.H);
        double maxH = punkte.Max(p => p.H);

        // Höhenstufen berechnen
        var hoeheNstufen = new List<double>();
        double startH = Math.Floor(minH / aequidistanz) * aequidistanz;
        for (double h = startH; h <= maxH; h += aequidistanz)
        {
            hoeheNstufen.Add(Math.Round(h, 2));
        }

        // Höhenlinien pro Stufe berechnen
        var hoeheNlinien = new List<Hoehenlinie>();

        foreach (var hoehe in hoeheNstufen)
        {
            var linien = BerechneHoehenlinieFuerStufe(punkte, dreiecke, hoehe, aequidistanz);
            hoeheNlinien.AddRange(linien);
        }

        // ═══ IMMER: Original-Punkte sichern + Glättung anwenden ═══
        foreach (var hl in hoeheNlinien)
        {
            // Original-Punkte sichern (für graue Darstellung)
            hl.OriginalPunkte = new List<(double x, double y)>(hl.Punkte);
            int origCount = hl.Punkte.Count;
            
            // Nur glätten wenn Typ > 0
            if (glaettungTyp > 0)
            {
                hl.Punkte = GlaetteLinie(hl.Punkte, glaettungTyp, glaettungFaktor, hl.Geschlossen);
                System.Diagnostics.Debug.WriteLine($"  Linie H={hl.Hoehe}: {origCount} → {hl.Punkte.Count} Punkte (Geglättet)");
            }
            else
            {
                System.Diagnostics.Debug.WriteLine($"  Linie H={hl.Hoehe}: {origCount} Punkte (Original, keine Glättung)");
            }
        }
        
        System.Diagnostics.Debug.WriteLine($"[Höhenlinien] Gesamt: {hoeheNlinien.Count} Linien, Glättung Typ={glaettungTyp}");

        return hoeheNlinien;
    }
    
    #region Private Methoden

    /// <summary>
    /// Wendet Glättung auf eine Höhenlinie an
    /// </summary>
    private List<(double x, double y)> GlaetteLinie(List<(double x, double y)> punkte, 
        int typ, double faktor, bool geschlossen)
    {
        if (punkte.Count < 3) return punkte; // Zu wenige Punkte zum Glätten
        
        // Anzahl Unterteilungen pro Segment (höher = feinere Kurve)
        int subdivisionen = Math.Max(3, (int)(10 / faktor));
        
        return typ switch
        {
            1 => GlaetteLinear(punkte, geschlossen),
            2 => GlaetteBSpline(punkte, geschlossen, subdivisionen),
            3 => GlaetteBezier(punkte, geschlossen, subdivisionen),
            _ => punkte
        };
    }

    /// <summary>
    /// Lineare Glättung: Punkte werden durch gewichteten Durchschnitt benachbarter Punkte ersetzt
    /// </summary>
    private List<(double x, double y)> GlaetteLinear(List<(double x, double y)> punkte, bool geschlossen)
    {
        int n = punkte.Count;
        var geglaettet = new List<(double x, double y)>();
        
        for (int i = 0; i < n; i++)
        {
            int prev = (i - 1 + n) % n;
            int next = (i + 1) % n;
            
            // Gewichteter Durchschnitt: 50% Original, 25% Vorgänger, 25% Nachfolger
            double x = punkte[i].x * 0.5 + punkte[prev].x * 0.25 + punkte[next].x * 0.25;
            double y = punkte[i].y * 0.5 + punkte[prev].y * 0.25 + punkte[next].y * 0.25;
            
            geglaettet.Add((x, y));
        }
        
        return geglaettet;
    }

    /// <summary>
    /// B-Spline Glättung: Quadratische B-Spline-Interpolation
    /// Erzeugt deutlich glattere Kurven als lineare Glättung
    /// </summary>
    private List<(double x, double y)> GlaetteBSpline(List<(double x, double y)> kontrollpunkte, 
        bool geschlossen, int unterteilungen)
    {
        var ergebnis = new List<(double x, double y)>();
        int n = kontrollpunkte.Count;
        
        for (int i = 0; i < n; i++)
        {
            int p0 = (i - 1 + n) % n;
            int p1 = i;
            int p2 = (i + 1) % n;
            int p3 = (i + 2) % n;
            
            for (int t = 0; t < unterteilungen; t++)
            {
                double u = (double)t / unterteilungen;
                
                // Quadratische B-Spline Basis-Funktionen
                double b0 = (1 - u) * (1 - u) / 2;
                double b1 = (-2 * u * u + 2 * u + 1) / 2;
                double b2 = u * u / 2;
                
                double x = b0 * kontrollpunkte[p0].x + b1 * kontrollpunkte[p1].x + b2 * kontrollpunkte[p2].x;
                double y = b0 * kontrollpunkte[p0].y + b1 * kontrollpunkte[p1].y + b2 * kontrollpunkte[p2].y;
                
                ergebnis.Add((x, y));
            }
        }
        
        return ergebnis;
    }

    /// <summary>
    /// Bézier-Glättung: Kubische Bézier-Interpolation zwischen Kontrollpunkten
    /// Erzeugt die glattesten Kurven
    /// </summary>
    private List<(double x, double y)> GlaetteBezier(List<(double x, double y)> kontrollpunkte, 
        bool geschlossen, int unterteilungen)
    {
        var ergebnis = new List<(double x, double y)>();
        int n = kontrollpunkte.Count;
        
        for (int i = 0; i < n; i++)
        {
            // 4 Kontrollpunkte für kubische Bézier-Kurve
            int p0 = (i - 1 + n) % n;
            int p1 = i;
            int p2 = (i + 1) % n;
            int p3 = (i + 2) % n;
            
            // Bézier-Kontrollpunkte berechnen
            double cp0x = kontrollpunkte[p1].x;
            double cp0y = kontrollpunkte[p1].y;
            double cp1x = kontrollpunkte[p1].x + (kontrollpunkte[p2].x - kontrollpunkte[p0].x) / 6;
            double cp1y = kontrollpunkte[p1].y + (kontrollpunkte[p2].y - kontrollpunkte[p0].y) / 6;
            double cp2x = kontrollpunkte[p2].x - (kontrollpunkte[p3].x - kontrollpunkte[p1].x) / 6;
            double cp2y = kontrollpunkte[p2].y - (kontrollpunkte[p3].y - kontrollpunkte[p1].y) / 6;
            double cp3x = kontrollpunkte[p2].x;
            double cp3y = kontrollpunkte[p2].y;
            
            // Bézier-Kurve auswerten
            for (int t = 0; t < unterteilungen; t++)
            {
                double u = (double)t / unterteilungen;
                double u2 = u * u;
                double u3 = u2 * u;
                double mu = 1 - u;
                double mu2 = mu * mu;
                double mu3 = mu2 * mu;
                
                double x = mu3 * cp0x + 3 * mu2 * u * cp1x + 3 * mu * u2 * cp2x + u3 * cp3x;
                double y = mu3 * cp0y + 3 * mu2 * u * cp1y + 3 * mu * u2 * cp2y + u3 * cp3y;
                
                ergebnis.Add((x, y));
            }
        }
        
        return ergebnis;
    }

    private List<Hoehenlinie> BerechneHoehenlinieFuerStufe(
        List<Punkt> punkte, 
        List<Dreieck> dreiecke, 
        double hoehe,
        double aequidistanz)
    {
        var linien = new List<Hoehenlinie>();
        var schnittpunkte = new Dictionary<(int, int), (double x, double y)>();
        
        // Für jedes Dreieck: Schnittpunkte mit Höhenstufe finden
        foreach (var dreieck in dreiecke)
        {
            if (dreieck.Code == 0) continue;
            
            var p1 = punkte[dreieck.P1];
            var p2 = punkte[dreieck.P2];
            var p3 = punkte[dreieck.P3];
            
            // Prüfen ob Höhenstufe im Dreieck liegt
            double minH = Math.Min(p1.H, Math.Min(p2.H, p3.H));
            double maxH = Math.Max(p1.H, Math.Max(p2.H, p3.H));
            
            if (hoehe >= minH && hoehe <= maxH)
            {
                // Schnittpunkte berechnen
                var sp = BerechneSchnittpunkte(p1, p2, p3, hoehe);
                if (sp.Count == 2)
                {
                    // Höhenlinie oder Teilstück erstellen
                    var hl = new Hoehenlinie(hoehe);
                    hl.Punkte.Add(sp[0]);
                    hl.Punkte.Add(sp[1]);
                    
                    // Zählkurve? (jede 5.)
                    if (Math.Abs(hoehe % (aequidistanz * 5)) < 0.01)
                    {
                        hl.Typ = 1;
                    }
                    
                    linien.Add(hl);
                }
            }
        }
        
        // Höhenlinien verbinden (zusammenhängende Segmente)
        var verbundeneLinien = VerbindeHoehenlinien(linien);
        
        return verbundeneLinien;
    }
    
    private List<(double x, double y)> BerechneSchnittpunkte(Punkt p1, Punkt p2, Punkt p3, double hoehe)
    {
        var schnittpunkte = new List<(double x, double y)>();
        
        // Kante p1-p2
        if ((p1.H - hoehe) * (p2.H - hoehe) <= 0 && p1.H != p2.H)
        {
            double t = (hoehe - p1.H) / (p2.H - p1.H);
            double x = p1.X + t * (p2.X - p1.X);
            double y = p1.Y + t * (p2.Y - p1.Y);
            schnittpunkte.Add((x, y));
        }
        
        // Kante p2-p3
        if ((p2.H - hoehe) * (p3.H - hoehe) <= 0 && p2.H != p3.H)
        {
            double t = (hoehe - p2.H) / (p3.H - p2.H);
            double x = p2.X + t * (p3.X - p2.X);
            double y = p2.Y + t * (p3.Y - p2.Y);
            schnittpunkte.Add((x, y));
        }
        
        // Kante p3-p1
        if ((p3.H - hoehe) * (p1.H - hoehe) <= 0 && p3.H != p1.H)
        {
            double t = (hoehe - p3.H) / (p1.H - p3.H);
            double x = p3.X + t * (p1.X - p3.X);
            double y = p3.Y + t * (p1.Y - p3.Y);
            schnittpunkte.Add((x, y));
        }
        
        // Maximal 2 Schnittpunkte pro Dreieck
        return schnittpunkte.Take(2).ToList();
    }
    
    private List<Hoehenlinie> VerbindeHoehenlinien(List<Hoehenlinie> segmente)
    {
        if (segmente.Count == 0)
            return new List<Hoehenlinie>();
        
        var verbundene = new List<Hoehenlinie>();
        var verwendet = new HashSet<int>();
        
        for (int i = 0; i < segmente.Count; i++)
        {
            if (verwendet.Contains(i)) continue;
            
            var aktuelleLinie = new Hoehenlinie(segmente[i].Hoehe, segmente[i].Typ);
            aktuelleLinie.Punkte.AddRange(segmente[i].Punkte);
            verwendet.Add(i);
            
            // Verbundene Segmente suchen
            bool gefunden;
            do
            {
                gefunden = false;
                double lastX = aktuelleLinie.Punkte[^1].X;
                double lastY = aktuelleLinie.Punkte[^1].Y;
                
                for (int j = 0; j < segmente.Count; j++)
                {
                    if (verwendet.Contains(j)) continue;
                    
                    double firstX = segmente[j].Punkte[0].X;
                    double firstY = segmente[j].Punkte[0].Y;
                    double lastXj = segmente[j].Punkte[^1].X;
                    double lastYj = segmente[j].Punkte[^1].Y;
                    
                    // Toleranz für Verbindung (0.01m)
                    double tol = 0.01;
                    
                    if (Math.Abs(lastX - firstX) < tol && Math.Abs(lastY - firstY) < tol)
                    {
                        // Anhängen (ab Punkt 1, da 0 identisch)
                        for (int k = 1; k < segmente[j].Punkte.Count; k++)
                        {
                            aktuelleLinie.Punkte.Add(segmente[j].Punkte[k]);
                        }
                        verwendet.Add(j);
                        gefunden = true;
                    }
                    else if (Math.Abs(lastX - lastXj) < tol && Math.Abs(lastY - lastYj) < tol)
                    {
                        // Umgekehrt anhängen
                        for (int k = segmente[j].Punkte.Count - 2; k >= 0; k--)
                        {
                            aktuelleLinie.Punkte.Add(segmente[j].Punkte[k]);
                        }
                        verwendet.Add(j);
                        gefunden = true;
                    }
                }
            } while (gefunden);
            
            // Prüfen ob geschlossen
            if (aktuelleLinie.Punkte.Count > 2)
            {
                double firstX = aktuelleLinie.Punkte[0].X;
                double lastX = aktuelleLinie.Punkte[^1].X;
                double firstY = aktuelleLinie.Punkte[0].Y;
                double lastY = aktuelleLinie.Punkte[^1].Y;
                
                if (Math.Abs(firstX - lastX) < 0.01 && Math.Abs(firstY - lastY) < 0.01)
                {
                    aktuelleLinie.AnfangsCode = 3;
                    aktuelleLinie.EndeCode = 3;
                }
            }
            
            // Nur Linien mit mind. 2 Punkten
            if (aktuelleLinie.Punkte.Count >= 2)
            {
                verbundene.Add(aktuelleLinie);
            }
        }
        
        return verbundene;
    }
    
    #endregion
}
