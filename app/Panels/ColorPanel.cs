using System;
using System.Collections.Generic;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Input;
using Avalonia.Layout;
using Avalonia.Media;
using AuroraStudio.Interop;
using AuroraStudio.Models;
using AuroraStudio.Views;

namespace AuroraStudio.Panels;

/// <summary>Color panel: SV square + hue slider + RGB/HEX + swatches. Fully functional.</summary>
public class ColorPanel : Border
{
    private readonly SvSquare _sv = new();
    private readonly Slider _hue = new() { Minimum = 0, Maximum = 360, Height = 18, Margin = new Thickness(0, 8, 0, 0) };
    private readonly TextBox _hex = new() { MinWidth = 80 };
    private readonly TextBox _rBox = new(), _gBox = new(), _bBox = new();
    private readonly Panel _swatches = new StackPanel { Orientation = Avalonia.Layout.Orientation.Horizontal, Spacing = 4 };
    private readonly Panel _recent = new StackPanel { Orientation = Avalonia.Layout.Orientation.Horizontal, Spacing = 4 };
    private Color _color = Colors.Black;
    private bool _updating;
    private Panel? _hueWrapper;
    private readonly MainWindow _win;

    private static readonly Color[] Defaults =
    {
        Color.FromRgb(0, 0, 0), Color.FromRgb(255, 255, 255), Color.FromRgb(231, 76, 60), Color.FromRgb(230, 126, 34),
        Color.FromRgb(241, 196, 15), Color.FromRgb(46, 204, 113), Color.FromRgb(26, 188, 156), Color.FromRgb(52, 152, 219),
        Color.FromRgb(155, 89, 182), Color.FromRgb(149, 165, 166), Color.FromRgb(44, 62, 80), Color.FromRgb(127, 140, 141),
    };

    public ColorPanel()
    {
        _win = null!; // attached later
    }

    public ColorPanel(MainWindow win)
    {
        _win = win;
        BuildUi();
        SetForeground(Colors.Black);
    }

    private void BuildUi()
    {
        Padding = new Thickness(0);
        var root = new StackPanel { Spacing = 8 };

        var header = PanelHeader("Color");

        _sv.OnColorChanged = c => ApplyColor(c, fromSv: true);
        var hueGrad = new LinearGradientBrush
        {
            StartPoint = new RelativePoint(0, 0.5, RelativeUnit.Relative),
            EndPoint = new RelativePoint(1, 0.5, RelativeUnit.Relative),
        };
        for (int i = 0; i <= 6; i++)
            hueGrad.GradientStops.Add(new GradientStop { Color = HsvToRgb(i * 60, 1, 1, 255), Offset = i / 6.0 });
        _hue.Template = null;
        var hueTrack = new Border { Background = hueGrad, CornerRadius = new CornerRadius(3), Height = 12 };
        _hue.MinHeight = 12;
        // wrap slider over colored track
        var hueWrap = new Panel();
        hueWrap.Children.Add(hueTrack);
        hueWrap.Children.Add(_hue);
        _hueWrapper = hueWrap;
        _hue.ValueChanged += (_, e) =>
        {
            if (_updating) return;
            var c = _color;
            var (h, s, v) = RgbToHsv(c);
            ApplyColor(HsvToRgb(e.NewValue, s == 0 ? 1 : s, v == 0 ? 1 : v, c.A), fromSv: false);
        };

        var hexRow = new StackPanel { Orientation = Avalonia.Layout.Orientation.Horizontal, Spacing = 6 };
        hexRow.Children.Add(new TextBlock { Text = "HEX", VerticalAlignment = Avalonia.Layout.VerticalAlignment.Center, Foreground = Brush(0x8A, 0x8F, 0x96), FontSize = 11.5 });
        hexRow.Children.Add(_hex);
        _hex.TextChanged += (_, _) =>
        {
            if (_updating) return;
            try
            {
                var c = Color.Parse(_hex.Text!.Trim());
                ApplyColor(c, false);
            }
            catch { }
        };

        var rgbRow = new StackPanel { Orientation = Avalonia.Layout.Orientation.Horizontal, Spacing = 6 };
        rgbRow.Children.Add(RgbField("R", _rBox));
        rgbRow.Children.Add(RgbField("G", _gBox));
        rgbRow.Children.Add(RgbField("B", _bBox));
        _rBox.TextChanged += (_, _) => TryRgbUpdate();
        _gBox.TextChanged += (_, _) => TryRgbUpdate();
        _bBox.TextChanged += (_, _) => TryRgbUpdate();

        root.Children.Add(header);
        root.Children.Add(_sv);
        root.Children.Add(_hueWrapper!);
        root.Children.Add(hexRow);
        root.Children.Add(rgbRow);
        root.Children.Add(new TextBlock { Text = "Swatches", Foreground = Brush(0x8A, 0x8F, 0x96), FontSize = 11.5, Margin = new Thickness(0, 2, 0, 0) });
        foreach (var c in Defaults)
            _swatches.Children.Add(Swatch(c));
        root.Children.Add(_swatches);
        root.Children.Add(new TextBlock { Text = "Recent", Foreground = Brush(0x8A, 0x8F, 0x96), FontSize = 11.5 });
        root.Children.Add(_recent);

        Child = new Border
        {
            Background = Brush(0x2A, 0x2C, 0x31),
            CornerRadius = new CornerRadius(6),
            Padding = new Thickness(10),
            Child = root,
        };
    }

