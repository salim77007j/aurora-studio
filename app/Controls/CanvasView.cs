using System;
using System.Collections.Generic;
using System.Globalization;
using System.Text.Json;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Input;
using Avalonia.Media;
using Avalonia.Media.Imaging;
using Avalonia.Platform;
using Avalonia.Threading;
using Avalonia.Reactive;
using AuroraStudio.Interop;
using AuroraStudio.Models;
using AuroraStudio.Views;

namespace AuroraStudio.Controls;

/// <summary>
/// The interactive document canvas: composite display (GPU-scaled via Skia),
/// zoom/pan, checkerboard, marching-ants selection, and all tool interactions.
/// </summary>
public class CanvasView : Control
{
    private MainWindow _win = null!;
    private WriteableBitmap? _bitmap;
    private byte[] _pixelBuf = Array.Empty<byte>();
    private ulong _lastVersion;
    private IBrush? _checkerBrush;
    private double _antPhase;
    private readonly DispatcherTimer _antTimer;

    // view state
    public double Zoom { get; private set; } = 1.0;
    public Vector Pan { get; private set; }
    private bool _panning;
    private Point _panStart;
    private Vector _panStartPan;
    private bool _spaceDown;

    // tool state
    private bool _drawing;
    private Point _lastDocPoint;
    private float _lastPressure;
    private readonly List<Point> _lassoPts = new();
    private Point _dragStart;
    private Point _dragCur;
    private bool _dragging;
    private Rect? _cropRect;
    private bool _shapeActive;
    private string? _statusExtra;

    public event EventHandler<string>? StatusMessage;

    private bool _fitted;

    public CanvasView()
    {
        Focusable = true;
        ClipToBounds = true;
        // defer the initial zoom-fit until the control has a real size (layout done)
        this.GetObservable(BoundsProperty).Subscribe(new AnonymousObserver<Rect>(b =>
        {
            if (!_fitted && b.Width > 10 && b.Height > 10)
            {
                _fitted = true;
                FitOrActual();
            }
        }));
        _antTimer = new DispatcherTimer { Interval = TimeSpan.FromMilliseconds(80) };
        _antTimer.Tick += (_, _) =>
        {
            _antPhase = (_antPhase + 1) % 8;
            InvalidateVisual();
        };
        _antTimer.Start();
    }

    public void Attach(MainWindow win)
    {
        _win = win;
        _win.ActiveDocChanged += (_, _) => OnDocSwitched();
        _win.DocContentChanged += (_, _) => { /* blit handled by poll loop */ };
    }

    protected override void OnKeyDown(KeyEventArgs e)
    {
        if (e.Key == Key.Space && !_spaceDown)
        {
            _spaceDown = true;
            Cursor = new Cursor(StandardCursorType.SizeAll);
            e.Handled = true;
        }
        base.OnKeyDown(e);
    }

    protected override void OnKeyUp(KeyEventArgs e)
    {
        if (e.Key == Key.Space)
        {
            _spaceDown = false;
            Cursor = Cursor.Default;
            e.Handled = true;
        }
        base.OnKeyUp(e);
    }

    private void OnDocSwitched()
    {
        ResetForDocSwitch();
        InvalidateVisual();
    }

    /// <summary>Drop cached composite + refit view (doc switch or preview-document switch).</summary>
    public void ResetForDocSwitch()
    {
        _bitmap = null;
        _pixelBuf = Array.Empty<byte>();
        _lastVersion = 0;
        _antsCacheDoc = 0;
        _antsCache = new List<List<Point>>();
        _cropRect = null;
        _transformRect = null;
        Zoom = 1.0;
        Pan = new Vector(0, 0);
        FitOrActual();
    }

    // ---------------- coordinate mapping ----------------

    private Point ScreenToDoc(Point p) => new((p.X - Pan.X) / Zoom, (p.Y - Pan.Y) / Zoom);
    private Point DocToScreen(Point p) => new(p.X * Zoom + Pan.X, p.Y * Zoom + Pan.Y);

