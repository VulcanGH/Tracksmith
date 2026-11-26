# Real-world testing — 1.0.1

Use copies of a few representative MKV/MKA files first: saves modify originals in
place. Operation records restore verified metadata values; they are not file backups.

1. Exit the running copy from the tray or Settings. Install to the default
   `C:\Program Files\Tracksmith` (administrator approval), or extract the portable ZIP.
   Media utilities remain separate prerequisites. Start Tracksmith normally.
2. Confirm the tray and browser open, the three-track favicon appears and Settings
   reports the expected MKVToolNix/MediaInfo availability. User data remains in
   `%APPDATA%\Tracksmith`; the application should run without administrator rights.
3. On a new library, confirm Tracksmith standard is active and titles stage on open.
   If retaining an older saved library, use Add standard profile, Make active and
   Save library. Review the codec rule templates in Rules before saving media.
4. Try TrueHD/Atmos, DTS-HD/DTS:X, AAC, mono/stereo and multichannel examples you own.
   Confirm DTS-HD Master Audio is written in full and video track titles match
   the filename without the final extension, retaining punctuation and release details.
   Confirm container titles use that same filename. Unspecified video language should
   use the first audio track's language (including regional tags), or `en` when that
   track is unspecified or absent. Defined video languages should be preserved.
   Compare the staged titles with inspection details and your existing Plex display.
   Check a count-only/unknown layout produces `8ch`, not an assumed `7.1`.
5. Try SRT, PGS and ASS, including Forced, SDH and named commentary. Confirm qualifiers
   are retained and audio/subtitle language codes, flags and track order stay
   unchanged. Report ambiguous legacy titles before accepting them.
6. Edit one staged title manually, reapply rules, and confirm your override remains.
   Duplicate the standard profile, change one codec template, save/make active and
   reopen a file to confirm independent customised defaults persist after restart.
7. Preview a small batch. Exclude one row, save the reviewed selection, then inspect
   the results and Activity report. Try Stop after current file: the active file must
   finish verification and remaining files must be skipped.
   Confirm Skip unchanged is checked initially; toggle it to reveal unchanged files.
   Search by filename/path and confirm selections survive clearing the search.
   Clear should remove sources and results, leaving saved Activity reports available.
8. Launch a second copy and confirm it reuses the first instance. Optionally enable
   Explorer/startup integration, test it, then disable it again. Closing the browser
   should leave the tray app running; Exit should finish any active work safely.

For an issue, record app/utility versions, detected codec/profile/channel layout,
original and proposed title, expected title, operation/report ID where relevant,
and a minimal anonymised inspection excerpt. Logs are in the config folder's logs
directory. Do not share session.json, session URLs or session credentials.

Include installer install/uninstall, tray controls and native file/folder selection
in hands-on checks. Packages are unsigned Windows builds. MP4 is out of scope.