    private Control RgbField(string label, TextBox box)
    {
        var sp = new StackPanel { Orientation = Avalonia.Layout.Orientation.Vertical, Spacing = 2 };
        sp.Children.Add(new TextBlock { Text = label, FontSize = 10.5, Foreground = Brush(0x8A, 0x8F, 0x96) });
        box.Width = 56;
        box.Text = "0";
        sp.Children.Add(box);
        return sp;
    }

    private void TryRgbUpdate()
    {
        if (_updating) return;
        if (byte.TryParse(_rBox.Text, out var r) && byte.TryParse(_gBox.Text, out var g) && byte.TryParse(_bBox.Text, out var b))
            ApplyColor(Color.FromRgb(r, g, b), false);
    }

    private Control Swatch(Color c)
    {
        var b = new Border
        {
            Width = 20, Height = 20,
            CornerRadius = new CornerRadius(3),
            Background = new SolidColorBrush(c),
            BorderBrush = Brush(0x4A, 0x4D, 0x54),
            BorderThickness = new Thickness(1),
            Cursor = new Cursor(StandardCursorType.Hand),
        };
        b.PointerPressed += (_, _) => ApplyColor(c, false);
        return b;
    }

    public void AddRecent(Color c)
    {
        foreach (var child in _recent.Children)
            if (child is Border bb && ((SolidColorBrush)bb.Background!).Color.Equals(c))
                return;
        _recent.Children.Insert(0, Swatch(c));
        if (_recent.Children.Count > 10) _recent.Children.RemoveAt(_recent.Children.Count - 1);
    }

    private void ApplyColor(Color c, bool fromSv)
    {
        _updating = true;
        _color = c;
        if (!fromSv) _sv.SetColor(c);
        _hex.Text = $"#{c.R:X2}{c.G:X2}{c.B:X2}";
        _rBox.Text = c.R.ToString();
        _gBox.Text = c.G.ToString();
        _bBox.Text = c.B.ToString();
        _updating = false;
        if (_win != null)
        {
            _win.ForegroundColor = c;
            _win.SetStatusMessage($"Foreground: #{c.R:X2}{c.G:X2}{c.B:X2}");
            AddRecent(c);
        }
    }

    public void SetForeground(Color c) => ApplyColor(c, false);

    public void RefreshExternal() { /* color persists across doc switches */ }

    // ---------- SV square ----------

    private class SvSquare : Control
    {
        public Action<Color>? OnColorChanged;
        private Color _color = Colors.Black;
        private double _sx = 0, _sy = 0; // 0..1
        private static readonly IBrush HueRed = new SolidColorBrush(Color.FromRgb(255, 0, 0));

        public SvSquare()
        {
            Height = 110;
            MinWidth = 120;
            ClipToBounds = true;
        }

        public void SetColor(Color c)
        {
            _color = c;
            var (_, s, v) = ColorPanel.RgbToHsv(c);
            _sx = s;
            _sy = 1 - v;
            InvalidateVisual();
        }

        protected override void OnPointerPressed(PointerPressedEventArgs e)
        {
            e.Pointer.Capture(this);
            Update(e.GetCurrentPoint(this).Position);
        }
        protected override void OnPointerMoved(PointerEventArgs e)
        {
            if (e.GetCurrentPoint(this).Properties.IsLeftButtonPressed)
                Update(e.GetCurrentPoint(this).Position);
        }

