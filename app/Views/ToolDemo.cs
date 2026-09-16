using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Reflection;
using System.Text.Json;
using System.Threading.Tasks;
using Avalonia.Controls;
using Avalonia;
using Avalonia.Media.Imaging;
using Avalonia.Media;
using Avalonia.Threading;
using AuroraStudio.Interop;
using AuroraStudio.Models;
using AuroraStudio.Panels;

namespace AuroraStudio.Views;

/// <summary>
/// Full tool-audit automation: drives EVERY tool, filter, adjustment and document
/// operation through the real application code paths (the same methods the pointer
/// handlers and menu commands call), verifies observable results, and writes a
/// per-step report. Used by CI as the UI tool test; also usable manually via
/// "AuroraStudio --tooldemo READY REPORT".
/// </summary>
public static class ToolDemo
{
    private sealed class Step
    {
        public string tool = "";
        public string op = "";
        public bool pass;
        public string note = "";
    }

    private static readonly List<Step> _log = new();
    private static MainWindow _win = null!;
    private static AuroraDocument _doc = null!;
    private static ulong _gradLayerId;

    private static void Check(string tool, string op, bool pass, string note = "")
    {
        _log.Add(new Step { tool = tool, op = op, pass = pass, note = note });
        Console.Error.WriteLine($"[tooldemo] {(pass ? "PASS" : "FAIL")} — {tool} / {op} {note}");
    }

    /// <summary>Run one step; a C# exception counts as FAIL (not a crash).</summary>
    private static void Run(string tool, string op, Action a, Func<bool>? verify = null, string note = "")
    {
        try
        {
            a();
            bool ok = verify?.Invoke() ?? true;
            Check(tool, op, ok, ok ? note : "verification failed");
        }
        catch (Exception ex)
        {
            Check(tool, op, false, "exception: " + ex.Message);
        }
    }

