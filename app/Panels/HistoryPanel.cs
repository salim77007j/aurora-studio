using System;
using System.Collections.ObjectModel;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Layout;
using Avalonia.Media;
using AuroraStudio.Interop;
using AuroraStudio.Models;
using AuroraStudio.Views;

namespace AuroraStudio.Panels;

/// <summary>History panel: click any state to jump back (real undo/redo navigation).</summary>
public class HistoryPanel : Border
{
    private readonly MainWindow _win;
    private readonly ObservableCollection<string> _entries = new();
    private readonly ListBox _list = new();
    private bool _updating;

    public HistoryPanel(MainWindow win)
    {
        _win = win;
        _list.ItemsSource = _entries;
        _list.MaxHeight = 180;
        _list.SelectionChanged += (_, _) =>
        {
            if (_updating) return;
            var doc = _win.ActiveDoc;
            if (doc == null || _list.SelectedIndex < 0) return;
            Engine.aurora_history_set(doc.Handle, (uint)_list.SelectedIndex);
            doc.RefreshState();
            _win.RefreshPanels();
            _win.Canvas.InvalidateAnts();
        };

        var root = new StackPanel { Spacing = 4 };
        root.Children.Add(ColorPanel.PanelHeader("History"));
        root.Children.Add(_list);

        Child = new Border
        {
            Background = ColorPanel.Brush(0x2A, 0x2C, 0x31),
            CornerRadius = new CornerRadius(6),
            Padding = new Thickness(10),
            Child = root,
        };
    }

    public void Refresh(AuroraDocument? doc)
    {
        _updating = true;
        _entries.Clear();
        if (doc != null)
        {
            _entries.Add("Document Opened");
            foreach (var e in doc.History.Entries)
                _entries.Add(e);
            _list.SelectedIndex = Math.Min(doc.History.Index, _entries.Count - 1);
        }
        _updating = false;
    }
}
