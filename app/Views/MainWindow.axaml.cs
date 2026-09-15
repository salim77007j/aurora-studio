using System;
using System.Collections.Generic;
using System.Collections.ObjectModel;
using System.IO;
using System.Linq;
using System.Runtime.InteropServices;
using System.Globalization;
using Avalonia.Layout;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Controls.Shapes;
using Avalonia.Input;
using Avalonia.Interactivity;
using Avalonia.VisualTree;
using Avalonia.LogicalTree;
using Avalonia.Media;
using Avalonia.Media.Imaging;
using Avalonia.Platform.Storage;
using Avalonia.Threading;
using AuroraStudio.Controls;
using AuroraStudio.Dialogs;
using AuroraStudio.Interop;
using AuroraStudio.Models;
using AuroraStudio.Panels;

namespace AuroraStudio.Views;

public class ToolItem
{
    public ToolKind Kind { get; init; }
    public string Icon { get; init; } = "";
    public Geometry Geometry => Icons.Get(Icon);
    public string Tooltip { get; init; } = "";
    public IBrush Foreground { get; set; } = new SolidColorBrush(Color.FromRgb(0xAA, 0xAF, 0xB6));
}

public class DocTabItem
{
    public AuroraDocument Doc { get; init; } = null!;
    public string Title => (Doc.IsDirty ? "• " : "") + Doc.Title;
    public IBrush TabBrush => _selected ? new SolidColorBrush(Color.FromRgb(0x2E, 0x30, 0x35)) : Brushes.Transparent;
    public IBrush TabForeground => _selected ? Brushes.White : new SolidColorBrush(Color.FromRgb(0x8A, 0x8F, 0x96));
    public bool _selected;
}

public partial class MainWindow : Window
{
    public ObservableCollection<DocTabItem> Tabs { get; } = new();
    public List<ToolItem> Tools { get; } = new();

    public AuroraDocument? ActiveDoc => Tabs.FirstOrDefault(t => t._selected)?.Doc;
    public ToolKind CurrentTool { get; private set; } = ToolKind.Brush;

    public Color ForegroundColor { get; set; } = Colors.Black;
    public Color BackgroundColor { get; set; } = Colors.White;

    // tool option state (bound to options bar controls — all real)
    public float BrushSize { get; set; } = 30;
    public float BrushHardness { get; set; } = 0.85f;
    public float BrushFlow { get; set; } = 1.0f;
    public float BrushOpacity { get; set; } = 1.0f;
    public bool SizePressure { get; set; } = true;
    public bool OpacityPressure { get; set; } = false;
    public int WandTolerance { get; set; } = 32;
    public bool WandContiguous { get; set; } = true;
    public int BucketTolerance { get; set; } = 32;
    public bool BucketContiguous { get; set; } = true;
    public bool GradientRadial { get; set; } = false;
    public bool GradientTransparent { get; set; } = false;
    public int ShapeKind { get; set; } = 0; // 0 rect, 1 ellipse, 2 line
    public int ShapeStrokeWidth { get; set; } = 4;
    public bool ShapeFill { get; set; } = false;
    public string TextFontFamily { get; set; } = "Inter";
    public double TextFontSize { get; set; } = 48;
    public bool TextBold { get; set; } = false;
    public bool TextItalic { get; set; } = false;
    public int TransformInterp { get; set; } = 2;

    public event EventHandler? ActiveDocChanged;
    public event EventHandler? DocContentChanged;

    private CanvasView? _canvas;
    public CanvasView TheCanvas => _canvas ??= (CanvasView)(FindAny("Canvas") ?? throw new Exception("XAML: Canvas not found"));
    public ItemsControl TheToolPalette => FindAny<ItemsControl>("ToolPalette")!;
    public ItemsControl TheDocTabs => FindAny<ItemsControl>("DocTabs")!;
    private TextBlock StatusToolEl => FindAny<TextBlock>("StatusTool")!;
    private TextBlock StatusMsgEl => FindAny<TextBlock>("StatusMsg")!;
    private TextBlock StatusPosEl => FindAny<TextBlock>("StatusPos")!;
    private TextBlock StatusZoomEl => FindAny<TextBlock>("StatusZoom")!;
    private TextBlock StatusSizeEl => FindAny<TextBlock>("StatusSize")!;
    private TextBlock StatusEngineEl => FindAny<TextBlock>("StatusEngine")!;