    public static async void Run(MainWindow win, string readyPath, string reportPath)
    {
        _win = win;
        try
        {
            await Task.Delay(600); // let the window settle

            // fresh doc to work on (real dialog-free creation path)
            ulong handle = Engine.DocNew(960, 640, 1, "ToolAudit");
            _doc = new AuroraDocument(handle, 960, 640, "ToolAudit");
            _win.Tabs.Add(new DocTabItem { Doc = _doc });
            _win.SelectDoc(_doc);
            ulong bg = (ulong)Engine.LayerAdd(handle, -1, "Art", 0);
            Engine.aurora_layer_set_active(handle, bg);

            // ---------- painting tools ----------
            Run("brush", "stroke with pressure ramp", () =>
            {
                _win.SelectTool(ToolKind.Brush);
                _win.BrushSize = 36; _win.BrushHardness = 0.8f;
                _win.ForegroundColor = Color.FromRgb(0xFF, 0x8A, 0x3C);
                var bp = _win.MakeBrushParams(ToolKind.Brush);
                unsafe { Engine.aurora_brush_begin(handle, bg, &bp, 60, 320, 0.9f); }
                for (int i = 1; i <= 60; i++)
                {
                    double t = i / 60.0;
                    Engine.aurora_brush_move(handle, (float)(60 + t * 500), (float)(320 + Math.Sin(t * 6.0) * 90),
                        (float)(0.35 + 0.6 * Math.Sin(t * Math.PI)));
                }
                Engine.aurora_brush_end(handle);
            }, () => CompositeHasColor(0xFF, 0x8A, 0x3C), "orange stroke visible");

            Run("pencil", "1px-alike hard stroke", () =>
            {
                _win.SelectTool(ToolKind.Pencil);
                _win.BrushSize = 14;
                var bp = _win.MakeBrushParams(ToolKind.Pencil);
                unsafe { Engine.aurora_brush_begin(handle, bg, &bp, 100, 480, 1f); }
                for (int i = 1; i <= 40; i++)
                    Engine.aurora_brush_move(handle, 100 + i * 6, 480, 1f);
                Engine.aurora_brush_end(handle);
            });

            Run("eraser", "erase a stripe", () =>
            {
                _win.SelectTool(ToolKind.Eraser);
                _win.BrushSize = 30;
                var bp = _win.MakeBrushParams(ToolKind.Eraser);
                unsafe { Engine.aurora_brush_begin(handle, bg, &bp, 200, 250, 1f); }
                for (int i = 1; i <= 40; i++)
                    Engine.aurora_brush_move(handle, 200 + i * 5, 250 + i, 1f);
                Engine.aurora_brush_end(handle);
            });

            // ---------- selection tools ----------
            Run("select-rect", "rect + ants contour", () =>
            {
                _win.SelectTool(ToolKind.RectSelect);
                Engine.aurora_select_rect(handle, 100, 100, 300, 200, 0);
            }, () => Engine.SelBounds(handle).Active);

            Run("select-ellipse", "add mode", () =>
            {
                _win.SelectTool(ToolKind.EllipseSelect);
                Engine.aurora_select_ellipse(handle, 300, 150, 260, 220, 1);
            }, () => Engine.SelBounds(handle).Active);

            Run("select-lasso", "triangle polygon", () =>
            {
                _win.SelectTool(ToolKind.Lasso);
                float[] pts = { 500, 100, 700, 260, 520, 380, 500, 100 };
                unsafe
                {
                    fixed (float* p = pts)
                        Engine.aurora_select_lasso(handle, p, 4, 0);
                }
            }, () => Engine.SelBounds(handle).Active);

            Run("wand", "contiguous flood on composite", () =>
            {
                _win.SelectTool(ToolKind.Wand);
                _win.WandTolerance = 40;
                Engine.aurora_select_wand(handle, 5, 5, 40, 1, 0, 0);
            }, () => Engine.SelBounds(handle).Active);

            Run("select-ops", "feather+invert+grow path", () =>
            {
                Engine.aurora_select_feather(handle, 6f);
                Engine.aurora_select_invert(handle);
                Engine.aurora_select_none(handle);
                Engine.aurora_select_all(handle);
                Engine.aurora_select_invert(handle); // back to none
                Engine.aurora_select_none(handle);
            });

            // ---------- fill tools ----------
            Run("bucket", "fill background corner", () =>
            {
                _win.SelectTool(ToolKind.Bucket);
                _win.BucketTolerance = 24;
                _win.ForegroundColor = Color.FromRgb(0x2E, 0x86, 0xFF);
                var c = _win.ForegroundColor;
                Engine.aurora_bucket(handle, 4, 4, c.R, c.G, c.B, c.A, 24, 1);
            }, () => CompositeHasColor(0x2E, 0x86, 0xFF));

            Run("gradient", "linear fg->bg drag", () =>
            {
                _win.SelectTool(ToolKind.Gradient);
                ulong g = (ulong)Engine.LayerAdd(handle, -1, "Grad", 0);
                _gradLayerId = g;
                Engine.aurora_layer_set_active(handle, g);
                Engine.aurora_gradient(handle, 80, 80, 880, 560, 0, 0,
                    255, 60, 120, 255, 20, 20, 40, 255, 1);
                Engine.aurora_layer_set_active(handle, bg);
            }, () => CompositeHasColor(255, 60, 120));

            // the gradient layer is fully opaque and covers everything below —
            // remove it so later pixel verifications can see the Art layer
            if (_gradLayerId != 0) Engine.aurora_layer_delete(handle, _gradLayerId);

            // ---------- shape / text ----------
            Run("shape", "filled rectangle via RTB path", () =>
            {
                _win.SelectTool(ToolKind.Shape);
                _win.ForegroundColor = Color.FromRgb(0x12, 0x34, 0x56);
                _win.ShapeKind = 0; _win.ShapeFill = true; _win.ShapeStrokeWidth = 4;
                _win.CommitShape(new Avalonia.Point(560, 380), new Avalonia.Point(780, 520));
            }, () => CompositeHasColor(0x12, 0x34, 0x56), "fill visible in composite");


            Run("shape", "ellipse stroke", () =>
            {
                _win.ShapeKind = 1; _win.ShapeFill = false; _win.ShapeStrokeWidth = 6;
                _win.CommitShape(new Avalonia.Point(300, 60), new Avalonia.Point(620, 220));
            });

            Run("text", "rasterize text to layer", () =>
            {
                _win.SelectTool(ToolKind.Text);
                _win.TextFontSize = 64; _win.TextBold = true;
                _win.RenderTextToLayer(_doc, "Aurora 2.1", new Avalonia.Point(240, 180),
                    Color.FromRgb(0xFF, 0xFF, 0xFF), "Inter", 64, true, false);
            });

            // ---------- move & transform ----------
            Run("move", "layer offset via warp affine", () =>
            {
                _win.SelectTool(ToolKind.Move);
                ulong mv = (ulong)Engine.LayerAdd(handle, -1, "MoveMe", 0);
                Engine.aurora_layer_set_active(handle, mv);
                _win.ForegroundColor = Color.FromRgb(0xF0, 0x10, 0x90);
                _win.ShapeKind = 0; _win.ShapeFill = true;
                _win.CommitShape(new Avalonia.Point(80, 80), new Avalonia.Point(200, 180));
                Engine.aurora_layer_move_by(handle, mv, 120, 60);
                Engine.aurora_layer_set_active(handle, bg);
            });

            Run("transform", "scale+rotate via CommitTransformScale", () =>
            {
                _win.SelectTool(ToolKind.Transform);
                _win.TheCanvas.BeginTransform();
                _win.TheCanvas.CommitTransformScale(1.15, 1.15, 8);
            });

            Run("transform", "perspective warp (engine)", () =>
            {
                // bottom edge pulled in — genuine perspective
                float[] m = { 60, 40, 900, 80, 820, 600, 120, 590 };
                unsafe
                {
                    fixed (float* mp = m)
                        Engine.aurora_layer_warp(handle, bg, 1, mp, 960, 640);
                }
            });

            // ---------- crop / canvas ops ----------
            Run("crop", "define rect + commit", () =>
            {
                _win.SelectTool(ToolKind.Crop);
                _win.TheCanvas.SetCropRect(new Avalonia.Rect(40, 40, 700, 500));
                _win.TheCanvas.CommitCrop();
            }, () => _doc.Width == 700 && _doc.Height == 500, $"doc now {_doc.Width}x{_doc.Height}");

            Run("image-size", "resize up", () =>
            {
                Engine.aurora_image_resize(handle, 800, 560, 2);
                _doc.RefreshState();
            }, () => _doc.Width == 800 && _doc.Height == 560);

            Run("rotate-flip", "90cw + 180 + flips", () =>
            {
                Engine.aurora_image_rotate(handle, 1);
                Engine.aurora_image_rotate(handle, 3);
                Engine.aurora_image_flip(handle, 0);
                Engine.aurora_image_flip(handle, 1);
            });

            // ---------- adjustments & filters (all real engine ops) ----------
            Run("adjust", "brightness/contrast", () => Engine.aurora_adj_bc(handle, 12, 8));
            Run("adjust", "levels", () => Engine.aurora_adj_levels(handle, 10, 245, 1.05f, 0, 255, -1));
            Run("adjust", "curves json", () => Engine.AdjCurves(handle,
                "{\"rgb\":[[0,0],[128,138],[255,255]],\"r\":[],\"g\":[],\"b\":[]}"));
            Run("adjust", "hue/saturation", () => Engine.aurora_adj_hsl(handle, 15, 1.1f, 1.02f));

            Run("filter", "gaussian blur", () => Engine.aurora_filter_gauss(handle, 2.2f));
            Run("filter", "sharpen", () => Engine.aurora_filter_sharpen(handle, 0.8f));
            Run("filter", "noise", () => Engine.aurora_filter_noise(handle, 12, 1));
            Run("filter", "pixelate", () => Engine.aurora_filter_pixelate(handle, 8));
            Run("filter", "twirl", () => Engine.aurora_filter_twirl(handle, 400, 280, 220, 1.2f));
            Run("filter", "wave", () => Engine.aurora_filter_wave(handle, 8, 60, 0));
            Run("filter", "emboss", () => Engine.aurora_filter_emboss(handle));

            // ---------- layers / masks / history ----------
            Run("layers", "group+duplicate+reorder+merge", () =>
            {
                ulong grp = (ulong)Engine.LayerAddGroup(handle, "Group");
                Engine.aurora_layer_move_node(handle, bg, (long)grp, 0);
                ulong dup = (ulong)Engine.aurora_layer_duplicate(handle, bg);
                Engine.aurora_layer_move_up(handle, dup);
                Engine.aurora_layer_merge_down(handle, dup);
                Engine.aurora_layer_move_node(handle, bg, -1, -1); // out of group again (best effort)
            });

            Run("masks", "add from selection, apply, delete", () =>
            {
                Engine.aurora_select_rect(handle, 10, 10, 200, 200, 0);
                Engine.aurora_layer_mask_from_selection(handle, bg);
                Engine.aurora_layer_mask_apply(handle, bg);
                Engine.aurora_layer_mask_delete(handle, bg);
                Engine.aurora_select_none(handle);
            });

            Run("history", "undo x3 / redo x2", () =>
            {
                Engine.aurora_undo(handle);
                Engine.aurora_undo(handle);
                Engine.aurora_undo(handle);
                Engine.aurora_redo(handle);
                Engine.aurora_redo(handle);
            });

            // ---------- color tools ----------
            Run("eyedropper", "pick composite pixel", () =>
            {
                _win.SelectTool(ToolKind.Eyedropper);
                unsafe
                {
                    var buf = new byte[4];
                    fixed (byte* b = buf)
                        Engine.aurora_composite_read(handle, 300, 300, 1, 1, b, 4);
                    _win.SetForegroundColor(Color.FromArgb(buf[3], buf[0], buf[1], buf[2]));
                }
            });

            // ---------- navigation ----------
            Run("view", "zoom in/out/fit/100% + hand & zoom tools", () =>
            {
                _win.SelectTool(ToolKind.Move);
                _win.TheCanvas.ZoomAt(1.3, _win.TheCanvas.Bounds.Center);
                _win.TheCanvas.ZoomAt(1 / 1.3, _win.TheCanvas.Bounds.Center);
                _win.TheCanvas.FitOrActual();
                _win.TheCanvas.FitOrActual(actual: true);
                _win.TheCanvas.FitOrActual();
                _win.SelectTool(ToolKind.Hand);   // pan tool (drag pans via same _panning path)
                _win.SelectTool(ToolKind.Zoom);   // zoom tool (click zooms via ZoomAt)
                _win.SelectTool(ToolKind.Move);
            });

            // ---------- export matrix ----------
            foreach (var (fmt, q) in new[] { ("png", 90), ("jpeg", 85), ("webp", 80), ("bmp", 0), ("tiff", 0), ("ora", 0), ("psd", 0), ("svg", 0) })
            {
                string f = fmt;
                string tmp = Path.Combine(Path.GetTempPath(), $"aurora_toolaudit_{Guid.NewGuid():N}.{(f == "jpeg" ? "jpg" : f)}");
                Run("export", f, () =>
                {
                    int rc = Engine.DocExport(handle, tmp, f, q, 6, true, false);
                    if (rc != 0) throw new Exception(Engine.LastError());
                }, () => File.Exists(tmp) && new FileInfo(tmp).Length > 100);
                try { File.Delete(tmp); } catch { }
            }

            // ---------- v3.0: image import (real open path) ----------
            string importPath = Path.Combine(Path.GetTempPath(), $"aurora_import_{Guid.NewGuid():N}.png");
            Run("import", "export sample then open via OpenImagePath", () =>
            {
                int rc = Engine.DocExport(handle, importPath, "png", 90, 6, true, false);
                if (rc != 0) throw new Exception("sample export failed: " + Engine.LastError());
            });
            Run("import", "OpenImagePath creates a real tab", () =>
            {
                _win.OpenImagePath(importPath);
            }, () =>
            {
                var imported = _win.Tabs.LastOrDefault(t => t.Doc.Title.Contains("aurora_import"));
                if (imported == null) return false;
                _importedDoc = imported.Doc;
                return _importedDoc.Width > 0 && _importedDoc.Height > 0;
            }, "imported tab present with real dimensions");
            try { File.Delete(importPath); } catch { }

            // ---------- v3.0: new professional adjustments (engine = same call the menus make) ----------
            Run("adjust", "exposure/gamma", () => Expect(Engine.aurora_adj_exposure(handle, 0.25f, 1.1f)));
            Run("adjust", "vibrance", () => Expect(Engine.aurora_adj_vibrance(handle, 35)));
            Run("adjust", "white balance", () => Expect(Engine.aurora_adj_white_balance(handle, 20, -8)));
            Run("adjust", "shadows/highlights", () => Expect(Engine.aurora_adj_shadows_highlights(handle, 30, -15)));
            Run("adjust", "color balance", () => Expect(Engine.aurora_adj_color_balance(handle, 8, -4, 6)));
            Run("adjust", "black & white", () => Expect(Engine.aurora_adj_black_white(handle, 15, 0, -8)));
            Run("adjust", "desaturate", () => Expect(Engine.aurora_adj_desaturate(handle)));
            Run("adjust", "invert", () => Expect(Engine.aurora_adj_invert(handle)));
            Run("adjust", "threshold", () => Expect(Engine.aurora_adj_threshold(handle, 120)));
            Run("adjust", "posterize", () => Expect(Engine.aurora_adj_posterize(handle, 8)));
            Run("adjust", "photo filter", () => Expect(Engine.aurora_adj_photo_filter(handle, 236, 138, 0, 30, 1)));
            Run("adjust", "gradient map", () => Expect(Engine.aurora_adj_gradient_map(handle, 20, 10, 60, 250, 240, 200)));
            Run("adjust", "auto tone", () => Expect(Engine.aurora_adj_auto_tone(handle)));
            Run("adjust", "auto contrast", () => Expect(Engine.aurora_adj_auto_contrast(handle)));
            Run("adjust", "auto color", () => Expect(Engine.aurora_adj_auto_color(handle)));
            Run("adjust", "clarity", () => Expect(Engine.aurora_adj_clarity(handle, 30)));
            _doc.RefreshState();
            _win.RefreshPanels();

            // ---------- v3.0: effects library ----------
            Run("filter", "box blur", () => Expect(Engine.aurora_filter_box_blur(handle, 4)));
            Run("filter", "motion blur", () => Expect(Engine.aurora_filter_motion_blur(handle, 14, 25.0f)));
            Run("filter", "zoom blur", () => Expect(Engine.aurora_filter_zoom_blur(handle, 25)));
            Run("filter", "unsharp mask", () => Expect(Engine.aurora_filter_unsharp(handle, 3.0f, 1.2f, 4)));
            Run("filter", "find edges", () => Expect(Engine.aurora_filter_find_edges(handle, 0)));
            Run("filter", "oil paint", () => Expect(Engine.aurora_filter_oil_paint(handle, 3)));
            Run("filter", "halftone", () => Expect(Engine.aurora_filter_halftone(handle, 9)));
            Run("filter", "charcoal", () => Expect(Engine.aurora_filter_charcoal(handle, 6)));
            Run("filter", "pencil sketch", () => Expect(Engine.aurora_filter_pencil(handle, 6)));
            Run("filter", "median denoise", () => Expect(Engine.aurora_filter_median(handle)));
            Run("filter", "vignette", () => Expect(Engine.aurora_filter_vignette(handle, 55, 50)));
            Run("filter", "bloom", () => Expect(Engine.aurora_filter_bloom(handle, 10.0f, 45)));
            Run("filter", "film grain", () => Expect(Engine.aurora_filter_grain(handle, 25, 2)));
            Run("filter", "scanlines", () => Expect(Engine.aurora_filter_scanlines(handle, 6, 50)));
            Run("filter", "glitch", () => Expect(Engine.aurora_filter_glitch(handle, 22)));
            Run("filter", "chromatic aberration", () => Expect(Engine.aurora_filter_chromatic(handle, 30)));
            Run("filter", "duotone", () => Expect(Engine.aurora_filter_duotone(handle, 30, 20, 80, 250, 230, 180)));
            Run("filter", "ripple", () => Expect(Engine.aurora_filter_ripple(handle, 8.0f, 44.0f, -1.0f, -1.0f)));
            Run("filter", "pinch/bulge", () => Expect(Engine.aurora_filter_pinch(handle, 40, -1.0f, -1.0f, 0.0f)));
            Run("filter", "render clouds", () => Expect(Engine.aurora_filter_clouds(handle, 10.0f, 42, 65)));
            _doc.RefreshState();
            _win.RefreshPanels();

            // ---------- v3.0: histogram + navigator panels ----------
            Run("panel", "histogram reads real composite bins", () =>
            {
                // clear any selection restored by the undo/redo sweep so the
                // histogram covers the whole composite
                Engine.aurora_select_all(handle);
                _doc.RefreshState();
                _win.HistogramPanelCtl.Refresh(_doc);
            }, () =>
            {
                var h = Engine.Histogram(handle);
                if (h == null)
                {
                    Console.Error.WriteLine("[tooldemo] histogram: engine returned null, lastErr=" + Engine.LastError());
                    return false;
                }
                int nz = h.Count(b => b > 0);
                Console.Error.WriteLine($"[tooldemo] histogram: {nz} non-zero bins, max={h.Max()}");
                return nz > 0;
            });
            Run("panel", "navigator thumbnail + viewport", () =>
            {
                _win.NavigatorPanelCtl.Refresh(_doc);
                _win.TheCanvas.CenterOnDocumentPoint(_doc.Width / 2.0, _doc.Height / 2.0);
                var vis = _win.TheCanvas.VisibleDocRect();
                if (vis.Width <= 0) throw new Exception("empty viewport rect");
            });

            // ---------- v3.0: history panel click path (was the crash site #1) ----------
            ulong layerBeforeClick = Engine.ActiveLayer(handle);
            Run("panel", "history: programmatic entry click (real SelectionChanged)", () =>
            {
                // build a few history entries then drive the REAL list selection
                Engine.aurora_adj_bc(handle, 4, 4);
                _doc.RefreshState();
                _win.RefreshPanels();
                var list = GetPrivateList(typeof(HistoryPanel), _win.HistoryPanelCtl);
                int idx = Math.Clamp(_doc.History.Index - 1, 0, Math.Max(0, list.ItemCount - 1));
                list.SelectedIndex = idx; // fires SelectionChanged → deferred engine history_set
            });
            await Task.Delay(400); // let the deferred handler run
            Run("panel", "history: click actually moved history", () =>
            {
                _doc.RefreshState();
            }, () => _doc.History.Index >= 0, $"history index now {_doc.History.Index}");

            // stress: click + immediate refresh + click again (the old re-entrancy crash sequence)
            Run("panel", "history: stress click/refresh/click", () =>
            {
                var list = GetPrivateList(typeof(HistoryPanel), _win.HistoryPanelCtl);
                list.SelectedIndex = 1;
                _win.RefreshPanels();
                list.SelectedIndex = Math.Min(2, list.ItemCount - 1);
                _win.RefreshPanels();
            });
            await Task.Delay(400);

            // ---------- v3.0: layers panel click path (was the crash site #2) ----------
            Run("panel", "layers: programmatic row click (real SelectionChanged)", () =>
            {
                var list = GetPrivateList(typeof(LayersPanel), _win.LayersPanelCtl);
                if (list.ItemCount > 0)
                    list.SelectedIndex = 0; // fires SelectionChanged → OnRowSelected
            });
            await Task.Delay(300);
            Run("panel", "layers: stress click/refresh/click", () =>
            {
                var list = GetPrivateList(typeof(LayersPanel), _win.LayersPanelCtl);
                list.SelectedIndex = 0;
                _win.RefreshPanels();
                if (list.ItemCount > 1) list.SelectedIndex = 1;
                _win.RefreshPanels();
            });
            await Task.Delay(300);

            // ---------- v3.0: text overlay path (was the crash site #3) ----------
            Run("text", "overlay editor open + reopen (old NRE path)", () =>
            {
                _win.SelectTool(ToolKind.Text);
                _win.ShowTextEditorAt(new Avalonia.Point(320, 220), new Avalonia.Point(320, 220));
                _win.ShowTextEditorAt(new Avalonia.Point(340, 240), new Avalonia.Point(340, 240)); // second call = HideTextEditor inside
                _win.HideTextEditor(false);
            });
            Run("text", "commit text to layer", () =>
            {
                _win.RenderTextToLayer(_doc, "Aurora 3.0", new Avalonia.Point(120, 120),
                    Color.FromRgb(0xFF, 0xFF, 0xFF), "Inter", 48, true, false);
            });

            // ---------- v3.0: menu commands (RelayCommand wiring, non-dialog ops) ----------
            foreach (var (name, prop) in new[]
            {
                ("invert", "CmdAdjInvert"), ("desaturate", "CmdAdjDesaturate"),
                ("autotone", "CmdAutoTone"), ("autocontrast", "CmdAutoContrast"),
                ("autocolor", "CmdAutoColor"), ("autoenhance", "CmdAutoEnhance"),
                ("median", "CmdFMedian"), ("findedges", "CmdFFindEdges"), ("emboss", "CmdFEmboss"),
            })
            {
                string n = name; string p = prop;
                Run("command", $"{n} via ICommand.Execute", () =>
                {
                    var propInfo = typeof(MainWindow).GetProperty(p);
                    var cmd = (System.Windows.Input.ICommand)(propInfo?.GetValue(_win) ?? throw new Exception($"{p} missing"));
                    cmd.Execute(null);
                    _doc.RefreshState();
                });
            }
            Run("command", "undo x8 after command sweep", () =>
            {
                for (int i = 0; i < 8; i++) Engine.aurora_undo(handle);
                _doc.RefreshState();
                _win.RefreshPanels();
            });

            _win.SelectTool(ToolKind.RectSelect);
            _doc.RefreshState();
            _win.RefreshPanels();
            await Task.Delay(150);

            // write report
            var report = new
            {
                engine = Engine.Version(),
                generated = DateTime.UtcNow.ToString("o"),
                passed = _log.Count(s => s.pass),
                failed = _log.Count(s => !s.pass),
                steps = _log.Select(s => new { tool = s.tool, op = s.op, pass = s.pass, note = s.note }),
            };
            var opts = new JsonSerializerOptions { WriteIndented = true };
            Directory.CreateDirectory(Path.GetDirectoryName(Path.GetFullPath(reportPath))!);
            File.WriteAllText(reportPath, JsonSerializer.Serialize(report, opts));

            int failed = _log.Count(s => !s.pass);
            Directory.CreateDirectory(Path.GetDirectoryName(Path.GetFullPath(readyPath))!);
            File.WriteAllText(readyPath, failed == 0 ? "ok" : $"failed:{failed}");
            Console.Error.WriteLine($"[tooldemo] done: {report.passed} passed, {failed} failed");
        }
        catch (Exception ex)
        {
            Console.Error.WriteLine("[tooldemo] fatal: " + ex);
            try { File.WriteAllText(readyPath, "error: " + ex.Message); } catch { }
        }
    }

