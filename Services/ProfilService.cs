using DGM_NT.Models;

namespace DGM_NT.Services;

/// <summary>
/// Service für Profilberechnung (Querprofile, Längsprofile, Schnittprofile)
/// Basierend auf dem Delphi-Code (profil.pas, oprofca2.pas)
/// </summary>
public class ProfilService
{
    /// <summary>
    /// Berechnet ein Querprofil entlang einer Linie
    /// </summary>
    public Profil BerechneQuerprofil(List<Punkt> punkte, List<Dreieck> dreiecke, 
        double startX, double startY, double endeX, double endeY, double maxAbstand)
    {
        var profil = new Profil
        {
            Name = "Querprofil",
            Typ = 1,
            AnfangX = startX,
            AnfangY = startY,
            EndeX = endeX,
            EndeY = endeY,
            MaxAbstand = maxAbstand
        };
        
        // Profilpunkte berechnen
        profil.Punkte = InterpoliereProfil(punkte, dreiecke, startX, startY, endeX, endeY, maxAbstand);
        
        // Station berechnen
        BerechneStationen(profil);
        
        return profil;
    }
    
    /// <summary>
    /// Berechnet ein Längsprofil entlang einer Trasse
    /// </summary>
    public Profil BerechneLaengsprofil(List<Punkt> punkte, List<Dreieck> dreiecke, 
        List<(double x, double y)> trassenPunkte, double maxAbstand)
    {
        var profil = new Profil
        {
            Name = "Längsprofil",
            Typ = 2,
            MaxAbstand = maxAbstand
        };
        
        // Entlang der Trassenpunkte interpolieren
        var allePunkte = new List<ProfilPunkt>();
        double gesamtStation = 0;
        
        for (int i = 0; i < trassenPunkte.Count - 1; i++)
        {
            var p1 = trassenPunkte[i];
            var p2 = trassenPunkte[i + 1];
            
            var segmentPunkte = InterpoliereProfil(punkte, dreiecke, p1.x, p1.y, p2.x, p2.y, maxAbstand);
            
            foreach (var pp in segmentPunkte)
            {
                pp.Station += gesamtStation;
                allePunkte.Add(pp);
            }
            
            double dx = p2.x - p1.x;
            double dy = p2.y - p1.y;
            gesamtStation += Math.Sqrt(dx * dx + dy * dy);
        }
        
        profil.Punkte = allePunkte;
        return profil;
    }
    
    /// <summary>
    /// Berechnet ein Schnittprofil (Schnitt durch das Gelände)
    /// </summary>
    public Profil BerechneSchnittprofil(List<Punkt> punkte, List<Dreieck> dreiecke, 
        double startX, double startY, double endeX, double endeY, int anzahlPunkte)
    {
        var profil = new Profil
        {
            Name = "Schnittprofil",
            Typ = 3,
            AnfangX = startX,
            AnfangY = startY,
            EndeX = endeX,
            EndeY = endeY
        };
        
        double dx = (endeX - startX) / (anzahlPunkte - 1);
        double dy = (endeY - startY) / (anzahlPunkte - 1);
        
        for (int i = 0; i < anzahlPunkte; i++)
        {
            double x = startX + i * dx;
            double y = startY + i * dy;
            double h = InterpoliereHoehe(punkte, dreiecke, x, y);
            
            var pp = new ProfilPunkt
            {
                Station = i * Math.Sqrt(dx * dx + dy * dy),
                H = h,
                X = x,
                Y = y
            };
            
            profil.Punkte.Add(pp);
        }
        
        return profil;
    }
    
    #region Private Methoden
    
    private List<ProfilPunkt> InterpoliereProfil(List<Punkt> punkte, List<Dreieck> dreiecke, 
        double startX, double startY, double endeX, double endeY, double maxAbstand)
    {
        var profilPunkte = new List<ProfilPunkt>();
        
        double gesamtLaenge = Math.Sqrt((endeX - startX) * (endeX - startX) + (endeY - startY) * (endeY - startY));
        int anzahl = Math.Max(2, (int)Math.Ceiling(gesamtLaenge / maxAbstand) + 1);
        
        double dx = (endeX - startX) / (anzahl - 1);
        double dy = (endeY - startY) / (anzahl - 1);
        
        for (int i = 0; i < anzahl; i++)
        {
            double x = startX + i * dx;
            double y = startY + i * dy;
            double h = InterpoliereHoehe(punkte, dreiecke, x, y);
            
            profilPunkte.Add(new ProfilPunkt
            {
                Station = i * Math.Sqrt(dx * dx + dy * dy),
                H = h,
                X = x,
                Y = y
            });
        }
        
        return profilPunkte;
    }
    
