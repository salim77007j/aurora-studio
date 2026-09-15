using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Text.Json;
using System.Threading.Tasks;
using Avalonia.Controls;
using Avalonia;
using Avalonia.Media.Imaging;
using Avalonia.Media;
using Avalonia.Threading;
using AuroraStudio.Interop;
using AuroraStudio.Models;

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
}
