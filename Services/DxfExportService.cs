using System.IO;
using DGM_NT.Models;

namespace DGM_NT.Services;

/// <summary>
/// Service für DXF-Export (AutoCAD-kompatibel)
/// Basierend auf dem Delphi-Code (odxf.pas, cadhl.pas)
/// </summary>
public class DxfExportService
{
    /// <summary>
    /// Exportiert das komplette DGM als DXF-Datei
    /// </summary>
    public void ExportDxf(string dateiPfad, List<Punkt> punkte, List<Dreieck> dreiecke,
        List<Bruchkante> bruchkanten, List<Hoehenlinie> hoeheNlinien, DgmProjekt? projekt = null)
    {
        using var writer = new StreamWriter(dateiPfad);
        
        // DXF Header
        SchreibeHeader(writer);
        SchreibeTables(writer, projekt);
        
        // Entities
        writer.WriteLine("0");
        writer.WriteLine("SECTION");
        writer.WriteLine("2");
        writer.WriteLine("ENTITIES");
        
        // Punkte
        SchreibePunkte(writer, punkte);
        
        // Dreiecke
        SchreibeDreiecke(writer, dreiecke, punkte);
        
        // Bruchkanten
        SchreibeBruchkanten(writer, bruchkanten, punkte);
        
        // Höhenlinien
        SchreibeHoeheNlinien(writer, hoeheNlinien);
        
        // Ende
        writer.WriteLine("0");
        writer.WriteLine("ENDSEC");
        writer.WriteLine("0");
        writer.WriteLine("EOF");
    }
    
    #region Private Methoden
    
    private void SchreibeHeader(StreamWriter writer)
    {
        writer.WriteLine("999");
        writer.WriteLine("DXF created by DGM-NT");
        writer.WriteLine("0");
        writer.WriteLine("SECTION");
        writer.WriteLine("2");
        writer.WriteLine("HEADER");
        writer.WriteLine("9");
        writer.WriteLine("$ACADVER");
        writer.WriteLine("1");
        writer.WriteLine("AC1009");  // AutoCAD R12
        writer.WriteLine("0");
        writer.WriteLine("ENDSEC");
    }
    
    private void SchreibeTables(StreamWriter writer, DgmProjekt? projekt)
    {
        writer.WriteLine("0");
        writer.WriteLine("SECTION");
        writer.WriteLine("2");
        writer.WriteLine("TABLES");
        
        // Layer-Tabelle
        writer.WriteLine("0");
        writer.WriteLine("TABLE");
        writer.WriteLine("2");
        writer.WriteLine("LAYER");
        writer.WriteLine("0");
        writer.WriteLine("ENDTAB");
        
        writer.WriteLine("0");
        writer.WriteLine("ENDSEC");
    }
    
    private void SchreibePunkte(StreamWriter writer, List<Punkt> punkte)
    {
        foreach (var p in punkte)
        {
            // Punkt als Kreis (Radius = 0.1)
            writer.WriteLine("0");
            writer.WriteLine("CIRCLE");
            writer.WriteLine("8");  // Layer
            writer.WriteLine("Punkte");
            writer.WriteLine("10"); // X
            writer.WriteLine(p.X.ToString("F3"));
            writer.WriteLine("20"); // Y
            writer.WriteLine(p.Y.ToString("F3"));
            writer.WriteLine("30"); // Z
            writer.WriteLine(p.H.ToString("F3"));
            writer.WriteLine("40"); // Radius
            writer.WriteLine("0.1");
            
            // Punktnummer als Text
            writer.WriteLine("0");
            writer.WriteLine("TEXT");
            writer.WriteLine("8");
            writer.WriteLine("Punktnr");
            writer.WriteLine("10");
            writer.WriteLine((p.X + 0.5).ToString("F3"));
            writer.WriteLine("20");
            writer.WriteLine((p.Y + 0.5).ToString("F3"));
            writer.WriteLine("30");
            writer.WriteLine(p.H.ToString("F3"));
            writer.WriteLine("40"); // Textgröße
            writer.WriteLine("1.0");
            writer.WriteLine("1");  // Text
            writer.WriteLine(p.PNr);
        }
    }
    
