using System;
using System.Collections.Generic;
using System.Linq;
using System.Collections.ObjectModel;
using AuroraStudio.Panels;
using System.Threading.Tasks;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Input;
using Avalonia.Layout;
using Avalonia.Media;
using AuroraStudio.Controls;
using AuroraStudio.Interop;
using AuroraStudio.Models;

namespace AuroraStudio.Dialogs;

public static class DialogUtil
{
    public static Window Make(string title, Control content, bool canResize = false)
    {
        return new Window
        {
            Title = title,
            SizeToContent = SizeToContent.WidthAndHeight,
            WindowStartupLocation = WindowStartupLocation.CenterOwner,
            CanResize = canResize,
            MinWidth = 360,
            Content = new Border { Background = Brush(0x22, 0x24, 0x27), Child = content },
        };
    }

    public static IBrush Brush(byte r, byte g, byte b) => new SolidColorBrush(Color.FromRgb(r, g, b));

    public static Grid FieldGrid((string Label, Control Box)[] fields)
    {
        var grid = new Grid { ColumnDefinitions = ColumnDefinitions.Parse("Auto,*"), Margin = new Thickness(0, 4) };
        for (int i = 0; i < fields.Length; i++)
        {
            grid.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
            var lab = new TextBlock
            {
                Text = fields[i].Label,
                VerticalAlignment = VerticalAlignment.Center,
                Margin = new Thickness(0, 0, 12, 6),
                Foreground = Brush(0x9A, 0xA0, 0xA6),
            };
            Grid.SetRow(lab, i);
            Grid.SetColumn(lab, 0);
            var box = fields[i].Box;
            box.Margin = new Thickness(0, 0, 0, 6);
            Grid.SetRow(box, i);
            Grid.SetColumn(box, 1);
            grid.Children.Add(lab);
            grid.Children.Add(box);
        }
        return grid;
    }

    public static (Panel okCancel, Button ok, Button cancel) OkCancel()
    {
        var ok = new Button { Content = "OK", Padding = new Thickness(18, 4), HorizontalAlignment = HorizontalAlignment.Right };
        var cancel = new Button { Content = "Cancel", Padding = new Thickness(14, 4), HorizontalAlignment = HorizontalAlignment.Right };
        var panel = new StackPanel
        {
            Orientation = Orientation.Horizontal,
            HorizontalAlignment = HorizontalAlignment.Right,
            Spacing = 8,
            Margin = new Thickness(0, 14, 0, 0),
        };
        panel.Children.Add(cancel);
        panel.Children.Add(ok);
        return (panel, ok, cancel);
    }

    public static (Panel row, NumericUpDown num) NumField(string label, double min, double max, double val, double increment = 1)
    {
        var num = new NumericUpDown
        {
            Minimum = (decimal)min,
            Maximum = (decimal)max,
            Value = (decimal)val,
            Increment = (decimal)increment,
            Width = 140,
            FormatString = "0.##",
        };
        var sp = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 10, Margin = new Thickness(0, 5) };
        sp.Children.Add(new TextBlock { Text = label, Width = 120, VerticalAlignment = VerticalAlignment.Center, Foreground = Brush(0x9A, 0xA0, 0xA6) });
        sp.Children.Add(num);
        return (sp, num);
    }

    public static (StackPanel row, CheckBox cb) CheckField(string label, bool val)
    {
        var cb = new CheckBox { IsChecked = val, Content = label };
        var sp = new StackPanel { Orientation = Orientation.Horizontal, Margin = new Thickness(0, 5) };
        sp.Children.Add(cb);
        return (sp, cb);
    }

    public static (StackPanel row, ComboBox combo) ComboField(string label, IEnumerable<string> items, int selected)
    {
        var combo = new ComboBox { MinWidth = 160, ItemsSource = new List<string>(items), SelectedIndex = selected };
        var sp = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 10, Margin = new Thickness(0, 5) };
        sp.Children.Add(new TextBlock { Text = label, Width = 120, VerticalAlignment = VerticalAlignment.Center, Foreground = Brush(0x9A, 0xA0, 0xA6) });
        sp.Children.Add(combo);
        return (sp, combo);
    }
}