    public void ZoomAt(double factor, Point centerScreen)
    {
        double nz = Math.Clamp(Zoom * factor, 0.02, 32.0);
        double k = nz / Zoom;
        var docPt = ScreenToDoc(centerScreen);
        Zoom = nz;
        Pan = new Vector(centerScreen.X - docPt.X * Zoom, centerScreen.Y - docPt.Y * Zoom);
        InvalidateVisual();
        _win?.UpdateStatus();
    }

    /// <summary>Pan the view so that the given document-space point sits at the viewport center (Navigator).</summary>
    public void CenterOnDocumentPoint(double docX, double docY)
    {
        double vw = Bounds.Width, vh = Bounds.Height;
        if (vw < 10 || vh < 10) return;
        Pan = new Vector(vw / 2 - docX * Zoom, vh / 2 - docY * Zoom);
        InvalidateVisual();
    }

    /// <summary>The visible portion of the document in doc coordinates (Navigator viewport rect).</summary>
    public Rect VisibleDocRect()
    {
        var tl = ScreenToDoc(new Point(0, 0));
        var br = ScreenToDoc(new Point(Bounds.Width, Bounds.Height));
        return new Rect(tl, br);
    }

    public void FitOrActual(bool actual = false)
    {
        var doc = _win?.ActiveDoc;
        if (doc == null) return;
        double vw = Bounds.Width, vh = Bounds.Height;
        if (vw < 10 || vh < 10) { Zoom = 1.0; Pan = new Vector(10, 10); InvalidateVisual(); return; }
        if (actual)
        {
            Zoom = 1.0;
        }
        else
        {
            Zoom = Math.Min(vw / doc.Width, vh / doc.Height) * 0.92;
        }
        Pan = new Vector((vw - doc.Width * Zoom) / 2, (vh - doc.Height * Zoom) / 2);
        InvalidateVisual();
        _win?.UpdateStatus();
    }

    // ---------------- composite blit ----------------

    public void PollComposite()
    {
        var doc = _win?.ActiveDoc;
        if (doc == null || doc.Handle == 0) return;
        unsafe
        {
            int x = 0, y = 0; uint w = 0, h = 0; ulong version = 0;
            int r;
            int* px = &x; int* py = &y; uint* pw = &w; uint* ph = &h; ulong* pv = &version;
            r = Engine.aurora_poll(doc.Handle, px, py, pw, ph, pv);
            if (version == _lastVersion && r != 1) return;
            _lastVersion = version;
            // no dirty region but version changed (engine composited internally) → full refetch
            if (r != 1) { x = 0; y = 0; w = doc.Width; h = doc.Height; }
            if (w == 0 || h == 0) return;
            EnsureBitmap(doc, doc.Width, doc.Height);
            int need = (int)(w * h * 4);
            if (_pixelBuf.Length < need) _pixelBuf = new byte[need];
            fixed (byte* buf = _pixelBuf)
                Engine.aurora_composite_read(doc.Handle, x, y, w, h, buf, (uint)need);
            if (_bitmap is null) return;
            using var frame = _bitmap.Lock();
            {
                uint stride = (uint)frame.RowBytes;
                for (uint row = 0; row < h; row++)
                {
                    uint dstOff = (uint)(y + row) * stride + (uint)x * 4;
                    uint srcOff = row * w * 4;
                    fixed (byte* src = _pixelBuf)
                    {
                        Buffer.MemoryCopy(src + srcOff, (byte*)frame.Address + dstOff, w * 4, w * 4);
                    }
                }
            }
            InvalidateVisual();
        }
    }

