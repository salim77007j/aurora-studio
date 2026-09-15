using System;
using System.IO;
using System.Linq;
using System.Threading.Tasks;
using Avalonia.Controls;
using Avalonia.Media;
using Avalonia.Threading;
using AuroraStudio.Interop;
using AuroraStudio.Models;

namespace AuroraStudio.Views;

/// <summary>
/// Automated demo: drives the REAL application through genuine engine operations
/// (document creation, painting via the brush pipeline, layers, filters, selection)
/// so that screenshots show actual app state — no mockups. Used by CI verification.
/// </summary>
public static class AutoDemo
{
    public static async void Run(MainWindow win, string readyPath)
    {
        try
        {
            await Task.Delay(600); // let the window settle

            // 1. fresh document with painted content
            var handle = Engine.DocNew(1200, 800, 1, "Aurora-Demo");
            var doc = new AuroraDocument(handle, 1200, 800, "Aurora-Demo");
            win.Tabs.Add(new DocTabItem { Doc = doc });
            win.SelectDoc(doc);

            // background gradient (real engine gradient op — uses the active layer)
            Engine.aurora_gradient(handle, 0, 0, 1200, 800, 0, 0,
                38, 58, 128, 255, 12, 14, 20, 255, 1);

            // 2. layer with brush strokes through the REAL brush pipeline
            ulong artId = (ulong)Engine.LayerAdd(handle, -1, "Painting", 0);
            Engine.aurora_layer_set_active(handle, artId);

            win.SelectTool(ToolKind.Brush);
            win.BrushSize = 42;
            win.BrushHardness = 0.75f;
            win.ForegroundColor = Color.FromRgb(0xFF, 0x9A, 0x3C);
            var bp = win.MakeBrushParams(ToolKind.Brush);
            unsafe
            {
                // orange wave stroke with pressure variation
                Engine.aurora_brush_begin(handle, artId, &bp, 80, 420, 0.9f);
                for (int i = 1; i <= 90; i++)
                {
                    double t = i / 90.0;
                    double x = 80 + t * 1000;
                    double y = 420 + Math.Sin(t * Math.PI * 2.2) * 150;
                    Engine.aurora_brush_move(handle, (float)x, (float)y, (float)(0.45f + 0.5 * Math.Sin(t * Math.PI)));
                    if (i % 12 == 0) win.TheCanvas.PollComposite();
                }
                Engine.aurora_brush_end(handle);
            }

            // teal accent stroke
            win.ForegroundColor = Color.FromRgb(0x35, 0xD0, 0xC5);
            bp = win.MakeBrushParams(ToolKind.Brush);
            unsafe
            {
                Engine.aurora_brush_begin(handle, artId, &bp, 180, 620, 1.0f);
                for (int i = 1; i <= 70; i++)
                {
                    double t = i / 70.0;
                    double x = 180 + t * 800;
                    double y = 620 + Math.Cos(t * Math.PI * 1.8) * 90;
                    Engine.aurora_brush_move(handle, (float)x, (float)y, 0.7f);
                }
                Engine.aurora_brush_end(handle);
            }

            // 3. highlight ellipse (separate layer, blend mode Screen)
            ulong glowId = (ulong)Engine.LayerAdd(handle, -1, "Glow", 0);
            Engine.aurora_layer_set_blend(handle, glowId, 2); // Screen
            Engine.aurora_layer_set_opacity(handle, glowId, 0.85f);

            // warm radial glow via the radial gradient (on the glow layer)
            Engine.aurora_layer_set_active(handle, glowId);
            Engine.aurora_gradient(handle, 820, 230, 660, 120, 1, 0,
                255, 214, 140, 255, 255, 214, 140, 0, 0);
            Engine.aurora_layer_set_active(handle, artId);

            // 4. magic wand selection + filter on the painting layer (real filter)
            Engine.aurora_select_rect(handle, 260, 260, 420, 300, 0);
            Engine.aurora_select_feather(handle, 18f);
            Engine.aurora_filter_gauss(handle, 2.5f);
            Engine.aurora_select_none(handle);

            doc.RefreshState();
            win.RefreshPanels();
            win.OnImageStructureChanged();

            // 5. second document tab to show multi-document support
            ulong h2 = (ulong)Engine.DocNew(640, 480, 1, "Untitled-2");
            var doc2 = new AuroraDocument(h2, 640, 480, "Untitled-2");
            Engine.aurora_gradient(h2, 0, 0, 640, 480, 0, 0,
                200, 60, 60, 255, 30, 30, 60, 255, 1);
            doc2.RefreshState();
            win.Tabs.Add(new DocTabItem { Doc = doc2 });

            // 6. UI states: select wand tool so options bar shows its real controls
            win.SelectTool(ToolKind.RectSelect);
            await Task.Delay(150);

            // signal ready for external capture
            Directory.CreateDirectory(Path.GetDirectoryName(readyPath)!);
            File.WriteAllText(readyPath, "ok");
        }
        catch (Exception ex)
        {
            Console.Error.WriteLine("[autodemo] failed: " + ex);
            try { File.WriteAllText(readyPath, "error: " + ex.Message); } catch { }
        }
    }
}
