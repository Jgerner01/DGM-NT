using DGM_NT.Models;

namespace DGM_NT.Services;

/// <summary>
/// Service für Volumenberechnung
/// Basierend auf dem Delphi-Code (volumen.pas, volumen1.pas)
/// </summary>
public class VolumenService
{
    /// <summary>
    /// Berechnet Volumen zwischen DGM und einer Bezugshöhe
    /// </summary>
    public VolumenErgebnis BerechneVolumenEbene(List<Punkt> punkte, List<Dreieck> dreiecke, 
        double bezugsHoehe, double auflockerung = 1.0, double dichte = 1.5)
    {
        var ergebnis = new VolumenErgebnis
        {
            GrundHoehe = bezugsHoehe,
            Auflockerung = auflockerung,
            Dichte = dichte,
            Typ = 1,
            Datum = DateTime.Now,
            PunkteZahl = punkte.Count,
            DreieckeZahl = dreiecke.Count
        };
        
        double einschnitt = 0;  // Abtrag (über Bezugshöhe)
        double aufschuettung = 0; // Auftrag (unter Bezugshöhe)
        double gesamtFlaeche = 0;
        double minH = double.MaxValue;
        double maxH = double.MinValue;
        
        foreach (var dreieck in dreiecke)
        {
            if (dreieck.Code == 0) continue;
            
            var p1 = punkte[dreieck.P1];
            var p2 = punkte[dreieck.P2];
            var p3 = punkte[dreieck.P3];
            
            // Volumen des Prismas berechnen
            var (volumen, flaeche) = BerechneDreieckVolumen(p1, p2, p3, bezugsHoehe);
            
            if (volumen > 0)
                einschnitt += volumen;
            else
                aufschuettung += Math.Abs(volumen);
            
            gesamtFlaeche += flaeche;
            
            // Min/Max Höhe tracken
            foreach (var p in new[] { p1, p2, p3 })
            {
                if (p.H < minH) minH = p.H;
                if (p.H > maxH) maxH = p.H;
            }
        }
        
        ergebnis.Einschnitt = einschnitt;
        ergebnis.Aufschuettung = aufschuettung;
        ergebnis.Grundflaeche = gesamtFlaeche;
        ergebnis.MinHoehe = minH;
        ergebnis.MaxHoehe = maxH;
        ergebnis.DurchschnittHoehe = gesamtFlaeche > 0 ? (einschnitt - aufschuettung) / gesamtFlaeche : 0;
        
        // Auflockerung berücksichtigen
        ergebnis.Einschnitt *= auflockerung;
        
        return ergebnis;
    }
    
    /// <summary>
    /// Berechnet Volumen zwischen zwei DGMs
    /// </summary>
    public VolumenErgebnis BerechneVolumenDgmGegenDgm(List<Punkt> punkteAlt, List<Dreieck> dreieckeAlt,
        List<Punkt> punkteNeu, List<Dreieck> dreieckeNeu)
    {
        // Vereinfacht: Differenz der Volumina gegen gleiche Bezugshöhe
        // In der Praxis würde man ein gemeinsames Gitter verwenden
        var ergebnis = new VolumenErgebnis
        {
            Typ = 2,
            Datum = DateTime.Now
        };
        
        // TODO: Vollständige Implementierung mit Gittervergleich
        return ergebnis;
    }
    
    /// <summary>
    /// Berechnet Volumen einer Baugrube
    /// </summary>
    public VolumenErgebnis BerechneBaugrubenVolumen(List<Punkt> punkte, List<Dreieck> dreiecke, 
        List<(double x, double y, double h)> baugrubePunkte, double auflockerung = 1.0)
    {
        var ergebnis = new VolumenErgebnis
        {
            Name = "Baugrube",
            Typ = 1,
            Auflockerung = auflockerung,
            Datum = DateTime.Now
        };
        
        // Baugruben-Umring erstellen
        var umring = baugrubePunkte.Select(p => (p.x, p.y)).ToList();
        
        // Nur Dreiecke innerhalb der Baugrube betrachten
        double einschnitt = 0;
        double aufschuettung = 0;
        double gesamtFlaeche = 0;
        
        foreach (var dreieck in dreiecke)
        {
            if (dreieck.Code == 0) continue;
            
            var p1 = punkte[dreieck.P1];
            var p2 = punkte[dreieck.P2];
            var p3 = punkte[dreieck.P3];
            
            // Prüfen ob Dreieck in Baugrube liegt
            if (!IstDreieckInBaugrube(p1, p2, p3, umring))
                continue;
            
            // Durchschnittliche Baugrubenhöhe
            double baugrubenH = baugrubePunkte.Average(p => p.h);
            
            var (volumen, flaeche) = BerechneDreieckVolumen(p1, p2, p3, baugrubenH);
            
            if (volumen > 0)
                einschnitt += volumen;
            else
                aufschuettung += Math.Abs(volumen);
            
            gesamtFlaeche += flaeche;
        }
        
        ergebnis.Einschnitt = einschnitt * auflockerung;
        ergebnis.Aufschuettung = aufschuettung;
        ergebnis.Grundflaeche = gesamtFlaeche;
        ergebnis.PunkteZahl = punkte.Count;
        ergebnis.DreieckeZahl = dreiecke.Count;
        
        return ergebnis;
    }
    
    #region Private Methoden
    
    private (double volumen, double flaeche) BerechneDreieckVolumen(Punkt p1, Punkt p2, Punkt p3, double bezugsH)
    {
        // Flächeninhalt des Dreiecks (2D)
        double flaeche = 0.5 * Math.Abs(
            (p2.X - p1.X) * (p3.Y - p1.Y) - (p3.X - p1.X) * (p2.Y - p1.Y));
        
        // Durchschnittliche Höhe über Bezug
        double h1 = p1.H - bezugsH;
        double h2 = p2.H - bezugsH;
        double h3 = p3.H - bezugsH;
        double durchschnH = (h1 + h2 + h3) / 3.0;
        
        // Volumen = Fläche * durchschnittliche Höhe
        double volumen = flaeche * durchschnH;
        
        return (volumen, flaeche);
    }
    
    private bool IstDreieckInBaugrube(Punkt p1, Punkt p2, Punkt p3, List<(double x, double y)> umring)
    {
        // Schwerpunkt des Dreiecks
        double sx = (p1.X + p2.X + p3.X) / 3;
        double sy = (p1.Y + p2.Y + p3.Y) / 3;
        
        return IsPointInPolygon(sx, sy, umring);
    }
    
    private bool IsPointInPolygon(double px, double py, List<(double x, double y)> polygon)
    {
        bool inside = false;
        int n = polygon.Count;
        
        for (int i = 0, j = n - 1; i < n; j = i++)
        {
            double xi = polygon[i].x, yi = polygon[i].y;
            double xj = polygon[j].x, yj = polygon[j].y;
            
            bool intersect = ((yi > py) != (yj > py)) && (px < (xj - xi) * (py - yi) / (yj - yi) + xi);
            if (intersect) inside = !inside;
        }
        
        return inside;
    }
    
    #endregion
}
