using System.Windows;
using System.Windows.Media;

namespace DGM_NT;

/// <summary>
/// Custom Drawing Panel – zwei DrawingVisuals:
///   _backgroundVisual : weißer Hintergrund (billig, immer neu)
///   _contentVisual    : Geometrie in Weltkoordinaten (gecacht)
///                       Pan/Zoom = nur MatrixTransform, KEIN Neuzeichnen
/// </summary>
public class DgmDrawPanel : FrameworkElement
{
    private readonly DrawingVisual _backgroundVisual = new();
    private readonly DrawingVisual _contentVisual    = new();
    private readonly VisualCollection _visuals;

    public DgmDrawPanel()
    {
        _visuals = new VisualCollection(this);
        _visuals.Add(_backgroundVisual);
        _visuals.Add(_contentVisual);
    }

    protected override int VisualChildrenCount => _visuals.Count;
    protected override Visual GetVisualChild(int index) => _visuals[index];

    /// <summary>Hintergrund neu zeichnen (immer billig).</summary>
    public void RenderBackground(Action<DrawingContext> draw)
    {
        using var dc = _backgroundVisual.RenderOpen();
        draw(dc);
    }

    /// <summary>Geometrie-Cache neu aufbauen (nur bei Datenänderung oder großem Zoom).</summary>
    public void RenderContent(Action<DrawingContext> draw)
    {
        using var dc = _contentVisual.RenderOpen();
        draw(dc);
    }

    /// <summary>
    /// Pan/Zoom als reine Matrix-Transformation – null CPU, GPU-Transform.
    /// Weltkoordinaten → Bildschirmkoordinaten:
    ///   sx = wx * Scale  + offsetX
    ///   sy = wy * -Scale + offsetY
    /// </summary>
    public void SetContentTransform(Matrix matrix)
    {
        _contentVisual.Transform = new MatrixTransform(matrix);
    }

    protected override void OnRenderSizeChanged(SizeChangedInfo sizeInfo)
    {
        base.OnRenderSizeChanged(sizeInfo);
    }
}
