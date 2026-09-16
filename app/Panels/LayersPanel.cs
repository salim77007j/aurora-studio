using System;
using System.Collections.Generic;
using System.Collections.ObjectModel;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Controls.Shapes;
using Avalonia.Media;
using Avalonia.Controls.Templates;
using Avalonia.Media.Imaging;
using Avalonia.Input;
using Avalonia.Layout;
using Avalonia.Threading;
using AuroraStudio.Interop;
using AuroraStudio.Models;
using AuroraStudio.Controls;
using AuroraStudio.Views;

namespace AuroraStudio.Panels;

public class LayerRow
{
    public LayerInfo Info { get; init; } = null!;
    public int Depth { get; init; }
    public string Name => Info?.Name ?? "(layer)";
    public bool IsGroup => Info?.IsGroup ?? false;
    public bool Visible => Info?.Visible ?? true;
    public string ThumbLabel => Info?.IsGroup == true ? "\u229e" : "";
    public IBrush ThumbBg { get; set; } = Brushes.Transparent;
    public Bitmap? ThumbImage { get; set; }
    public bool Selected => _win != null && _win.ActiveDoc?.State.ActiveId == Info?.Id;
    public MainWindow? _win;
    public IBrush RowBg => Selected ? new SolidColorBrush(Color.FromRgb(0x33, 0x46, 0x5F)) : Brushes.Transparent;
}

/// <summary>Layers panel: tree with thumbnails, visibility, blend, opacity, full layer ops.</summary>
/// <remarks>
/// Crash-proofing notes (v3.0): the old version called the XAML-generated field
/// `MainWindow.Canvas` from the SelectionChanged handler — that field is never
/// populated with the runtime XAML loader, so every row click threw
/// NullReferenceException (the user's "crashes when I use certain tools").
/// Now every control access goes through null-safe accessors, collection rebuilds
/// detach the ItemsSource first (no SelectionModel re-entrancy), and every event
/// body is wrapped so an exception can never propagate into Avalonia internals.
/// </remarks>
public class LayersPanel : Border
{
    private readonly MainWindow _win;
    private readonly ObservableCollection<LayerRow> _rows = new();
    private readonly ListBox _list;
    private readonly ComboBox _blend = new();
    private readonly Slider _opacity = new() { Minimum = 0, Maximum = 100, Value = 100 };
    private readonly TextBlock _nameLabel = new();
    private bool _updating;