    private void EnsureBitmap(AuroraDocument doc, uint w, uint h)
    {
        if (_bitmap != null)
        {
            var ps = _bitmap.PixelSize;
            if (ps.Width == (int)w && ps.Height == (int)h) return;
            _bitmap.Dispose();
            _bitmap = null;
        }
        _bitmap = new WriteableBitmap(new PixelSize((int)w, (int)h), new Vector(96, 96),
            Avalonia.Platform.PixelFormat.Rgba8888, Avalonia.Platform.AlphaFormat.Unpremul);
        // initialize fully transparent
        using var frame = _bitmap.Lock();
        unsafe
        {
            var span = new Span<byte>((void*)frame.Address, frame.RowBytes * (int)h);
            span.Fill(0);
        }
    }

    // ---------------- rendering ----------------

    private IBrush CheckerBrush()
    {
        if (_checkerBrush != null) return _checkerBrush;
        const int tile = 8;
        var bmp = new RenderTargetBitmap(new PixelSize(tile * 2, tile * 2));
        using (var ctx = bmp.CreateDrawingContext())
        {
            ctx.FillRectangle(Brushes.White, new Rect(0, 0, tile, tile));
            ctx.FillRectangle(Brushes.White, new Rect(tile, tile, tile, tile));
            ctx.FillRectangle(new SolidColorBrush(Color.FromRgb(204, 204, 204)), new Rect(tile, 0, tile, tile));
            ctx.FillRectangle(new SolidColorBrush(Color.FromRgb(204, 204, 204)), new Rect(0, tile, tile, tile));
        }
        _checkerBrush = new ImageBrush(bmp)
        {
            TileMode = TileMode.Tile,
            Stretch = Stretch.None,
            DestinationRect = new RelativeRect(new Rect(0, 0, tile * 2, tile * 2), RelativeUnit.Absolute),
        };
        return _checkerBrush;
    }

    public override void Render(DrawingContext ctx)
    {
        var bg = new SolidColorBrush(Color.FromRgb(0x18, 0x19, 0x1B));
        ctx.FillRectangle(bg, Bounds);
        var doc = _win?.ActiveDoc;
        if (doc == null || _bitmap == null) return;

        var dest = new Rect(Pan.X, Pan.Y, doc.Width * Zoom, doc.Height * Zoom);

        // checkerboard + border
        ctx.FillRectangle(CheckerBrush(), dest);
        // border
        var borderPen = new Pen(new SolidColorBrush(Color.FromRgb(0x3A, 0x3D, 0x44)), 1);
        ctx.DrawRectangle(null, borderPen, dest);

        // image with interpolation mode
        using (ctx.PushRenderOptions(new RenderOptions { BitmapInterpolationMode = Zoom >= 3.0 ? BitmapInterpolationMode.None : BitmapInterpolationMode.MediumQuality }))
        {
            ctx.DrawImage(_bitmap, dest);
        }

        // selection marching ants
        DrawSelectionAnts(ctx, doc);
        // tool overlays
        DrawToolOverlay(ctx, doc);
    }

    private void DrawSelectionAnts(DrawingContext ctx, AuroraDocument doc)
    {
        if (_antsCacheDoc != doc.Handle)
        {
            _antsCacheDoc = doc.Handle;
            _antsCache = ParseContours(Engine.SelectContour(doc.Handle));
        }
        if (_antsCache.Count == 0) return;
        var geo = new StreamGeometry();
        using (var gctx = geo.Open())
        {
            foreach (var loop in _antsCache)
            {
                if (loop.Count < 2) continue;
                var first = DocToScreen(loop[0]);
                gctx.BeginFigure(first, false);
                for (int i = 1; i < loop.Count; i++)
                    gctx.LineTo(DocToScreen(loop[i]));
                gctx.EndFigure(false);
            }
        }
        var black = new Pen(Brushes.Black, 1, new DashStyle(new double[] { 4, 4 }, _antPhase));
        var white = new Pen(Brushes.White, 1, new DashStyle(new double[] { 4, 4 }, _antPhase + 4));
        var geoReal = geo;
        ctx.DrawGeometry(null, white, geoReal);
        ctx.DrawGeometry(null, black, geoReal);
    }

