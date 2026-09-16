using System;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Layout;
using Avalonia.Media;
using AuroraStudio.Interop;
using AuroraStudio.Models;
using AuroraStudio.Views;

namespace AuroraStudio.Panels;

/// <summary>
/// Live luminance histogram of the composited image (engine-computed, real data).
/// Refreshes on document changes; updates at most every 200 ms to stay responsive.
/// </summary>
public class HistogramPanel : Border
{
    private readonly MainWindow _win;
    private readonly HistogramControl _hist = new();
    private DateTime _lastRefresh = DateTime.MinValue;

    public HistogramPanel(MainWindow win)
    {
        _win = win;
        var root = new StackPanel { Spacing = 4 };
        root.Children.Add(ColorPanel.PanelHeader("Histogram"));
        root.Children.Add(_hist);

        Child = new Border
        {
            Background = ColorPanel.Brush(0x26, 0x28, 0x2D),
            CornerRadius = new CornerRadius(8),
            Padding = new Thickness(10),
            Child = root,
        };
    }

    public void Refresh(AuroraDocument? doc)
    {
        // throttle: histogram reads the full composite (expensive on large docs)
        if ((DateTime.UtcNow - _lastRefresh).TotalMilliseconds < 200) return;
        _lastRefresh = DateTime.UtcNow;
        try
        {
            var bins = doc == null ? null : Engine.Histogram(doc.Handle);
            _hist.SetData(bins);
        }
        catch (Exception ex)
        {
            Program.WriteCrash("HISTOGRAM", ex);
        }
    }

    /// <summary>Custom-drawn 256-bin histogram (dark UI, gradient bars, grid).</summary>
    private class HistogramControl : Control
    {
        private byte[]? _bins;
        private static readonly IBrush BarBrush = new LinearGradientBrush
        {
            StartPoint = new RelativePoint(0, 1, RelativeUnit.Relative),
            EndPoint = new RelativePoint(0, 0, RelativeUnit.Relative),
            GradientStops =
            {
                new GradientStop { Color = Color.FromRgb(0x5B, 0x8D, 0xFF), Offset = 0 },
                new GradientStop { Color = Color.FromRgb(0x9F, 0xC5, 0xFF), Offset = 1 },
            },
        };

        public void SetData(byte[]? bins)
        {
            _bins = bins;
            InvalidateVisual();
        }

        public HistogramControl()
        {
            Height = 74;
            MinWidth = 120;
        }

        public override void Render(DrawingContext ctx)
        {
            var r = new Rect(Bounds.Size);
            // background + grid
            ctx.FillRectangle(new SolidColorBrush(Color.FromRgb(0x1C, 0x1E, 0x22)), r);
            var gridPen = new Pen(new SolidColorBrush(Color.FromRgb(0x30, 0x33, 0x39)), 1);
            for (int i = 1; i < 4; i++)
            {
                double x = r.Width * i / 4.0;
                ctx.DrawLine(gridPen, new Point(x, 0), new Point(x, r.Height));
            }
            if (_bins == null || _bins.Length != 256) return;
            var clip = r.Deflate(1);
            using (ctx.PushClip(clip))
            {
                double bw = clip.Width / 256.0;
                for (int i = 0; i < 256; i++)
                {
                    double h = _bins[i] / 255.0 * (clip.Height - 2);
                    if (h < 0.5) continue;
                    ctx.FillRectangle(BarBrush, new Rect(clip.X + i * bw, clip.Bottom - h, Math.Max(1.0, bw), h));
                }
            }
        }
    }
}
