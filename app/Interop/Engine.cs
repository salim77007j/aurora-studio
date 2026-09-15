using System;
using System.Runtime.InteropServices;
using System.Text;

namespace AuroraStudio.Interop;

/// <summary>P/Invoke bindings to the Rust aurora_engine cdylib.</summary>
public static unsafe class Engine
{
    private const string Dll = "aurora_engine";

    [DllImport(Dll)] public static extern int aurora_version(byte* buf, uint cap);
    [DllImport(Dll)] public static extern int aurora_last_error(byte* buf, uint cap);
    [DllImport(Dll)] public static extern ulong aurora_doc_new(uint w, uint h, int bg, byte* name);
    [DllImport(Dll)] public static extern ulong aurora_doc_open(byte* path);
    [DllImport(Dll)] public static extern ulong aurora_doc_clone(ulong h);
    [DllImport(Dll)] public static extern int aurora_doc_free(ulong h);
    [DllImport(Dll)] public static extern int aurora_doc_json(ulong h, byte* buf, uint cap);
    [DllImport(Dll)] public static extern int aurora_history_json(ulong h, byte* buf, uint cap);
    [DllImport(Dll)] public static extern int aurora_poll(ulong h, int* x, int* y, uint* w, uint* hh, ulong* version);
    [DllImport(Dll)] public static extern int aurora_composite_read(ulong h, int x, int y, uint w, uint hh, byte* buf, uint cap);
    [DllImport(Dll)] public static extern int aurora_selection_bounds(ulong h, int* x, int* y, uint* w, uint* hh);

    [DllImport(Dll)] public static extern ulong aurora_layer_add(ulong h, long parentId, byte* name, int belowActive);
    [DllImport(Dll)] public static extern ulong aurora_layer_add_group(ulong h, byte* name);
    [DllImport(Dll)] public static extern int aurora_layer_delete(ulong h, ulong id);
    [DllImport(Dll)] public static extern ulong aurora_layer_duplicate(ulong h, ulong id);
    [DllImport(Dll)] public static extern int aurora_layer_merge_down(ulong h, ulong id);
    [DllImport(Dll)] public static extern int aurora_layer_set_visible(ulong h, ulong id, int v);
    [DllImport(Dll)] public static extern int aurora_layer_set_opacity(ulong h, ulong id, float o);
    [DllImport(Dll)] public static extern int aurora_layer_set_blend(ulong h, ulong id, int blend);
    [DllImport(Dll)] public static extern int aurora_layer_set_name(ulong h, ulong id, byte* name);
    [DllImport(Dll)] public static extern int aurora_layer_set_active(ulong h, ulong id);
    [DllImport(Dll)] public static extern ulong aurora_active_layer(ulong h);
    [DllImport(Dll)] public static extern int aurora_layer_set_locked(ulong h, ulong id, int v);
    [DllImport(Dll)] public static extern int aurora_layer_move_node(ulong h, ulong id, long newParent, int newIndex);
    [DllImport(Dll)] public static extern int aurora_layer_move_up(ulong h, ulong id);
    [DllImport(Dll)] public static extern int aurora_layer_move_down(ulong h, ulong id);
    [DllImport(Dll)] public static extern int aurora_layer_mask_from_selection(ulong h, ulong id);
    [DllImport(Dll)] public static extern int aurora_layer_mask_apply(ulong h, ulong id);
    [DllImport(Dll)] public static extern int aurora_layer_mask_delete(ulong h, ulong id);
    [DllImport(Dll)] public static extern int aurora_layer_thumbnail(ulong h, ulong id, uint size, byte* buf, uint cap);

    [DllImport(Dll)] public static extern int aurora_select_rect(ulong h, int x, int y, int w, int hh, int mode);
    [DllImport(Dll)] public static extern int aurora_select_ellipse(ulong h, int x, int y, int w, int hh, int mode);
    [DllImport(Dll)] public static extern int aurora_select_lasso(ulong h, float* pts, int n, int mode);
    [DllImport(Dll)] public static extern int aurora_select_wand(ulong h, int x, int y, int tolerance, int contiguous, int sampleLayer, int mode);
    [DllImport(Dll)] public static extern int aurora_select_all(ulong h);
    [DllImport(Dll)] public static extern int aurora_select_none(ulong h);
    [DllImport(Dll)] public static extern int aurora_select_invert(ulong h);
    [DllImport(Dll)] public static extern int aurora_select_feather(ulong h, float radius);
    [DllImport(Dll)] public static extern int aurora_select_contour(ulong h, byte* buf, uint cap);