    private ulong _antsCacheDoc;
    private List<List<Point>> _antsCache = new();

    public void InvalidateAnts()
    {
        _antsCacheDoc = 0;
        InvalidateVisual();
    }

    private static List<List<Point>> ParseContours(string json)
    {
        var result = new List<List<Point>>();
        try
        {
            using var jd = JsonDocument.Parse(json);
            foreach (var loop in jd.RootElement.EnumerateArray())
            {
                var pts = new List<Point>();
                var arr = loop;
                int n = arr.GetArrayLength();
                for (int i = 0; i + 1 < n; i += 2)
                    pts.Add(new Point(arr[i].GetDouble(), arr[i + 1].GetDouble()));
                result.Add(pts);
            }
        }
        catch { }
        return result;
    }

    private void DrawToolOverlay(DrawingContext ctx, AuroraDocument doc)
    {
        var accent = new SolidColorBrush(Color.FromRgb(0x4F, 0x8C, 0xFF));
        var pen = new Pen(accent, 1.2);
        var tool = _win.CurrentTool;

        if (tool == ToolKind.RectSelect && _dragging)
            ctx.DrawRectangle(null, pen, NormRect(_dragStart, DocToScreen(_dragCur)));
        else if (tool == ToolKind.EllipseSelect && _dragging)
            ctx.DrawEllipse(null, pen, NormRect(_dragStart, DocToScreen(_dragCur)));
        else if (tool == ToolKind.Lasso && _lassoPts.Count > 1)
        {
            var geo = new StreamGeometry();
            using (var g = geo.Open())
            {
                g.BeginFigure(DocToScreen(_lassoPts[0]), false);
                for (int i = 1; i < _lassoPts.Count; i++)
                    g.LineTo(DocToScreen(_lassoPts[i]));
                g.EndFigure(true);
            }
            ctx.DrawGeometry(null, pen, geo);
        }
        else if (tool == ToolKind.Gradient && _dragging)
        {
            var a = DocToScreen(_dragStart);
            var b = DocToScreen(_dragCur);
            ctx.DrawLine(pen, a, b);
            ctx.DrawEllipse(accent, null, a, 4, 4);
            ctx.DrawEllipse(Brushes.White, pen, b, 4, 4);
        }
        else if (tool == ToolKind.Shape && _dragging)
        {
            var r = NormRect(_dragStart, DocToScreen(_dragCur));
            if (_win.ShapeKind == 0) ctx.DrawRectangle(null, pen, r);
            else if (_win.ShapeKind == 1) ctx.DrawEllipse(null, pen, r);
            else ctx.DrawLine(pen, DocToScreen(_dragStart), DocToScreen(_dragCur));
        }
        else if (tool == ToolKind.Crop)
        {
            var r = _cropRect ?? new Rect(0, 0, doc.Width, doc.Height);
            var tl = DocToScreen(new Point(r.X, r.Y));
            var br = DocToScreen(new Point(r.Right, r.Bottom));
            var sr = new Rect(tl, br);
            // dim outside (four rects)
            var dim = new SolidColorBrush(Color.FromArgb(110, 0, 0, 0));
            ctx.FillRectangle(dim, new Rect(0, 0, Bounds.Width, Math.Max(0, sr.Top)));
            ctx.FillRectangle(dim, new Rect(0, sr.Bottom, Bounds.Width, Math.Max(0, Bounds.Height - sr.Bottom)));
            ctx.FillRectangle(dim, new Rect(0, sr.Top, Math.Max(0, sr.Left), Math.Max(0, sr.Height)));
            ctx.FillRectangle(dim, new Rect(sr.Right, sr.Top, Math.Max(0, Bounds.Width - sr.Right), Math.Max(0, sr.Height)));
            ctx.DrawRectangle(null, new Pen(Brushes.White, 1.2), sr);
            // thirds
            var thirdPen = new Pen(new SolidColorBrush(Color.FromArgb(140, 255, 255, 255)), 0.8, new DashStyle(new double[] { 4, 4 }, 0));
            for (int i = 1; i <= 2; i++)
            {
                double x = sr.X + sr.Width * i / 3;
                double y = sr.Y + sr.Height * i / 3;
                ctx.DrawLine(thirdPen, new Point(x, sr.Y), new Point(x, sr.Bottom));
                ctx.DrawLine(thirdPen, new Point(sr.X, y), new Point(sr.Right, y));
            }
            // handles
            foreach (var hp in HandlePoints(sr))
                ctx.DrawRectangle(Brushes.White, new Pen(accent, 1), new Rect(hp.X - 4, hp.Y - 4, 8, 8));
        }
        else if (tool == ToolKind.Transform && _transformRect.HasValue)
        {
            var r = _transformRect.Value;
            var sr = RectBetween(DocToScreen(new Point(r.X, r.Y)), DocToScreen(new Point(r.Right, r.Bottom)));
            ctx.DrawRectangle(null, new Pen(Brushes.White, 1.2, new DashStyle(new double[] { 4, 3 }, 0)), sr);
            foreach (var hp in HandlePoints(sr))
                ctx.DrawRectangle(accent, new Pen(Brushes.White, 1), new Rect(hp.X - 4, hp.Y - 4, 8, 8));
        }
    }