// ═══════════════════ New document ═══════════════════

public class NewDocumentDialog
{
    public Task<(uint w, uint h, int bg, string name)?> ShowDialogAsync(Window owner)
    {
        var tcs = new TaskCompletionSource<(uint, uint, int, string)?>();

        var (wRow, wNum) = DialogUtil.NumField("Width (px)", 1, 16384, 1200);
        var (hRow, hNum) = DialogUtil.NumField("Height (px)", 1, 16384, 800);
        var (presetRow, preset) = DialogUtil.ComboField("Preset",
            new[] { "Custom", "1200 × 800", "1920 × 1080", "2048 × 2048", "4000 × 4000", "A4 300dpi (2480 × 3508)" }, 0);
        preset.SelectionChanged += (_, _) =>
        {
            switch (preset.SelectedIndex)
            {
                case 1: wNum.Value = 1200; hNum.Value = 800; break;
                case 2: wNum.Value = 1920; hNum.Value = 1080; break;
                case 3: wNum.Value = 2048; hNum.Value = 2048; break;
                case 4: wNum.Value = 4000; hNum.Value = 4000; break;
                case 5: wNum.Value = 2480; hNum.Value = 3508; break;
            }
        };
        var (bgRow, bgCombo) = DialogUtil.ComboField("Background", new[] { "White", "Transparent", "Black" }, 0);
        var nameBox = new TextBox { Text = $"Untitled-{System.DateTime.Now:yyyyMMdd-HHmm}" };

        var grid = new StackPanel { Margin = new Thickness(22), Spacing = 2 };
        grid.Children.Add(DialogUtil.FieldGrid(new[] { ("Name", (Control)nameBox) }));
        grid.Children.Add(presetRow);
        grid.Children.Add(wRow);
        grid.Children.Add(hRow);
        grid.Children.Add(bgRow);
        var (oc, ok, cancel) = DialogUtil.OkCancel();
        grid.Children.Add(oc);

        var dlg = DialogUtil.Make("New Document", grid);
        ok.Click += (_, _) =>
        {
            tcs.TrySetResult(((uint)wNum.Value, (uint)hNum.Value, bgCombo.SelectedIndex,
                string.IsNullOrWhiteSpace(nameBox.Text) ? "Untitled" : nameBox.Text!.Trim()));
            dlg.Close();
        };
        cancel.Click += (_, _) => { tcs.TrySetResult(null); dlg.Close(); };
        dlg.Opened += (_, _) => ok.Focus();
        _ = dlg.ShowDialog(owner);
        dlg.Closed += (_, _) => tcs.TrySetResult(null);
        return tcs.Task;
    }
}

// ═══════════════════ Export ═══════════════════

public class ExportOptions
{
    public string Format { get; set; } = "png";
    public int Quality { get; set; } = 92;
    public int Compression { get; set; } = 6;
    public bool Frames { get; set; }
    public string? Path { get; set; }
}

public class ExportDialog
{
    private readonly AuroraDocument _doc;
    public ExportDialog(AuroraDocument doc) { _doc = doc; }

