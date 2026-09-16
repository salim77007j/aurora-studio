using System;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Input;
using Avalonia.Layout;
using Avalonia.Media;
using Avalonia.Media.Imaging;
using AuroraStudio.Interop;
using AuroraStudio.Models;
using AuroraStudio.Views;

namespace AuroraStudio.Panels;

/// <summary>
/// Navigator panel: live document thumbnail + viewport rectangle.
/// Click / drag inside the thumbnail to center the real canvas there.
/// </summary>
public class NavigatorPanel : Border
{
    private readonly MainWindow _win;
    private readonly NavigatorView _view = new();

    public NavigatorPanel(MainWindow win)
    {
        _win = win;
        _view.Win = win;
        var root = new StackPanel { Spacing = 4 };
        root.Children.Add(ColorPanel.PanelHeader("Navigator"));
        root.Children.Add(_view);

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
        try { _view.RefreshThumb(doc); }
        catch (Exception ex) { Program.WriteCrash("NAVIGATOR", ex); }
    }

    private class NavigatorView : Control
    {
        public MainWindow? Win;
        private WriteableBitmap? _thumb;
        private bool _dragging;
        private DateTime _lastPoll = DateTime.MinValue;

        public NavigatorView()
        {
            Height = 110;
            MinWidth = 120;
            ClipToBounds = true;
            Cursor = new Cursor(StandardCursorType.Hand);
        }

        public void RefreshThumb(AuroraDocument? doc)
        {
            // recompute at most every 250 ms
            if ((DateTime.UtcNow - _lastPoll).TotalMilliseconds < 250) return;
            _lastPoll = DateTime.UtcNow;
            try
            {
                _thumb = null;
                if (doc != null && Win?.ActiveDoc != null)
                {
                    // read a downscaled composite directly from the engine
                    int tw = 160, th = 110;
                    var doc2 = Win.ActiveDoc;
                    double scale = Math.Min((double)tw / Math.Max(1, doc2.Width), (double)th / Math.Max(1, doc2.Height));
                    int w = Math.Max(1, (int)(doc2.Width * scale));
                    int h = Math.Max(1, (int)(doc2.Height * scale));
                    var buf = new byte[w * h * 4];
                    // engine-side nearest-sample: read the full composite is too heavy; use
                    // aurora_composite_read on a scaled region? — read full composite once (it is cached)
                    int dw = (int)doc2.Width, dh = (int)doc2.Height;
                    var full = new byte[dw * dh * 4];
                    unsafe
                    {
                        fixed (byte* p = full)
                        {
                            if (Engine.aurora_composite_read(doc2.Handle, 0, 0, (uint)dw, (uint)dh, p, (uint)full.Length) == 0)
                            {
                                for (int y = 0; y < h; y++)
                                {
                                    int sy = Math.Min(dh - 1, (int)(y / scale));
                                    for (int x = 0; x < w; x++)
                                    {
                                        int sx = Math.Min(dw - 1, (int)(x / scale));
                                        int so = (sy * dw + sx) * 4;
                                        int to = (y * w + x) * 4;
                                        buf[to] = full[so];
                                        buf[to + 1] = full[so + 1];
                                        buf[to + 2] = full[so + 2];
                                        buf[to + 3] = full[so + 3];
                                    }
                                }
                                var wb = new WriteableBitmap(new PixelSize(w, h), new Vector(96, 96),
                                    Avalonia.Platform.PixelFormat.Rgba8888, Avalonia.Platform.AlphaFormat.Unpremul);
                                using (var frame = wb.Lock())
                                {
                                    unsafe
                                    {
                                        fixed (byte* sp = buf)
                                        {
                                            for (int y2 = 0; y2 < h; y2++)
                                                Buffer.MemoryCopy(sp + (long)y2 * w * 4, (byte*)frame.Address + (long)y2 * frame.RowBytes, w * 4, w * 4);
                                        }
                                    }
                                }
                                _thumb = wb;
                            }
                        }
                    }
                }
                InvalidateVisual();
            }
            catch (Exception ex)
            {
                Program.WriteCrash("NAVIGATOR", ex);
            }
        }

        protected override void OnPointerPressed(PointerPressedEventArgs e)
        {
            _dragging = true;
            e.Pointer.Capture(this);
            JumpTo(e.GetCurrentPoint(this).Position);
        }

        protected override void OnPointerMoved(PointerEventArgs e)
        {
            if (_dragging) JumpTo(e.GetCurrentPoint(this).Position);
        }

        protected override void OnPointerReleased(PointerReleasedEventArgs e)
        {
            _dragging = false;
        }

        private void JumpTo(Point p)
        {
            try
            {
                if (Win == null || _thumb == null) return;
                var doc = Win.ActiveDoc;
                if (doc == null) return;
                double docX = p.X / Bounds.Width * doc.Width;
                double docY = p.Y / Bounds.Height * doc.Height;
                Win.TheCanvas.CenterOnDocumentPoint(docX, docY);
            }
            catch (Exception ex)
            {
                Program.WriteCrash("NAVIGATOR", ex);
            }
        }

        public override void Render(DrawingContext ctx)
        {
            var r = new Rect(Bounds.Size);
            ctx.FillRectangle(new SolidColorBrush(Color.FromRgb(0x1C, 0x1E, 0x22)), r);
            if (_thumb is WriteableBitmap tb)
            {
                double sw = tb.PixelSize.Width, sh = tb.PixelSize.Height;
                double scale = Math.Min(r.Width / sw, r.Height / sh);
                var dest = new Rect((r.Width - sw * scale) / 2, (r.Height - sh * scale) / 2, sw * scale, sh * scale);
                ctx.DrawImage(tb, dest);
                // viewport rect
                try
                {
                    var win = Win;
                    var doc = win?.ActiveDoc;
                    if (win != null && doc != null)
                    {
                        var vis = win.TheCanvas.VisibleDocRect();
                        if (vis.Width > 0 && vis.Height > 0)
                        {
                            var k = new Rect(dest.X + vis.X / doc.Width * dest.Width,
                                             dest.Y + vis.Y / doc.Height * dest.Height,
                                             vis.Width / doc.Width * dest.Width,
                                             vis.Height / doc.Height * dest.Height);
                            ctx.DrawRectangle(null, new Pen(new SolidColorBrush(Color.FromRgb(0x7F, 0xB1, 0xFF)), 1.5), k.Intersect(dest).Deflate(0.5));
                        }
                    }
                }
                catch { /* viewport rect is best-effort */ }
            }
        }
    }
}
