# Metadata retrieval performance investigation

Measured 27 November 2026 on this Windows machine. Baseline: Tracksmith
1.0.2. Implemented and verified locally in 1.0.3; nothing published or
installed over the existing application.

MKVToolNix remains the authoritative container reader and writer. Editable
tracks now load before technical analysis and chapter/tag exports finish.
Routine audio now uses FFprobe plus targeted native Rust checks. The isolated
MediaInfo DLL worker remains available at its existing depth for explicit full
retries, profiles needing full/video facts and compatibility if FFprobe is
unavailable. Verified facts are cached independently of editable metadata.

## Current FFprobe audio implementation

The final release audio-provider implementation completed the same 12-file
batch in 5.604 seconds uncached and 1.913 seconds cached, versus 22.772 and
2.050 seconds on the preceding pipelined MediaInfo build. All twelve files
had identical proposed changes; the two previous timeout/sample warnings
were no longer needed. The list appeared in 0.092 seconds. These runs used
separate empty application caches, with uncontrolled OS caches; they are
observed timings, not guaranteed cold-storage speedup ratios. The earlier debug run was 4.116/1.695 seconds; both raw reports are retained.
Final release format and writer regressions passed.

| File | Tracks visible | Audio ready | Cached reopen |
| --- | ---: | ---: | ---: |
| Schindler's List | 0.757 | 1.183 | 0.318 |
| Dune | 0.487 | 0.865 | 0.293 |
| Jurassic Park III | 0.589 | 0.892 | 0.313 |
| Ready Player One | 0.174 | 0.487 | 0.110 |

All four previews retained identical proposals to the preceding build.
The tested JOC/Atmos, dependent-channel layouts and TrueHD Atmos facts
remained confirmed; originals were never saved in these benchmarks.

Selection and gap handling:

- Container/subtitle-only profiles and rules targeting absent track types skip
  technical tools. Changing a profile requests newly required facts for open
  media without resetting manual edits. MKVToolNix remains
  the authoritative source for mutable metadata and all guarded writes.
- Audio requirements use one audio-selected FFprobe packet/stream call,
  bounded to five wall seconds and an initial 0.5-second packet interval.
  FFprobe retains its normal discovery settings; packet selection is not
  a global reduction of codec discovery depth.
- The Rust bridge walks actual EBML element boundaries from the file, not
  a backward byte-pattern guess. It accepts Block/SimpleBlock payload
  positions only, binds their track number to the source UID and checks
  codec agreement and unique stream binding. Missing/ambiguous evidence
  preserves affected names. No positional/tag join is used.
- Plain/unconfirmed TrueHD additionally reads already identified audio
  blocks, validates all lacing modes and complete access units, then checks
  major-sync CRC before accepting Atmos or a later channel map. Reads are
  capped at 1 MiB per block and 16 MiB total; no full audio extraction is
  performed. Already detected Atmos avoids this gap work.
- Unknown layouts or conflicting E-AC-3 channel counts trigger a bounded
  two-second audio-frame inspection for the affected stream only. JOC and
  DTS:X facts already supplied by FFprobe do not invoke extra parsers.
  Unsupported or inconsistent results stay incomplete and cannot rename.
- Normal audio failures do not silently fall back to MediaInfo or remux.
  Full retries explicitly use MediaInfo; full/video-required profiles use
  the retained 90-second path. Missing FFprobe uses the legacy compatibility
  path with a visible diagnostic. Configure `tools.ffprobe` if necessary.
- Full versus audio coverage and provider/policy fingerprints govern cache
  reuse. The existing API `mediainfo` field carries normalized technical
  facts for compatibility; raw provider/profile strings can differ.

Production API fixture tests verify duplicate streams with identical tags,
manual metadata writes and cache restart, zero routine MediaInfo calls,
no probing for container-only rules and safe refusal when packet identity
is unavailable. The public late-Atmos reproduction is now named with Atmos
and the corrected eight-channel layout by the native parser. The official
DTS:X sample is correctly named by the same runtime path. Native unit tests
also cover CRC corruption/truncation, all lacing modes, negative lace
deltas, multi-byte track numbers, unknown-size clusters and fake packet
offsets inside frame bytes. Temporary source copies were removed.

