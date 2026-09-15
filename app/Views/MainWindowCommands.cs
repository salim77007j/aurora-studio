using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Text.Json;
using System.Threading.Tasks;
using System.Windows.Input;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Input;
using Avalonia.Media;
using Avalonia.Platform.Storage;
using Avalonia.Threading;
using AuroraStudio.Controls;
using AuroraStudio.Dialogs;
using AuroraStudio.Interop;
using AuroraStudio.Models;

namespace AuroraStudio.Views;

/// <summary>Menu commands, dialogs, keyboard shortcuts, workspace.</summary>
public partial class MainWindow : Window
{
    public ICommand CmdNew => Cmd("CmdNew", () => DoNew());
    public ICommand CmdOpen => Cmd("CmdOpen", () => DoOpen());
    public ICommand CmdSave => Cmd("CmdSave", () => DoSave());
    public ICommand CmdExport => Cmd("CmdExport", () => DoExport());
    public ICommand CmdCloseDoc => Cmd("CmdCloseDoc", () => DoCloseDoc());
    public ICommand CmdExit => Cmd("CmdExit", () => Close());
    public ICommand CmdUndo => Cmd("CmdUndo", () => DoUndo());
    public ICommand CmdRedo => Cmd("CmdRedo", () => DoRedo());
    public ICommand CmdFill => Cmd("CmdFill", () => DoFill());
    public ICommand CmdClear => Cmd("CmdClear", () => DoClear());
    public ICommand CmdShortcuts => Cmd("CmdShortcuts", () => new ShortcutsDialog().Show(this));
    public ICommand CmdPrefs => Cmd("CmdPrefs", () => ShowAbout("Preferences", "Accent color, panel visibility and shortcuts are stored in\n%APPDATA%\\AuroraStudio\\settings.json.\n\nUse View → Workspace to switch panel layouts."));
    public ICommand CmdImageSize => Cmd("CmdImageSize", () => DoImageSize());
    public ICommand CmdCanvasSize => Cmd("CmdCanvasSize", () => DoCanvasSize());
    public ICommand CmdRot90 => Cmd("CmdRot90", () => { var d = Doc(); if (d != null) { Engine.aurora_image_rotate(d.Handle, 1); RefreshAll(); } });
    public ICommand CmdRot180 => Cmd("CmdRot180", () => { var d = Doc(); if (d != null) { Engine.aurora_image_rotate(d.Handle, 2); RefreshAll(); } });
    public ICommand CmdRot270 => Cmd("CmdRot270", () => { var d = Doc(); if (d != null) { Engine.aurora_image_rotate(d.Handle, 3); RefreshAll(); } });
    public ICommand CmdFlipH => Cmd("CmdFlipH", () => { var d = Doc(); if (d != null) { Engine.aurora_image_flip(d.Handle, 0); RefreshAll(); } });
    public ICommand CmdFlipV => Cmd("CmdFlipV", () => { var d = Doc(); if (d != null) { Engine.aurora_image_flip(d.Handle, 1); RefreshAll(); } });
    public ICommand CmdCropSel => Cmd("CmdCropSel", () => DoCropSelection());
    public ICommand CmdFlatten => Cmd("CmdFlatten", () => { var d = Doc(); if (d != null) { Engine.aurora_doc_flatten(d.Handle); RefreshAll(); } });