    public Task<ExportOptions?> ShowDialogAsync(Window owner)
    {
        var tcs = new TaskCompletionSource<ExportOptions?>();
        var opts = new ExportOptions();

        var formats = new Dictionary<string, string>
        {
            ["PNG (.png) — lossless, alpha"] = "png",
            ["JPEG (.jpg) — quality setting"] = "jpeg",
            ["WebP (.webp) — lossless"] = "webp",
            ["GIF (.gif) — layers as frames optional"] = "gif",
            ["BMP (.bmp)"] = "bmp",
            ["TIFF (.tif)"] = "tiff",
            ["SVG (.svg) — embedded raster"] = "svg",
            ["OpenRaster (.ora) — layered"] = "ora",
            ["PSD (.psd) — layered"] = "psd",
        };
        var (fmtRow, fmtCombo) = DialogUtil.ComboField("Format", formats.Keys, 0);
        var (qRow, qNum) = DialogUtil.NumField("JPEG quality", 1, 100, opts.Quality);
        var (cRow, cNum) = DialogUtil.NumField("PNG compression", 0, 9, opts.Compression);
        var (fRow, fCheck) = DialogUtil.CheckField("GIF: export each visible layer as a frame", false);

        void UpdateVis()
        {
            string? key = fmtCombo.SelectedItem as string;
            string fmt = key != null ? formats[key] : "png";
            qRow.IsVisible = fmt == "jpeg";
            cRow.IsVisible = fmt == "png";
            fRow.IsVisible = fmt == "gif";
        }
        fmtCombo.SelectionChanged += (_, _) => UpdateVis();

        var grid = new StackPanel { Margin = new Thickness(22), Spacing = 2 };
        grid.Children.Add(new TextBlock
        {
            Text = $"Exporting: {_doc.Title} — {_doc.Width} × {_doc.Height} px",
            Foreground = DialogUtil.Brush(0x9A, 0xA0, 0xA6),
            Margin = new Thickness(0, 0, 0, 10),
        });
        grid.Children.Add(fmtRow);
        grid.Children.Add(qRow);
        grid.Children.Add(cRow);
        grid.Children.Add(fRow);
        var (oc, ok, cancel) = DialogUtil.OkCancel();
        ok.Content = "Choose File…";
        grid.Children.Add(oc);
        var dlg = DialogUtil.Make("Export As", grid);

        ok.Click += async (_, _) =>
        {
            string? key = fmtCombo.SelectedItem as string;
            opts.Format = key != null ? formats[key] : "png";
            opts.Quality = (int)qNum.Value;
            opts.Compression = (int)cNum.Value;
            opts.Frames = fCheck.IsChecked == true;
            var ext = opts.Format switch
            {
                "jpeg" => "jpg", "tiff" => "tif", _ => opts.Format,
            };
            var file = await owner.StorageProvider.SaveFilePickerAsync(new Avalonia.Platform.Storage.FilePickerSaveOptions
            {
                Title = "Export Image",
                DefaultExtension = ext,
                SuggestedFileName = System.IO.Path.ChangeExtension(_doc.Title, null) is { } n ? n.TrimEnd('.') : "export",
            });
            if (file != null)
            {
                opts.Path = file.Path.LocalPath;
                tcs.TrySetResult(opts);
                dlg.Close();
            }
        };
        cancel.Click += (_, _) => { tcs.TrySetResult(null); dlg.Close(); };
        _ = dlg.ShowDialog(owner);
        dlg.Closed += (_, _) => tcs.TrySetResult(null);
        UpdateVis();
        return tcs.Task;
    }
}

// ═══════════════════ Image / Canvas size ═══════════════════

public class ImageSizeDialog
{
    private readonly uint _w, _h;
    private readonly string _title;
    private readonly bool _anchor;

    public ImageSizeDialog(uint curW, uint curH, uint origW, uint origH, string title, bool allowAnchor = false)
    {
        _w = curW; _h = curH; _title = title; _anchor = allowAnchor;
    }