    private static Rect NormRect(Point a, Point b) => RectBetween(a, b);
    private static Rect RectBetween(Point a, Point b) =>
        new Rect(new Point(Math.Min(a.X, b.X), Math.Min(a.Y, b.Y)),
                 new Point(Math.Max(a.X, b.X), Math.Max(a.Y, b.Y)));

    private static IEnumerable<Point> HandlePoints(Rect r)
    {
        yield return r.TopLeft; yield return new Point(r.Center.X, r.Top); yield return r.TopRight;
        yield return new Point(r.Left, r.Center.Y); yield return new Point(r.Right, r.Center.Y);
        yield return r.BottomLeft; yield return new Point(r.Center.X, r.Bottom); yield return r.BottomRight;
    }

    // ---------------- input ----------------

    protected override void OnPointerPressed(PointerPressedEventArgs e)
    {
        try { OnPointerPressedCore(e); }
        catch (Exception ex) { Program.WriteCrash("CANVAS-PRESS", ex); }
    }

    private void OnPointerPressedCore(PointerPressedEventArgs e)
    {
        Focus();
        var doc = _win.ActiveDoc;
        if (doc == null) return;
        var pt = e.GetCurrentPoint(this);
        var screen = pt.Position;
        var props = pt.Properties;

        if (pt.Properties.IsMiddleButtonPressed || (_spaceDown && pt.Properties.IsLeftButtonPressed))
        {
            _panning = true;
            _panStart = screen;
            _panStartPan = Pan;
            e.Pointer.Capture(this);
            e.Handled = true;
            return;
        }

        var tool = _win.CurrentTool;

        // hand tool: drag to pan
        if (tool == ToolKind.Hand)
        {
            _panning = true;
            _panStart = screen;
            _panStartPan = Pan;
            e.Pointer.Capture(this);
            e.Handled = true;
            return;
        }

        // zoom tool: click = in, alt/right-click = out
        if (tool == ToolKind.Zoom)
        {
            bool zoomOut = e.KeyModifiers.HasFlag(KeyModifiers.Alt) || pt.Properties.IsRightButtonPressed;
            ZoomAt(zoomOut ? 1 / 1.5 : 1.5, screen);
            e.Handled = true;
            return;
        }

        if (!pt.Properties.IsLeftButtonPressed) return;

        var dp = ScreenToDoc(screen);
        e.Pointer.Capture(this);
        _drawing = true;
        _dragStart = dp;
        _dragCur = dp;
        _dragging = true;

        switch (tool)
        {
            case ToolKind.Brush:
            case ToolKind.Pencil:
            case ToolKind.Eraser:
            {
                _lastDocPoint = dp;
                _lastPressure = GetPressure(e);
                var bp = _win.MakeBrushParams(tool);
                unsafe
                {
                    Engine.aurora_brush_begin(doc.Handle, Engine.ActiveLayer(doc.Handle), &bp, (float)dp.X, (float)dp.Y, _lastPressure);
                }
                PollComposite();
                break;
            }
            case ToolKind.RectSelect:
            case ToolKind.EllipseSelect:
            case ToolKind.Lasso:
                _lassoPts.Clear();
                _lassoPts.Add(dp);
                break;
            case ToolKind.Wand:
            {
                int mode = SelectMode(e);
                Engine.aurora_select_wand(doc.Handle, (int)dp.X, (int)dp.Y, _win.WandTolerance, _win.WandContiguous ? 1 : 0, 0, mode);
                doc.RefreshState();
                InvalidateAnts();
                _win.RefreshPanels();
                break;
            }
            case ToolKind.Eyedropper:
                PickColor(dp);
                break;
            case ToolKind.Bucket:
            {
                var c = _win.ForegroundColor;
                Engine.aurora_bucket(doc.Handle, (int)dp.X, (int)dp.Y, c.R, c.G, c.B, c.A, _win.BucketTolerance, _win.BucketContiguous ? 1 : 0);
                doc.RefreshState();
                _win.RefreshPanels();
                break;
            }
            case ToolKind.Gradient:
            case ToolKind.Shape:
            case ToolKind.Transform:
                _shapeActive = true;
                break;
            case ToolKind.Crop:
                _cropRect = null; // will set on drag
                break;
            case ToolKind.Text:
                _win.ShowTextEditorAt(screen, dp);
                _drawing = false;
                break;
        }
        InvalidateVisual();
        e.Handled = true;
    }