    public LayersPanel(MainWindow win)
    {
        _win = win;
        _list = new ListBox { ItemsSource = _rows, MaxHeight = 260, MinHeight = 80 };
        _list.ItemTemplate = MakeRowTemplate();
        _list.SelectionChanged += OnRowSelected;

        _blend.ItemsSource = BlendModeNames.Names;
        _blend.SelectedIndex = 0;
        _blend.SelectionChanged += (_, _) =>
        {
            if (_updating || _blend.SelectedIndex < 0) return;
            try
            {
                var row = SelectedRow();
                var doc = _win.ActiveDoc;
                if (row?.Info != null && doc != null)
                {
                    Engine.aurora_layer_set_blend(doc.Handle, row.Info.Id, _blend.SelectedIndex);
                    doc.RefreshState();
                    _win.TheCanvas.InvalidateAnts();
                }
            }
            catch (Exception ex) { Program.WriteCrash("LAYERS", ex); }
        };
        _opacity.ValueChanged += (_, e) =>
        {
            if (_updating) return;
            try
            {
                var row = SelectedRow();
                var doc = _win.ActiveDoc;
                if (row?.Info != null && doc != null)
                {
                    Engine.aurora_layer_set_opacity(doc.Handle, row.Info.Id, (float)(e.NewValue / 100.0));
                    doc.RefreshState();
                }
            }
            catch (Exception ex) { Program.WriteCrash("LAYERS", ex); }
        };

        var buttons = new WrapPanel { Orientation = Avalonia.Layout.Orientation.Horizontal, Margin = new Thickness(0, 6, 0, 0) };
        buttons.Children.Add(IconBtn("act.plus", "New Layer", CmdLayerNew));
        buttons.Children.Add(IconBtn("act.group", "New Group", CmdLayerGroup));
        buttons.Children.Add(IconBtn("act.duplicate", "Duplicate", CmdLayerDup));
        buttons.Children.Add(IconBtn("act.trash", "Delete", CmdLayerDel));
        buttons.Children.Add(IconBtn("act.mask", "Add Mask from Selection", CmdMaskAdd));
        buttons.Children.Add(IconBtn("act.check", "Apply Mask", CmdMaskApply));
        buttons.Children.Add(IconBtn("act.merge", "Merge Down", CmdMergeDown));
        buttons.Children.Add(IconBtn("act.up", "Move Up", CmdLayerUp));
        buttons.Children.Add(IconBtn("act.down", "Move Down", CmdLayerDown));

        var props = new StackPanel { Orientation = Orientation.Vertical, Spacing = 4, Margin = new Thickness(0, 6, 0, 0) };
        _nameLabel.Foreground = ColorPanel.Brush(0xB9, 0xBE, 0xC5);
        _nameLabel.FontSize = 11.5;
        props.Children.Add(_nameLabel);
        props.Children.Add(_blend);
        var opRow = new StackPanel { Orientation = Avalonia.Layout.Orientation.Horizontal, Spacing = 6 };
        opRow.Children.Add(new TextBlock { Text = "Opacity", FontSize = 11.5, Foreground = ColorPanel.Brush(0x8A, 0x8F, 0x96), VerticalAlignment = Avalonia.Layout.VerticalAlignment.Center });
        opRow.Children.Add(_opacity);
        props.Children.Add(opRow);

        var root = new StackPanel { Spacing = 4 };
        root.Children.Add(ColorPanel.PanelHeader("Layers"));
        root.Children.Add(_list);
        root.Children.Add(props);
        root.Children.Add(buttons);

        Child = new Border
        {
            Background = ColorPanel.Brush(0x26, 0x28, 0x2D),
            CornerRadius = new CornerRadius(8),
            Padding = new Thickness(10),
            Child = root,
        };
    }

    private FuncDataTemplate<LayerRow> MakeRowTemplate()
    {
        return new FuncDataTemplate<LayerRow>((row, _) =>
        {
            var panel = new Panel();
            var content = new Border { Padding = new Thickness(4, 2, 4, 2), CornerRadius = new CornerRadius(3) };
            var sp = new StackPanel { Orientation = Avalonia.Layout.Orientation.Horizontal, Spacing = 8 };
            sp.Margin = new Thickness(row?.Depth * 16 ?? 0, 0, 0, 0);

            // visibility toggle
            var eye = new TextBlock { Text = row?.Visible == true ? "◉" : "◌", Width = 18, VerticalAlignment = Avalonia.Layout.VerticalAlignment.Center, Cursor = new Cursor(Avalonia.Input.StandardCursorType.Hand), FontSize = 13 };
            eye.Tapped += (_, _) =>
            {
                try
                {
                    if (row?._win is MainWindow w && w.ActiveDoc != null && row.Info != null)
                    {
                        Engine.aurora_layer_set_visible(w.ActiveDoc.Handle, row.Info.Id, row.Visible ? 0 : 1);
                        w.ActiveDoc.RefreshState();
                        w.RefreshPanels();
                    }
                }
                catch (Exception ex) { Program.WriteCrash("LAYERS", ex); }
            };
            sp.Children.Add(eye);

            // thumbnail
            var thumb = new Border
            {
                Width = 30, Height = 30,
                CornerRadius = new CornerRadius(3),
                Background = row?.ThumbBg ?? Brushes.Transparent,
                Child = row?.ThumbImage is Bitmap img
                    ? new Image { Source = img, Width = 28, Height = 28, Stretch = Stretch.Uniform }
                    : new TextBlock { Text = row?.ThumbLabel ?? "", HorizontalAlignment = Avalonia.Layout.HorizontalAlignment.Center, VerticalAlignment = Avalonia.Layout.VerticalAlignment.Center, Foreground = ColorPanel.Brush(0x8A, 0x8F, 0x96) },
            };
            sp.Children.Add(thumb);

            var name = new TextBlock { Text = row?.Name ?? "", VerticalAlignment = Avalonia.Layout.VerticalAlignment.Center, FontSize = 12.5 };
            name.Tapped += (_, _) =>
            {
                try
                {
                    if (row?._win is MainWindow w && w.ActiveDoc != null && row.Info != null)
                    {
                        Engine.aurora_layer_set_active(w.ActiveDoc.Handle, row.Info.Id);
                        w.ActiveDoc.RefreshState();
                        w.RefreshPanels();
                    }
                }
                catch (Exception ex) { Program.WriteCrash("LAYERS", ex); }
            };
            sp.Children.Add(name);
            content.Child = sp;
            panel.Children.Add(content);
            panel.DataContext = row;
            return panel;
        });
    }