    public ICommand CmdLayerNew => Cmd("CmdLayerNew", () => { var d = Doc(); if (d != null) { Engine.LayerAdd(d.Handle, -1, "Layer", 0); RefreshAll(); } });
    public ICommand CmdLayerGroup => Cmd("CmdLayerGroup", () => { var d = Doc(); if (d != null) { Engine.LayerAddGroup(d.Handle, "Group"); RefreshAll(); } });
    public ICommand CmdLayerDup => Cmd("CmdLayerDup", () => { var d = Doc(); if (d != null) { Engine.aurora_layer_duplicate(d.Handle, d.State.ActiveId); RefreshAll(); } });
    public ICommand CmdLayerDel => Cmd("CmdLayerDel", () => { var d = Doc(); if (d != null) { Engine.aurora_layer_delete(d.Handle, d.State.ActiveId); RefreshAll(); } });
    public ICommand CmdMaskAdd => Cmd("CmdMaskAdd", () => { var d = Doc(); if (d != null) { Engine.aurora_layer_mask_from_selection(d.Handle, d.State.ActiveId); RefreshAll(); } });
    public ICommand CmdMaskApply => Cmd("CmdMaskApply", () => { var d = Doc(); if (d != null) { Engine.aurora_layer_mask_apply(d.Handle, d.State.ActiveId); RefreshAll(); } });
    public ICommand CmdMaskDel => Cmd("CmdMaskDel", () => { var d = Doc(); if (d != null) { Engine.aurora_layer_mask_delete(d.Handle, d.State.ActiveId); RefreshAll(); } });
    public ICommand CmdMergeDown => Cmd("CmdMergeDown", () => { var d = Doc(); if (d != null) { Engine.aurora_layer_merge_down(d.Handle, d.State.ActiveId); RefreshAll(); } });
    public ICommand CmdLayerUp => Cmd("CmdLayerUp", () => { var d = Doc(); if (d != null) { Engine.aurora_layer_move_up(d.Handle, d.State.ActiveId); RefreshAll(); } });
    public ICommand CmdLayerDown => Cmd("CmdLayerDown", () => { var d = Doc(); if (d != null) { Engine.aurora_layer_move_down(d.Handle, d.State.ActiveId); RefreshAll(); } });

    public ICommand CmdSelAll => Cmd("CmdSelAll", () => { var d = Doc(); if (d != null) { Engine.aurora_select_all(d.Handle); TheCanvas.InvalidateAnts(); } });
    public ICommand CmdSelNone => Cmd("CmdSelNone", () => { var d = Doc(); if (d != null) { Engine.aurora_select_none(d.Handle); TheCanvas.InvalidateAnts(); } });
    public ICommand CmdSelInvert => Cmd("CmdSelInvert", () => { var d = Doc(); if (d != null) { Engine.aurora_select_invert(d.Handle); TheCanvas.InvalidateAnts(); } });
    public ICommand CmdFeather => Cmd("CmdFeather", () => DoFeather());

    public ICommand CmdFGauss => Cmd("CmdFGauss", () => RunFilterDialog("Gaussian Blur", new[] { ("Radius", 0.5, 60.0, 3.0) },
        (h, v) => Engine.aurora_filter_gauss(h, (float)v[0])));
    public ICommand CmdFSharpen => Cmd("CmdFSharpen", () => RunFilterDialog("Sharpen", new[] { ("Amount", 0.0, 3.0, 0.8) },
        (h, v) => Engine.aurora_filter_sharpen(h, (float)v[0])));
    public ICommand CmdFNoise => Cmd("CmdFNoise", () => RunFilterDialog("Add Noise", new[] { ("Amount", 0.0, 100.0, 15.0) },
        (h, v) => Engine.aurora_filter_noise(h, (int)v[0], 1)));
    public ICommand CmdFPixelate => Cmd("CmdFPixelate", () => RunFilterDialog("Pixelate", new[] { ("Block size", 2.0, 200.0, 12.0) },
        (h, v) => Engine.aurora_filter_pixelate(h, (int)v[0])));
    public ICommand CmdFTwirl => Cmd("CmdFTwirl", () => RunFilterDialogCentered("Twirl", new[] { ("Angle °", -720.0, 720.0, 180.0), ("Radius", 10.0, 2000.0, 300.0) },
        (h, v, cx, cy) => Engine.aurora_filter_twirl(h, (float)cx, (float)cy, (float)v[1], (float)v[0])));
    public ICommand CmdFWave => Cmd("CmdFWave", () => RunFilterDialog("Wave", new[] { ("Amplitude", 1.0, 300.0, 20.0), ("Wavelength", 4.0, 500.0, 80.0) },
        (h, v) => Engine.aurora_filter_wave(h, (float)v[0], (float)v[1], 0)));
    public ICommand CmdFEmboss => Cmd("CmdFEmboss", () => ApplySimple("Emboss", h => Engine.aurora_filter_emboss(h)));