Evidence: [batch timings](../artifacts/metadata-bulk-ffprobe.json),
[large-file API timings](../artifacts/metadata-api-ffprobe.json), and
[format regressions](../artifacts/ffprobe-runtime-regression.json).

## Earlier MediaInfo/cache implementation results

End-to-end API timings in seconds on the same machine. First tracks means
the editable identification response; technical ready includes background
work from the start of that request. Cached reopen is the median of three
opens. Restart tests use the persistent cache.

| File | Tracks visible | Technical ready | Cached reopen | After restart |
| --- | ---: | ---: | ---: | ---: |
| Schindler's List (37 tracks) | 2.850 | 14.536, partial audio | 0.428 | 0.509 |
| Dune (22 tracks) | 0.729 | 1.245, full | 0.228 | 0.340 |
| Jurassic Park III (52 tracks) | 3.252 | 12.453, partial audio | 0.253 | 0.392 |
| Ready Player One (4 tracks) | 1.148 | 1.629, full | 0.146 | 0.197 |

The old whole-file worker alone exceeded the 25-second benchmark limit on
Schindler's List and Jurassic Park III. The old Dune API call took 1.246
seconds initially and 0.363/0.379 seconds on repeated opens. These are
observed before/after measurements, not controlled cold-cache ratios:
identification, storage and OS caches can dominate first access. None of
the four final previews proposed changing an existing audio title. Original
file sizes and modification times were unchanged; benchmarks never save.

Implemented boundaries:

- Quick identification and native analysis run outside the engine lock.
  Background work has bounded concurrency, cancellation, generation checks
  and identity validation before results attach. The UI shows progress,
  incomplete results, Stop checking and Retry full analysis.
- Persistent atomic JSON records contain technical facts only, bounded to
  4,096 records and 128 MiB total on disk, 64 in memory and 4 MiB per record.
  File identity/stamps, provider fingerprint,
  application version and parsing policy invalidate incompatible records.
  Fresh identification always supplies editable metadata. Failed analysis
  is not cached as success.
- Batch review retains its verified facts through saving. Own writes carry
  facts forward only after fresh structure and write verification succeed.
  Required chapters/tags are captured before writing; inability to capture
  them refuses the write. All original writer checks remain.
- The normal DLL call gets eight seconds before a bounded 30-second audio
  sample fallback. This is a fallback, never the default path. Full retry
  and profiles requiring whole-file/video facts retain the 90-second budget.
  No global ParseSpeed reduction was made. Sample coverage is explicit;
  video facts and whole-file bitrate/duration are not invented from it.
- Samples require unique source UID plus independently verified sample
  number/type mapping. Pending, missing or ambiguous technical facts cannot
  drive automatic technical names. Existing immersive labels conflicting
  with detected evidence are preserved with a review note. Manual edits,
  selection and undo history survive enrichment; saving does not restage
  automatic names.

Regression checks cover cache restart/invalidation, own-write reuse,
responsive API access during delayed native work, stale results, stopping
and replacing analysis, full retries, safe naming and refusal before any
write when preservation metadata is unavailable. Native UI checks used a
disposable fixture to verify progress, the native picker, manual editing
and verified saving.

## Follow-up: bulk import

The live test batch stopped after 45 of 253 files in 84.8 seconds. The
initial implementation still combined identification and technical analysis
serially for each row, so one slow analysis blocked listing the next file.
The 64-record disk limit could also evict the beginning of a large batch
before it was scanned again.

Batch discovery now publishes queued rows immediately. A single bounded
read-ahead worker identifies upcoming tracks while the single technical
worker checks the current file. The identified document is reused, without
a second mkvmerge call. At most one completed document waits in the channel;
the producer can own one additional read in progress. Save availability and
the checked count exclude pending work. Cancellation drains no extra files
into analysis, joins read-ahead and marks unfinished rows as skipped.

Twelve existing files from the reported H: batch were tested read-only,
using isolated configuration and an empty application cache for each
before/after run. Each was then repeated with the populated cache:

| Measurement, seconds | Before | After |
| --- | ---: | ---: |
| First file listed, uncached | 1.803 | 0.048 |
| All 12 listed, uncached | 26.567 | 0.048 |
| First file fully checked, uncached | 1.803 | 1.146 |
| All checks complete, uncached | 26.567 | 22.772 |
| All checks complete, cached | 2.073 | 2.050 |

This primarily improves responsiveness; initial deep analysis remains a
real cost. Uncontrolled OS caches and storage prevent attributing every
timing difference to the pipeline. All 12 files had identical final changes,
warnings and statuses before/after, and unchanged source sizes/mtimes.
The disk cache now retains up to 4,096 records within 128 MiB, independently
of its 64-record memory bound. A regression checks persistence beyond the
memory limit, and delayed-DLL tests check visible queued rows, blocked
premature saves, cache reuse and cancellation with read-ahead waiting.

Reproduce with `scripts/metadata_batch_benchmark.py`, using the executable,
repeated `--file` arguments and `--output` as for the API benchmark below.
Raw local evidence: [before](../artifacts/metadata-bulk-before.json) and
[after](../artifacts/metadata-bulk-after.json).

## Method and limits

- Eight existing MKVs, 16.28–29.53 GiB, opened read-only. Source sizes and
  modification times were unchanged. No source save or remux was performed.
- MKVToolNix 103.0; MediaInfo.dll 26.05; installed Tracksmith 1.0.2 worker;
  FFprobe build `2026-07-27-git-a757b708ae-full_build-www.gyan.dev`.
- Usually three repetitions, reporting median elapsed time. Slow commands
  stop after one repetition. A 25-second deadline censors slow results;
  this is shorter than Tracksmith's existing 90-second inspection timeout.
- OS caches were not flushed. Ordering warms inputs, and first-access
  latency varies substantially. These are observations of the tested paths,
  not controlled cold-cache speedup ratios. Repeated production workers
  also timed out on Jurassic Park III, Superman and Crime 101.
- Windows process CPU and I/O counters were collected. Read bytes include
  cached/logical I/O; they do not measure physical disk throughput. No
  kernel disk trace was collected.
- No MediaInfo CLI was installed. ParseSpeed was set through the DLL SDK,
  using the same option as CLI `--ParseSpeed=...`. Native phase timings
  exclude the Python worker's startup; its process-total time must not be
  compared directly with a single Rust worker call.

Reproduce using [metadata_benchmark.py](../scripts/metadata_benchmark.py):

```powershell
python scripts/metadata_benchmark.py --file "E:\Movies\Example.mkv" `
  --output artifacts/metadata-benchmark.json
```

Use repeated `--file` arguments for a corpus, `--repeats` to change the
default three runs, or `--only dll_0 --only ffprobe_small` to select methods.
The script launches bounded workers and cleans its temporary audio samples.

Measure editable loading, enrichment and cache reuse through the real API:

```powershell
python scripts/metadata_api_benchmark.py target/release/tracksmith.exe `
  --file "E:\Movies\Example.mkv" --output artifacts/metadata-api-after.json
```

This uses isolated configuration and never calls the media save endpoint.

## Comparative timings

Seconds, including subprocess startup for each column below. A timeout is
an incomplete result, not a successful 25-second scan. A dash was not tested.

| File | mkvmerge -J | Baseline Rust/DLL worker | FFprobe default | FFprobe small probe |
| --- | ---: | ---: | ---: | ---: |
| Schindler's List | 0.291 | timeout | 0.195 | 0.052 |
| Dune | 0.276 | 0.056 | 0.053 | 0.044 |
| Ready Player One | 0.223 | 0.039 | 0.088 | 0.042 |
| Jurassic Park III | 3.129 | timeout | 0.057 | 0.051 |
| Gladiator | 3.330 | — | 0.050 | 0.060 |
| The Thing | 0.090 | — | 0.045 | 0.043 |
| Superman | 0.197 | timeout | 0.133 | 0.046 |
| Crime 101 | 3.854 | timeout | 0.051 | 0.047 |

FFprobe settings:

- Default: `-show_streams -show_format -of json`.
- Small: also `-probesize 262144 -analyzeduration 250000
  -max_probe_packets 128`.
- Medium: `-probesize 5000000 -analyzeduration 5000000`, approximately
  0.045–0.053 seconds in the six files tested at that setting.

The [FFmpeg format documentation](https://ffmpeg.org/ffmpeg-formats.html#Format-Options)
defines these byte, microsecond and packet budgets. A probe budget limits
discovery; it is not a guarantee that every relevant feature was found.

### MediaInfo parsing depths

Seconds inside native DLL calls: Open + Inform + JSON decoding. Each cell
is the median of completed native repetitions, not worker process total.
Ordering/cache differences prevent ranking intermediate depths by these
numbers alone.

| File | ParseSpeed 0 | 0.1 | 0.3 | 0.5 |
| --- | ---: | ---: | ---: | ---: |
| Schindler's List | timeout | timeout | timeout | timeout |
| Dune | 0.020 | 0.037 | 0.031 | 0.030 |
| Ready Player One | 0.007 | 0.017 | 0.017 | 0.016 |
| Jurassic Park III | 0.059 | 6.438 | 0.848 | 0.760 |
| Superman | 0.033 | timeout | 21.946 | 0.563 |
| Crime 101 | 0.060 | 21.410 | 0.863 | 0.844 |

ParseSpeed 0 helps many inputs but does not fix Schindler's List. It must
not become an unconditional replacement for the current analysis depth.
MediaInfo's [AC-3 parser](https://github.com/MediaArea/MediaInfoLib/blob/v26.05/Source/MediaInfo/Audio/File_Ac3.cpp)
uses fewer validation frames below 0.3. Its
[Matroska parser](https://github.com/MediaArea/MediaInfoLib/blob/v26.05/Source/MediaInfo/Multiple/File_Mk.cpp)
also changes packet and fill limits at zero. This changes evidence coverage,
not just execution speed.

### Audio-only samples

MKVToolNix copied the first 30 seconds of all audio streams to a temporary
MKA, excluding video, subtitles, attachments, chapters and tags. There was
no transcoding. Samples were about 2.9–11.2 MB in the first four files.

| File | Sample preparation | MediaInfo DLL core on sample |
| --- | ---: | ---: |
| Schindler's List | 1.923 | 0.058 |
| Dune | 2.552 | 0.008 |
| Ready Player One | 1.251 | 0.010 |
| Jurassic Park III | 0.554 | 0.110 |
| Gladiator | 1.123 | 0.065 |
| The Thing | 0.824 | 0.038 |
| Superman | 0.299 | 0.027 |
| Crime 101 | 0.501 | 0.082 |

These are separate measured phases, not an end-to-end application latency.
Worker startup and independent sample identification would add overhead.
Sampling solves the observed slow-file audio retrieval problem, but
preparation alone costs much more than normal MediaInfo inspection on
Dune and Ready Player One. It belongs on a fallback path.

## Baseline costs and redundant work

- `src/media/mediainfo.rs` already calls the DLL through Rust FFI using
  `libloading`, inside a disposable process. There is no GUI invocation.
  The existing process boundary provides timeout and crash containment.
- Native DLL load was roughly 0.7–2.5 ms. On Dune, warm Open was about
  22 ms, Inform 7 ms, and JSON decoding 0.3 ms. Moving the DLL into the
  server could save milliseconds on fast files, but cannot remove slow
  parsing and would make a native hang much harder to contain.
- Schindler's production worker used about 0.59 CPU seconds in 25 wall
  seconds, with about 132 MB of logical reads and 2,019 read operations.
  An explicit-depth run later read about 769 MB and still timed out.
  This is consistent with I/O-bound parser traversal. The precise physical
  device or parser wait was not established by a kernel trace.
- `src/media/process.rs` polls completion every 20 ms and starts two pipe
  reader threads per call. This adds bounded overhead, not tens of seconds.
- A separate live API check on Dune measured inspection at 1.246 seconds
  on its first call, then 0.363 and 0.379 seconds. Rule previews took
  2.3–3.2 ms. Optimising the naming engine would not address this delay.
- In 1.0.2, batch preview, save preparation, and post-save inspection each request
  technical analysis. Reopening also reanalyses instead of reusing facts.
  The existing snapshots are short-lived inspection/plan state, not a
  reusable technical-facts cache. Batch already defers chapter/tag exports.
- In 1.0.2, work runs through `spawn_blocking`, so it does not directly block Tokio's
  event loop. However, inspection holds the engine lock until external
  tools finish; competing media operations are rejected as busy. The
  inspect response contains no tracks until enrichment has finished.

## Accuracy checks and alternative providers

Observed checks covered TrueHD Atmos, E-AC-3 JOC/Atmos, E-AC-3 dependent
substreams, DTS-HD MA, channel layouts/counts, available lossless bit depth,
10-bit HEVC, HDR10/HDR10+, and Dolby Vision profiles 7 and 8.

- Across seven completed whole-file ParseSpeed-0 results, 14 audio tracks
  matched the default-depth audio sample on codec/profile, additional
  features, commercial format, channels, layout, bit depth and sample rate,
  joined by UID. Schindler's whole-file zero-depth result was unavailable.
- FFprobe default and small probes agreed on all retained evidence fields
  across all eight inputs. They detected TrueHD Atmos and E-AC-3 Atmos;
  Jurassic Park III reported an eight-channel `5.1.2` layout. A channel
  count of eight must not independently be treated as proof of 7.1.
- MKVToolNix already returned `TrueHD Atmos` and `DTS-HD Master Audio`
  labels in this corpus, plus counts and some bit depths. Reuse those
  facts. It did not provide the explicit audio layouts/JOC evidence needed
  for every existing naming case.
- Basic FFprobe stream reports exposed Dolby Vision configuration records
  but omitted HDR10+ evidence present in MediaInfo. Reading one video
  frame on Superman exposed HDR10+ and Dolby Vision RPU metadata and took
  0.515 seconds. Replacing MediaInfo with only `-show_streams` loses detail.
- The sampled FFprobe JSON did not expose Matroska track UIDs. It cannot
  simply be attached to editable tracks by array position. Replacing the
  provider would require reliable identity mapping as well as format parity.
- Additional targeted DTS:X, late Atmos and track-identity checks are
  recorded below. These supplement the eight-film corpus.

### Additional FFprobe validation and decision

The official [FFmpeg DTS:X fixture](https://fate-suite.ffmpeg.org/dts/dtsx_7.1.4_24bit.dts)
was remuxed to a disposable MKA. FFprobe default, small and medium probes
all detected DTS-HD MA + DTS:X, eight bed channels, 7.1 and 24-bit depth.
The default first run took 0.244 seconds; warmed small/medium runs took
about 0.071 seconds. MediaInfo at depths 0, 0.3, 0.5 and 1 also detected
DTS:X. Its layout includes an `Objects` marker; that marker must not count
as an extra physical channel. MKVToolNix reported DTS-HD MA without DTS:X.

The public [late-start TrueHD Atmos reproduction](https://github.com/MediaArea/MediaInfoLib/issues/2685)
was tested separately. FFprobe default, small and medium all reported plain
TrueHD, six channels and 24-bit depth. MediaInfo at 0, 0.3 and 0.5 also
missed Atmos; ParseSpeed 1 detected it. This demonstrates why a missing
flag is inconclusive even when multiple shallow providers agree. It does
not justify globally scanning every movie at maximum depth. Temporary
copies of both upstream samples were removed after recording evidence.

A generated two-track fixture had identical FLAC streams, distinct large
Matroska UIDs and track numbers, and different editable titles/languages.
FFprobe reported `id: N/A` for both, with identical codec-private hashes.
Position, codec, extradata and mutable tags cannot safely identify these
tracks. FFmpeg's [Matroska reader](https://github.com/FFmpeg/FFmpeg/blob/master/libavformat/matroskadec.c)
parses TrackUID internally; ordinary FFprobe output does not expose that
as a stable track ID. Direct library integration alone does not solve it.

FFprobe should remain a candidate for an audio-first path: accept positive
facts only when their track mapping and required fields are established,
and use MediaInfo for ambiguous mapping or unconfirmed facts. An initial
single-audio/unambiguous case could be small, but a general unconditional provider
replacement was not enabled at this stage of the investigation. The observed DTS:X result is good;
late Atmos and stream identity still prevent unconditional replacement.
Channel tests include real 5.1.2 and DTS:X 7.1 material plus layout regression
cases; they are not exhaustive coverage of every unusual configuration.

We only need a small facts contract: codec/profile, immersive evidence,
physical layout/channel count, lossless bit depth and any video/HDR facts
actually used by active rules. Another tool is useful only if it improves
those facts or reliably maps them to the authoritative container tracks.
Adding more wrappers around the same parsers would add complexity without
addressing the measured bottleneck. No new runtime dependency was added.

These checks establish observed agreement, not exhaustive ground truth.

### Revisited: small additions to FFprobe

The following read-only prototypes motivated the implemented runtime
checks above. Their early results are retained as investigation history.

**Track mapping:** FFprobe packet positions point to Matroska block payloads
in the tested build. The block's VINT TrackNumber can be joined to the
authoritative number/UID from mkvmerge. A small reader checked surrounding
Block/SimpleBlock element headers and required consistent, one-to-one
mapping. It distinguished two identical FLAC streams with identical mutable
tags, validating 11 distinct blocks per track, in 0.027–0.036 seconds for
the combined FFprobe call. Four real movies also mapped successfully;
their audio-selected packet/stream calls took 0.032–0.152 seconds.
The earlier ordinary stream-ID collision remains real, but this is a
promising solution without modifying or forking FFmpeg. Production work
must validate lacing, parser-adjusted/missing positions, BlockGroups,
multi-byte numbers, delayed tracks and malformed input. Unknown positions
or ambiguous block boundaries must fall back, never guess.
See [packet mapping evidence](../artifacts/ffprobe-packet-mapping-validation.json),
the [Matroska block specification](https://www.rfc-editor.org/rfc/rfc9559.html#section-10.1)
and [FFmpeg's demuxer](https://github.com/FFmpeg/FFmpeg/blob/master/libavformat/matroskadec.c).

**Late TrueHD Atmos:** Stream-only inspection of the public reproduction
again reported six channels, 5.1(side), with no Atmos profile. Audio-frame
inspection took 0.130 seconds, decoded 374 frames and updated the layout
to eight channels/7.1, but still exposed no Atmos profile. Packet header
inspection found a major-sync change at 0.337 seconds: four substreams
and the spatial-presentation flag, matching
[FFmpeg's TrueHD Atmos condition](https://github.com/FFmpeg/FFmpeg/blob/master/libavcodec/mlpdec.c).
An experimental field reader identified that positive signal; a production
reader must also validate framing, lengths and header checksums. This is
header inspection, not a requirement to decode/render Atmos objects.
See [late-frame/header evidence](../artifacts/ffprobe-late-frame-validation.json).
Temporary fixtures were removed; original movie sizes/mtimes were unchanged.

The revised recommendation is an FFprobe-first audio provider with a small
verified track bridge, followed by targeted TrueHD/E-AC-3 header inspection
where the required evidence is missing. Keep MediaInfo for unresolved facts
and video/HDR requirements not met by the fast path. Do not build a general
replacement for FFmpeg or MediaInfo. Compare normalized naming results,
not raw provider strings, across the full corpus before switching defaults.
Short negative scans still cannot prove a feature never occurs later;
coverage and conflicting evidence must govern deeper analysis.

Direct FFmpeg integration would use
[`avformat_open_input` and `avformat_find_stream_info`](https://ffmpeg.org/doxygen/trunk/group__lavf__decoding.html).
It still performs packet analysis; removing the executable does not remove
that work. This machine has a static FFprobe build, not an installed shared
library SDK. No direct-libavformat benchmark was performed. Shipping and
maintaining libavformat/libavcodec/libavutil integration adds dependency,
ABI and native-failure handling costs without solving the identity issue.

[matroska-demuxer](https://docs.rs/matroska-demuxer/latest/matroska_demuxer/)
offers native Rust container access, but container headers alone do not
replace immersive bitstream detection. Replacing MKVToolNix also requires
parity for UIDs, languages, flags and writer verification. No native Rust
container replacement was built or benchmarked; there is no evidence that
this is the important bottleneck.

[AudioProbe](https://github.com/wabisabi926/audioprobe#limitations) explicitly
does not evaluate E-AC-3 dependent substreams or Atmos/JOC. That conflicts
with files verified here. It was evaluated from its documented scope and
rejected as a replacement, without adding or building the dependency.

## Implementation sequence and remaining provider work

1. **Make naming safe and reuse existing facts.** Determine requirements
   from the active rules and actual tracks. Request extra analysis only
   for missing facts. Preserve technical audio titles on failure or
   unconfirmed mapping. Never classify pending/failed analysis as an
   unchanged batch result, or interpret it as “no Atmos”.
2. **Cache technical facts separately from editable metadata.** Always
   reread MKV identification for titles, languages, flags and structure.
   Reuse verified bitstream facts through preview/save/verification;
   continue verifying all written properties and preserved chapters/tags.
3. **Return the editable track list after identification.** Enrich in a
   bounded background worker with explicit pending/ready/failed status.
   Release the engine lock during analysis and verify file identity and
   inspection generation before attaching results. Keep manual edits;
   stage technical names only when their required facts are ready.
   Container/subtitle rules that already have complete facts can proceed.
4. **Handle slow files without reducing quality globally.** Keep the
   normal isolated DLL path for fast files; use a short, measured read-only
   worker deadline before trying the audio-only fallback. Match samples
   through preserved source UID and independently identified sample track
   numbers. Do not use sample duration/size/bitrate as whole-file values.
   Treat quick/sampled negative immersive results as incomplete when
   stronger existing evidence conflicts; preserve the title and request
   deeper analysis. Do not globally adopt ParseSpeed 0 yet.

The implementation uses an in-session facts cache and persists bounded,
versioned records as atomic JSON; no database or new service is needed.
Records use canonical path, stable file identity and stamps, provider/version
and parsing policy; verified track signatures govern own-write carry-over.
Invalidate on external changes, replacement, provider/settings changes,
or incompatible structure. Changing rule profiles should request missing
facts without discarding valid cached facts.

For Tracksmith's own title/language/flag writes, retain facts only within
the existing locked operation after fresh identification and structural
verification succeed. Advance the cache's file stamp then. This is a
controlled exception for verified metadata-only writes, not permission to
reuse a cache after arbitrary external changes with the same track UIDs.
Do not cache failed analysis as a completed result.

The regression coverage and targeted external fixture checks described
above are complete. Writer safety checks, batch progress and the
unpaginated list remain in place. Future FFprobe adoption must preserve
these boundaries and demonstrate reliable track mapping.

## Local evidence retained

- [Primary provider/depth measurements](../artifacts/metadata-benchmark.json)
- [DTS comparisons](../artifacts/metadata-benchmark-dts.json)
- [Additional Atmos/HDR comparisons](../artifacts/metadata-benchmark-joc.json)
- [Repeated production-worker timings](../artifacts/metadata-benchmark-warm.json)
- [Live API timings](../artifacts/metadata-api-profile.json)
- [HDR frame evidence](../artifacts/metadata-hdr-frame.json)
- [Audio field comparison checks](../artifacts/metadata-accuracy-checks.json)
- [Implemented API/cache timings](../artifacts/metadata-api-after.json)
- [DTS:X-positive evidence](../artifacts/ffprobe-dtsx-validation.json)
- [Late Atmos evidence](../artifacts/ffprobe-edge-validation.json)
- [Ambiguous FFprobe identity evidence](../artifacts/ffprobe-identity-validation.json)
- [Archived audio-only prototype](../artifacts/audio-only-prototype.patch)

Benchmark syntax and the recorded field comparisons passed. All temporary
audio samples and the isolated API server/configuration were cleaned up.
The evidence files and prototype patch are retained intentionally for review.