    private LayerRow? SelectedRow()
    {
        if (_list.SelectedItem is LayerRow r && r.Info != null) return r;
        return null;
    }

    public void Refresh(AuroraDocument? doc)
    {
        _updating = true;
        try
        {
            // Detach first: the SelectionModel must never observe a partial list
            // (this was the source of the ArgumentOutOfRangeException re-entrancy).
            _list.ItemsSource = null;
            _rows.Clear();
            if (doc == null)
            {
                _list.ItemsSource = _rows;
                _nameLabel.Text = "";
                return;
            }
            // top-first display
            var flat = new List<(LayerRow Row, LayerInfo Info)>();
            void Walk(List<LayerInfo> list, int depth)
            {
                foreach (var li in list)
                {
                    var row = new LayerRow { Info = li, Depth = depth, _win = _win };
                    row.ThumbBg = Checker();
                    _rows.Add(row);
                    flat.Add((row, li));
                    if (li.Children != null && li.Children.Count > 0 && li.Expanded)
                        Walk(li.Children, depth + 1);
                }
            }
            Walk(doc.State.Layers, 0);

            // thumbnails (first 24 layers to keep refresh snappy)
            int count = Math.Min(flat.Count, 24);
            for (int i = 0; i < count; i++)
            {
                var (row, li) = flat[i];
                if (li.IsGroup) continue;
                try
                {
                    var buf = new byte[32 * 32 * 4];
                    int need = Engine.LayerThumbnail(doc.Handle, li.Id, 32, buf);
                    if (need == buf.Length)
                    {
                        var wb = new WriteableBitmap(new PixelSize(32, 32), new Vector(96, 96),
                            Avalonia.Platform.PixelFormat.Rgba8888, Avalonia.Platform.AlphaFormat.Unpremul);
                        using (var frame = wb.Lock())
                        unsafe
                        {
                            fixed (byte* p = buf)
                            {
                                byte* dst = (byte*)frame.Address;
                                for (int y2 = 0; y2 < 32; y2++)
                                    Buffer.MemoryCopy(p + y2 * 32 * 4, dst + (long)y2 * frame.RowBytes, 32 * 4, 32 * 4);
                            }
                        }
                        row.ThumbImage = wb;
                    }
                }
                catch { /* thumbnail is best-effort */ }
            }

            var sel = doc.State.FindLayer(doc.State.Layers, doc.State.ActiveId);
            if (sel != null)
            {
                _nameLabel.Text = sel.Name;
                _blend.SelectedIndex = Math.Clamp(sel.Blend, 0, BlendModeNames.Names.Length - 1);
                _opacity.Value = sel.Opacity * 100;
            }
            _list.ItemsSource = _rows;

            // restore selection highlight without re-triggering engine calls
            int selIdx = -1;
            for (int i = 0; i < _rows.Count; i++)
                if (_rows[i].Info?.Id == doc.State.ActiveId) { selIdx = i; break; }
            if (selIdx >= 0) _list.SelectedIndex = selIdx;
        }
        catch (Exception ex)
        {
            Program.WriteCrash("LAYERS", ex);
        }
        finally
        {
            _updating = false;
            _list.InvalidateVisual();
        }
    }

    private static IBrush Checker()
    {
        var b = new LinearGradientBrush
        {
            GradientStops =
            {
                new GradientStop { Color = Color.FromRgb(0x3A, 0x3D, 0x44), Offset = 0 },
                new GradientStop { Color = Color.FromRgb(0x2A, 0x2C, 0x31), Offset = 1 },
            },
        };
        return b;
    }

