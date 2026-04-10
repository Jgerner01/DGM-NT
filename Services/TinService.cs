using DGM_NT.Models;

namespace DGM_NT.Services;

public class TinService
{
    public List<Dreieck> DelaunayTriangulation(List<Punkt> punkte, double suchradius = 0, double toleranz = 0.0001, double minFlaeche = 1e-10, bool degenerierteEntfernen = true)
    {
        if (punkte.Count < 3) throw new ArgumentException("Mindestens 3 Punkte erforderlich");
        
        System.Diagnostics.Debug.WriteLine($"[TIN] Start mit {punkte.Count} Punkten, sm={suchradius}, tol={toleranz}");

        int n = punkte.Count;
        double minX = punkte.Min(p => p.X), minY = punkte.Min(p => p.Y);
        double maxX = punkte.Max(p => p.X), maxY = punkte.Max(p => p.Y);
        double dmax = Math.Max(maxX - minX, maxY - minY);
        double xMid = (minX + maxX) / 2, yMid = (minY + maxY) / 2;
        double offset = dmax * 100;

        // Super-Dreieck
        var superP1 = new Punkt { PNr = "__S1", X = xMid - offset, Y = yMid - offset, H = 0 };
        var superP2 = new Punkt { PNr = "__S2", X = xMid + 2 * offset, Y = yMid - offset, H = 0 };
        var superP3 = new Punkt { PNr = "__S3", X = xMid + offset, Y = yMid + 2 * offset, H = 0 };
        int s1 = n, s2 = n + 1, s3 = n + 2;
        punkte.Add(superP1); punkte.Add(superP2); punkte.Add(superP3);

        var dreiecke = new List<Dreieck> { new Dreieck(s1, s2, s3) };

        // Bowyer-Watson
        for (int i = 0; i < n; i++)
        {
            InsertPoint(dreiecke, punkte, i, suchradius, toleranz);
            if (dreiecke.Count == 0)
            {
                System.Diagnostics.Debug.WriteLine($"[TIN] FEHLER: Keine Dreiecke mehr nach Punkt {i}!");
                break;
            }
        }

        System.Diagnostics.Debug.WriteLine($"[TIN] Vor Super-Remove: {dreiecke.Count} Dreiecke");
        dreiecke.RemoveAll(d => d.P1 >= n || d.P2 >= n || d.P3 >= n);
        punkte.RemoveRange(n, 3);
        System.Diagnostics.Debug.WriteLine($"[TIN] Nach Super-Remove: {dreiecke.Count} Dreiecke");

        // Degenerierte entfernen
        if (degenerierteEntfernen)
        {
            int before = dreiecke.Count;
            dreiecke.RemoveAll(d =>
            {
                var p1 = punkte[d.P1]; var p2 = punkte[d.P2]; var p3 = punkte[d.P3];
                double area = Math.Abs((p2.X - p1.X) * (p3.Y - p1.Y) - (p3.X - p1.X) * (p2.Y - p1.Y));
                return area < minFlaeche;
            });
            if (before != dreiecke.Count) System.Diagnostics.Debug.WriteLine($"[TIN] {before - dreiecke.Count} degenerierte entfernt");
        }

        foreach (var d in dreiecke) BerechneDreieckParameter(d, punkte);
        System.Diagnostics.Debug.WriteLine($"[TIN] Ergebnis: {dreiecke.Count} Dreiecke");
        return dreiecke;
    }

    public List<Dreieck> DelaunayTriangulation(List<Punkt> punkte) => DelaunayTriangulation(punkte, 0, 0.0001, 1e-10, true);