    /// <summary>
    /// Interpoliert Höhe an einer Position aus dem TIN
    /// </summary>
    private double InterpoliereHoehe(List<Punkt> punkte, List<Dreieck> dreiecke, double x, double y)
    {
        // Enthaltendes Dreieck finden
        foreach (var dreieck in dreiecke)
        {
            if (dreieck.Code == 0) continue;
            
            var p1 = punkte[dreieck.P1];
            var p2 = punkte[dreieck.P2];
            var p3 = punkte[dreieck.P3];
            
            if (IsPointInTriangle(x, y, p1, p2, p3))
            {
                // Baryzentrische Interpolation
                return InterpolateZ(x, y, p1, p2, p3);
            }
        }
        
        // Kein Dreieck gefunden - nächstgelegenen Punkt verwenden
        double minDist = double.MaxValue;
        double naechsteH = 0;
        
        foreach (var p in punkte)
        {
            double dist = (p.X - x) * (p.X - x) + (p.Y - y) * (p.Y - y);
            if (dist < minDist)
            {
                minDist = dist;
                naechsteH = p.H;
            }
        }
        
        return naechsteH;
    }
    
    private bool IsPointInTriangle(double x, double y, Punkt p1, Punkt p2, Punkt p3)
    {
        double d1 = Sign(x, y, p1, p2);
        double d2 = Sign(x, y, p2, p3);
        double d3 = Sign(x, y, p3, p1);
        
        bool hasNeg = (d1 < 0) || (d2 < 0) || (d3 < 0);
        bool hasPos = (d1 > 0) || (d2 > 0) || (d3 > 0);
        
        return !(hasNeg && hasPos);
    }
    
    private double Sign(double px, double py, Punkt p1, Punkt p2)
    {
        return (px - p2.X) * (p1.Y - p2.Y) - (p1.X - p2.X) * (py - p2.Y);
    }
    
    private double InterpolateZ(double x, double y, Punkt p1, Punkt p2, Punkt p3)
    {
        // Ebene durch 3 Punkte: ax + by + cz + d = 0
        double ax = p2.X - p1.X;
        double ay = p2.Y - p1.Y;
        double az = p2.H - p1.H;
        double bx = p3.X - p1.X;
        double by = p3.Y - p1.Y;
        double bz = p3.H - p1.H;
        
        // Normalenvektor
        double nx = ay * bz - az * by;
        double ny = az * bx - ax * bz;
        double nz = ax * by - ay * bx;
        
        // z = -(nx*(x-x1) + ny*(y-y1)) / nz + z1
        if (Math.Abs(nz) < 1e-10)
            return (p1.H + p2.H + p3.H) / 3; // Horizontales Dreieck
        
        return p1.H - (nx * (x - p1.X) + ny * (y - p1.Y)) / nz;
    }
    
    private void BerechneStationen(Profil profil)
    {
        double station = 0;
        profil.Punkte[0].Station = 0;
        
        for (int i = 1; i < profil.Punkte.Count; i++)
        {
            var prev = profil.Punkte[i - 1];
            var curr = profil.Punkte[i];
            
            double dx = curr.X - prev.X;
            double dy = curr.Y - prev.Y;
            station += Math.Sqrt(dx * dx + dy * dy);
            curr.Station = station;
            
            // Steigung berechnen
            if (i < profil.Punkte.Count - 1)
            {
                var next = profil.Punkte[i + 1];
                double dh = next.H - curr.H;
                double dist = Math.Sqrt((next.X - curr.X) * (next.X - curr.X) + (next.Y - curr.Y) * (next.Y - curr.Y));
                curr.Steigung = dist > 0 ? (dh / dist) * 100 : 0; // in Prozent
            }
        }
    }
    
    #endregion
}
