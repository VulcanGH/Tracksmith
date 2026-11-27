<p align="center">
  <img src="frontend/public/tracksmith.svg" width="72" height="72" alt="Tracksmith logo">
</p>

<h1 align="center">Tracksmith</h1>
<p align="center"><strong>Media metadata. Refined.</strong></p>

A local Windows 11 media metadata editor built with Rust, Axum, React and TypeScript.
Tracksmith includes editable standard naming, reviewed batches and native Windows integration. All changes
are staged for review; the existing in-place writer and verification remain the only save path.

## Install

Download the [latest Windows x64 release](https://github.com/VulcanGH/Tracksmith/releases/latest):

- **Setup EXE:** installs to `C:\Program Files\Tracksmith`; Windows requests administrator approval.
- **Portable ZIP:** extract to a writable folder and run `tracksmith.exe`.
- **SHA256SUMS:** hashes for both packages.

Install [MKVToolNix](https://mkvtoolnix.download/downloads.html#windows) and
[FFprobe from FFmpeg](https://ffmpeg.org/download.html#build-windows) separately.
Standard installation locations and PATH are detected automatically.
[MediaInfo](https://mediaarea.net/en/MediaInfo/Download/Windows) is optional for
full analysis, video/HDR rules and compatibility when FFprobe is unavailable.
Its DLL is used; its GUI executable is not a CLI.
No media tools are bundled. Tracksmith supports MKV/MKA; MP4 is outside scope.

Packages are unsigned. In-app updates verify the matching GitHub release package's SHA-256.
Start with copies of representative files: saves modify originals in place, and metadata restoration
records are not full-file backups. See [naming](docs/NAMING.md), [updates](docs/UPDATES.md) and
[security/data handling](SECURITY.md).

## Build and run

Prerequisites: stable Rust (MSVC with Visual Studio C++ build tools recommended), Node.js 24 LTS or newer, and npm.
Node is a build dependency only. The release executable embeds the compiled interface.

```powershell
powershell -ExecutionPolicy Bypass -File scripts/build.ps1 -Release
.\target\release\tracksmith.exe
```

The executable opens the browser interface and a notification-area icon. Its session URL contains a per-process credential in the URL fragment;
the interface stores it in that tab's session storage and removes it from the address bar. A service
restart requires opening the new session link. Closing the browser leaves the service running;
exit through the tray or Settings. An active write and verification finish before shutdown.
Use `--background` to suppress browser opening, and `--no-tray` for automation.
A second invocation reuses the instance for the same configuration folder. An optional file/folder
argument queues an Explorer selection for explicit opening without discarding editor drafts.

The application binds **127.0.0.1:7476** only. Use `--port 7576` to override the port.
Port precedence: CLI, `TRACKSMITH_PORT`, configuration, default. There is no remote bind option.

### Batch and Windows integration

In **Batch editor**, add files or folders, include subfolders by default and filter by filename,
then choose a saved profile and **Preview batch**. Expand rows to review each before/after value;
exclude files with their checkboxes, then **Save reviewed files**. The batch retains the reviewed
profile revision even if rules subsequently change. Changed file identities or metadata require
a new preview. Saves run serially through the same guarded writer and verification as Editor.
Discovered files appear immediately as queued. Track reading runs ahead of the
single technical worker, with bounded buffering; pending rows are never treated
as unchanged or ready to save. The checked count advances only after review.
**Stop after current file** allows the active operation to finish. A failed save or verification
stops the queue; remaining files are explicitly skipped. There is no automatic rollback or retry.

**Clear** resets batch sources, results, searches and selections when no job is running;
saved Activity reports remain available. **Skip unchanged** is on by default and hides
confirmed unchanged files without warnings. Errors and warnings remain visible.
**Search listed files** filters the current results by filename or path without rescanning
or changing selections; saving includes selected files hidden by search. There is no pagination.
Warning-only rows show **Needs review** and expand their details automatically.
**Check again** refreshes one row without replacing the rest of the batch or its
reviewed profile. **Open in Editor** opens that file for track review, manual
edits or full analysis; save or revert existing editor drafts first. After
saving in Editor, check that batch row again before saving it from the batch.
Delayed audio streams receive targeted packet checks automatically, without
increasing the scan depth for every file or falling back to MediaInfo.

Batch preview reads editable metadata with mkvmerge. Profiles needing audio
facts use FFprobe and targeted Rust header checks; container/subtitle-only
profiles skip technical probing. Video/HDR or whole-file technical rules use
full analysis only when required by the selected profile.
Verified technical facts are cached separately from titles, languages and flags. The
cache survives restarts and is invalidated by external file changes, replacement or
provider changes. Batch review retains its own facts so saving does not repeat
bitstream analysis even if older cache entries have been evicted.
Chapter/tag extraction is deferred to full inspection at save time. The guarded writer and
post-write verification are unchanged. Every file must still be inspected to classify it as unchanged.
While previewing, a spinner and checked/total count show that scanning is still running.
The save button remains visible but disabled until **Preview complete** confirms that
the full batch is ready for review. **Stop after current file** remains available.

There is no fixed batch file-count cap. Resource limits remain: 32 sources, 100,000 discovery entries, 10,000 folders, 32 nesting levels and
8 MiB of preview rows. Discovery skips reparse points. Warning summaries are bounded to 8 KiB
per row; per-file write records retain the detailed save result. **Activity** shows up to 20 recent
batch reports within a 32 MiB response budget. Reports persist under `batches`; per-file metadata
records remain under `operations`. Application logs are under `logs` (rotated at startup above 2 MiB).

The tray offers Open, file/folder selection, logs, About and safe Exit. **Settings → Windows**
provides opt-in current-user startup and Explorer menus for MKV/MKA files and folders. No default
file association changes. Keep a portable executable at a stable path before enabling integrations.
The single-instance launch channel uses an additional credential unavailable to browser code;
an authenticated browser request alone cannot grant an arbitrary launch path.

### Packages

Built-in update checks are in About, Settings and the tray. Downloads use verified
GitHub Releases packages and restart only after active work and drafts are clear.
See [updates and release publishing](docs/UPDATES.md). Only newer published stable
releases are offered by the updater.

After a release build, run `scripts/package.ps1 -Compiler PATH\TO\ISCC.exe` with Inno Setup 7
(or the local `.tools/innosetup/ISCC.exe`). It creates an unsigned portable ZIP and an installer
defaulting to `C:\Program Files\Tracksmith` in `artifacts/packages`. Installation requires administrator
approval; the application runs as the original user and keeps user data in AppData. Exit Tracksmith before upgrading or uninstalling; packaging
does not forcibly close it. Configuration and operation records are preserved on uninstall.
Media utilities are detected separately and are not bundled. MIT licensing and third-party notices
are included in both packages. The Windows executable embeds its interface and static C runtime.
MP4 support is out of scope by user decision.

### Configuration

The first run creates `%APPDATA%\Tracksmith\config.json`. Override the directory using an absolute
`TRACKSMITH_CONFIG_DIR`. Overrides apply to the current run and do not rewrite saved preferences.

```json
{
  "version": 1,
  "port": 7476,
  "media_roots": [],
  "tools": { "mkvmerge": null, "mkvpropedit": null, "ffprobe": null, "mediainfo": null }
}
```

Configuration writes use a synced temporary file and atomic replacement in the same directory.
Unknown configuration versions are rejected; future migrations must be explicit. Media roots and
configured tool paths must be absolute. Optional `tools.mkvextract` and `tools.mediainfo_dll` can also
be set. FFprobe is detected on PATH or in `C:\Program Files\ffmpeg\bin`;
set `tools.ffprobe` to its absolute executable path for other installations.
Standard Windows MKVToolNix and MediaInfo DLL installation paths are detected automatically;
`tools.mediainfo` must refer to a CLI, not the Windows GUI executable.

### Open and edit a file

1. In **Editor**, choose **Open file** to select an MKV/MKA anywhere Windows can access, including network shares. No folder setup is required.
2. Optional: add media folders in **Settings** to also open files there by pasted path using **Inspect path**.
3. Edit the container title, track titles, BCP 47 languages or supported flags. Expand a track for
   additional flags and technical data; attachments, chapters/editions and tags have their own views.
4. Choose **Review save** (or Ctrl+S), inspect the before/after plan, then choose **Write metadata**.
5. Read the verification result. A verified save offers **Review metadata restoration**, which stages
   an inverse plan for review before another write.

### Compact editing and browsing

- **Browse / recent** opens the server-side browser. **Choose folder** uses the native Windows dialog
  for local drives or UNC shares. It grants that folder for the current session without requiring Settings.
- The browser lists folders and MKV/MKA files, supports name filtering and pages of 200 entries, and
  reports scan limits or inaccessible links. Drive letters are shown without probing disconnected drives.
- Recent files and locations are remembered locally (20 each). Clicking one explicitly reopens that
  previously selected path; arbitrary paths cannot be supplied through the recent-history endpoint.
- Starring a browsed folder keeps it as a favourite and a persistent media location. Unstarring removes
  the shortcut; remove the location in Settings to revoke its underlying folder grant.
- Select track checkboxes, use Shift for a range, or select by track type. Choose a property/value and
  **Apply to N** to stage one bulk edit. Unsupported targets reject the entire bulk operation.
- **Undo/Redo** uses Ctrl+Z, Ctrl+Shift+Z or Ctrl+Y. Typing in one field is grouped into one step; bulk
  edits and multi-row pastes are single steps. History is bounded to 60 steps and resets after a save/open.
- Modified cells have individual revert buttons. Expanded rows also offer **Revert track**; **Revert**
  clears all staged edits and can itself be undone.
- **Copy** exports selected titles/languages as TSV. Paste one or two columns into the title column,
  or a single language column into a language cell. Quoted tabs/newlines are supported; overflow and
  unsupported cells are refused before staging. Paste follows the original container track order.
- **Columns** saves visibility choices in this browser. Tab navigates fields; Alt+Up/Down moves between
  rows. Language inputs retain their native searchable dropdown keyboard controls.
- Chapters/editions, tags and attachments use readable tables. Their full XML/technical metadata remains
  available. The compact header includes size, duration and track count; small captions are mostly
  11.5–12 px with 30 px controls and separate flag columns.

### Rules and profiles

Use **Rules** to create, enable, duplicate and reorder rules; drag rows or use the priority arrows.
Structured controls support nested AND/OR matching groups, typed comparisons and bounded regexes,
title templates with a variable picker, title clearing, language values/fallbacks and supported flags.
New libraries start with the **Tracksmith standard** profile: 34 editable enabled
naming rules, automatically staged on open, with explicit review required before any save.
Audio uses `Language Codec Layout + Immersive (Role)`; subtitles use `Language (Format, Qualifiers)`.
Video track and container titles use the filename without its final extension.
Unspecified video language uses the first audio track's language, or `en` when that
track has no defined language or no audio exists. Defined video languages, audio/subtitle
languages, flags and track order are preserved.
Unknown audio/subtitle formats retain their titles. Channel layouts come from validated technical metadata; count-only
fallbacks use `8ch`, not an assumed `7.1`. Atmos/DTS:X never come from title text. Regional language
information is retained and unspecified language displays `Unknown`.

Edit individual codec templates and overwrite policies in **Rules**, or **Duplicate** a profile to
copy its rules independently. Disable automatic staging, disable individual rules or choose another active
profile in **Profiles**. Built-in defaults update automatically when no rules library
has been explicitly saved. Existing saved libraries retain their settings. **Add standard profile** adds a fresh
factory copy for review without replacing your rules; save and make it active to adopt it.
Explicit commentary descriptions and subtitle role labels in existing titles are retained where recognised;
Forced/SDH/accessibility flags also contribute labels. Signs & Songs is never inferred merely from Forced.
Review ambiguous legacy titles before saving. See [the naming standard](docs/NAMING.md) and
[real-world test guide](docs/REAL-WORLD-TESTING.md).

In **Profiles**, assign rules, configure preferred language fallbacks, choose overwrite behaviour and
optionally stage rules automatically on file open. The first preferred language is the explicit fallback;
it is never described as detected audio/subtitle language. Conditional rules retain their additional
replacement conditions even when a profile overrides overwrite behaviour.

Select a profile in Editor and choose **Apply rules**. Rule proposals and manual overrides are separate;
manual edits, including deliberate retention of an original value, survive reapplication and profile
changes. Lock buttons or **Clear overrides** release those choices; apply rules again to refresh proposals.
Undo/redo includes rule application and override changes. Saving/opening a new file resets transient state.

Lower priority numbers win simultaneous conflicts; equal priorities use rule IDs. Evaluation settles
against staged metadata in up to eight passes, so dependent language/name rules can resolve together.
Conflicts identify the pass and both rules. Cycles and evaluations beyond 100,000 steps are rejected
without staging results. Unknown facts never satisfy negative comparisons as if they were known false.
Missing template/fallback values explicitly skip the action or target. Current-title substitution is
excluded from templates to avoid accidental suffix accumulation.

Open a file in Editor, then use **Test rule** or **Test profile** to preview resolved values, conflicts
and missing-value notes, including unsaved rule/profile changes. Tests never write the media file.
Import/export uses a versioned JSON library; imported data is validated before rendering and remains
unsaved until **Save library**. The library lives in `%APPDATA%\Tracksmith\rules.json`, with atomic
replacement, a cross-process save lock and revision checks. It supports up to 128 rules, 32 profiles,
four nested group levels, 64 condition/group nodes per rule and 1,024-byte regex patterns.

MKVToolNix supplies editable metadata and chapter/tag preservation. FFprobe
supplies routine audio facts. Tracksmith validates packet positions against
actual Matroska Block/SimpleBlock boundaries, then binds TrackNumber to the
source UID; identical streams are never joined by title, language or array order.
Native TrueHD checks validate frame sizes, lacing and major-sync checksums before
accepting later Atmos signalling or corrected layouts. Deeper audio-frame
inspection runs only for missing layouts or conflicting E-AC-3 channel counts.
Unknown technical facts preserve affected names and remain visible for review. No codec/title guesses are used to infer Atmos
or DTS:X. Chapters/editions, tags and attachments are inspect-only in this build.

The editor loads the track list first, then checks technical details in the
background without holding the engine lock. You can make manual edits while it
works. Technical rules wait for verified facts; results preserve manual edits and
undo history and cannot attach to a replaced inspection or changed file. Saving
does not automatically reapply rules over a manually saved title.

Routine audio uses a bounded FFprobe call and native gap checks, with no
MediaInfo call or remux. A short scan's missing Atmos/DTS:X flag is not proof
of absence; existing immersive labels remain protected. Unresolved audio does
not silently trigger MediaInfo. **Retry full analysis** explicitly uses it.

MediaInfo remains isolated at its existing parsing depth. It is selected for
video/HDR or whole-file requirements and explicit full retries. If FFprobe is
not installed, a visible diagnostic explains the slower compatibility path.
A compatibility full-file attempt that exceeds eight seconds or fails tries
a temporary stream copy
of the first 30 seconds of audio, without re-encoding or changing the source.
Sample facts are bound through preserved UIDs and independently identified sample
track numbers. Sample statistics are not used as whole-file duration or bitrate.
Incomplete video facts remain marked as unavailable, with **Retry full
analysis** available. **Stop checking** cancels read-only analysis; switching
files also cancels the previous editor's work. Active writes always finish.
Explicit retries and profiles requiring video facts or
whole-stream bitrate keep the original 90-second full-file budget. Formats
MKVToolNix cannot split may remain unconfirmed when
full analysis fails; their technical titles are preserved. Existing immersive
labels are preserved for manual review when they conflict with detected facts.

The bounded cache lives in `%APPDATA%\Tracksmith\technical-cache-v1` (up to 4,096
records and 128 MiB total on disk, at most 4 MiB each; 64 records in memory).
Editable metadata is always read fresh. Cache records
are renewed after Tracksmith's own metadata-only writes only when the existing
property, structure and chapter/tag verification succeeds. Failed analysis is
not recorded as a successful result. See the
[performance investigation](docs/METADATA-PERFORMANCE.md) for measurements and the
continuing evaluation of FFprobe as an audio-first provider.

Language edits use registered BCP 47 tags and the attributed ISO 639-2 table in [data](data/README.md).
When a tag cannot be represented in the legacy field, the plan explicitly shows `und` there while
preserving its canonical IETF tag. A preferred language is never presented as detected content language.

The native picker grants access to the selected file for the current process session, without granting
its containing folder or neighbouring files. The file-picker endpoint accepts a browser marker, never a
client-supplied file path. Up to 32 recent selected files are retained; selection grants are not persisted. The Windows
dialog uses XFold's browser-owner handshake; keep the Tracksmith tab in front while opening it.

All paths are canonicalised and constrained to configured folders or individually selected files. Traversal, alternate data streams,
device paths, symbolic links and reparse points are rejected. Unavailable configured folders are
excluded from the session with a diagnostic, rather than preventing startup. Changing locations clears
cached inspections, plans and individual file/folder grants. History and favourite fields are optional
additions to configuration v1; older files remain readable without a migration. Folder scans are capped
at 5,000 entries and metadata request bodies at 512 KiB; limits are reported explicitly.

Before saving, Tracksmith checks physical file identity, size and modification time, write access,
read-only state and conflicting Tracksmith operations. Windows uses 128-bit file IDs plus volume ID.
A temporary guard prevents file deletion/replacement during save; a per-file OS lock coordinates
Tracksmith processes for the same Windows user. It cannot lock out arbitrary writes by other applications
while permitting mkvpropedit to write. There is a small handoff window between the access probe and
the utility's own write handle. Avoid concurrent external editing of an open file.

The writer calls mkvpropedit with structured arguments and stable UID selectors, then reinspects the
file. It verifies requested values, track identity/order, unrelated identification data, chapters and
tags. File-size changes are allowed as metadata grows. A nonzero/warning exit or verification discrepancy
is reported explicitly. A write continues even if its browser request disconnects; there is no forced
write timeout. Inspections have a 90-second limit per utility and bounded output.

Before each write, the application records changed properties' original values in
`%APPDATA%\Tracksmith\operations`. Restoration covers those values after verified operations and
refuses to overwrite newer changes. It restores discovered effective metadata, not original container
bytes or all possible element-presence distinctions. These records are **not file backups** and cannot
repair an interrupted or damaged file. Failed/uncertain saves are not automatically rolled back.
Full payload hashing/extraction is a development-fixture check, never a production save operation.

Logs go to `%APPDATA%\Tracksmith\logs\tracksmith.log`; set
`RUST_LOG=tracksmith=debug,tower_http=info` for diagnostics. Files above 2 MiB rotate at startup,
retaining one previous log. Session links are printed separately from routine tracing and stored
in the private instance handoff file; neither UI nor launch credentials enter routine logs.

### Development

```powershell
cargo run --locked --bin export-schema
cd frontend
npm ci
npm run generate:api
npm run build
cd ..
cargo run --locked --bin tracksmith
```

Serve the built frontend through Axum for an authenticated application session. `npm run dev` is
available for isolated interface work; it has no cross-origin API access or development auth bypass.
After a frontend change, rebuild assets and restart the backend. OpenAPI comes from Rust/utoipa;
generated TypeScript types are committed. The API's status validator is checked against those types.

Optional tooling in ignored `.tools/` can be selected with `scripts/local-env.ps1`; nothing in the
release depends on this local bootstrap directory. Rust GNU local builds use Rust's self-contained
link libraries. CI uses the standard Windows MSVC toolchain.

## Narrow verification

```powershell
cargo test --locked --lib
cd frontend
npm run test:editor
npm run test:batch
cd ..
python scripts/smoke.py target/release/tracksmith.exe
python scripts/rules_check.py target/release/tracksmith.exe
python scripts/metadata_check.py target/release/tracksmith.exe
python scripts/ffprobe_check.py target/release/tracksmith.exe
python scripts/batch_check.py target/release/tracksmith.exe
python scripts/batch_check.py target/release/tracksmith.exe --benchmark-files 12
python scripts/feasibility.py --mkvtoolnix "C:\Program Files\MKVToolNix" --mediainfo-dll "C:\Program Files\MediaInfo\MediaInfo.dll"
```

The focused backend tests cover configuration, API protections, scope/individual-file-grant/stale-file/operation-lock
guards, structured command/language handling and browser scope/filtering. Frontend workflow tests
cover grouped typing, bulk edits, undo/redo/revert, TSV and proposal/manual separation. The executable smoke
check covers embedded assets, authenticated status and browser scope/pagination/filter/favourites using
disposable configuration and empty placeholder files.
The metadata regression check requires GCC, FFmpeg and MKVToolNix. It uses a
test-only DLL and disposable files to verify delayed analysis, cancellation,
cache invalidation, write reuse and refusal when preservation metadata is missing.
The FFprobe check verifies duplicate-track identity, unknown-data refusal,
selective tool use, cache/write reuse and zero routine MediaInfo calls. Optional
`--late-sample` and `--dtsx-sample` accept regression samples copied into its
temporary workspace; no supplied source is modified.
See [metadata performance results](docs/METADATA-PERFORMANCE.md) for provider
comparisons and read-only corpus benchmark commands.
The batch benchmark generates disposable audio fixtures, runs three previews and
prints their elapsed times without saving media; it does not model large files or network storage.
The rules checks cover convergence, conflicts, missing/unknown facts, protected manual originals, cycle
and regex rejection, revision conflicts, and proposal/manual undo behaviour. `rules_check.py` generates
one short audio fixture and verifies the production rule API leaves its bytes unchanged; it never saves
media. Broad fixture/write regression checks are not repeated for rule-only changes.
The independent feasibility script generates its own tiny media and never accepts an existing media file
as input. It requires FFmpeg and MKVToolNix. MediaInfo can
be supplied through `--mediainfo-cli` or the installed Windows DLL; the GUI executable is not a CLI.
Add `--backend target/release/tracksmith.exe` to exercise the production HTTP inspection/plan/save/restore
pipeline on that same disposable fixture, including read-only and stale-file refusal and payload checks.
Reports go to ignored `artifacts/`. Media utilities are used from local installations and are not bundled.

## Licence

[MIT](LICENSE). See [third-party notices](docs/notices/THIRD-PARTY-NOTICES.txt),
[XFold attribution](docs/notices/XFold-LICENSE.txt) and [language-data attribution](data/README.md).
