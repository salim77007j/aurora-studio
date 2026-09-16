using System;
using System.Collections.ObjectModel;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Layout;
using Avalonia.Media;
using Avalonia.Threading;
using AuroraStudio.Interop;
using AuroraStudio.Models;
using AuroraStudio.Views;

namespace AuroraStudio.Panels;

/// <summary>History panel: click any state to jump back (real undo/redo navigation).</summary>
/// <remarks>
/// Crash-proofing notes (v3.0): the old version mutated the ItemsSource
/// ObservableCollection synchronously inside the SelectionChanged callback, which
/// re-entered Avalonia's SelectionModel mid-commit and threw ArgumentOutOfRangeException
/// (the crash the user saw when clicking history entries). Two defenses:
///   1. The handler defers all work via Dispatcher.UIThread.Post — it runs after
///      Avalonia's selection commit fully completes.
///   2. Refresh() detaches ItemsSource before mutating the list, so no live
///      SelectionModel ever observes a partial collection.
/// </remarks>
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
        _list.MaxHeight = 190;
        _list.SelectionChanged += OnListSelectionChanged;

        var root = new StackPanel { Spacing = 4 };
        root.Children.Add(ColorPanel.PanelHeader("History"));
        root.Children.Add(_list);

        Child = new Border
        {
            Background = ColorPanel.Brush(0x26, 0x28, 0x2D),
            CornerRadius = new CornerRadius(8),
            Padding = new Thickness(10),
            Child = root,
        };
    }

    private void OnListSelectionChanged(object? sender, SelectionChangedEventArgs e)
    {
        if (_updating) return;
        // Defer: never touch the items source or engine state while Avalonia is
        // still committing the selection operation.
        Dispatcher.UIThread.Post(() =>
        {
            try
            {
                if (_updating) return;
                var doc = _win.ActiveDoc;
                if (doc == null || _list.SelectedIndex < 0) return;
                // list[0] = "Document Opened" (depth 0); list[k] = engine depth k
                int target = _list.SelectedIndex;
                if ((uint)target != doc.History.Index)
                    Engine.aurora_history_set(doc.Handle, (uint)target);
                doc.RefreshState();
                _win.TheCanvas.InvalidateAnts();
                _win.RefreshPanels();
            }
            catch (Exception ex)
            {
                Program.WriteCrash("HISTORY", ex);
            }
        }, DispatcherPriority.Background);
    }

    public void Refresh(AuroraDocument? doc)
    {
        _updating = true;
        try
        {
            // Detach BEFORE mutating so the SelectionModel never sees a partial list.
            _list.ItemsSource = null;
            _entries.Clear();
            if (doc != null)
            {
                _entries.Add("Document Opened");
                foreach (var e in doc.History.Entries)
                    _entries.Add(e);
                _list.ItemsSource = _entries;
                // current depth d ↔ list index d ("Document Opened" is depth 0)
                _list.SelectedIndex = Math.Clamp(doc.History.Index, 0, _entries.Count - 1);
            }
            else
            {
                _list.ItemsSource = _entries;
                _list.SelectedIndex = -1;
            }
        }
        catch (Exception ex)
        {
            Program.WriteCrash("HISTORY", ex);
        }
        finally
        {
            _updating = false;
        }
    }
}