    public Task<(uint w, uint h, int interp, int anchorOrMinus1)?> ShowDialogAsync(Window owner)
    {
        var tcs = new TaskCompletionSource<(uint, uint, int, int)?>();

        var (wRow, wNum) = DialogUtil.NumField("Width (px)", 1, 16384, _w);
        var (hRow, hNum) = DialogUtil.NumField("Height (px)", 1, 16384, _h);
        var aspect = _w / (double)_h;
        bool locked = true;
        var (lockRow, lockCheck) = DialogUtil.CheckField("Keep aspect ratio", true);
        lockCheck.IsCheckedChanged += (_, _) => locked = lockCheck.IsChecked == true;
        wNum.ValueChanged += (_, _) =>
        {
            if (locked && !_anchor)
                hNum.Value = (decimal)Math.Max(1, Math.Round((double)(wNum.Value ?? 1) / aspect));
        };
        var (iRow, interp) = DialogUtil.ComboField("Interpolation", new[] { "Nearest", "Bilinear (smooth)", "Bicubic (sharp)" }, 2);
        var (aRow, anchor) = DialogUtil.ComboField("Anchor", new[] { "Top Left", "Top", "Top Right", "Left", "Center", "Right", "Bottom Left", "Bottom", "Bottom Right" }, 4);

        var grid = new StackPanel { Margin = new Thickness(22) };
        grid.Children.Add(wRow);
        grid.Children.Add(hRow);
        if (!_anchor) grid.Children.Add(lockRow);
        grid.Children.Add(iRow);
        grid.Children.Add(aRow);
        aRow.IsVisible = _anchor;
        if (_anchor) iRow.IsVisible = false;
        var (oc, ok, cancel) = DialogUtil.OkCancel();
        grid.Children.Add(oc);
        var dlg = DialogUtil.Make(_title, grid);

        ok.Click += (_, _) =>
        {
            int anchorVal = anchor.SelectedIndex < 0 ? 4 : anchor.SelectedIndex;
            tcs.TrySetResult(((uint)wNum.Value, (uint)hNum.Value, interp.SelectedIndex, _anchor ? anchorVal : -1));
            dlg.Close();
        };
        cancel.Click += (_, _) => { tcs.TrySetResult(null); dlg.Close(); };
        _ = dlg.ShowDialog(owner);
        dlg.Closed += (_, _) => tcs.TrySetResult(null);
        return tcs.Task;
    }
}

// ═══════════════════ Filter params with live preview ═══════════════════

public class FilterParamDialog
{
    private readonly string _title;
    private readonly (string Name, double Min, double Max, double Val)[] _pars;
    private readonly Action<double[]>? _preview;
    private readonly string? _note;

    public FilterParamDialog(string title, (string, double, double, double)[] pars, Action<double[]>? preview, string? note = null)
    {
        _title = title;
        _pars = pars;
        _preview = preview;
        _note = note;
    }

    public Task<(bool ok, double[] vals)> ShowDialogAsync(Window owner)
    {
        var tcs = new TaskCompletionSource<(bool, double[])>();
        var grid = new StackPanel { Margin = new Thickness(22) };
        if (_note != null)
            grid.Children.Add(new TextBlock { Text = _note, Foreground = DialogUtil.Brush(0x8A, 0x8F, 0x96), Margin = new Thickness(0, 0, 0, 8) });
        var nums = new List<NumericUpDown>();
        foreach (var p in _pars)
        {
            var (row, num) = DialogUtil.NumField(p.Name, p.Min, p.Max, p.Val);
            nums.Add(num);
            num.ValueChanged += (_, _) =>
            {
                if (_preview != null)
                {
                    _preview(nums.Select(n => (double)n.Value).ToArray());
                }
            };
            grid.Children.Add(row);
        }
        if (_preview != null)
            grid.Children.Add(new TextBlock { Text = "Live preview is shown on the canvas.", Foreground = DialogUtil.Brush(0x6E, 0x73, 0x78), FontSize = 11, Margin = new Thickness(0, 8, 0, 0) });
        var (oc, ok, cancel) = DialogUtil.OkCancel();
        grid.Children.Add(oc);
        var dlg = DialogUtil.Make(_title, grid);

        ok.Click += (_, _) =>
        {
            tcs.TrySetResult((true, nums.Select(n => (double)n.Value).ToArray()));
            dlg.Close();
        };
        cancel.Click += (_, _) => { tcs.TrySetResult((false, nums.Select(n => (double)n.Value).ToArray())); dlg.Close(); };
        _ = dlg.ShowDialog(owner);
        dlg.Closed += (_, _) => tcs.TrySetResult((false, nums.Select(n => (double)n.Value).ToArray()));
        return tcs.Task;
    }
}

// ═══════════════════ Levels ═══════════════════

public class LevelsDialog
{
    private readonly Action<double, double, double, double, double, int> _preview;

    public LevelsDialog(Action<double, double, double, double, double, int> preview)
    {
        _preview = preview;
    }