    [StructLayout(LayoutKind.Sequential)]
    public struct BrushFfi
    {
        public float Size, Hardness, Flow, Opacity, Spacing;
        public int Eraser;
        public byte R, G, B, A;
        public int SizePressure, OpacityPressure, Pencil;
    }

    [DllImport(Dll)] public static extern int aurora_brush_begin(ulong h, ulong layerId, BrushFfi* p, float x, float y, float pressure);
    [DllImport(Dll)] public static extern int aurora_brush_move(ulong h, float x, float y, float pressure);
    [DllImport(Dll)] public static extern int aurora_brush_end(ulong h);

    [DllImport(Dll)] public static extern int aurora_fill(ulong h, byte r, byte g, byte b, byte a);
    [DllImport(Dll)] public static extern int aurora_bucket(ulong h, int x, int y, byte r, byte g, byte b, byte a, int tolerance, int contiguous);
    [DllImport(Dll)] public static extern int aurora_gradient(ulong h, float x0, float y0, float x1, float y1, int kind, int fill,
        byte fr, byte fg, byte fb, byte fa, byte br, byte bg, byte bb, byte ba, int dither);
    [DllImport(Dll)] public static extern int aurora_paste_pixels(ulong h, ulong layerId, int x, int y, uint w, uint hh, byte* buf, uint cap);
    [DllImport(Dll)] public static extern int aurora_paste_png(ulong h, ulong layerId, int x, int y, byte* buf, uint cap);
    [DllImport(Dll)] public static extern int aurora_layer_clear(ulong h);

    [DllImport(Dll)] public static extern int aurora_adj_curves(ulong h, byte* json);
    [DllImport(Dll)] public static extern int aurora_adj_levels(ulong h, float inBlack, float inWhite, float gamma, float outBlack, float outWhite, int channel);
    [DllImport(Dll)] public static extern int aurora_adj_bc(ulong h, float brightness, float contrast);
    [DllImport(Dll)] public static extern int aurora_adj_hsl(ulong h, float hue, float sat, float light);

    [DllImport(Dll)] public static extern int aurora_filter_gauss(ulong h, float radius);
    [DllImport(Dll)] public static extern int aurora_filter_sharpen(ulong h, float amount);
    [DllImport(Dll)] public static extern int aurora_filter_noise(ulong h, int amount, int mono);
    [DllImport(Dll)] public static extern int aurora_filter_pixelate(ulong h, int size);
    [DllImport(Dll)] public static extern int aurora_filter_twirl(ulong h, float cx, float cy, float radius, float angle);
    [DllImport(Dll)] public static extern int aurora_filter_wave(ulong h, float amplitude, float wavelength, int vertical);
    [DllImport(Dll)] public static extern int aurora_filter_emboss(ulong h);

    [DllImport(Dll)] public static extern int aurora_undo(ulong h);
    [DllImport(Dll)] public static extern int aurora_redo(ulong h);
    [DllImport(Dll)] public static extern int aurora_history_set(ulong h, uint index);

    [DllImport(Dll)] public static extern int aurora_image_resize(ulong h, uint nw, uint nh, int interp);
    [DllImport(Dll)] public static extern int aurora_canvas_resize(ulong h, uint nw, uint nh, int anchor);
    [DllImport(Dll)] public static extern int aurora_doc_crop(ulong h, int x, int y, uint w, uint hh);
    [DllImport(Dll)] public static extern int aurora_image_rotate(ulong h, int turns);
    [DllImport(Dll)] public static extern int aurora_image_flip(ulong h, int axis);
    [DllImport(Dll)] public static extern int aurora_doc_flatten(ulong h);
    [DllImport(Dll)] public static extern int aurora_doc_set_name(ulong h, byte* name);

    [DllImport(Dll)] public static extern int aurora_layer_flip(ulong h, ulong id, int axis);
    [DllImport(Dll)] public static extern int aurora_layer_rotate(ulong h, ulong id, float degrees, int expand);
    [DllImport(Dll)] public static extern int aurora_layer_move_by(ulong h, ulong id, int dx, int dy);
    [DllImport(Dll)] public static extern int aurora_layer_warp(ulong h, ulong id, int mode, float* m, uint nw, uint nh);

    [DllImport(Dll)] public static extern int aurora_doc_export(ulong h, byte* path, byte* format, int quality, int compression, int lossless, int frames);

    [DllImport(Dll)] public static extern int aurora_selftest();

    // ---------- helpers ----------

    private static byte[] _errBuf = new byte[1024];