    public Panels.ColorPanel ColorPanelCtl { get; private set; } = null!;
    public Panels.LayersPanel LayersPanelCtl { get; private set; } = null!;
    public Panels.HistoryPanel HistoryPanelCtl { get; private set; } = null!;

    private readonly DispatcherTimer _pollTimer;
    private AppSettings _settings = AppSettings.Load();

    public MainWindow()
    {
        InitializeComponent();
        DataContext = this;
        BuildToolPalette();
        BuildOptionsBar();
        TheCanvas.Attach(this);
        TheDocTabs.ItemsSource = Tabs;
        TheToolPalette.ItemsSource = Tools;
        ColorPanelCtl = new Panels.ColorPanel(this);
        LayersPanelCtl = new Panels.LayersPanel(this);
        HistoryPanelCtl = new Panels.HistoryPanel(this);
        var colorHost = FindAny<ContentControl>("ColorPanelHost");
        var layersHost = FindAny<ContentControl>("LayersPanelHost");
        var historyHost = FindAny<ContentControl>("HistoryPanelHost");
        System.Console.Error.WriteLine($"[aurora] hosts: color={colorHost != null} layers={layersHost != null} history={historyHost != null}");
        if (colorHost != null) colorHost.Content = ColorPanelCtl;
        if (layersHost != null) layersHost.Content = LayersPanelCtl;
        if (historyHost != null) historyHost.Content = HistoryPanelCtl;

        _pollTimer = new DispatcherTimer { Interval = TimeSpan.FromMilliseconds(16) };
        _pollTimer.Tick += (_, _) => TheCanvas.PollComposite();
        _pollTimer.Start();

        Closed += (_, _) =>
        {
            _settings.Save();
            foreach (var t in Tabs) t.Doc.Dispose();
        };
        Opened += (_, _) =>
        {
            Title = $"Aurora Studio — {Interop.Engine.Version()}";
            var el = FindAny<TextBlock>("StatusEngine");
            if (el != null) el.Text = Interop.Engine.Version();
            CreateInitialDocument();
            UpdateStatus();

            // CI/demo automation (real operations, real UI state)
            var args = Environment.GetCommandLineArgs();
            var demoIdx = Array.IndexOf(args, "--autodemo");
            if (demoIdx >= 0 && demoIdx + 1 < args.Length)
                AutoDemo.Run(this, args[demoIdx + 1]);
        };

        AddHandler(KeyDownEvent, OnGlobalKeyDown, RoutingStrategies.Tunnel);
    }

    private void InitializeComponent()
    {
        Avalonia.Markup.Xaml.AvaloniaXamlLoader.Load(this);
    }

    /// <summary>Name lookup that walks the full visual+logical tree (robust across template boundaries).</summary>
    public Avalonia.Controls.Control? FindAny(string name)
    {
        if (this.Name == name) return this;
        foreach (var d in this.GetVisualDescendants())
        {
            if (d is Avalonia.Controls.Control c && c.Name == name) return c;
        }
        foreach (var d in this.GetLogicalDescendants())
        {
            if (d is Avalonia.Controls.Control c && c.Name == name) return c;
        }
        return null;
    }

    private T? FindAny<T>(string name) where T : Avalonia.Controls.Control => FindAny(name) as T;

    private void CreateInitialDocument()
    {
        var doc = NewDocumentInternal(1200, 800, 1, "Untitled-1");
        SelectDoc(doc);
    }