    public Task<(bool ok, (double inBlack, double inWhite, double gamma, double outBlack, double outWhite, int channel) p)> ShowDialogAsync(Window owner)
    {
        var tcsOuter = new TaskCompletionSource<(bool, (double, double, double, double, double, int))>();
        bool okResult = false;
        double inB = 0, inW = 255, gam = 1.0, outB = 0, outW = 255;
        int channel = -1;

        var grid = new StackPanel { Margin = new Thickness(22) };
        var (cRow, chCombo) = DialogUtil.ComboField("Channel", new[] { "RGB", "Red", "Green", "Blue" }, 0);
        var sliders = new List<Slider>();
        (Panel row, Slider s) MakeSlider(string label, double min, double max, double val)
        {
            var s = new Slider { Minimum = min, Maximum = max, Value = val, Width = 240 };
            sliders.Add(s);
            var sp = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 10, Margin = new Thickness(0, 5) };
            sp.Children.Add(new TextBlock { Text = label, Width = 100, VerticalAlignment = VerticalAlignment.Center, Foreground = DialogUtil.Brush(0x9A, 0xA0, 0xA6) });
            sp.Children.Add(s);
            return (sp, s);
        }
        var (r1, s1) = MakeSlider("In Black", 0, 254, 0);
        var (r2, s2) = MakeSlider("In White", 1, 255, 255);
        var (r3, s3) = MakeSlider("Gamma", 0.1, 5, 1.0);
        var (r4, s4) = MakeSlider("Out Black", 0, 255, 0);
        var (r5, s5) = MakeSlider("Out White", 0, 255, 255);

        void Fire()
        {
            _preview(s1.Value, s2.Value, s3.Value, s4.Value, s5.Value, channel);
        }
        chCombo.SelectionChanged += (_, _) => { channel = chCombo.SelectedIndex - 1; Fire(); };
        foreach (var s in sliders) s.ValueChanged += (_, _) => Fire();

        grid.Children.Add(cRow);
        grid.Children.Add(r1);
        grid.Children.Add(r2);
        grid.Children.Add(r3);
        grid.Children.Add(r4);
        grid.Children.Add(r5);
        var (oc, ok, cancel) = DialogUtil.OkCancel();
        grid.Children.Add(oc);
        var dlg = DialogUtil.Make("Levels", grid);

        ok.Click += (_, _) =>
        {
            okResult = true;
            inB = s1.Value; inW = s2.Value; gam = s3.Value; outB = s4.Value; outW = s5.Value;
            dlg.Close();
        };
        cancel.Click += (_, _) => dlg.Close();
        dlg.Closed += (_, _) => tcsOuter.TrySetResult((okResult, (inB, inW, gam, outB, outW, channel)));
        _ = dlg.ShowDialog(owner);
        return tcsOuter.Task;
    }
}

// ═══════════════════ Curves ═══════════════════

public class CurvesDialog
{
    private readonly Action<string> _preview;
    public CurvesDialog(Action<string> preview) { _preview = preview; }