    public ICommand CmdAdjBC => Cmd("CmdAdjBC", () => RunFilterDialog("Brightness / Contrast", new[]
    {
        ("Brightness", -100.0, 100.0, 0.0), ("Contrast", -100.0, 100.0, 0.0),
    }, (h, v) => Engine.aurora_adj_bc(h, (float)v[0], (float)v[1])));
    public ICommand CmdAdjHSL => Cmd("CmdAdjHSL", () => RunFilterDialog("Hue / Saturation", new[]
    {
        ("Hue", -180.0, 180.0, 0.0), ("Saturation", -100.0, 100.0, 0.0), ("Lightness", -100.0, 100.0, 0.0),
    }, (h, v) => Engine.aurora_adj_hsl(h, (float)v[0], (float)v[1], (float)v[2])));
    public ICommand CmdAdjLevels => Cmd("CmdAdjLevels", () => DoLevels());
    public ICommand CmdAdjCurves => Cmd("CmdAdjCurves", () => DoCurves());

    public ICommand CmdZoomIn => Cmd("CmdZoomIn", () => TheCanvas.ZoomAt(1.25, TheCanvas.Bounds.Center));
    public ICommand CmdZoomOut => Cmd("CmdZoomOut", () => TheCanvas.ZoomAt(1 / 1.25, TheCanvas.Bounds.Center));
    public ICommand CmdZoomFit => Cmd("CmdZoomFit", () => TheCanvas.FitOrActual());
    public ICommand CmdZoom100 => Cmd("CmdZoom100", () => TheCanvas.FitOrActual(actual: true));
    public ICommand CmdWsDefault => Cmd("CmdWsDefault", () => SetWorkspace(true, true, true));
    public ICommand CmdWsMinimal => Cmd("CmdWsMinimal", () => SetWorkspace(false, false, false));
    public ICommand CmdWsPainting => Cmd("CmdWsPainting", () => SetWorkspace(true, false, false));
    public ICommand CmdToggleHistory => Cmd("CmdToggleHistory", () => ToggleHistory());

    public ICommand CmdAbout => Cmd("CmdAbout", () => ShowAbout("Aurora Studio",
        "Aurora Studio 2.0 — professional image editor & digital painting.\n\n" +
        "Native desktop application (Avalonia UI + Rust engine).\n" +
        "No web technologies, no runtimes to install.\n\n" +
        "Layers · Blend modes · Masks · Pressure-sensitive brushes\n" +
        "Selections (rect / ellipse / lasso / wand, feather & invert)\n" +
        "Transforms · Filters · Curves & Levels · Full undo history\n" +
        "PNG · JPEG · WebP · GIF · BMP · TIFF · SVG · OpenRaster · PSD\n\n" +
        Interop.Engine.Version()));

    private readonly Dictionary<string, ICommand> _cmds = new();
    private ICommand Cmd(string key, Action action)
    {
        if (_cmds.TryGetValue(key, out var existing)) return existing;
        var c = new RelayCommand(action);
        _cmds[key] = c;
        return c;
    }

    private AuroraDocument? Doc() => ActiveDoc;
    private static byte[] Str(string s) => System.Text.Encoding.UTF8.GetBytes(s + "\0");

    private void RefreshAll()
    {
        var d = Doc();
        if (d == null) return;
        d.RefreshState();
        OnImageStructureChanged();
        RefreshPanels();
    }

    // ══════════ file ops ══════════

    private async void DoNew()
    {
        var dlg = new NewDocumentDialog();
        var cfg = await dlg.ShowDialogAsync(this);
        if (cfg != null)
        {
            var handle = Engine.DocNew(cfg.Value.w, cfg.Value.h, cfg.Value.bg, cfg.Value.name);
            if (handle == 0) { ShowError("New Document", Engine.LastError()); return; }
            var doc = new AuroraDocument(handle, cfg.Value.w, cfg.Value.h, cfg.Value.name);
            Tabs.Add(new DocTabItem { Doc = doc });
            SelectDoc(doc);
        }
    }