    protected override void OnPointerMoved(PointerEventArgs e)
    {
        try { OnPointerMovedCore(e); }
        catch (Exception ex) { Program.WriteCrash("CANVAS-MOVE", ex); }
    }

    private void OnPointerMovedCore(PointerEventArgs e)
    {
        var doc = _win.ActiveDoc;
        if (doc == null) return;
        var pt = e.GetCurrentPoint(this);
        var screen = pt.Position;
        var dp = ScreenToDoc(screen);

        _win.SetCursorDocPos(dp);

        if (_panning)
        {
            Pan = _panStartPan + (screen - _panStart);
            InvalidateVisual();
            return;
        }
        if (!_drawing) return;

        var tool = _win.CurrentTool;
        switch (tool)
        {
            case ToolKind.Brush:
            case ToolKind.Pencil:
            case ToolKind.Eraser:
            {
                _lastPressure = GetPressure(e);
                Engine.aurora_brush_move(doc.Handle, (float)dp.X, (float)dp.Y, _lastPressure);
                PollComposite();
                _lastDocPoint = dp;
                break;
            }
            case ToolKind.Lasso:
                _lassoPts.Add(dp);
                InvalidateVisual();
                break;
            case ToolKind.RectSelect:
            case ToolKind.EllipseSelect:
            case ToolKind.Gradient:
            case ToolKind.Shape:
                _dragCur = dp;
                InvalidateVisual();
                break;
            case ToolKind.Crop:
                if (_dragging && _cropRect == null && _dragStart != dp)
                    _cropRect = NormRect(_dragStart, dp);
                InvalidateVisual();
                break;
            case ToolKind.Transform:
                InvalidateVisual();
                break;
        }
        e.Handled = true;
    }

    protected override void OnPointerReleased(PointerReleasedEventArgs e)
    {
        try { OnPointerReleasedCore(e); }
        catch (Exception ex) { Program.WriteCrash("CANVAS-RELEASE", ex); }
    }

