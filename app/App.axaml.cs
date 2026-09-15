using System;
using System.Linq;
using Avalonia;
using Avalonia.Controls.ApplicationLifetimes;
using Avalonia.Markup.Xaml;
using AuroraStudio.Models;
using AuroraStudio.Views;
using Avalonia.Styling;

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
            // capture unhandled exceptions into the log instead of dying silently
            AppDomain.CurrentDomain.UnhandledException += (_, e) =>
            {
                try
                {
                    var dir = System.IO.Path.Combine(
                        Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData), "AuroraStudio");
                    System.IO.Directory.CreateDirectory(dir);
                    System.IO.File.AppendAllText(System.IO.Path.Combine(dir, "crash.log"),
                        DateTime.Now + " FATAL: " + e.ExceptionObject + Environment.NewLine);
                }
                catch { }
            };

            desktop.MainWindow = new MainWindow();
        }

        base.OnFrameworkInitializationCompleted();
    }
}
