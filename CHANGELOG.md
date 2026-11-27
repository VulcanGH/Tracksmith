# Changelog

## 1.0.5

- Add **Verify independently** to batch review rows with warnings or errors.
  Request fresh full-file MediaInfo analysis instead of reusing cached or short
  technical checks, then refresh that row using its reviewed profile.
- Clear warnings when the independent check resolves them. Keep conflicting
  facts, unavailable analysis and file errors visible without changing the
  media file or dismissing unresolved warnings.
- Add regression coverage for independent Atmos confirmation, unresolved
  warnings, unreadable files, cache bypass and unchanged source bytes.

## 1.0.4

- Recover audio identities missed by the initial quick scan with a bounded,
  targeted check of only the missing streams. Handle delayed audio starts and
  interleaved/laced packet ordering without slowing every file or using MediaInfo.
- Mark warning-only batch files as **Needs review**, expand warnings automatically,
  and explain why names were preserved instead of reporting them as unchanged.
- Add **Check again** to refresh one batch row using its reviewed profile while
  preserving the rest of the batch. Add **Open in Editor** for track review,
  manual edits and full analysis; protect existing editor drafts.
- Keep a saving batch busy until its report has been written, so completion
  reliably means the next editor or batch action is available.
- Build packages from a fresh, disposable staging directory and verify the
  executable version before packaging. Pin CI actions to reviewed commits and
  exclude local credentials from Git. Remove superseded profiling scripts and
  condense the metadata investigation into current guidance and measurements.

## 1.0.3

### Faster metadata loading and reuse

- Use FFprobe for routine audio facts and skip technical tools entirely for
  container/subtitle-only rules. Keep MediaInfo for full-file/video requirements,
  explicit retries and a visible compatibility path if FFprobe is unavailable.
- Bind audio streams through validated Matroska packet boundaries and track
  numbers, including otherwise identical tracks. Add checksum-verified native
  TrueHD header checks for later Atmos signalling and corrected channel layouts.
- Deepen audio-frame inspection only for missing layouts or conflicting E-AC-3
  channel counts. Unresolved facts preserve names instead of silently running
  MediaInfo; retain existing JOC/Atmos, DTS:X and codec-profile naming.
- Load editable tracks before background technical analysis completes. Show
  progress and a retry action for incomplete technical details.
- Release the media engine lock during identification and enrichment, including
  batch analysis. Keep the isolated MediaInfo DLL worker and its normal parsing
  depth; bound slow full-file analysis before trying an audio-only sample.
- Cache verified technical facts in memory and across restarts. Invalidate them
  when a file changes or is replaced, or the provider/settings change.
- Reuse reviewed facts through batch saving and verified metadata-only edits.
  Keep fresh MKV identification, chapter/tag preservation and all writer checks.
- List discovered batch files immediately with queued, track-reading and technical
  checking states. Read ahead of the single analysis worker without repeating
  identification. Keep pending files visible and unavailable for saving.
- Retain up to 4,096 cached files within a 128 MiB disk limit so scans of large
  libraries can reuse earlier results; keep the memory cache limited to 64 files.
- Enable **Subfolders** by default when adding folders to a batch. It can still
  be unchecked before previewing.

### Accurate naming and preserved edits

- Hold technical naming until its track facts are verified. Keep manual edits,
  undo history and selection when background results arrive, and discard stale
  analysis after a file changes or its inspection is replaced.
- Preserve existing Atmos/DTS:X titles when detection conflicts, with a note
  for manual review. A short probe's missing immersive flag is inconclusive.
- Treat MediaInfo's DTS:X `Objects` marker as metadata rather than an additional
  physical channel when formatting a verified bed layout.

## 1.0.2

### Clearer batch progress

- Show a spinner and **Finding files…** while discovering batch files, followed
  by **Scanning files… N of M checked**, with the number needing changes.
- Keep **Save reviewed files** visible and disabled during scanning, with
  **Available when scanning finishes** beside it. Saving becomes available only
  after the complete preview is ready.
- Show **Preview complete — N files ready to save** when scanning finishes.
  Empty batches, unchanged files, errors and stopped scans have distinct messages.
- Keep **Stop after current file** available during scanning and saving. Respect
  reduced-motion preferences for the spinner.

### Commentary naming fix

- Fix repeated renaming of correctly formatted commentary subtitles and combined
  audio roles. Preserve genuine commentary descriptions, including Unicode names,
  commas and balanced parentheses. Repair codec/qualifier-only descriptions
  generated by the old parser, such as `Commentary — PGS,`.
- Keep correctly normalised commentary titles unchanged across repeated naming
  runs for both audio and subtitles, including combinations with other roles.

## 1.0.1

Improved batch review for checking large media libraries, with updated standard
naming defaults and faster previews that preserve technical naming accuracy.

### Batch workflow

- Add **Skip unchanged**, checked by default and always visible in the top
  toolbar. Files that need no changes are hidden; errors and warning-bearing
  files remain visible for review. Toggle it off to see every inspected file.
- Add **Search listed files** to filter preview results by filename or path
  without rescanning. Selections survive filtering, and the save button reports
  selected files hidden by search. Results remain in one list, without pagination.
- Add **Clear** to reset batch sources, results, selections and searches.
  Saved Activity reports are retained. Clear is unavailable during active work.
- Show counts for visible, selected, changed, unchanged and failed files.

### Standard naming defaults

- Set the container title to the exact filename without its final extension,
  matching the existing video-track title convention. Preserve punctuation,
  spacing and release details.
- Fill an unspecified video language from the first audio track in container
  order, retaining regional tags such as `en-GB`. If that track has no defined
  language, or no audio exists, use `en`. Preserve defined video languages and
  all audio/subtitle languages.
- Keep both behaviours editable through Rules and staged for review before save.

### Performance and accuracy

- Defer chapter/tag extraction until save instead of repeating it for every
  batch preview. Full inspection and post-write verification remain in place.
- Retain MediaInfo for rules that use technical details, including the standard
  Atmos, DTS-profile and channel-layout naming rules. Simple profiles that do not
  use those details can preview using MKVToolNix track metadata alone.
- Stop batch polling once work has settled and render per-file change tables
  only when expanded. Every file is still inspected before it is classified
  as unchanged.
- Remove the misleading fallback version label while the UI is connecting.

### Upgrading from 1.0.0

The updated standard defaults apply automatically when no rules library has been
explicitly saved. No profile setup is required for those installations.

Explicitly saved rules and profiles retain their settings. Only if you have an
older saved library and want to replace its naming defaults, choose
**Rules → Add standard profile**, save the library and make the new profile active.
Existing custom profiles remain available.

Available through the in-app updater as a stable Windows x64 release, with an
installer, portable ZIP and SHA-256 checksums. MKVToolNix and MediaInfo remain
separate prerequisites.

## 1.0.0

First public Windows x64 release.

- Inspect and edit MKV/MKA container and track metadata without remuxing.
- Compact track editor with bulk changes, undo/redo and manual overrides.
- Editable standard audio/subtitle naming and filename-based video track titles.
- Rules, templates and independently customisable profiles.
- Reviewed serial batches, progress and safe cancellation, with no fixed file-count cap.
- Guarded in-place saves, post-write verification and metadata restoration records.
- Native file/folder selection, tray controls, Explorer integration and optional startup.
- Built-in stable-release updates for installed and portable copies, with SHA-256 verification.
- Program Files installer, portable ZIP and checksums. MIT-licensed source and third-party notices.

Windows packages are unsigned. MKVToolNix and MediaInfo are separate prerequisites.
MP4 is not supported. Metadata restoration records are not full-file backups.