    private void OnPointerReleasedCore(PointerReleasedEventArgs e)
    {
        var doc = _win.ActiveDoc;
        _panning = false;
        if (doc == null || !_drawing) { base.OnPointerReleased(e); return; }
        var dp = ScreenToDoc(e.GetCurrentPoint(this).Position);
        var tool = _win.CurrentTool;
        _drawing = false;
        _dragging = false;

        switch (tool)
        {
            case ToolKind.Brush:
            case ToolKind.Pencil:
            case ToolKind.Eraser:
                Engine.aurora_brush_end(doc.Handle);
                doc.RefreshState();
                _win.RefreshPanels();
                break;
            case ToolKind.RectSelect:
            {
                var r = NormRect(_dragStart, dp);
                if (r.Width >= 1 && r.Height >= 1)
                    Engine.aurora_select_rect(doc.Handle, (int)r.X, (int)r.Y, (int)r.Width, (int)r.Height, SelectMode(e));
                doc.RefreshState();
                InvalidateAnts();
                _win.RefreshPanels();
                break;
            }
            case ToolKind.EllipseSelect:
            {
                var r = NormRect(_dragStart, dp);
                if (r.Width >= 1 && r.Height >= 1)
                    Engine.aurora_select_ellipse(doc.Handle, (int)r.X, (int)r.Y, (int)r.Width, (int)r.Height, SelectMode(e));
                doc.RefreshState();
                InvalidateAnts();
                _win.RefreshPanels();
                break;
            }
            case ToolKind.Lasso:
            {
                _lassoPts.Add(dp);
                if (_lassoPts.Count >= 3)
                {
                    var flat = new float[_lassoPts.Count * 2];
                    for (int i = 0; i < _lassoPts.Count; i++)
                    {
                        flat[i * 2] = (float)_lassoPts[i].X;
                        flat[i * 2 + 1] = (float)_lassoPts[i].Y;
                    }
                    unsafe
                    {
                        fixed (float* fp = flat)
                            Engine.aurora_select_lasso(doc.Handle, fp, _lassoPts.Count, SelectMode(e));
                    }
                }
                _lassoPts.Clear();
                doc.RefreshState();
                InvalidateAnts();
                _win.RefreshPanels();
                break;
            }
            case ToolKind.Gradient:
            {
                var c = _win.ForegroundColor;
                var bg = _win.BackgroundColor;
                unsafe
                {
                    Engine.aurora_gradient(doc.Handle, (float)_dragStart.X, (float)_dragStart.Y, (float)dp.X, (float)dp.Y,
                        _win.GradientRadial ? 1 : 0, _win.GradientTransparent ? 1 : 0,
                        c.R, c.G, c.B, c.A, bg.R, bg.G, bg.B, bg.A, 1);
                }
                doc.RefreshState();
                _win.RefreshPanels();
                break;
            }
            case ToolKind.Shape:
            {
                _win.CommitShape(_dragStart, dp);
                doc.RefreshState();
                break;
            }
            case ToolKind.Crop:
            {
                if (_cropRect == null && NormRect(_dragStart, dp).Width > 4)
                    _cropRect = NormRect(_dragStart, dp);
                break;
            }
        }
        InvalidateVisual();
        e.Handled = true;
        base.OnPointerReleased(e);
    }

    protected override void OnPointerWheelChanged(PointerWheelEventArgs e)
    {
        try { OnPointerWheelChangedCore(e); }
        catch (Exception ex) { Program.WriteCrash("CANVAS-WHEEL", ex); }
    }

    private void OnPointerWheelChangedCore(PointerWheelEventArgs e)
    {
        if (e.KeyModifiers.HasFlag(KeyModifiers.Shift))
        {
            Pan = new Vector(Pan.X + e.Delta.Y * 30, Pan.Y);
            InvalidateVisual();
        }
        else
        {
            var factor = Math.Pow(1.15, e.Delta.Y);
            ZoomAt(factor, e.GetCurrentPoint(this).Position);
        }
        e.Handled = true;
    }

