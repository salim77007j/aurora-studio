using System;
using System.Linq;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Controls.ApplicationLifetimes;
using Avalonia.Markup.Xaml;
using Avalonia.Media;
using Avalonia.Styling;
using Avalonia.Threading;
using AuroraStudio.Models;
using AuroraStudio.Views;

namespace AuroraStudio;

public class App : Application
{
    public static AppSettings Settings { get; private set; } = new();

    public override void Initialize()
    {
        AvaloniaXamlLoader.Load(this);
    }

    public override void OnFrameworkInitializationCompleted()
    {
        Settings = AppSettings.Load();

        if (ApplicationLifetime is IClassicDesktopStyleApplicationLifetime desktop)
        {
            // UI-thread exception shield: log + tell the user instead of crashing.
            // Avalonia re-throws dispatcher exceptions by default, which terminates
            // the process — that is what made the app "close suddenly" for users.
            Dispatcher.UIThread.UnhandledException += (_, e) =>
            {
                Program.WriteCrash("UI", e.Exception);
                e.Handled = true; // keep the app alive
                try
                {
                    if (desktop.MainWindow is { } w)
                        ShowSoftError(w, e.Exception);
                }
                catch { }
            };

            desktop.MainWindow = new MainWindow();
        }

        base.OnFrameworkInitializationCompleted();
    }

    private static void ShowSoftError(Window owner, Exception ex)
    {
        var dlg = new Window
        {
            Title = "Aurora Studio — something went wrong",
            SizeToContent = SizeToContent.WidthAndHeight,
            CanResize = false,
            WindowStartupLocation = WindowStartupLocation.CenterOwner,
            ShowInTaskbar = false,
            Content = new StackPanel
            {
                Margin = new Thickness(22),
                Spacing = 12,
                Children =
                {
                    new TextBlock
                    {
                        Text = "That operation could not be completed.",
                        FontWeight = FontWeight.SemiBold,
                        Foreground = Brushes.White,
                    },
                    new TextBlock
                    {
                        Text = ex.Message,
                        TextWrapping = TextWrapping.Wrap,
                        MaxWidth = 460,
                        Foreground = new SolidColorBrush(Color.FromRgb(0xB8, 0xBD, 0xC4)),
                    },
                    new TextBlock
                    {
                        Text = "Details were written to %APPDATA%\\AuroraStudio\\crash.log — the document is still open.",
                        TextWrapping = TextWrapping.Wrap,
                        MaxWidth = 460,
                        FontSize = 12,
                        Foreground = new SolidColorBrush(Color.FromRgb(0x8A, 0x8F, 0x96)),
                    },
                    new Button
                    {
                        Content = "Continue",
                        HorizontalAlignment = Avalonia.Layout.HorizontalAlignment.Right,
                        Padding = new Thickness(18, 5),
                    },
                },
            },
        };
        ((dlg.Content as StackPanel)!.Children[^1] as Button)!.Click += (_, _) => dlg.Close();
        dlg.Show(owner);
    }
}
