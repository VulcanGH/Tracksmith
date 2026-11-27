Tracksmith — Media metadata. Refined.

Windows x64. Run tracksmith.exe to open the browser interface and tray icon.
Closing the browser leaves Tracksmith running. Exit from the tray or Settings;
an active file write and verification will finish before shutdown.

Configuration: %APPDATA%\Tracksmith
Logs: %APPDATA%\Tracksmith\logs
Metadata records: %APPDATA%\Tracksmith\operations
Batch reports: %APPDATA%\Tracksmith\batches
Override configuration folder with TRACKSMITH_CONFIG_DIR.

Install MKVToolNix and FFprobe (from FFmpeg) separately. FFprobe handles routine
audio facts; MediaInfo is optional for full analysis, video/HDR requirements
and compatibility when FFprobe is unavailable. Standard installation paths and
PATH are checked; explicit paths can be set in config.json. This package does
not include third-party media utilities. MKV/MKA only; MP4 is out of scope.

The installer defaults to C:\Program Files\Tracksmith and requests administrator
approval. The application itself runs as your normal user; data stays in AppData.

New libraries use the editable Tracksmith standard profile. Audio examples:
English Dolby TrueHD 7.1 + Atmos; English DTS-HD Master Audio 7.1 + DTS:X.
Subtitles: English (SRT); English (PGS, Forced); English (SRT, SDH).
Video track titles use the exact filename without its final extension.
Naming is staged automatically on open, never saved automatically. Review before
saving. Rules lets you edit each format's template; Profiles lets you disable
automatic staging, duplicate an independent profile, or select your own defaults.
Existing saved libraries are preserved. Add standard profile restores a fresh
factory copy for review without overwriting custom rules.

Open files anywhere using the native picker. Batch: add files/folders, select
a saved rule profile, preview, expand rows to review changes, exclude files,
then save reviewed files. Subfolders is checked by default. Files are listed
before checks finish; saves run one at a time, with no fixed batch file-count cap.
Warnings expand automatically. Check again refreshes one row; Open in Editor
allows manual review or full analysis. Existing editor drafts are protected.
Cancellation completes the current file and skips the remainder. A failed
write or verification stops the queue. Modified files require a new preview.

Writes change original files in place. Metadata records are not backups.

Optional Windows startup and Explorer context menus are in Settings. Portable
users should keep the executable in a stable location before enabling them,
and disable them before moving/deleting it. Configuration remains in AppData.

Command line: tracksmith.exe [--background] [--no-tray] [--port NUMBER] [PATH]
A second invocation reuses the existing instance for that configuration folder.
Explorer selections wait in the interface; existing drafts are preserved.

Updates: About / Settings / tray -> Check for updates. Stable GitHub Releases only;
installation is explicit. Finish work and clear drafts before updating. Installed
copies use Windows Setup; portable copies replace their executable. Only newer
published stable releases are offered. See docs/UPDATES.md.

This Windows release is unsigned. The source is MIT-licensed; LICENSE and
third-party notices are included. Stable releases and source are available at
https://github.com/VulcanGH/Tracksmith

Exit Tracksmith from its tray or Settings before upgrading or uninstalling.
The installer never forcibly terminates it. Uninstall preserves AppData records.
Disable optional per-user integrations in Settings before uninstalling, especially
when installation/uninstallation uses another administrator account.
