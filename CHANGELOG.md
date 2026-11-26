# Changelog

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

Existing rules and profiles are preserved. To adopt the updated naming defaults,
choose **Rules → Add standard profile**, save the library and make the new profile
active. Existing custom profiles remain available.

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