    private async void DoOpen()
    {
        var files = await StorageProvider.OpenFilePickerAsync(new FilePickerOpenOptions
        {
            Title = "Open Image",
            AllowMultiple = false,
            FileTypeFilter = new[]
            {
                new FilePickerFileType("All Supported Images")
                {
                    Patterns = new[] { "*.png", "*.jpg", "*.jpeg", "*.webp", "*.gif", "*.bmp", "*.tif", "*.tiff", "*.svg", "*.ora", "*.psd" }
                },
                FilePickerFileTypes.All,
            },
        });
        if (files.Count == 0) return;
        var path = files[0].Path.LocalPath;
        await Task.Run(() =>
        {
            try
            {
                var handle = Engine.DocOpen(path);
                if (handle == 0) throw new Exception(Engine.LastError());
                Dispatcher.UIThread.Post(() =>
                {
                    var doc = new AuroraDocument(handle, 1, 1, Path.GetFileName(path)) { FilePath = path };
                    doc.RefreshState();
                    Tabs.Add(new DocTabItem { Doc = doc });
                    SelectDoc(doc);
                    SetStatusMessage("Opened " + path);
                });
            }
            catch (Exception ex)
            {
                Dispatcher.UIThread.Post(() => ShowError("Open failed", ex.Message));
            }
        });
    }

    private void DoSave()
    {
        var d = Doc();
        if (d == null) return;
        if (d.FilePath == null) { DoExport(); return; }
        var fmt = EngineLocalFormat.detect(d.FilePath);
        int rc = Engine.DocExport(d.Handle, d.FilePath, fmt, 92, 6, true, false);
        SetStatusMessage(rc == 0 ? "Saved " + d.FilePath : "Save failed: " + Engine.LastError());
    }

    private async void DoExport()
    {
        var d = Doc();
        if (d == null) return;
        var dlg = new ExportDialog(d);
        var opts = await dlg.ShowDialogAsync(this);
        if (opts == null || opts.Path == null) return;
        int rc = Engine.DocExport(d.Handle, opts.Path, opts.Format, opts.Quality, opts.Compression, true, opts.Frames);
        if (rc == 0)
        {
            SetStatusMessage($"Exported: {opts.Path}");
        }
        else
        {
            ShowError("Export failed", Engine.LastError());
        }
    }

    private void DoCloseDoc()
    {
        var t = Tabs.FirstOrDefault(x => x._selected);
        if (t == null) return;
        Tabs.Remove(t);
        t.Doc.Dispose();
        if (Tabs.Count == 0) CreateInitialDocument();
        else SelectDoc(Tabs[0].Doc);
    }

    // ══════════ edit ops ══════════

    private void DoUndo() { var d = Doc(); if (d != null) { Engine.aurora_undo(d.Handle); d.RefreshState(); RefreshPanels(); TheCanvas.InvalidateAnts(); TheCanvas.PollComposite(); UpdateStatus(); } }
    private void DoRedo() { var d = Doc(); if (d != null) { Engine.aurora_redo(d.Handle); d.RefreshState(); RefreshPanels(); TheCanvas.InvalidateAnts(); TheCanvas.PollComposite(); UpdateStatus(); } }

    private void DoFill()
    {
        var d = Doc();
        if (d == null) return;
        Engine.aurora_fill(d.Handle, ForegroundColor.R, ForegroundColor.G, ForegroundColor.B, ForegroundColor.A);
        d.RefreshState();
        RefreshPanels();
    }

    private void DoClear()
    {
        var d = Doc();
        if (d == null) return;
        Engine.aurora_layer_clear(d.Handle);
        d.RefreshState();
        RefreshPanels();
    }

    private async void DoFeather()
    {
        var d = Doc();
        if (d == null) return;
        var dlg = new FilterParamDialog("Feather Selection", new[] { ("Radius", 1.0, 250.0, 8.0) }, null);
        var (ok, vals) = await dlg.ShowDialogAsync(this);
        if (ok)
        {
            Engine.aurora_select_feather(d.Handle, (float)vals[0]);
            d.RefreshState();
            TheCanvas.InvalidateAnts();
        }
    }

    // ══════════ image ops ══════════

    private async void DoImageSize()
    {
        var d = Doc();
        if (d == null) return;
        var dlg = new ImageSizeDialog(d.Width, d.Height, d.Width, d.Height, "Image Size");
        var res = await dlg.ShowDialogAsync(this);
        if (res == null) return;
        var (nw, nh, interp, _) = res.Value;
        if (nw == 0 || nh == 0) return;
        Engine.aurora_image_resize(d.Handle, nw, nh, interp);
        RefreshAll();
    }