    private Button IconBtn(string icon, string tip, Action action)
    {
        var path = new Path
        {
            Data = Icons.Get(icon),
            Stroke = ColorPanel.Brush(0xAA, 0xAF, 0xB6),
            StrokeThickness = 1.5,
            StrokeLineCap = PenLineCap.Round,
            Fill = null,
            Width = 15, Height = 15,
            Stretch = Stretch.Uniform,
        };
        var b = new Button
        {
            Content = path,
            Width = 30, Height = 26,
            Padding = new Thickness(2),
        };
        ToolTip.SetTip(b, tip);
        b.Click += (_, _) =>
        {
            try { action(); }
            catch (Exception ex) { Program.WriteCrash("LAYERS", ex); _win.SetStatusMessage("Layer op failed: " + ex.Message); }
        };
        return b;
    }

    // ---------- layer ops ----------

    private void CmdLayerNew()
    {
        var doc = _win.ActiveDoc;
        if (doc == null) return;
        Engine.LayerAdd(doc.Handle, -1, $"Layer {doc.State.Layers.Count + 1}", 0);
        doc.RefreshState();
        _win.RefreshPanels();
    }
    private void CmdLayerGroup()
    {
        var doc = _win.ActiveDoc;
        if (doc == null) return;
        Engine.LayerAddGroup(doc.Handle, "Group");
        doc.RefreshState();
        _win.RefreshPanels();
    }
    private void CmdLayerDup()
    {
        var doc = _win.ActiveDoc;
        if (doc == null) return;
        Engine.aurora_layer_duplicate(doc.Handle, doc.State.ActiveId);
        doc.RefreshState();
        _win.RefreshPanels();
    }
    private void CmdLayerDel()
    {
        var doc = _win.ActiveDoc;
        if (doc == null) return;
        Engine.aurora_layer_delete(doc.Handle, doc.State.ActiveId);
        doc.RefreshState();
        _win.RefreshPanels();
    }
    private void CmdMaskAdd()
    {
        var doc = _win.ActiveDoc;
        if (doc == null) return;
        int rc = Engine.aurora_layer_mask_from_selection(doc.Handle, doc.State.ActiveId);
        _win.SetStatusMessage(rc == 0 ? "Mask added from selection" : "Mask: " + Engine.LastError());
        doc.RefreshState();
        _win.RefreshPanels();
    }
    private void CmdMaskApply()
    {
        var doc = _win.ActiveDoc;
        if (doc == null) return;
        Engine.aurora_layer_mask_apply(doc.Handle, doc.State.ActiveId);
        doc.RefreshState();
        _win.RefreshPanels();
    }
    private void CmdMergeDown()
    {
        var doc = _win.ActiveDoc;
        if (doc == null) return;
        int rc = Engine.aurora_layer_merge_down(doc.Handle, doc.State.ActiveId);
        if (rc != 0) _win.SetStatusMessage(Engine.LastError());
        doc.RefreshState();
        _win.RefreshPanels();
    }
    private void CmdLayerUp()
    {
        var doc = _win.ActiveDoc;
        if (doc == null) return;
        Engine.aurora_layer_move_node(doc.Handle, doc.State.ActiveId, -1, -2);
        doc.RefreshState();
        _win.RefreshPanels();
    }
    private void CmdLayerDown()
    {
        var doc = _win.ActiveDoc;
        if (doc == null) return;
        Engine.aurora_layer_move_node(doc.Handle, doc.State.ActiveId, -1, -3);
        doc.RefreshState();
        _win.RefreshPanels();
    }

    private void OnRowSelected(object? sender, SelectionChangedEventArgs e)
    {
        if (_updating) return;
        try
        {
            var row = SelectedRow();
            var doc = _win.ActiveDoc;
            if (row?.Info != null && doc != null)
            {
                Engine.aurora_layer_set_active(doc.Handle, row.Info.Id);
                doc.RefreshState();
                _win.TheCanvas.InvalidateAnts();
            }
        }
        catch (Exception ex)
        {
            Program.WriteCrash("LAYERS", ex);
        }
    }
}