    public List<Bruchkante> ErkennungBruchkanten(List<Punkt> punkte, List<Dreieck> dreiecke)
    {
        var bruchkanten = new List<Bruchkante>();
        var edgeTris = new Dictionary<(int, int), List<int>>();

        foreach (var d in dreiecke)
        {
            AddEdgeTri(edgeTris, d.P1, d.P2, dreiecke.IndexOf(d));
            AddEdgeTri(edgeTris, d.P2, d.P3, dreiecke.IndexOf(d));
            AddEdgeTri(edgeTris, d.P3, d.P1, dreiecke.IndexOf(d));
        }

        int bkId = 0;
        foreach (var kvp in edgeTris)
        {
            if (kvp.Value.Count != 2) continue; // Nur interne Kanten
            var tris = kvp.Value;
            double n1 = dreiecke[tris[0]].Neigung, n2 = dreiecke[tris[1]].Neigung;
            if (Math.Abs(n1 - n2) > 15.0)
            {
                var bk = new Bruchkante(++bkId);
                bk.Punkte.Add(kvp.Key.Item1); bk.Punkte.Add(kvp.Key.Item2);
                bruchkanten.Add(bk);
            }
        }
        return bruchkanten;
    }

    // ═══ Bowyer-Watson mit korrekter Kantenorientierung ═══

    private void InsertPoint(List<Dreieck> dreiecke, List<Punkt> punkte, int idx, double suchradius, double toleranz)
    {
        var bad = new List<Dreieck>();
        foreach (var tri in dreiecke)
        {
            if (IsPointInCircumcircle(punkte[idx], punkte[tri.P1], punkte[tri.P2], punkte[tri.P3], toleranz))
                bad.Add(tri);
        }

        if (bad.Count == 0) return;

        // Alle Kanten der schlechten Dreiecke sammeln
        var allEdges = new List<(int a, int b)>();
        foreach (var tri in bad)
        {
            allEdges.Add((tri.P1, tri.P2));
            allEdges.Add((tri.P2, tri.P3));
            allEdges.Add((tri.P3, tri.P1));
        }

        // Boundary-Kanten finden (nur einmal vorhanden)
        var boundary = new List<(int a, int b)>();
        foreach (var edge in allEdges)
        {
            bool shared = false;
            foreach (var other in allEdges)
            {
                if (edge == other) continue;
                if (edge.a == other.b && edge.b == other.a) { shared = true; break; }
            }
            if (!shared) boundary.Add(edge);
        }

        // Schlechte Dreiecke entfernen
        foreach (var tri in bad) dreiecke.Remove(tri);

        // Neue Dreiecke mit korrekter Orientierung erstellen
        // Suchradius prüfen: Nur Dreiecke erstellen wenn Kantenlänge <= suchradius
        foreach (var edge in boundary)
        {
            if (suchradius > 0)
            {
                var pa = punkte[edge.a]; var pb = punkte[edge.b]; var pc = punkte[idx];
                double dab = Abstand(pa, pb), dac = Abstand(pa, pc), dbc = Abstand(pb, pc);
                double sm2 = suchradius * suchradius;
                if (dab * dab > 4 * sm2 || dac * dac > 4 * sm2 || dbc * dbc > 4 * sm2)
                    continue; // Zu lange Kante – Delphi-Logik: if s>4*sm2 then skip
            }
            dreiecke.Add(new Dreieck(edge.a, edge.b, idx));
        }
    }

    // ═══ Kantenlängen-Filter ═══

    private void UnterteileNachKantenlaenge(List<Dreieck> dreiecke, List<Punkt> punkte, double maxLen)
    {
        bool changed; int iter = 0;
        do
        {
            changed = false; iter++;
            var neue = new List<Dreieck>();
            foreach (var d in dreiecke)
            {
                var p1 = punkte[d.P1]; var p2 = punkte[d.P2]; var p3 = punkte[d.P3];
                double maxL = Math.Max(Abstand(p1, p2), Math.Max(Abstand(p2, p3), Abstand(p3, p1)));
                if (maxL > maxLen)
                {
                    var m12 = InterpolierePunkt(p1, p2, 0.5);
                    var m23 = InterpolierePunkt(p2, p3, 0.5);
                    var m31 = InterpolierePunkt(p3, p1, 0.5);
                    int i1 = AddPunktWennFehlt(punkte, m12);
                    int i2 = AddPunktWennFehlt(punkte, m23);
                    int i3 = AddPunktWennFehlt(punkte, m31);
                    neue.Add(new Dreieck(d.P1, i1, i3));
                    neue.Add(new Dreieck(i1, d.P2, i2));
                    neue.Add(new Dreieck(i3, i2, d.P3));
                    neue.Add(new Dreieck(i1, i2, i3));
                    changed = true;
                }
                else neue.Add(d);
            }
            dreiecke.Clear(); dreiecke.AddRange(neue);
        } while (changed && iter < 10);
    }