    public Task<(bool ok, string json)> ShowDialogAsync(Window owner)
    {
        var tcsOuter = new TaskCompletionSource<(bool, string)>();
        bool okResult = false;
        string json = "";

        var grid = new StackPanel { Margin = new Thickness(22), Spacing = 8 };
        var (cRow, chCombo) = DialogUtil.ComboField("Channel", new[] { "RGB", "Red", "Green", "Blue" }, 0);
        var editor = new CurveEditor();

        chCombo.SelectionChanged += (_, _) =>
        {
            editor.ActiveChannel = chCombo.SelectedIndex - 1;
            editor.InvalidateVisual();
        };
        editor.Changed += (_, _) =>
        {
            _preview(BuildJson());
        };
        string BuildJson()
        {
            var rgb = editor.PointsDict.GetValueOrDefault(-1, new List<(float, float)> { (0, 0), (1, 1) });
            var r = editor.PointsDict.GetValueOrDefault(0, new List<(float, float)> { (0, 0), (1, 1) });
            var g = editor.PointsDict.GetValueOrDefault(1, new List<(float, float)> { (0, 0), (1, 1) });
            var b = editor.PointsDict.GetValueOrDefault(2, new List<(float, float)> { (0, 0), (1, 1) });
            string Pts(List<(float, float)> l) => "[" + string.Join(",", l.Select(p => $"[{p.Item1:0.###},{p.Item2:0.###}]")) + "]";
            return $"{{\"rgb\":{Pts(rgb)},\"r\":{Pts(r)},\"g\":{Pts(g)},\"b\":{Pts(b)}}}";
        }

        var (oc, ok, cancel) = DialogUtil.OkCancel();
        grid.Children.Add(ColorPanel.PanelHeader("Curves"));
        grid.Children.Add(cRow);
        grid.Children.Add(new Border { Child = editor, CornerRadius = new CornerRadius(4), ClipToBounds = true });
        grid.Children.Add(new TextBlock { Text = "Drag points to shape the curve. Click empty area to add, right-click a point to remove.", FontSize = 11, Foreground = DialogUtil.Brush(0x8A, 0x8F, 0x96), TextWrapping = TextWrapping.Wrap });
        grid.Children.Add(oc);

        var dlg = DialogUtil.Make("Curves", grid);
        editor.Width = 360;
        editor.Height = 300;
        ok.Click += (_, _) =>
        {
            okResult = true;
            json = BuildJson();
            dlg.Close();
        };
        cancel.Click += (_, _) => dlg.Close();
        dlg.Closed += (_, _) => tcsOuter.TrySetResult((okResult, json));
        _ = dlg.ShowDialog(owner);
        return tcsOuter.Task;
    }
}

// ═══════════════════ Curve editor control ═══════════════════

public class CurveEditor : Control
{
    // channel: -1 rgb, 0 r, 1 g, 2 b
    private int _channel = -1;
    public int ActiveChannel
    {
        get => _channel;
        set { _channel = value; InvalidateVisual(); }
    }

    public Dictionary<int, List<(float X, float Y)>> PointsDict { get; } = new()
    {
        [-1] = new List<(float, float)> { (0, 0), (1, 1) },
        [0] = new List<(float, float)> { (0, 0), (1, 1) },
        [1] = new List<(float, float)> { (0, 0), (1, 1) },
        [2] = new List<(float, float)> { (0, 0), (1, 1) },
    };

    public event EventHandler? Changed;
    private int _dragIndex = -1;

    public CurveEditor()
    {
        ClipToBounds = true;
    }

    private static IBrush ChanColor(int ch) => ch switch
    {
        0 => new SolidColorBrush(Color.FromRgb(0xE7, 0x4C, 0x3C)),
        1 => new SolidColorBrush(Color.FromRgb(0x2E, 0xCC, 0x71)),
        2 => new SolidColorBrush(Color.FromRgb(0x34, 0x98, 0xDB)),
        _ => new SolidColorBrush(Color.FromRgb(0xBB, 0xC0, 0xC7)),
    };

    private Point PtToScreen((float X, float Y) p, Rect r) => new(r.X + p.X * r.Width, r.Y + (1 - p.Y) * r.Height);
    private (float X, float Y) ScreenToPt(Point p, Rect r) => ((float)((p.X - r.X) / r.Width), (float)(1 - (p.Y - r.Y) / r.Height));

    protected override void OnPointerPressed(PointerPressedEventArgs e)
    {
        var pts = PointsDict[_channel];
        var pos = e.GetCurrentPoint(this).Position;
        var r = PlotRect();
        int hit = -1;
        for (int i = 0; i < pts.Count; i++)
        {
            var sp = PtToScreen(pts[i], r);
            var ddx = sp.X - pos.X; var ddy = sp.Y - pos.Y;
            if (ddx * ddx + ddy * ddy < 81) { hit = i; break; }
        }
        var props = e.GetCurrentPoint(this).Properties;
        if (props.IsRightButtonPressed)
        {
            if (hit > 0 && hit < pts.Count - 1)
            {
                pts.RemoveAt(hit);
                Changed?.Invoke(this, EventArgs.Empty);
                InvalidateVisual();
            }
            return;
        }
        if (hit >= 0)
        {
            _dragIndex = hit;
        }
        else
        {
            var np = ScreenToPt(pos, r);
            pts.Add(np);
            pts.Sort((a, b) => a.X.CompareTo(b.X));
            _dragIndex = pts.FindIndex(p => p.X == np.X && p.Y == np.Y);
            Changed?.Invoke(this, EventArgs.Empty);
        }
        e.Pointer.Capture(this);
        InvalidateVisual();
    }