        private void Update(Point p)
        {
            _sx = Math.Clamp(p.X / Bounds.Width, 0, 1);
            _sy = Math.Clamp(p.Y / Bounds.Height, 0, 1);
            var c = ColorPanel.HsvToRgb(ColorPanel.HueOf(_color), _sx, 1 - _sy, 255);
            _color = c;
            OnColorChanged?.Invoke(Color.FromRgb(c.R, c.G, c.B));
            InvalidateVisual();
        }

        public override void Render(DrawingContext ctx)
        {
            var r = new Rect(Bounds.Size);
            double hue = ColorPanel.HueOf(_color);
            var baseCol = ColorPanel.HsvToRgb(hue, 1, 1, 255);
            ctx.FillRectangle(new SolidColorBrush(Color.FromRgb(baseCol.R, baseCol.G, baseCol.B)), r);
            // white → transparent horizontal gradient
            var whiteGrad = new LinearGradientBrush
            {
                StartPoint = new RelativePoint(0, 0.5, RelativeUnit.Relative),
                EndPoint = new RelativePoint(1, 0.5, RelativeUnit.Relative),
                GradientStops =
                {
                    new GradientStop { Color = Colors.White, Offset = 0 },
                    new GradientStop { Color = Color.FromArgb(0, 255, 255, 255), Offset = 1 },
                },
            };
            ctx.FillRectangle(whiteGrad, r);
            var blackGrad = new LinearGradientBrush
            {
                StartPoint = new RelativePoint(0.5, 0, RelativeUnit.Relative),
                EndPoint = new RelativePoint(0.5, 1, RelativeUnit.Relative),
                GradientStops =
                {
                    new GradientStop { Color = Color.FromArgb(0, 0, 0, 0), Offset = 0 },
                    new GradientStop { Color = Colors.Black, Offset = 1 },
                },
            };
            ctx.FillRectangle(blackGrad, r);
            // cursor
            var cx = _sx * r.Width;
            var cy = _sy * r.Height;
            ctx.DrawEllipse(null, new Pen(Brushes.White, 1.5), new Point(cx, cy), 6, 6);
            ctx.DrawEllipse(null, new Pen(Brushes.Black, 1), new Point(cx, cy), 7.5, 7.5);
        }
    }

    // ---------- color math ----------

    public static double HueOf(Color c)
    {
        var (h, _, _) = RgbToHsv(c);
        return h;
    }

    public static (double H, double S, double V) RgbToHsv(Color c)
    {
        double r = c.R / 255.0, g = c.G / 255.0, b = c.B / 255.0;
        double max = Math.Max(r, Math.Max(g, b)), min = Math.Min(r, Math.Min(g, b));
        double d = max - min;
        double h = 0;
        if (d > 1e-6)
        {
            if (max == r) h = ((g - b) / d) % 6;
            else if (max == g) h = (b - r) / d + 2;
            else h = (r - g) / d + 4;
            h *= 60;
            if (h < 0) h += 360;
        }
        return (h, max <= 0 ? 0 : d / max, max);
    }

    public static Color HsvToRgb(double h, double s, double v, byte a)
    {
        h = (h % 360 + 360) % 360;
        double c = v * s;
        double x = c * (1 - Math.Abs(h / 60 % 2 - 1));
        double m = v - c;
        double r = 0, g = 0, b = 0;
        if (h < 60) { r = c; g = x; }
        else if (h < 120) { r = x; g = c; }
        else if (h < 180) { g = c; b = x; }
        else if (h < 240) { g = x; b = c; }
        else if (h < 300) { r = x; b = c; }
        else { r = c; b = x; }
        return Color.FromArgb(a, (byte)((r + m) * 255), (byte)((g + m) * 255), (byte)((b + m) * 255));
    }

    internal static IBrush Brush(byte r, byte g, byte b) => new SolidColorBrush(Color.FromRgb(r, g, b));

    internal static Border PanelHeader(string title)
    {
        var sp = new StackPanel { Orientation = Avalonia.Layout.Orientation.Horizontal, Spacing = 6 };
        sp.Children.Add(new TextBlock
        {
            Text = title.ToUpperInvariant(),
        });
        return new Border
        {
            Classes = { "panelHead" },
            Child = sp,
        };
    }
}