    // ═══ Winkelfilter ═══

    private void VerfeinereNachWinkel(List<Dreieck> dreiecke, List<Punkt> punkte, double minWinkelGrad)
    {
        bool changed; int iter = 0;
        do
        {
            changed = false; iter++;
            var neue = new List<Dreieck>();
            foreach (var d in dreiecke)
            {
                var p1 = punkte[d.P1]; var p2 = punkte[d.P2]; var p3 = punkte[d.P3];
                double winkel = BerechneMinWinkel(p1, p2, p3);
                if (winkel < minWinkelGrad)
                {
                    var umkreis = BerechneUmkreismittelpunkt(p1, p2, p3);
                    int idx = AddPunktWennFehlt(punkte, umkreis);
                    neue.Add(new Dreieck(d.P1, d.P2, idx));
                    neue.Add(new Dreieck(d.P2, d.P3, idx));
                    neue.Add(new Dreieck(d.P3, d.P1, idx));
                    changed = true;
                }
                else neue.Add(d);
            }
            dreiecke.Clear(); dreiecke.AddRange(neue);
        } while (changed && iter < 15);
    }

    // ═══ Helper ═══

    private Punkt InterpolierePunkt(Punkt p1, Punkt p2, double t) =>
        new Punkt { PNr = "SUB", X = p1.X + (p2.X - p1.X) * t, Y = p1.Y + (p2.Y - p1.Y) * t, H = p1.H + (p2.H - p1.H) * t };

    private int AddPunktWennFehlt(List<Punkt> punkte, Punkt p)
    {
        int idx = punkte.FindIndex(e => Math.Abs(e.X - p.X) < 0.001 && Math.Abs(e.Y - p.Y) < 0.001);
        if (idx >= 0) return idx;
        punkte.Add(p); return punkte.Count - 1;
    }

    private double BerechneMinWinkel(Punkt p1, Punkt p2, Punkt p3)
    {
        double a = Abstand(p2, p3), b = Abstand(p1, p3), c = Abstand(p1, p2);
        if (a == 0 || b == 0 || c == 0) return 0;
        double cosA = Math.Max(-1, Math.Min(1, (b * b + c * c - a * a) / (2 * b * c)));
        double cosB = Math.Max(-1, Math.Min(1, (a * a + c * c - b * b) / (2 * a * c)));
        double cosC = Math.Max(-1, Math.Min(1, (a * a + b * b - c * c) / (2 * a * b)));
        return Math.Min(Math.Acos(cosA), Math.Min(Math.Acos(cosB), Math.Acos(cosC))) * 180 / Math.PI;
    }

    private Punkt BerechneUmkreismittelpunkt(Punkt p1, Punkt p2, Punkt p3)
    {
        double d = 2 * (p1.X * (p2.Y - p3.Y) + p2.X * (p3.Y - p1.Y) + p3.X * (p1.Y - p2.Y));
        if (Math.Abs(d) < 1e-10) return new Punkt { X = (p1.X + p2.X + p3.X) / 3, Y = (p1.Y + p2.Y + p3.Y) / 3, H = (p1.H + p2.H + p3.H) / 3 };
        double u1 = p1.X * p1.X + p1.Y * p1.Y;
        double u2 = p2.X * p2.X + p2.Y * p2.Y;
        double u3 = p3.X * p3.X + p3.Y * p3.Y;
        double ux = (u1 * (p2.Y - p3.Y) + u2 * (p3.Y - p1.Y) + u3 * (p1.Y - p2.Y)) / d;
        double uy = (u1 * (p3.X - p2.X) + u2 * (p1.X - p3.X) + u3 * (p2.X - p1.X)) / d;
        double h = InterpoliereHoehe(p1, p2, p3, ux, uy);
        return new Punkt { PNr = "CIRC", X = ux, Y = uy, H = h };
    }