    private async void DoCanvasSize()
    {
        var d = Doc();
        if (d == null) return;
        var dlg = new ImageSizeDialog(d.Width, d.Height, d.Width, d.Height, "Canvas Size", allowAnchor: true);
        var res = await dlg.ShowDialogAsync(this);
        if (res == null) return;
        var (nw, nh, _, anchor) = res.Value;
        if (nw == 0 || nh == 0) return;
        Engine.aurora_canvas_resize(d.Handle, nw, nh, anchor);
        RefreshAll();
    }

    private void DoCropSelection()
    {
        var d = Doc();
        if (d == null) return;
        var b = Engine.SelBounds(d.Handle);
        if (!b.Active) { SetStatusMessage("No selection to crop to"); return; }
        Engine.aurora_doc_crop(d.Handle, b.X, b.Y, b.W, b.H);
        RefreshAll();
    }

    // ══════════ filter / adjustment dialogs with live preview ══════════

    private void ApplySimple(string name, Func<ulong, int> apply)
    {
        var d = Doc();
        if (d == null) return;
        int rc = apply(d.Handle);
        SetStatusMessage(rc == 0 ? $"{name} applied" : $"{name}: {Engine.LastError()}");
        d.RefreshState();
        RefreshPanels();
    }

    /// <summary>Filter dialog with live preview: works on a cloned doc handle; commits to the real doc on OK.</summary>
    private async void RunFilterDialog(string title, (string Name, double Min, double Max, double Val)[] pars,
        Func<ulong, double[], int> apply)
    {
        var d = Doc();
        if (d == null) return;
        ulong preview = Engine.aurora_doc_clone(d.Handle);
        if (preview == 0) { ShowError("Preview", "Could not create preview: " + Engine.LastError()); return; }

        // temporarily display the preview document
        var prevDoc = new AuroraDocument(preview, d.Width, d.Height, title + " (preview)");
        prevDoc.RefreshState();
        SwitchDisplay(prevDoc);

        var dlg = new FilterParamDialog(title, pars, vals =>
        {
            // jump history back to 0 then apply fresh params
            Engine.aurora_history_set(preview, 0);
            apply(preview, vals);
            prevDoc.RefreshState();
            OnImageStructureChanged();
        });
        var (ok, vals) = await dlg.ShowDialogAsync(this);

        SwitchDisplay(d);
        prevDoc.Dispose();

        if (ok)
        {
            int rc = apply(d.Handle, vals);
            if (rc != 0) ShowError(title, Engine.LastError());
            d.RefreshState();
            RefreshPanels();
            OnImageStructureChanged();
        }
        else
        {
            d.RefreshState();
            OnImageStructureChanged();
        }
    }

    private async void RunFilterDialogCentered(string title, (string Name, double Min, double Max, double Val)[] pars,
        Func<ulong, double[], double, double, int> apply)
    {
        var d = Doc();
        if (d == null) return;
        ulong preview = Engine.aurora_doc_clone(d.Handle);
        if (preview == 0) { ShowError("Preview", "Could not create preview"); return; }
        var prevDoc = new AuroraDocument(preview, d.Width, d.Height, title + " (preview)");
        prevDoc.RefreshState();
        SwitchDisplay(prevDoc);

        double cx = d.Width / 2.0, cy = d.Height / 2.0;
        var dlg = new FilterParamDialog(title, pars, vals =>
        {
            Engine.aurora_history_set(preview, 0);
            apply(preview, vals, cx, cy);
            prevDoc.RefreshState();
            OnImageStructureChanged();
        }, note: $"Center: ({cx:0}, {cy:0}) (document center)");
        var (ok, vals) = await dlg.ShowDialogAsync(this);

        SwitchDisplay(d);
        prevDoc.Dispose();
        if (ok)
        {
            int rc = apply(d.Handle, vals, cx, cy);
            if (rc != 0) ShowError(title, Engine.LastError());
            d.RefreshState();
            RefreshPanels();
            OnImageStructureChanged();
        }
        else
        {
            d.RefreshState();
            OnImageStructureChanged();
        }
    }

