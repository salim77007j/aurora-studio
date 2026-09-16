using System;
using System.Threading.Tasks;
using Avalonia;
using Project = Avalonia.Platform;

namespace AuroraStudio;

internal static class Program
{
    [STAThread]
    public static void Main(string[] args)
    {
        // Engine self-test mode — runs before any UI init so it works headless (CI-safe).
        if (System.Linq.Enumerable.Contains(args, "--selftest"))
        {
            int rc = AuroraStudio.Interop.Engine.aurora_selftest();
            Console.WriteLine($"[aurora] engine selftest: {(rc == 0 ? "PASS" : "FAIL " + rc + " " + AuroraStudio.Interop.Engine.LastError())}");
            Environment.Exit(rc == 0 ? 0 : 1);
            return;
        }

        // v3.0 exhaustive ops audit — every new adjustment/effect applies + undoes cleanly.
        if (System.Linq.Enumerable.Contains(args, "--opsaudit"))
        {
            int rc = AuroraStudio.Interop.Engine.aurora_ops_audit();
            Console.WriteLine($"[aurora] ops audit: {(rc == 0 ? "PASS (37 ops)" : "FAIL " + rc + " " + AuroraStudio.Interop.Engine.LastError())}");
            Environment.Exit(rc == 0 ? 0 : 1);
            return;
        }

        // ---- global crash capture: log every unhandled exception instead of dying silently ----
        AppDomain.CurrentDomain.UnhandledException += (_, e) =>
            WriteCrash("APPDOMAIN", e.ExceptionObject as Exception ?? new Exception(e.ExceptionObject?.ToString()));
        TaskScheduler.UnobservedTaskException += (_, e) =>
        {
            WriteCrash("TASK", e.Exception);
            e.SetObserved();
        };

        try
        {
            BuildAvaloniaApp().StartWithClassicDesktopLifetime(args);
        }
        catch (Exception ex)
        {
            WriteCrash("STARTUP", ex);
            throw;
        }
    }

    internal static void WriteCrash(string kind, Exception ex)
    {
        try
        {
            string dir = System.IO.Path.Combine(
                Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData), "AuroraStudio");
            System.IO.Directory.CreateDirectory(dir);
            System.IO.File.AppendAllText(System.IO.Path.Combine(dir, "crash.log"),
                $"{DateTime.Now:yyyy-MM-dd HH:mm:ss} {kind}: {ex}{Environment.NewLine}---{Environment.NewLine}");
        }
        catch { }
        try { Console.Error.WriteLine($"[aurora:{kind}] {ex}"); } catch { }
    }

    public static AppBuilder BuildAvaloniaApp()
        => AppBuilder.Configure<App>()
            .UsePlatformDetect()
            .WithInterFont()
            .LogToTrace();
}