    protected override void OnPointerMoved(PointerEventArgs e)
    {
        if (_dragIndex < 0) return;
        if (!e.GetCurrentPoint(this).Properties.IsLeftButtonPressed) return;
        var pts = PointsDict[_channel];
        var r = PlotRect();
        var np = ScreenToPt(e.GetCurrentPoint(this).Position, r);
        np.X = Math.Clamp(np.X, 0f, 1f);
        np.Y = Math.Clamp(np.Y, 0f, 1f);
        // keep endpoints pinned at x=0 / x=1
        if (_dragIndex > 0 && _dragIndex < pts.Count - 1)
            np.X = Math.Clamp(np.X, pts[_dragIndex - 1].X + 0.01f, pts[_dragIndex + 1].X - 0.01f);
        pts[_dragIndex] = np;
        Changed?.Invoke(this, EventArgs.Empty);
        InvalidateVisual();
    }

    protected override void OnPointerReleased(PointerReleasedEventArgs e)
    {
        _dragIndex = -1;
        base.OnPointerReleased(e);
    }

    private Rect PlotRect() => new(new Point(6, 6), new Size(Math.Max(10, Bounds.Width - 12), Math.Max(10, Bounds.Height - 12)));

    public override void Render(DrawingContext ctx)
    {
        var r = PlotRect();
        ctx.FillRectangle(new SolidColorBrush(Color.FromRgb(0x1A, 0x1B, 0x1D)), r);
        var gridPen = new Pen(new SolidColorBrush(Color.FromRgb(0x30, 0x32, 0x37)), 1);
        for (int i = 1; i < 4; i++)
        {
            double fx = r.X + r.Width * i / 4;
            double fy = r.Y + r.Height * i / 4;
            ctx.DrawLine(gridPen, new Point(fx, r.Top), new Point(fx, r.Bottom));
            ctx.DrawLine(gridPen, new Point(r.Left, fy), new Point(r.Right, fy));
        }
        var pts = PointsDict[_channel];
        // histogram-ish diagonal reference
        ctx.DrawLine(new Pen(new SolidColorBrush(Color.FromRgb(0x3A, 0x3D, 0x44)), 1, DashStyle.Dash, PenLineCap.Round, PenLineJoin.Miter),
            PtToScreen((0, 0), r), PtToScreen((1, 1), r));
        // spline through points (monotone cubic approximated with polyline of many samples)
        var pen = new Pen(ChanColor(_channel), 2);
        var geometry = new StreamGeometry();
        using (var g = geometry.Open())
        {
            var samples = AuroraStudio.Dialogs.CurveSampler.Sample(pts, 128);
            g.BeginFigure(PtToScreen(samples[0], r), false);
            for (int i = 1; i < samples.Count; i++)
                g.LineTo(PtToScreen(samples[i], r));
            g.EndFigure(false);
        }
        ctx.DrawGeometry(null, pen, geometry);
        // points
        foreach (var p in pts)
        {
            var sp = PtToScreen(p, r);
            ctx.DrawEllipse(ChanColor(_channel), new Pen(Brushes.White, 1.2), sp, 4.5, 4.5);
        }
    }
}

