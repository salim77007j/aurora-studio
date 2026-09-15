using System;
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

        // Global startup crash capture — writes a crash log next to settings instead of dying silently.
        try
        {
            BuildAvaloniaApp().StartWithClassicDesktopLifetime(args);
        }
        catch (Exception ex)
        {
            try
            {
                string dir = System.IO.Path.Combine(
                    Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData), "AuroraStudio");
                System.IO.Directory.CreateDirectory(dir);
                System.IO.File.AppendAllText(System.IO.Path.Combine(dir, "crash.log"),
                    DateTime.Now + " STARTUP FATAL: " + ex + Environment.NewLine);
                Console.Error.WriteLine(ex);
            }
            catch { }
            throw;
        }
    }

    public static AppBuilder BuildAvaloniaApp()
        => AppBuilder.Configure<App>()
            .UsePlatformDetect()
            .WithInterFont()
            .LogToTrace();
}