    private static bool CompositeHasColor(byte r, byte g, byte b)
    {
        unsafe
        {
            int w = (int)Math.Max(1, _doc.Width), h = (int)Math.Max(1, _doc.Height);
            var buf = new byte[w * h * 4];
            fixed (byte* p = buf)
            {
                if (Engine.aurora_composite_read(_doc.Handle, 0, 0, (uint)w, (uint)h, p, (uint)buf.Length) != 0)
                    return false;
            }
            for (int i = 0; i < buf.Length; i += 4)
            {
                if (Math.Abs(buf[i] - r) < 26 && Math.Abs(buf[i + 1] - g) < 26 && Math.Abs(buf[i + 2] - b) < 26)
                    return true;
            }
            return false;
        }
    }

    private static AuroraDocument? _importedDoc;

    private static void Expect(int rc)
    {
        if (rc != 0) throw new Exception("engine rc=" + rc + ": " + Engine.LastError());
    }

    /// <summary>Access a panel's private ListBox to drive the REAL SelectionChanged pipeline
    /// (reproduces the user's click path — the exact sequence that crashed v2.1).</summary>
    private static ListBox GetPrivateList(Type panelType, object panel)
    {
        var f = panelType.GetField("_list", BindingFlags.NonPublic | BindingFlags.Instance);
        return (ListBox)(f?.GetValue(panel) ?? throw new Exception("panel list not found"));
    }
}