public static class CurveSampler
{
    /// <summary>Monotone cubic (Fritsch–Carlson) sampling matching the engine's implementation.</summary>
    public static List<(float X, float Y)> Sample(List<(float X, float Y)> pts, int n)
    {
        var result = new List<(float, float)>(n);
        if (pts.Count == 0) return result;
        var sorted = pts.OrderBy(p => p.X).ToList();
        if (sorted.Count == 1) { for (int i = 0; i < n; i++) result.Add((i / (float)(n - 1), sorted[0].Y)); return result; }
        int m = sorted.Count;
        var dx = new double[m - 1];
        var dy = new double[m - 1];
        var slope = new double[m - 1];
        for (int i = 0; i < m - 1; i++)
        {
            dx[i] = Math.Max(1e-6, sorted[i + 1].X - sorted[i].X);
            dy[i] = sorted[i + 1].Y - sorted[i].Y;
            slope[i] = dy[i] / dx[i];
        }
        var mm = new double[m];
        mm[0] = slope[0];
        mm[m - 1] = slope[m - 2];
        for (int i = 1; i < m - 1; i++)
        {
            if (slope[i - 1] * slope[i] <= 0) mm[i] = 0;
            else
            {
                double wx = dx[i - 1] + dx[i];
                mm[i] = 3.0 * (dx[i] / wx * slope[i - 1] + dx[i - 1] / wx * slope[i]);
            }
        }
        for (int k = 0; k < n; k++)
        {
            double u = k / (double)(n - 1);
            double v = u;
            if (u <= sorted[0].X) v = sorted[0].Y;
            else if (u >= sorted[m - 1].X) v = sorted[m - 1].Y;
            else
            {
                for (int i = 0; i < m - 1; i++)
                {
                    if (u >= sorted[i].X && u <= sorted[i + 1].X)
                    {
                        double h = sorted[i + 1].X - sorted[i].X;
                        double t = (u - sorted[i].X) / h;
                        double t2 = t * t, t3 = t2 * t;
                        double h00 = 2 * t3 - 3 * t2 + 1;
                        double h10 = t3 - 2 * t2 + t;
                        double h01 = -2 * t3 + 3 * t2;
                        double h11 = t3 - t2;
                        v = h00 * sorted[i].Y + h10 * h * mm[i] + h01 * sorted[i + 1].Y + h11 * h * mm[i + 1];
                        break;
                    }
                }
            }
            result.Add(((float)u, (float)Math.Clamp(v, 0, 1)));
        }
        return result;
    }
}

// ═══════════════════ Shortcuts reference ═══════════════════

public class ShortcutsDialog
{
    public void Show(Window owner)
    {
        var items = new List<(string, string)>
        {
            ("V / B / N / E", "Move / Brush / Pencil / Eraser"),
            ("M / J / L / W", "Rect / Ellipse / Lasso / Magic Wand"),
            ("I / G / R", "Eyedropper / Bucket / Gradient"),
            ("T / U / C", "Text / Shape / Crop"),
            ("Ctrl+T", "Free Transform"),
            ("[ / ]", "Decrease / increase brush size"),
            ("Ctrl+Z / Ctrl+Y", "Undo / Redo"),
            ("Ctrl+A / Ctrl+D / Ctrl+Shift+I", "Select all / Deselect / Invert"),
            ("Ctrl+N / O / S", "New / Open / Save"),
            ("Ctrl+Shift+E", "Export As…"),
            ("Ctrl+Shift+N / Ctrl+J / Ctrl+E", "New layer / Duplicate / Merge down"),
            ("Ctrl+0 / Ctrl+1 / Ctrl+±", "Fit / 100% / Zoom"),
            ("Delete", "Clear selection content"),
            ("Space + drag / Middle drag", "Pan canvas"),
            ("Wheel", "Zoom at cursor (Shift+Wheel: pan)"),
            ("Shift while selecting", "Add to selection"),
            ("Alt while selecting", "Subtract from selection"),
            ("Pen (Windows Ink)", "Pressure-sensitive strokes"),
        };
        var list = new ListBox { MaxHeight = 420, MinWidth = 460 };
        var rows = new ObservableCollection<string>(items.Select(i => $"{i.Item1,-32}  {i.Item2}"));
        list.ItemsSource = rows;
        var grid = new StackPanel { Margin = new Thickness(22), Spacing = 10 };
        grid.Children.Add(list);
        var close = new Button { Content = "Close", Padding = new Thickness(16, 4), HorizontalAlignment = HorizontalAlignment.Right };
        close.Click += (_, _) => ((Window)close.Tag!).Close();
        grid.Children.Add(close);
        var dlg = DialogUtil.Make("Keyboard Shortcuts", grid);
        close.Tag = dlg;
        _ = dlg.ShowDialog(owner);
    }
}