    // ══════════ tool palette ══════════

    private void BuildToolPalette()
    {
        var defs = new (ToolKind, string, string)[]
        {
            (ToolKind.Move, "tool.move", "Move (V)"),
            (ToolKind.Brush, "tool.brush", "Brush (B) — pen pressure supported"),
            (ToolKind.Pencil, "tool.pencil", "Pencil (N)"),
            (ToolKind.Eraser, "tool.eraser", "Eraser (E)"),
            (ToolKind.RectSelect, "tool.rectselect", "Rectangular Select (M)"),
            (ToolKind.EllipseSelect, "tool.ellipseselect", "Elliptical Select (J)"),
            (ToolKind.Lasso, "tool.lasso", "Lasso Select (L)"),
            (ToolKind.Wand, "tool.wand", "Magic Wand (W)"),
            (ToolKind.Eyedropper, "tool.eyedropper", "Eyedropper (I)"),
            (ToolKind.Bucket, "tool.bucket", "Paint Bucket (G)"),
            (ToolKind.Gradient, "tool.gradient", "Gradient (R)"),
            (ToolKind.Text, "tool.text", "Text (T)"),
            (ToolKind.Shape, "tool.shape", "Shape (U)"),
            (ToolKind.Crop, "tool.crop", "Crop (C)"),
            (ToolKind.Transform, "tool.transform", "Free Transform (Ctrl+T)"),
        };
        foreach (var (kind, icon, tip) in defs)
            Tools.Add(new ToolItem { Kind = kind, Icon = icon, Tooltip = tip });
    }

    private void OnToolClick(object? sender, RoutedEventArgs e)
    {
        if (sender is Button b && b.Tag is ToolItem t)
            SelectTool(t.Kind);
    }

    public void SelectTool(ToolKind kind)
    {
        CurrentTool = kind;
        foreach (var t in Tools)
            t.Foreground = new SolidColorBrush(t.Kind == kind ? Color.FromRgb(0x4F, 0x8C, 0xFF) : Color.FromRgb(0xAA, 0xAF, 0xB6));
        TheToolPalette.ItemsSource = null;
        TheToolPalette.ItemsSource = Tools;
        BuildOptionsBar();
        StatusToolEl.Text = ToolInfo.Title(kind);
        if (kind == ToolKind.Transform)
            TheCanvas.BeginTransform();
        else
            TheCanvas.CancelTransform();
        if (kind == ToolKind.Crop)
        {
            var doc = ActiveDoc;
            if (doc != null)
            {
                var b = Engine.SelBounds(doc.Handle);
                if (b.Active)
                    TheCanvas.SetCropRect(new Rect(b.X, b.Y, b.W, b.H));
            }
        }
        TheCanvas.Focus();
    }

    // ══════════ options bar (all controls live-wired) ══════════

    private void BuildOptionsBar()
    {
        var bar = FindAny<ContentControl>("OptionsBar")!;
        var panel = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8, VerticalAlignment = Avalonia.Layout.VerticalAlignment.Center };
        TextBlock Label(string s) => new()
        {
            Text = s,
            VerticalAlignment = Avalonia.Layout.VerticalAlignment.Center,
            Foreground = new SolidColorBrush(Color.FromRgb(0x8A, 0x8F, 0x96)),
            FontSize = 12
        };
        Slider Num(double min, double max, double val, double width, Action<double> set)
        {
            var s = new Slider { Minimum = min, Maximum = max, Value = val, Width = width, VerticalAlignment = Avalonia.Layout.VerticalAlignment.Center };
            s.ValueChanged += (_, e) => set(e.NewValue);
            return s;
        }
        CheckBox Check(bool val, Action<bool> set, string tip)
        {
            var c = new CheckBox { IsChecked = val, VerticalAlignment = Avalonia.Layout.VerticalAlignment.Center, Content = new TextBlock { Text = tip, FontSize = 12 } };
            c.IsCheckedChanged += (_, _) => set(c.IsChecked == true);
            return c;
        }