    private int SelectMode(PointerEventArgs e)
    {
        if (e.KeyModifiers.HasFlag(KeyModifiers.Shift)) return 1;   // add
        if (e.KeyModifiers.HasFlag(KeyModifiers.Alt)) return 2;     // subtract
        return 0;
    }

    private static float GetPressure(PointerEventArgs e)
    {
        // Windows Ink pen pressure; mouse/trackpad fallback = 0.5 (neutral)
        var p = e.GetCurrentPoint(null);
        try
        {
            var pr = p.Properties.Pressure;
            if (pr is > 0.0f and <= 1.0f && p.Pointer.Type == PointerType.Pen)
                return (float)pr;
        }
        catch { }
        return 0.5f;
    }

    private void PickColor(Point dp)
    {
        var doc = _win.ActiveDoc;
        if (doc == null) return;
        unsafe
        {
            var buf = new byte[4];
            fixed (byte* b = buf)
            {
                Engine.aurora_composite_read(doc.Handle, (int)dp.X, (int)dp.Y, 1, 1, b, 4);
            }
            _win.SetForegroundColor(Color.FromArgb(buf[3], buf[0], buf[1], buf[2]));
        }
    }

    // ---------------- crop & transform API ----------------

    public Rect? CropRect => _cropRect;
    public void SetCropRect(Rect? r) { _cropRect = r; InvalidateVisual(); }
    public void CommitCrop()
    {
        var doc = _win.ActiveDoc;
        if (doc == null || _cropRect == null) return;
        var r = _cropRect.Value;
        Engine.aurora_doc_crop(doc.Handle, (int)r.X, (int)r.Y, (uint)r.Width, (uint)r.Height);
        _cropRect = null;
        doc.RefreshState();
        _win.OnImageStructureChanged();
        _win.RefreshPanels();
    }

    private Rect? _transformRect;
    public void BeginTransform()
    {
        var doc = _win.ActiveDoc;
        if (doc == null) return;
        if (Engine.SelBounds(doc.Handle).Active)
        {
            var b = Engine.SelBounds(doc.Handle);
            _transformRect = new Rect(b.X, b.Y, b.W, b.H);
        }
        else
        {
            _transformRect = new Rect(0, 0, doc.Width, doc.Height);
        }
        InvalidateVisual();
    }

    public void CommitTransformScale(double sx, double sy, double rotateDeg)
    {
        var doc = _win.ActiveDoc;
        if (doc == null || _transformRect == null) return;
        var r = _transformRect.Value;
        // affine: translate to center, rotate, scale, translate back
        double cx = r.X + r.Width / 2.0, cy = r.Y + r.Height / 2.0;
        double rad = rotateDeg * Math.PI / 180.0;
        double cos = Math.Cos(rad), sin = Math.Sin(rad);
        // forward: p' = R*S*(p - c) + c
        double a = cos * sx, b = -sin * sy, tx = cx - a * cx - b * cy + 0;
        double c = sin * sx, d = cos * sy;
        tx = cx - (a * cx + b * cy);
        double ty = cy - (c * cx + d * cy);
        float[] m = { (float)a, (float)b, (float)tx, (float)c, (float)d, (float)ty };
        unsafe
        {
            fixed (float* mp = m)
                Engine.aurora_layer_warp(doc.Handle, Engine.ActiveLayer(doc.Handle), 0, mp, doc.Width, doc.Height);
        }
        _transformRect = null;
        doc.RefreshState();
        _win.OnImageStructureChanged();
        _win.RefreshPanels();
    }

    public void CancelTransform()
    {
        _transformRect = null;
        InvalidateVisual();
    }

    protected override void OnTextInput(TextInputEventArgs e)
    {
        // typing goes to text overlay, not the canvas
        base.OnTextInput(e);
    }
}