    private double InterpoliereHoehe(Punkt p1, Punkt p2, Punkt p3, double x, double y)
    {
        double denom = (p2.Y - p3.Y) * (p1.X - p3.X) + (p3.X - p2.X) * (p1.Y - p3.Y);
        if (Math.Abs(denom) < 1e-10) return (p1.H + p2.H + p3.H) / 3;
        double a = ((p2.Y - p3.Y) * (x - p3.X) + (p3.X - p2.X) * (y - p3.Y)) / denom;
        double b = ((p3.Y - p1.Y) * (x - p3.X) + (p1.X - p3.X) * (y - p3.Y)) / denom;
        return a * p1.H + b * p2.H + (1 - a - b) * p3.H;
    }

    private double Abstand(Punkt p1, Punkt p2)
    {
        double dx = p2.X - p1.X, dy = p2.Y - p1.Y;
        return Math.Sqrt(dx * dx + dy * dy);
    }

    private bool IsPointInCircumcircle(Punkt p, Punkt p1, Punkt p2, Punkt p3, double toleranz)
    {
        double ax = p1.X - p.X, ay = p1.Y - p.Y;
        double bx = p2.X - p.X, by = p2.Y - p.Y;
        double cx = p3.X - p.X, cy = p3.Y - p.Y;
        double det = (ax * ax + ay * ay) * (bx * cy - cx * by)
                   - (bx * bx + by * by) * (ax * cy - cx * ay)
                   + (cx * cx + cy * cy) * (ax * by - bx * ay);
        double orient = ax * (by - cy) + bx * (cy - ay) + cx * (ay - by);
        // Toleranz für numerische Stabilität (wie Delphi: tol = 0.0001)
        return (orient > 0 && det > toleranz) || (orient < 0 && det < -toleranz);
    }

    private void BerechneDreieckParameter(Dreieck d, List<Punkt> punkte)
    {
        var p1 = punkte[d.P1]; var p2 = punkte[d.P2]; var p3 = punkte[d.P3];
        double ax = p2.X - p1.X, ay = p2.Y - p1.Y, az = p2.H - p1.H;
        double bx = p3.X - p1.X, by = p3.Y - p1.Y, bz = p3.H - p1.H;
        d.NormalX = ay * bz - az * by; d.NormalY = az * bx - ax * bz; d.NormalZ = ax * by - ay * bx;
        double len = Math.Sqrt(d.NormalX * d.NormalX + d.NormalY * d.NormalY + d.NormalZ * d.NormalZ);
        if (len > 0) { d.NormalX /= len; d.NormalY /= len; d.NormalZ /= len; }
        d.Neigung = Math.Acos(Math.Abs(d.NormalZ)) * 180.0 / Math.PI;
        d.Flaeche = 0.5 * Math.Abs((p2.X - p1.X) * (p3.Y - p1.Y) - (p3.X - p1.X) * (p2.Y - p1.Y));
        d.SchwerpunktX = (p1.X + p2.X + p3.X) / 3; d.SchwerpunktY = (p1.Y + p2.Y + p3.Y) / 3; d.SchwerpunktH = (p1.H + p2.H + p3.H) / 3;
    }

    private void AddEdgeTri(Dictionary<(int, int), List<int>> dict, int a, int b, int triIdx)
    {
        var key = a < b ? (a, b) : (b, a);
        if (!dict.TryGetValue(key, out var list)) { list = new List<int>(); dict[key] = list; }
        list.Add(triIdx);
    }
}
