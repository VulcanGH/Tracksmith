# Metadata inspection and measured performance

Tracksmith uses only the providers required by the selected rules and the
track types present in each file. MKVToolNix remains authoritative for
editable metadata and every guarded write.

## Provider selection

- Container/subtitle-only rules skip technical probing.
- Routine audio uses FFprobe, followed by narrowly targeted gap checks.
- Packet positions are validated against actual Matroska Block/SimpleBlock
  boundaries and bound through track number to the source UID. Stream order,
  mutable names and languages never establish identity.
- Missing streams receive bounded, stream-specific packet reads. This covers
  delayed starts and packet ordering without deepening every file's scan.
- Native TrueHD checks validate frame sizes, lacing and major-sync checksums
  before accepting later Atmos signalling or corrected layouts.
- Missing layouts or conflicting E-AC-3 channel counts request deeper frame
  inspection for the affected stream only. Existing confirmed JOC/Atmos and
  DTS:X facts need no extra parser.
- MediaInfo remains isolated for explicit full retries, selected full/video
  requirements and compatibility when FFprobe is unavailable. It retains its
  original parsing depth; audio sampling remains a slow-file fallback.
- Unresolved technical facts preserve affected names and remain visible for
  review. A short probe's negative result cannot prove a feature never appears
  later. Existing immersive labels remain protected when evidence conflicts.

## Responsiveness and reuse

Editable tracks load before background enrichment completes. Identification
and analysis run outside the engine lock, with bounded concurrency,
cancellation and identity/generation validation before results attach.
Manual edits, selection and undo history survive background results.

Verified technical facts are cached separately from mutable metadata:
4,096 records and 128 MiB total on disk, 64 in memory and 4 MiB per record.
External file changes, replacement, provider fingerprints and parsing-policy
changes invalidate records. Metadata-only writes retain facts only after
fresh structure, property and chapter/tag verification succeed.

Batch discovery lists queued files immediately. Read-ahead overlaps track
reading with the single technical worker. Pending results cannot be saved.
Warning-only rows show Needs review. Check again refreshes one row using its
reviewed profile; other rows and selections remain intact.

## Measurements

Measured on Windows on 27 November 2026. Application caches were empty for
initial runs; OS caches were not flushed. Storage and cache effects mean
these observations are not guaranteed cold-storage speedup ratios.
All original media was read-only; source sizes/mtimes remained unchanged.

| Twelve-file batch | Preceding MediaInfo path | FFprobe path |
| --- | ---: | ---: |
| All checks complete, initial run | 22.772 s | 5.604 s |
| All checks complete, cached repeat | 2.050 s | 1.913 s |

The twelve files retained identical proposed changes. Four additional large
movie previews also retained identical proposals, including verified TrueHD
Atmos, E-AC-3 JOC, dependent-channel layouts and 5.1.2 channel naming.

The 1.0.4 follow-up resolved seven warning-bearing files in 3.905 seconds
on a debug build, with an empty application cache and warm OS caches. A
cached repeat took 1.202 seconds. Targeted recovery covered audio beginning
around 19 or 24 seconds and tracks with early timestamps that appeared later
in demuxed packet order. Six files needed no changes; one gained three
commentary-title proposals with confirmed codec profiles/channel layouts.

## Reproduce and verify

```powershell
python scripts/metadata_batch_benchmark.py target/release/tracksmith.exe `
  --file "E:\Movies\Example.mkv" --output artifacts/batch-benchmark.json
```

Repeat `--file` for a small corpus. The benchmark uses isolated configuration,
checks source stamps and never calls the media save endpoint. Raw reports
belong in ignored `artifacts/`, not in source control or release packages.

`ffprobe_check.py` verifies duplicate identity, delayed starts, unknown-data
refusal, targeted retries, reviewed-profile retention and cache/write reuse.
Optional late-Atmos and DTS:X samples are copied into temporary storage.
`metadata_check.py` checks isolation, cancellation and writer preservation
with a test-only DLL. Rust tests cover checksums, malformed/truncated headers,
lacing, fake packet offsets and multi-byte track numbers. These development
fixtures and tests are not part of the distributed application.