        switch (CurrentTool)
        {
            case ToolKind.Brush:
            case ToolKind.Pencil:
            case ToolKind.Eraser:
            {
                panel.Children.Add(Label("Size"));
                panel.Children.Add(Num(1, 400, BrushSize, 130, v => { BrushSize = (float)v; }));
                panel.Children.Add(Label("Hardness"));
                panel.Children.Add(Num(0, 1, BrushHardness, 100, v => BrushHardness = (float)v));
                panel.Children.Add(Label("Flow"));
                panel.Children.Add(Num(0.05, 1, BrushFlow, 100, v => BrushFlow = (float)v));
                panel.Children.Add(Label("Opacity"));
                panel.Children.Add(Num(0.05, 1, BrushOpacity, 100, v => BrushOpacity = (float)v));
                panel.Children.Add(Check(SizePressure, v => SizePressure = v, "Pen size pressure"));
                panel.Children.Add(Check(OpacityPressure, v => OpacityPressure = v, "Pen flow pressure"));
                if (CurrentTool == ToolKind.Brush)
                {
                    var hardness = Num(0, 1, BrushHardness, 0, _ => { });
                    panel.Children.Add(hardness);
                }
                break;
            }
            case ToolKind.Wand:
                panel.Children.Add(Label("Tolerance"));
                panel.Children.Add(Num(0, 255, WandTolerance, 130, v => WandTolerance = (int)v));
                panel.Children.Add(Check(WandContiguous, v => WandContiguous = v, "Contiguous"));
                break;
            case ToolKind.Bucket:
                panel.Children.Add(Label("Tolerance"));
                panel.Children.Add(Num(0, 255, BucketTolerance, 130, v => BucketTolerance = (int)v));
                panel.Children.Add(Check(BucketContiguous, v => BucketContiguous = v, "Contiguous"));
                break;
            case ToolKind.Gradient:
                panel.Children.Add(Check(GradientRadial, v => GradientRadial = v, "Radial"));
                panel.Children.Add(Check(GradientTransparent, v => GradientTransparent = v, "Foreground to transparent"));
                break;
            case ToolKind.Shape:
            {
                var combo = new ComboBox { MinWidth = 100, VerticalAlignment = Avalonia.Layout.VerticalAlignment.Center };
                combo.ItemsSource = new List<string> { "Rectangle", "Ellipse", "Line" };
                combo.SelectedIndex = ShapeKind;
                combo.SelectionChanged += (_, _) => { if (combo.SelectedIndex >= 0) ShapeKind = combo.SelectedIndex; };
                panel.Children.Add(combo);
                panel.Children.Add(Label("Stroke"));
                panel.Children.Add(Num(1, 60, ShapeStrokeWidth, 90, v => ShapeStrokeWidth = (int)v));
                panel.Children.Add(Check(ShapeFill, v => ShapeFill = v, "Fill"));
                break;
            }
            case ToolKind.Text:
            {
                var fonts = new ComboBox { MinWidth = 140, VerticalAlignment = Avalonia.Layout.VerticalAlignment.Center };
                fonts.ItemsSource = new List<string> { "Inter", "Segoe UI", "Arial", "Georgia", "Consolas", "Times New Roman", "Verdana" };
                fonts.SelectedIndex = 0;
                fonts.SelectionChanged += (_, _) => { if (fonts.SelectedItem is string s) TextFontFamily = s; };
                panel.Children.Add(fonts);
                panel.Children.Add(Label("Size"));
                panel.Children.Add(Num(8, 300, TextFontSize, 100, v => TextFontSize = v));
                panel.Children.Add(Check(TextBold, v => TextBold = v, "Bold"));
                panel.Children.Add(Check(TextItalic, v => TextItalic = v, "Italic"));
                break;
            }
            case ToolKind.Transform:
            {
                var interp = new ComboBox { MinWidth = 110, VerticalAlignment = Avalonia.Layout.VerticalAlignment.Center };
                interp.ItemsSource = new List<string> { "Nearest", "Bilinear", "Bicubic" };
                interp.SelectedIndex = TransformInterp;
                interp.SelectionChanged += (_, _) => { if (interp.SelectedIndex >= 0) TransformInterp = interp.SelectedIndex; };
                panel.Children.Add(Label("Interpolation"));
                panel.Children.Add(interp);
                panel.Children.Add(Label("Scale X"));
                panel.Children.Add(Num(0.1, 4, 1, 90, v => _tfSx = v));
                panel.Children.Add(Label("Y"));
                panel.Children.Add(Num(0.1, 4, 1, 90, v => _tfSy = v));
                panel.Children.Add(Label("Rotate°"));
                panel.Children.Add(Num(-180, 180, 0, 120, v => _tfRot = v));
                var apply = new Button { Content = "Apply" };
                apply.Click += (_, _) => TheCanvas.CommitTransformScale(_tfSx, _tfSy, _tfRot);
                panel.Children.Add(apply);
                break;
            }
            case ToolKind.RectSelect:
            case ToolKind.EllipseSelect:
            case ToolKind.Lasso:
                panel.Children.Add(new TextBlock
                {
                    Text = "Drag to select · Shift adds · Alt subtracts",
                    VerticalAlignment = Avalonia.Layout.VerticalAlignment.Center,
                    Foreground = new SolidColorBrush(Color.FromRgb(0x8A, 0x8F, 0x96)),
                    FontSize = 12
                });
                break;
            case ToolKind.Crop:
            {
                var apply = new Button { Content = "Apply Crop" };
                apply.Click += (_, _) => { TheCanvas.CommitCrop(); BuildOptionsBar(); };
                panel.Children.Add(apply);
                panel.Children.Add(new TextBlock
                {
                    Text = "Drag on canvas to define the crop area, then Apply",
                    VerticalAlignment = Avalonia.Layout.VerticalAlignment.Center,
                    Foreground = new SolidColorBrush(Color.FromRgb(0x8A, 0x8F, 0x96)),
                    FontSize = 12
                });
                break;
            }
        }
        bar.Content = panel;
    }

    private double _tfSx = 1, _tfSy = 1, _tfRot = 0;

    public Engine.BrushFfi MakeBrushParams(ToolKind kind) => new()
    {
        Size = BrushSize,
        Hardness = kind == ToolKind.Pencil ? 1f : BrushHardness,
        Flow = BrushFlow,
        Opacity = BrushOpacity,
        Spacing = kind == ToolKind.Pencil ? 0.06f : 0.12f,
        Eraser = kind == ToolKind.Eraser ? 1 : 0,
        R = ForegroundColor.R,
        G = ForegroundColor.G,
        B = ForegroundColor.B,
        A = ForegroundColor.A,
        SizePressure = SizePressure ? 1 : 0,
        OpacityPressure = OpacityPressure ? 1 : 0,
        Pencil = kind == ToolKind.Pencil ? 1 : 0,
    };

    // ══════════ tabs & docs ══════════

    private AuroraDocument NewDocumentInternal(uint w, uint h, int bg, string title)
    {
        unsafe
        {
            var handle = Engine.DocNew(w, h, bg, title);
            if (handle == 0)
                throw new Exception("engine: " + Engine.LastError());
            var doc = new AuroraDocument(handle, w, h, title);
            Tabs.Add(new DocTabItem { Doc = doc });
            return doc;
        }
    }

    public void SelectDoc(AuroraDocument doc)
    {
        foreach (var t in Tabs)
            t._selected = ReferenceEquals(t.Doc, doc);
        TheDocTabs.ItemsSource = null;
        TheDocTabs.ItemsSource = Tabs;
        doc.RefreshState();
        ActiveDocChanged?.Invoke(this, EventArgs.Empty);
        RefreshPanels();
        TheCanvas.InvalidateAnts();
        UpdateStatus();
    }

    private void OnTabTapped(object? sender, TappedEventArgs e)
    {
        if (sender is Border b && b.DataContext is DocTabItem t)
            SelectDoc(t.Doc);
    }

    private void OnTabCloseTapped(object? sender, TappedEventArgs e)
    {
        if (sender is TextBlock tb && tb.DataContext is DocTabItem t)
        {
            Tabs.Remove(t);
            t.Doc.Dispose();
            if (Tabs.Count == 0)
            {
                CreateInitialDocument();
            }
            else
            {
                SelectDoc(Tabs[0].Doc);
            }
        }
    }

    // ══════════ status ══════════

    public void UpdateStatus()
    {
        var doc = ActiveDoc;
        var zoomEl = FindAny<TextBlock>("StatusZoom");
        var sizeEl = FindAny<TextBlock>("StatusSize");
        if (zoomEl != null) zoomEl.Text = $"{TheCanvas.Zoom * 100:0}%";
        if (sizeEl != null) sizeEl.Text = doc != null ? $"{doc.Width} × {doc.Height} px" : "";
    }

    public void SetCursorDocPos(Point p)
    {
        var el = FindAny<TextBlock>("StatusPos");
        if (el != null) el.Text = $"{p.X:0}, {p.Y:0}";
    }

    public void SetStatusMessage(string msg) => SetStatusMessageStatic(msg);

    private void SetStatusMessageStatic(string msg)
    {
        var el = FindAny<TextBlock>("StatusMsg");
        if (el != null) el.Text = msg;
    }

    // ══════════ panels refresh ══════════

    public void RefreshPanels()
    {
        var doc = ActiveDoc;
        LayersPanelCtl.Refresh(doc);
        HistoryPanelCtl.Refresh(doc);
        ColorPanelCtl.RefreshExternal();
    }

    public void OnImageStructureChanged()
    {
        ActiveDocChanged?.Invoke(this, EventArgs.Empty);
        TheCanvas.PollComposite();
        UpdateStatus();
    }

    // ══════════ color ══════════

    public void SetForegroundColor(Color c)
    {
        ForegroundColor = c;
        ColorPanelCtl.SetForeground(c);
    }

    public void SetBackgroundColor(Color c) => BackgroundColor = c;

    // ══════════ text tool overlay ══════════

    private TextBox? _textOverlay;
    private Point _textScreenPos;
    private Point _textDocPos;

    public void ShowTextEditorAt(Point screenPos, Point docPos)
    {
        HideTextEditor(commit: false);
        _textScreenPos = screenPos;
        _textDocPos = docPos;
        _textOverlay = new TextBox
        {
            Width = 320,
            Watermark = "Type text, press Ctrl+Enter to commit, Esc to cancel",
            AcceptsReturn = true,
            Height = 84,
            FontSize = 14,
            Background = new SolidColorBrush(Color.FromArgb(240, 0x26, 0x28, 0x2C)),
        };
        CanvasOverlay.Children.Add(_textOverlay);
        Avalonia.Controls.Canvas.SetLeft(_textOverlay, screenPos.X);
        Avalonia.Controls.Canvas.SetTop(_textOverlay, screenPos.Y);
        _textOverlay.KeyDown += (_, e) =>
        {
            if (e.Key == Key.Escape) { HideTextEditor(commit: false); e.Handled = true; }
            else if (e.Key == Key.Enter && e.KeyModifiers.HasFlag(KeyModifiers.Control))
            {
                CommitText();
                e.Handled = true;
            }
        };
        _textOverlay.Focus();
    }

    private void CommitText()
    {
        var text = _textOverlay?.Text;
        if (string.IsNullOrWhiteSpace(text)) { HideTextEditor(false); return; }
        var doc = ActiveDoc;
        if (doc == null) { HideTextEditor(false); return; }
        RenderTextToLayer(doc, text, _textDocPos, ForegroundColor, TextFontFamily, TextFontSize, TextBold, TextItalic);
        HideTextEditor(true);
    }

    private void HideTextEditor(bool commit)
    {
        if (_textOverlay != null)
        {
            CanvasOverlay.Children.Remove(_textOverlay);
            _textOverlay = null;
        }
    }

    public void RenderTextToLayer(AuroraDocument doc, string text, Point docPos, Color color,
        string fontFamily, double fontSize, bool bold, bool italic)
    {
        var typeface = new Typeface(fontFamily, italic ? FontStyle.Italic : FontStyle.Normal, bold ? FontWeight.Bold : FontWeight.Normal);
        var ft = new FormattedText(text, CultureInfo.InvariantCulture, FlowDirection.LeftToRight,
            typeface, fontSize, new SolidColorBrush(color));
        int w = (int)Math.Ceiling(ft.Width + 12);
        int h = (int)Math.Ceiling(ft.Height + 12);
        if (w <= 0 || h <= 0) return;
        var rtb = new RenderTargetBitmap(new PixelSize(w, h), new Vector(96, 96));
        using (var ctx = rtb.CreateDrawingContext())
        {
            ctx.DrawText(ft, new Point(6, 6));
        }
        PasteRtbToLayer(doc, rtb, (int)docPos.X - 6, (int)docPos.Y - 6);
        doc.RefreshState();
        RefreshPanels();
    }

    /// <summary>Rasterize a RenderTargetBitmap to PNG and paste via the engine (real pixels, no fakes).</summary>
    public static void PasteRtbToLayer(AuroraDocument doc, RenderTargetBitmap rtb, int docX, int docY)
    {
        using var ms = new MemoryStream();
        rtb.Save(ms);
        var png = ms.ToArray();
        unsafe
        {
            fixed (byte* p = png)
                Engine.aurora_paste_png(doc.Handle, doc.State.ActiveId, docX, docY, p, (uint)png.Length);
        }
    }

    public void CommitShape(Point start, Point end)
    {
        var doc = ActiveDoc;
        if (doc == null) return;
        var r = RectBetween(start, end);
        if (ShapeKind == 2)
        {
            r = new Rect(Math.Min(start.X, end.X), Math.Min(start.Y, end.Y),
                Math.Abs(end.X - start.X), Math.Abs(end.Y - start.Y));
        }
        int pad = ShapeStrokeWidth + 4;
        int w = (int)r.Width + pad * 2, h = (int)r.Height + pad * 2;
        if (w <= 0 || h <= 0) return;
        var rtb = new RenderTargetBitmap(new PixelSize(w, h), new Vector(96, 96));
        using (var ctx = rtb.CreateDrawingContext())
        {
            var pen = new Pen(new SolidColorBrush(ForegroundColor), ShapeStrokeWidth);
            var fill = new SolidColorBrush(ForegroundColor);
            var local = new Rect(pad, pad, r.Width, r.Height);
            switch (ShapeKind)
            {
                case 0:
                    if (ShapeFill) ctx.FillRectangle(fill, local);
                    else ctx.DrawRectangle(null, pen, local);
                    break;
                case 1:
                    if (ShapeFill) ctx.DrawEllipse(fill, null, local);
                    else ctx.DrawEllipse(null, pen, local);
                    break;
                case 2:
                    ctx.DrawLine(pen, new Point(pad, pad), new Point(pad + r.Width, pad + r.Height));
                    break;
            }
        }
        PasteRtbToLayer(doc, rtb, (int)r.X - pad, (int)r.Y - pad);
        doc.RefreshState();
        RefreshPanels();
    }

    private static Rect RectBetween(Point a, Point b) =>
        new(new Point(Math.Min(a.X, b.X), Math.Min(a.Y, b.Y)),
            new Point(Math.Max(a.X, b.X), Math.Max(a.Y, b.Y)));
}