    private void SchreibeDreiecke(StreamWriter writer, List<Dreieck> dreiecke, List<Punkt> punkte)
    {
        foreach (var d in dreiecke)
        {
            if (d.Code == 0) continue;
            
            var p1 = punkte[d.P1];
            var p2 = punkte[d.P2];
            var p3 = punkte[d.P3];
            
            // 3D Face
            writer.WriteLine("0");
            writer.WriteLine("3DFACE");
            writer.WriteLine("8");
            writer.WriteLine("Dreiecke");
            
            writer.WriteLine("10");
            writer.WriteLine(p1.X.ToString("F3"));
            writer.WriteLine("20");
            writer.WriteLine(p1.Y.ToString("F3"));
            writer.WriteLine("30");
            writer.WriteLine(p1.H.ToString("F3"));
            
            writer.WriteLine("11");
            writer.WriteLine(p2.X.ToString("F3"));
            writer.WriteLine("21");
            writer.WriteLine(p2.Y.ToString("F3"));
            writer.WriteLine("31");
            writer.WriteLine(p2.H.ToString("F3"));
            
            writer.WriteLine("12");
            writer.WriteLine(p3.X.ToString("F3"));
            writer.WriteLine("22");
            writer.WriteLine(p3.Y.ToString("F3"));
            writer.WriteLine("32");
            writer.WriteLine(p3.H.ToString("F3"));
            
            writer.WriteLine("13");
            writer.WriteLine(p3.X.ToString("F3"));
            writer.WriteLine("23");
            writer.WriteLine(p3.Y.ToString("F3"));
            writer.WriteLine("33");
            writer.WriteLine(p3.H.ToString("F3"));
        }
    }
    
    private void SchreibeBruchkanten(StreamWriter writer, List<Bruchkante> bruchkanten, List<Punkt> punkte)
    {
        foreach (var bk in bruchkanten)
        {
            if (!bk.Sichtbar || bk.Punkte.Count < 2) continue;
            
            string layer = bk.Typ == 0 ? "Bruchkanten_hart" : "Bruchkanten_weich";
            
            for (int i = 0; i < bk.Punkte.Count - 1; i++)
            {
                var p1 = punkte[bk.Punkte[i]];
                var p2 = punkte[bk.Punkte[i + 1]];
                
                writer.WriteLine("0");
                writer.WriteLine("LINE");
                writer.WriteLine("8");
                writer.WriteLine(layer);
                
                writer.WriteLine("10");
                writer.WriteLine(p1.X.ToString("F3"));
                writer.WriteLine("20");
                writer.WriteLine(p1.Y.ToString("F3"));
                writer.WriteLine("30");
                writer.WriteLine(p1.H.ToString("F3"));
                
                writer.WriteLine("11");
                writer.WriteLine(p2.X.ToString("F3"));
                writer.WriteLine("21");
                writer.WriteLine(p2.Y.ToString("F3"));
                writer.WriteLine("31");
                writer.WriteLine(p2.H.ToString("F3"));
            }
        }
    }
    
    private void SchreibeHoeheNlinien(StreamWriter writer, List<Hoehenlinie> hoeheNlinien)
    {
        foreach (var hl in hoeheNlinien)
        {
            if (hl.Punkte.Count < 2) continue;
            
            string layer = hl.Typ == 1 ? "Zaehllinien" : "Hoehenlinien";
            
            // 3D Polyline
            writer.WriteLine("0");
            writer.WriteLine("POLYLINE");
            writer.WriteLine("8");
            writer.WriteLine(layer);
            writer.WriteLine("66");
            writer.WriteLine("1");
            writer.WriteLine("70");
            writer.WriteLine(hl.Geschlossen ? "1" : "0");
            
            foreach (var pt in hl.Punkte)
            {
                writer.WriteLine("0");
                writer.WriteLine("VERTEX");
                writer.WriteLine("8");
                writer.WriteLine(layer);
                writer.WriteLine("10");
                writer.WriteLine(pt.X.ToString("F3"));
                writer.WriteLine("20");
                writer.WriteLine(pt.Y.ToString("F3"));
                writer.WriteLine("30");
                writer.WriteLine(hl.Hoehe.ToString("F3"));
            }
            
            // SEQEND
            writer.WriteLine("0");
            writer.WriteLine("SEQEND");
            
            // Höhenbeschriftung
            if (hl.Punkte.Count > 0)
            {
                var firstPt = hl.Punkte[0];
                writer.WriteLine("0");
                writer.WriteLine("TEXT");
                writer.WriteLine("8");
                writer.WriteLine("Hl-Schrift");
                writer.WriteLine("10");
                writer.WriteLine((firstPt.X + 0.5).ToString("F3"));
                writer.WriteLine("20");
                writer.WriteLine((firstPt.Y + 0.5).ToString("F3"));
                writer.WriteLine("30");
                writer.WriteLine(hl.Hoehe.ToString("F3"));
                writer.WriteLine("40");
                writer.WriteLine("1.0");
                writer.WriteLine("1");
                writer.WriteLine(hl.Hoehe.ToString("F2"));
            }
        }
    }
    
    #endregion
}
