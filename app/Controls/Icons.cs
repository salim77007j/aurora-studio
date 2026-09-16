using System.Collections.Generic;
using Avalonia;
using Avalonia.Media;

namespace AuroraStudio.Controls;

/// <summary>
/// Hand-crafted Fluent-style vector icons (24×24 grid, stroke-based, round caps).
/// Crisp at any DPI — no bitmap assets, no licensing concerns.
/// v3.0: refined geometry, consistent 1.6 stroke rhythm, +20 new panel/feature glyphs.
/// </summary>
public static class Icons
{
    private static readonly Dictionary<string, Geometry> Cache = new();

    public static Geometry Get(string name)
    {
        if (Cache.TryGetValue(name, out var g)) return g;
        var path = PathData.TryGetValue(name, out var d) ? d : "";
        g = Geometry.Parse(path);
        Cache[name] = g;
        return g;
    }

    public static bool Has(string name) => PathData.ContainsKey(name);

    private static readonly Dictionary<string, string> PathData = new()
    {
        // ---------------- tools ----------------
        ["tool.move"] = "M12 2 L12 22 M2 12 L22 12 M12 2 L8.5 5.5 M12 2 L15.5 5.5 M12 22 L8.5 18.5 M12 22 L15.5 18.5 M2 12 L5.5 8.5 M2 12 L5.5 15.5 M22 12 L18.5 8.5 M22 12 L18.5 15.5",
        ["tool.brush"] = "M20 4 C14 6 9 11 6.5 16 L8 17.5 C13 15 18 10 20 4 Z M6.5 16 C5 17 4.5 18.5 4.5 20 C6.5 20 8 19.5 8.8 17.9",
        ["tool.pencil"] = "M4 20 L5 16 L16 5 L19 8 L8 19 L4 20 Z M14 7 L17 10",
        ["tool.eraser"] = "M9 20 L4.5 15.5 C4 15 4 14 4.5 13.5 L13 5 C13.5 4.5 14.5 4.5 15 5 L20 10 C20.5 10.5 20.5 11.5 20 12 L12 20 Z M9 20 L21 20",
        ["tool.rectselect"] = "M4 4 L7 4 M9.5 4 L12.5 4 M15 4 L18 4 M20 4 L20 7 M20 9.5 L20 12.5 M20 15 L20 18 M20 20 L17 20 M14.5 20 L11.5 20 M9 20 L6 20 M4 20 L4 17 M4 14.5 L4 11.5 M4 9 L4 6 M4 6.5 L4 4 L6.5 4",
        ["tool.ellipseselect"] = "M12 5 C6.9 5 3 7.7 3 11 C3 14.3 6.9 17 12 17 C17.1 17 21 14.3 21 11 C21 7.7 17.1 5 12 5 Z",
        ["tool.lasso"] = "M12 4 C6.5 4 3 7 3 10.5 C3 13.5 6 15.5 9.5 16 M12 4 C17.5 4 21 7 21 10.5 C21 14 17.5 16.5 12 16.5 C11 16.5 10.2 16.4 9.5 16 M9.5 16 C8.5 16.3 8 17 8 17.8 C8 18.7 8.8 19.3 9.8 19.3 C10.9 19.3 11.6 18.6 11.6 17.6",
        ["tool.wand"] = "M6 18 L14.5 9.5 M12 4 L12.9 6.6 L15.5 7.5 L12.9 8.4 L12 11 L11.1 8.4 L8.5 7.5 L11.1 6.6 Z M18 11 L18.6 12.7 L20.3 13.3 L18.6 13.9 L18 15.6 L17.4 13.9 L15.7 13.3 L17.4 12.7 Z M6.5 3.5 L7 5 L8.5 5.5 L7 6 L6.5 7.5 L6 6 L4.5 5.5 L6 5 Z",
        ["tool.eyedropper"] = "M19.5 4.5 C18 3 15.7 3 14.3 4.5 L11 8 L10 7 L8.5 8.5 L15.5 15.5 L17 14 L16 13 L19.5 9.7 C21 8.3 21 6 19.5 4.5 Z M10 10 L4.5 15.5 C4 16 4 17 4.2 17.6 L3.5 20.5 L6.4 19.8 C7 20 8 20 8.5 19.5 L14 14",
        ["tool.bucket"] = "M12 3 L4.5 10.5 C4 11 4 12 4.5 12.5 L10 18 C11 19 12.6 19 13.6 18 L19 12.6 C20 11.6 20 10 19 9 L13.5 3.5 M12 3 L15 6 M19 16.5 C19 16.5 20.8 18.8 20.8 20 A1.8 1.8 0 0 1 17.2 20 C17.2 18.8 19 16.5 19 16.5 Z",
        ["tool.gradient"] = "M4 4 L20 4 L20 20 L4 20 Z M4 4 L20 20 M4 4 L4 12 L20 12",
        ["tool.text"] = "M5 6 L5 4 L19 4 L19 6 M12 4 L12 20 M9 20 L15 20",
        ["tool.shape"] = "M4 4 L13 4 L13 13 L4 13 Z M15 10 A5.5 5.5 0 1 1 15 21 A5.5 5.5 0 1 1 15 10",
        ["tool.crop"] = "M7 2 L7 17 L22 17 M2 7 L17 7 L17 22",
        ["tool.transform"] = "M7 4 L17 4 M4 7 L4 17 M20 7 L20 17 M7 20 L17 20 M4 4 L4 4.01 M20 4 L20 4.01 M4 20 L4 20.01 M20 20 L20 20.01 M7.5 12 L16.5 12 M12 7.5 L12 16.5",
        ["tool.hand"] = "M8 12 L8 5.5 A1.5 1.5 0 0 1 11 5.5 L11 11 M11 10 L11 4 A1.5 1.5 0 0 1 14 4 L14 10 M14 10.5 L14 5.5 A1.5 1.5 0 0 1 17 5.5 L17 12 M17 8.5 A1.5 1.5 0 0 1 20 8.5 L20 14 C20 17.5 18 21 14 21 L11.5 21 C9.5 21 8.5 20 7.5 18.5 L4.5 14 A1.4 1.4 0 0 1 6.7 12.2 L8 13.5",
        ["tool.zoom"] = "M10.5 4 A6.5 6.5 0 1 1 10.49 4 Z M15.5 15.5 L21 21 M8 10.5 L13 10.5 M10.5 8 L10.5 13",
        // ---------------- panel actions ----------------
        ["act.new"] = "M14 3 L6 3 L6 21 L18 21 L18 10 Z M14 3 L14 10 L18 10",
        ["act.open"] = "M3 6 C3 5.4 3.4 5 4 5 L9 5 L11 7 L20 7 C20.6 7 21 7.4 21 8 L21 18 C21 18.6 20.6 19 20 19 L4 19 C3.4 19 3 18.6 3 18 Z",
        ["act.save"] = "M5 3 L17 3 L21 7 L21 21 L5 21 L5 3 Z M8 3 L8 8 L16 8 L16 3 M8 13 L16 13 L16 21 L8 21 Z",
        ["act.export"] = "M12 15 L12 3 M12 3 L8 7 M12 3 L16 7 M5 13 L5 21 L19 21 L19 13",
        ["act.undo"] = "M8 5 L3 10 L8 15 M3 10 L15 10 C18.3 10 21 12.7 21 16 L21 18",
        ["act.redo"] = "M16 5 L21 10 L16 15 M21 10 L9 10 C5.7 10 3 12.7 3 16 L3 18",
        ["act.zoomin"] = "M10.5 4 A6.5 6.5 0 1 1 10.49 4 Z M15.5 15.5 L21 21 M10.5 7.5 L10.5 13.5 M7.5 10.5 L13.5 10.5",
        ["act.zoomout"] = "M10.5 4 A6.5 6.5 0 1 1 10.49 4 Z M15.5 15.5 L21 21 M7.5 10.5 L13.5 10.5",
        ["act.fit"] = "M4 9 L4 4 L9 4 M15 4 L20 4 L20 9 M20 15 L20 20 L15 20 M9 20 L4 20 L4 15",
        ["act.plus"] = "M12 5 L12 19 M5 12 L19 12",
        ["act.minus"] = "M5 12 L19 12",
        ["act.trash"] = "M4 7 L20 7 M9 7 L9 4 L15 4 L15 7 M6.5 7 L7.5 21 L16.5 21 L17.5 7 M10 11 L10 17 M14 11 L14 17",
        ["act.duplicate"] = "M8 8 L21 8 L21 21 L8 21 Z M3 16 L3 3 L16 3 L16 8 M8 3 L16 3",
        ["act.group"] = "M3 5 L10 5 L12 7 L21 7 L21 19 L3 19 Z M7 12 L17 12 M7 15.5 L14 15.5",
        ["act.mask"] = "M12 3 A9 9 0 1 1 11.99 3 Z M12 3 A9 9 0 0 0 12 21 L12 3",
        ["act.merge"] = "M12 3 L12 15 M12 15 L8 11 M12 15 L16 11 M4 19 L20 19",
        ["act.up"] = "M12 19 L12 5 M12 5 L6 11 M12 5 L18 11",
        ["act.down"] = "M12 5 L12 19 M12 19 L6 13 M12 19 L18 13",
        ["act.eye"] = "M2 12 C4.5 7 7.8 5 12 5 C16.2 5 19.5 7 22 12 C19.5 17 16.2 19 12 19 C7.8 19 4.5 17 2 12 Z M12 9 A3 3 0 1 1 11.99 9 Z",
        ["act.eyeoff"] = "M4 4 L20 20 M9.9 5.2 C10.6 5.1 11.3 5 12 5 C16.2 5 19.5 7 22 12 C21.1 13.7 20.1 15.1 19 16.2 M13.2 13.2 A3 3 0 1 1 10.8 10.8 M5.6 7.6 C4.2 8.7 3 10.2 2 12 C4.5 17 7.8 19 12 19 C13.1 19 14.2 18.9 15.2 18.5",
        ["act.check"] = "M4 12.5 L9.5 18 L20 6.5",
        ["act.close"] = "M6 6 L18 18 M18 6 L6 18",
        ["act.settings"] = "M12 8.5 A3.5 3.5 0 1 1 11.99 8.5 Z M12 2.5 L13 5 L15.8 4.2 L16.5 7 L19.3 7.7 L18.5 10.5 L21 12 L18.5 13.5 L19.3 16.3 L16.5 17 L15.8 19.8 L13 19 L12 21.5 L11 19 L8.2 19.8 L7.5 17 L4.7 16.3 L5.5 13.5 L3 12 L5.5 10.5 L4.7 7.7 L7.5 7 L8.2 4.2 L11 5 Z",
        ["act.workspace"] = "M4 4 L20 4 L20 20 L4 20 Z M12 4 L12 20 M4 8 L12 8",
        ["act.help"] = "M12 3 A9 9 0 1 1 11.99 3 Z M9.5 9.5 C9.5 8 10.5 7 12 7 C13.5 7 14.5 8 14.5 9.5 C14.5 11.4 12 11.5 12 13.5 M12 16.5 L12 16.51",

        // ---------------- v3.0: panels & features ----------------
        ["panel.histogram"] = "M3 20 L21 20 M5 20 L5 13 M8 20 L8 9 M11 20 L11 15 M14 20 L14 6 M17 20 L17 11 M20 20 L20 16",
        ["panel.navigator"] = "M12 3 A9 9 0 1 1 11.99 3 Z M7 7 L10.5 14.5 L14 11 L17 16 M7 7 L17 16",
        ["panel.history"] = "M12 3 A9 9 0 1 1 11.99 3 Z M12 7 L12 12 L16 14 M12 3 L12 5 M12 19 L12 21",
        ["panel.layers"] = "M12 3 L21 8 L12 13 L3 8 Z M5.5 12 L3 13.5 L12 18.5 L21 13.5 L18.5 12 M5.5 16 L3 17.5 L12 22.5 L21 17.5 L18.5 16",
        ["panel.color"] = "M12 3 C12 3 5 10.5 5 15 A7 7 0 0 0 19 15 C19 10.5 12 3 12 3 Z M9.5 15 A2.5 2.5 0 0 0 12 17.5",
        ["act.wand2"] = "M4 20 L12.5 11.5 M10 6 L11 8.5 L13.5 9.5 L11 10.5 L10 13 L9 10.5 L6.5 9.5 L9 8.5 Z M17 4 L18 6 L20 7 L18 8 L17 10 L16 8 L14 7 L16 6 Z",
        ["act.auto"] = "M4 18 L9 6 L12 14 L14 9 L20 18 M15.5 5.5 L16.5 7.5 M18.5 6.5 L20 7",
        ["act.import"] = "M12 3 L12 13 M12 13 L8 9 M12 13 L16 9 M4 15 L4 21 L20 21 L20 15",
        ["act.refresh"] = "M20 12 A8 8 0 1 1 18.9 8 M19 3 L19 8 L14 8",
        ["act.fit2"] = "M9 4 L4 4 L4 9 M15 4 L20 4 L20 9 M20 15 L20 20 L15 20 M9 20 L4 20 L4 15 M4 12 L20 12",
        ["act.swatch"] = "M4 4 L20 4 L20 16 L12 16 L10 20 L8 20 L8 16 L4 16 Z",
        ["act.swap"] = "M7 8 L20 8 M20 8 L16 4 M20 8 L16 12 M17 16 L4 16 M4 16 L8 12 M4 16 L8 20",
        ["act.target"] = "M12 3 A9 9 0 1 1 11.99 3 Z M12 7 A5 5 0 1 1 11.99 7 Z M12 10.5 A1.5 1.5 0 1 1 11.99 10.5 Z",
        ["act.filter"] = "M3 5 L21 5 L14 13 L14 20 L10 18 L10 13 Z",
        ["act.grid"] = "M4 4 L20 4 L20 20 L4 20 Z M4 9.3 L20 9.3 M4 14.6 L20 14.6 M9.3 4 L9.3 20 M14.6 4 L14.6 20",
    };
}
