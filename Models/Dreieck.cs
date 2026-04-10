namespace DGM_NT.Models;

/// <summary>
/// Repräsentiert ein Dreieck im TIN (Triangulated Irregular Network)
/// Entspricht dem 'dreieck' Record im Delphi-Code
/// </summary>
public class Dreieck
{
    /// <summary>Index des 1. Eckpunkts (gegen Uhrzeigersinn)</summary>
    public int P1 { get; set; }
    
    /// <summary>Index des 2. Eckpunkts</summary>
    public int P2 { get; set; }
    
    /// <summary>Index des 3. Eckpunkts</summary>
    public int P3 { get; set; }
    
    /// <summary>Code: 0=ungültig, 1=gültig, >1=Bruchkanten-Code</summary>
    public byte Code { get; set; } = 1;
    
    /// <summary>Normalvektor X (für 3D-Darstellung)</summary>
    public double NormalX { get; set; }
    
    /// <summary>Normalvektor Y</summary>
    public double NormalY { get; set; }
    
    /// <summary>Normalvektor Z</summary>
    public double NormalZ { get; set; }
    
    /// <summary>Neigungswinkel in Grad</summary>
    public double Neigung { get; set; }
    
    /// <summary>Flächeninhalt des Dreiecks</summary>
    public double Flaeche { get; set; }
    
    /// <summary>Schwerepunkt X</summary>
    public double SchwerpunktX { get; set; }
    
    /// <summary>Schwerepunkt Y</summary>
    public double SchwerpunktY { get; set; }
    
    /// <summary>Schwerepunkt H</summary>
    public double SchwerpunktH { get; set; }
    
    public Dreieck() { }
    
    public Dreieck(int p1, int p2, int p3)
    {
        P1 = p1;
        P2 = p2;
        P3 = p3;
        Code = 1;
    }
    
    public override string ToString() => $"Dreieck [{P1}, {P2}, {P3}] Code={Code}";
}