    private async void DoLevels()
    {
        var d = Doc();
        if (d == null) return;
        ulong preview = Engine.aurora_doc_clone(d.Handle);
        if (preview == 0) return;
        var prevDoc = new AuroraDocument(preview, d.Width, d.Height, "Levels (preview)");
        prevDoc.RefreshState();
        SwitchDisplay(prevDoc);

        var dlg = new LevelsDialog((inB, inW, gamma, outB, outW, ch) =>
        {
            Engine.aurora_history_set(preview, 0);
            Engine.aurora_adj_levels(preview, (float)inB, (float)inW, (float)gamma, (float)outB, (float)outW, ch);
            prevDoc.RefreshState();
            OnImageStructureChanged();
        });
        var (ok, p) = await dlg.ShowDialogAsync(this);
        SwitchDisplay(d);
        prevDoc.Dispose();
        if (ok)
        {
            Engine.aurora_adj_levels(d.Handle, (float)p.inBlack, (float)p.inWhite, (float)p.gamma, (float)p.outBlack, (float)p.outWhite, p.channel);
            d.RefreshState();
            RefreshPanels();
            OnImageStructureChanged();
        }
        else { d.RefreshState(); OnImageStructureChanged(); }
    }

    private async void DoCurves()
    {
        var d = Doc();
        if (d == null) return;
        ulong preview = Engine.aurora_doc_clone(d.Handle);
        if (preview == 0) return;
        var prevDoc = new AuroraDocument(preview, d.Width, d.Height, "Curves (preview)");
        prevDoc.RefreshState();
        SwitchDisplay(prevDoc);

        var dlg = new CurvesDialog(json =>
        {
            Engine.aurora_history_set(preview, 0);
            Engine.AdjCurves(preview, json);
            prevDoc.RefreshState();
            OnImageStructureChanged();
        });
        var (ok, json) = await dlg.ShowDialogAsync(this);
        SwitchDisplay(d);
        prevDoc.Dispose();
        if (ok && !string.IsNullOrEmpty(json))
        {
            Engine.AdjCurves(d.Handle, json);
            d.RefreshState();
            RefreshPanels();
            OnImageStructureChanged();
        }
        else { d.RefreshState(); OnImageStructureChanged(); }
    }

    /// <summary>Temporarily display another document in the canvas (for filter previews).</summary>
    private void SwitchDisplay(AuroraDocument doc)
    {
        _displayOverride = doc;
        OnImageStructureChanged();
        TheCanvas.InvalidateAnts();
    }

    private AuroraDocument? _displayOverride;

    // ══════════ workspace & panels ══════════

    private void SetWorkspace(bool color, bool layers, bool history)
    {
        ColorPanelCtl.IsVisible = color;
        LayersPanelCtl.IsVisible = layers;
        HistoryPanelCtl.IsVisible = history;
        SetStatusMessage("Workspace updated");
    }

    private void ToggleHistory()
    {
        HistoryPanelCtl.IsVisible = !HistoryPanelCtl.IsVisible;
    }

    // ══════════ misc UI ══════════

    private void ShowError(string title, string message)
    {
        var dlg = new Window
        {
            Title = title,
            SizeToContent = SizeToContent.WidthAndHeight,
            WindowStartupLocation = WindowStartupLocation.CenterOwner,
            CanResize = false,
            Content = new StackPanel
            {
                Margin = new Thickness(20),
                Spacing = 14,
                Children =
                {
                    new TextBlock { Text = message, TextWrapping = TextWrapping.Wrap, MaxWidth = 420 },
                    new Button { Content = "OK", HorizontalAlignment = Avalonia.Layout.HorizontalAlignment.Right, Padding = new Thickness(18, 4) },
                },
            },
        };
        ((dlg.Content as StackPanel)!.Children[1] as Button)!.Click += (_, _) => dlg.Close();
        dlg.Show(this);
        SetStatusMessage(title + ": " + message);
    }

    private void ShowAbout(string title, string text)
    {
        ShowError(title, text);
    }

    // ══════════ keyboard shortcuts ══════════

