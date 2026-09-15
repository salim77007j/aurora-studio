using System;
using System.Collections.Generic;
using System.Collections.ObjectModel;
using System.Linq;
using System.Text.Json;
using System.Text.Json.Serialization;
using AuroraStudio.Interop;

namespace AuroraStudio.Models;

public class LayerInfo
{
    [JsonPropertyName("id")] public ulong Id { get; set; }
    [JsonPropertyName("name")] public string Name { get; set; } = "";
    [JsonPropertyName("kind")] public string Kind { get; set; } = "layer";
    [JsonPropertyName("visible")] public bool Visible { get; set; } = true;
    [JsonPropertyName("opacity")] public double Opacity { get; set; } = 1.0;
    [JsonPropertyName("blend")] public int Blend { get; set; }
    [JsonPropertyName("hasMask")] public bool HasMask { get; set; }
    [JsonPropertyName("locked")] public bool Locked { get; set; }
    [JsonPropertyName("expanded")] public bool Expanded { get; set; } = true;
    [JsonPropertyName("children")] public List<LayerInfo>? Children { get; set; }

    [JsonIgnore] public bool IsGroup => Kind == "group";
    [JsonIgnore] public string BlendName => BlendModeNames.Names[Math.Clamp(Blend, 0, BlendModeNames.Names.Length - 1)];
    [JsonIgnore] public string DisplayName => (Visible ? "" : "\u2715 ") + Name + (HasMask ? " \u25a6" : "");
}

public static class BlendModeNames
{
    public static readonly string[] Names =
    {
        "Normal", "Multiply", "Screen", "Overlay", "Darken", "Lighten", "Color Dodge",
        "Color Burn", "Hard Light", "Soft Light", "Difference", "Exclusion", "Hue",
        "Saturation", "Color", "Luminosity"
    };
}

public class DocState
{
    [JsonPropertyName("w")] public uint W { get; set; }
    [JsonPropertyName("h")] public uint H { get; set; }
    [JsonPropertyName("name")] public string Name { get; set; } = "Untitled";
    [JsonPropertyName("activeId")] public ulong ActiveId { get; set; }
    [JsonPropertyName("layers")] public List<LayerInfo> Layers { get; set; } = new();

    public LayerInfo? FindLayer(List<LayerInfo>? list, ulong id)
    {
        if (list == null) return null;
        foreach (var l in list)
        {
            if (l.Id == id) return l;
            var f = FindLayer(l.Children, id);
            if (f != null) return f;
        }
        return null;
    }
}

public class HistoryState
{
    [JsonPropertyName("index")] public int Index { get; set; }
    [JsonPropertyName("entries")] public List<string> Entries { get; set; } = new();
}

/// <summary>A wrapper for one open document (one engine handle).</summary>
public sealed class AuroraDocument : IDisposable
{
    public ulong Handle { get; private set; }
    public string Title { get; set; } = "Untitled";
    public string? FilePath { get; set; }
    public uint Width { get; private set; }
    public uint Height { get; private set; }
    public bool IsDirty => History != null && History.Index > 0;
    public DocState State { get; private set; } = new();
    public HistoryState History { get; private set; } = new();
    public DateTime LastModified { get; set; } = DateTime.Now;
    public int HistoryIndex { get; set; } // for jump tracking

    public AuroraDocument(ulong handle, uint w, uint h, string title)
    {
        Handle = handle;
        Width = w;
        Height = h;
        Title = title;
        RefreshState();
    }

    public void RefreshState()
    {
        var json = Engine.DocJson(Handle);
        try
        {
            State = JsonSerializer.Deserialize<DocState>(json) ?? new DocState();
            Width = State.W;
            Height = State.H;
        }
        catch { /* keep old state */ }
        var hjson = Engine.HistoryJson(Handle);
        try
        {
            History = JsonSerializer.Deserialize<HistoryState>(hjson) ?? new HistoryState();
        }
        catch { }
    }

    public void Close() => Dispose();

    public void Dispose()
    {
        if (Handle != 0)
        {
            unsafe { Engine.aurora_doc_free(Handle); }
            Handle = 0;
        }
        GC.SuppressFinalize(this);
    }

    ~AuroraDocument() => Dispose();
}