    public static string LastError()
    {
        fixed (byte* p = _errBuf)
        {
            aurora_last_error(p, (uint)_errBuf.Length);
            int len = 0;
            while (len < _errBuf.Length && _errBuf[len] != 0) len++;
            return Encoding.UTF8.GetString(_errBuf, 0, len);
        }
    }

    /// <summary>Query the engine for the CURRENT active layer (never stale).</summary>
    public static ulong ActiveLayer(ulong h) => h == 0 ? 0 : aurora_active_layer(h);

    public static string Version()
    {
        var buf = new byte[128];
        fixed (byte* p = buf)
        {
            aurora_version(p, (uint)buf.Length);
            int len = 0;
            while (len < buf.Length && buf[len] != 0) len++;
            return Encoding.UTF8.GetString(buf, 0, len);
        }
    }

    public static ulong DocNew(uint w, uint h, int bg, string name)
    {
        var bytes = Encoding.UTF8.GetBytes(name + "\0");
        fixed (byte* p = bytes)
            return aurora_doc_new(w, h, bg, p);
    }

    public static ulong DocOpen(string path)
    {
        var bytes = Encoding.UTF8.GetBytes(path + "\0");
        fixed (byte* p = bytes)
            return aurora_doc_open(p);
    }

    private static byte[] _buf = new byte[1 << 16];

    public unsafe delegate int JsonFn(byte* buf, uint cap);

    /// <summary>Read a JSON string from the engine with automatic buffer growth.</summary>
    public static string ReadJson(JsonFn fn)
    {
        for (int attempt = 0; attempt < 4; attempt++)
        {
            int needed;
            fixed (byte* p = _buf)
                needed = fn(p, (uint)_buf.Length);
            if (needed < 0) return string.Empty;
            if (needed < _buf.Length)
            {
                int len = needed;
                return Encoding.UTF8.GetString(_buf, 0, len);
            }
            _buf = new byte[needed * 2 + 16];
        }
        return string.Empty;
    }

    public static string DocJson(ulong h) => ReadJson((byte* b, uint c) => aurora_doc_json(h, b, c));
    public static string HistoryJson(ulong h) => ReadJson((byte* b, uint c) => aurora_history_json(h, b, c));
    public static string SelectContour(ulong h) => ReadJson((byte* b, uint c) => aurora_select_contour(h, b, c));

    public static int DocExport(ulong h, string path, string format, int quality, int compression, bool lossless, bool frames)
    {
        var pb = Encoding.UTF8.GetBytes(path + "\0");
        var fb = Encoding.UTF8.GetBytes(format + "\0");
        fixed (byte* pp = pb, ff = fb)
            return aurora_doc_export(h, pp, ff, quality, compression, lossless ? 1 : 0, frames ? 1 : 0);
    }

    public static int LayerSetName(ulong h, ulong id, string name)
    {
        var nb = Encoding.UTF8.GetBytes(name + "\0");
        fixed (byte* p = nb)
            return aurora_layer_set_name(h, id, p);
    }

    public static int DocSetName(ulong h, string name)
    {
        var nb = Encoding.UTF8.GetBytes(name + "\0");
        fixed (byte* p = nb)
            return aurora_doc_set_name(h, p);
    }

    public static int AdjCurves(ulong h, string json)
    {
        var jb = Encoding.UTF8.GetBytes(json + "\0");
        fixed (byte* p = jb)
            return aurora_adj_curves(h, p);
    }

    public static unsafe int LayerAdd(ulong h, long parent, string name, int belowActive)
    {
        var nb = System.Text.Encoding.UTF8.GetBytes(name + "\0");
        fixed (byte* p = nb)
            return (int)aurora_layer_add(h, parent, p, belowActive);
    }

    public static unsafe int LayerAddGroup(ulong h, string name)
    {
        var nb = System.Text.Encoding.UTF8.GetBytes(name + "\0");
        fixed (byte* p = nb)
            return (int)aurora_layer_add_group(h, p);
    }

    public static (int X, int Y, uint W, uint H, bool Active) SelBounds(ulong h)
    {
        int x = 0, y = 0; uint w = 0, hh = 0;
        int r;
        unsafe
        {
            int* px = &x; int* py = &y; uint* pw = &w; uint* ph = &hh;
            r = aurora_selection_bounds(h, px, py, pw, ph);
        }
        return (x, y, w, hh, r == 1);
    }

    public static int LayerThumbnail(ulong h, ulong id, uint size, byte[] buf)
    {
        fixed (byte* p = buf)
            return aurora_layer_thumbnail(h, id, size, p, (uint)buf.Length);
    }
}