    private void OnGlobalKeyDown(object? sender, KeyEventArgs e)
    {
        // ignore when typing in a text field
        if (this.FocusManager?.GetFocusedElement() is TextBox or ComboBoxItem or ComboBox or NumericUpDown)
        {
            return;
        }
        var doc = Doc();
        var mods = e.KeyModifiers;
        var key = e.Key;

        // tool shortcuts (single letters, no modifiers)
        if (mods == KeyModifiers.None && doc != null)
        {
            ToolKind? t = key switch
            {
                Key.V => ToolKind.Move,
                Key.B => ToolKind.Brush,
                Key.N => ToolKind.Pencil,
                Key.E => ToolKind.Eraser,
                Key.M => ToolKind.RectSelect,
                Key.J => ToolKind.EllipseSelect,
                Key.L => ToolKind.Lasso,
                Key.W => ToolKind.Wand,
                Key.I => ToolKind.Eyedropper,
                Key.G => ToolKind.Bucket,
                Key.R => ToolKind.Gradient,
                Key.T => ToolKind.Text,
                Key.U => ToolKind.Shape,
                Key.C => ToolKind.Crop,
                _ => null,
            };
            if (t != null)
            {
                SelectTool(t.Value);
                e.Handled = true;
                return;
            }
            if (key == Key.OemOpenBrackets || key == Key.OemMinus && false) { }
        }

        if (mods.HasFlag(KeyModifiers.Control))
        {
            switch (key)
            {
                case Key.N when mods.HasFlag(KeyModifiers.Shift): CmdLayerNew.Execute(null); e.Handled = true; break;
                case Key.N: DoNew(); e.Handled = true; break;
                case Key.O: DoOpen(); e.Handled = true; break;
                case Key.S: DoSave(); e.Handled = true; break;
                case Key.E when mods.HasFlag(KeyModifiers.Shift): DoExport(); e.Handled = true; break;
                case Key.E: CmdMergeDown.Execute(null); e.Handled = true; break;
                case Key.Z: DoUndo(); e.Handled = true; break;
                case Key.Y: DoRedo(); e.Handled = true; break;
                case Key.J: CmdLayerDup.Execute(null); e.Handled = true; break;
                case Key.A: CmdSelAll.Execute(null); e.Handled = true; break;
                case Key.D: CmdSelNone.Execute(null); e.Handled = true; break;
                case Key.I when mods.HasFlag(KeyModifiers.Shift): CmdSelInvert.Execute(null); e.Handled = true; break;
                case Key.T: SelectTool(ToolKind.Transform); TheCanvas.BeginTransform(); e.Handled = true; break;
                case Key.OemPlus or Key.Add: TheCanvas.ZoomAt(1.25, TheCanvas.Bounds.Center); e.Handled = true; break;
                case Key.OemMinus or Key.Subtract: TheCanvas.ZoomAt(1 / 1.25, TheCanvas.Bounds.Center); e.Handled = true; break;
                case Key.D0: TheCanvas.FitOrActual(); e.Handled = true; break;
                case Key.D1: TheCanvas.FitOrActual(actual: true); e.Handled = true; break;
                case Key.Delete or Key.Back: DoClear(); e.Handled = true; break;
            }
        }
        else if (mods == KeyModifiers.None && key == Key.Delete && doc != null)
        {
            DoClear();
            e.Handled = true;
        }
        // brush size via [ and ]
        else if (mods == KeyModifiers.None && key is Key.OemOpenBrackets or Key.OemCloseBrackets && doc != null)
        {
            BrushSize = Math.Clamp(BrushSize + (key == Key.OemCloseBrackets ? 5 : -5), 1, 400);
            BuildOptionsBar();
            SetStatusMessage($"Brush size: {BrushSize:0}px");
            e.Handled = true;
        }
    }
}

public class RelayCommand : System.Windows.Input.ICommand
{
    private readonly Action _action;
    public RelayCommand(Action a) => _action = a;
    public event EventHandler? CanExecuteChanged;
    public bool CanExecute(object? parameter) => true;
    public void Execute(object? parameter) => _action();
}

public static class EngineLocalFormat
{
    public static string detect(string path)
    {
        var lower = path.ToLowerInvariant();
        if (lower.EndsWith(".png")) return "png";
        if (lower.EndsWith(".jpg") || lower.EndsWith(".jpeg")) return "jpeg";
        if (lower.EndsWith(".webp")) return "webp";
        if (lower.EndsWith(".gif")) return "gif";
        if (lower.EndsWith(".bmp")) return "bmp";
        if (lower.EndsWith(".tif") || lower.EndsWith(".tiff")) return "tiff";
        if (lower.EndsWith(".svg")) return "svg";
        if (lower.EndsWith(".ora")) return "ora";
        if (lower.EndsWith(".psd")) return "psd";
        return "png";
    }
}


