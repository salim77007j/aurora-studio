using System;
using System.Collections.Generic;
using System.IO;
using System.Text.Json;

namespace AuroraStudio.Models;

public class AppSettings
{
    public string Accent { get; set; } = "#4F8CFF";
    public bool ShowHistoryPanel { get; set; } = true;
    public bool ShowColorPanel { get; set; } = true;
    public bool ShowLayersPanel { get; set; } = true;
    public string Workspace { get; set; } = "Default";
    public Dictionary<string, string> Shortcuts { get; set; } = new();
    public double WindowWidth { get; set; } = 1440;
    public double WindowHeight { get; set; } = 900;

    [System.Text.Json.Serialization.JsonIgnore]
    public static readonly Dictionary<string, string> DefaultShortcuts = new()
    {
        ["Tool.Move"] = "V", ["Tool.Brush"] = "B", ["Tool.Pencil"] = "N", ["Tool.Eraser"] = "E",
        ["Tool.RectSelect"] = "M", ["Tool.EllipseSelect"] = "J", ["Tool.Lasso"] = "L", ["Tool.Wand"] = "W",
        ["Tool.Eyedropper"] = "I", ["Tool.Bucket"] = "G", ["Tool.Gradient"] = "R",
        ["Tool.Text"] = "T", ["Tool.Shape"] = "U", ["Tool.Crop"] = "C", ["Tool.Transform"] = "Ctrl+T",
        ["Edit.Undo"] = "Ctrl+Z", ["Edit.Redo"] = "Ctrl+Y",
        ["Select.All"] = "Ctrl+A", ["Select.Deselect"] = "Ctrl+D", ["Select.Invert"] = "Ctrl+Shift+I",
        ["File.New"] = "Ctrl+N", ["File.Open"] = "Ctrl+O", ["File.Save"] = "Ctrl+S", ["File.Export"] = "Ctrl+Shift+E",
        ["View.ZoomIn"] = "Ctrl+Plus", ["View.ZoomOut"] = "Ctrl+Minus", ["View.Fit"] = "Ctrl+0", ["View.ActualSize"] = "Ctrl+1",
    };

    private static string SettingsPath
    {
        get
        {
            string dir = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData), "AuroraStudio");
            Directory.CreateDirectory(dir);
            return Path.Combine(dir, "settings.json");
        }
    }

    public static AppSettings Load()
    {
        try
        {
            if (File.Exists(SettingsPath))
            {
                var s = JsonSerializer.Deserialize<AppSettings>(File.ReadAllText(SettingsPath));
                if (s != null) return s;
            }
        }
        catch { }
        return new AppSettings();
    }

    public void Save()
    {
        try
        {
            File.WriteAllText(SettingsPath, JsonSerializer.Serialize(this, new JsonSerializerOptions { WriteIndented = true }));
        }
        catch { }
    }
}

public enum ToolKind
{
    Move, Brush, Pencil, Eraser,
    RectSelect, EllipseSelect, Lasso, Wand,
    Eyedropper, Bucket, Gradient,
    Text, Shape, Crop, Transform
}

public static class ToolInfo
{
    public static string Title(ToolKind t) => t switch
    {
        ToolKind.Move => "Move",
        ToolKind.Brush => "Brush",
        ToolKind.Pencil => "Pencil",
        ToolKind.Eraser => "Eraser",
        ToolKind.RectSelect => "Rectangular Select",
        ToolKind.EllipseSelect => "Elliptical Select",
        ToolKind.Lasso => "Lasso Select",
        ToolKind.Wand => "Magic Wand",
        ToolKind.Eyedropper => "Eyedropper",
        ToolKind.Bucket => "Paint Bucket",
        ToolKind.Gradient => "Gradient",
        ToolKind.Text => "Text",
        ToolKind.Shape => "Shape",
        ToolKind.Crop => "Crop",
        ToolKind.Transform => "Free Transform",
        _ => ""
    };

    public static string Shortcut(ToolKind t) => t switch
    {
        ToolKind.Move => "V", ToolKind.Brush => "B", ToolKind.Pencil => "N", ToolKind.Eraser => "E",
        ToolKind.RectSelect => "M", ToolKind.EllipseSelect => "J", ToolKind.Lasso => "L", ToolKind.Wand => "W",
        ToolKind.Eyedropper => "I", ToolKind.Bucket => "G", ToolKind.Gradient => "R",
        ToolKind.Text => "T", ToolKind.Shape => "U", ToolKind.Crop => "C", ToolKind.Transform => "Ctrl+T",
        _ => ""
    };
}
