# Tracksmith standard naming

The factory profile contains 34 ordinary, editable rules. It is active on a new
library and automatically **stages** metadata changes on open. Saving always
requires review. Installations without an explicitly saved rules library receive
the updated defaults automatically; no profile setup is required.
Explicitly saved libraries retain their settings. Only to replace an older saved
profile with these defaults, use **Add standard profile**, save the library and
make that profile active. Custom profiles remain available.

## Video and container titles

Video tracks stage their title from the exact filename without
the final extension: `Amélie.2001.1080p.mkv` → `Amélie.2001.1080p`. Punctuation,
spaces and release details are retained. The container title uses the same value.
Audio/subtitle tracks do not receive these rules. Both rules can be edited or disabled.

Video language `und` or absent is set from the first audio track in container order,
including regional tags such as `en-GB`. If that audio track has no defined language,
or no audio track exists, the fallback is `en`. Later audio tracks are not searched.
Defined video languages are preserved. This rule does not change audio/subtitle languages.

## Audio

`Language Codec[ Channels][ + Immersive][ (Role)]`

| Identified format/profile | Label |
| --- | --- |
| AC-3 | Dolby Digital |
| E-AC-3 | Dolby Digital Plus |
| TrueHD | Dolby TrueHD |
| DTS | DTS |
| DTS-ES, confirmed variant | DTS-ES / DTS-ES Discrete / DTS-ES Matrix |
| DTS 96/24 | DTS 96/24 |
| DTS-HD, unspecified extension | DTS-HD |
| DTS-HD Master Audio / XLL | DTS-HD Master Audio |
| DTS-HD High Resolution Audio / XBR | DTS-HD HRA |
| DTS Express / LBR | DTS Express |
| AAC, unspecified profile | AAC |
| AAC LC | AAC-LC |
| AAC with SBR | HE-AAC |
| AAC with SBR and PS | HE-AAC v2 |
| FLAC / ALAC | FLAC / ALAC |
| Linear PCM | LPCM |
| Opus / Vorbis | Opus / Vorbis |
| MPEG audio layer III / II / I | MP3 / MP2 / MP1 |

Codec and immersive labels use identified codec IDs/names and validated per-track
MediaInfo enrichment, never a title such as “Atmos”. Unknown formats keep their titles.
Known DTS-HD extensions take precedence over generic DTS. AAC profiles are only
named when inspection reports their identifying features.

Channels count main channels and LFE separately using the reported channel labels:
`C` → `1.0`; `L R` → `2.0`; `L R C Ls Rs` → `5.0`;
`L R C LFE Ls Rs` → `5.1`; an identified eight-channel arrangement with one LFE → `7.1`.
Explicit height positions can produce `7.1.4`, but Atmos alone never establishes
that layout. Duplicate, unrecognised or contradictory layouts fall back to a
reported count (`8ch`); no known layout/count means no channel suffix. MediaInfo's
reported presentation count takes precedence over a container count, which can
describe the core of an enhanced codec.

Examples:

- `English Dolby TrueHD 7.1 + Atmos`
- `English Dolby Digital Plus 5.1 + Atmos`
- `English DTS-HD Master Audio 7.1 + DTS:X`
- `English DTS-ES Matrix 5.1` (does not invent a discrete rear channel)
- `English FLAC 6.0` (six channels without LFE)
- `English AAC-LC 2.0`
- `English FLAC 8ch` (count known; arrangement unknown)
- `English Dolby TrueHD` (neither layout nor count known)
- `Unknown FLAC 1.0` (language unspecified)
- `Portuguese (Brazil) AAC-LC 2.0`

Commentary and audio-description flags, plus explicit existing role descriptions,
append `(Commentary)` or `(Audio Description)`. Recognised named commentary is
retained, e.g. `(Commentary — Director)`. Names/roles are not inferred from filenames.

## Subtitles

`Language (Format[, Qualifiers])`

Formats: `SRT`, `PGS`, `ASS`, `SSA`, `VobSub`, `WebVTT`, `DVB`.
Qualifier order: Forced, SDH, CC, Commentary, Signs & Songs.

- `English (SRT)`
- `English (PGS, Forced)`
- `English (SRT, SDH)`
- `English (PGS, Forced, SDH)`
- `English (ASS, Signs & Songs)`
- `English (SRT, Commentary — Director)`

Forced, hearing-impaired and commentary flags contribute the corresponding labels.
Existing explicit role labels are also retained; `CC` remains distinct from `SDH`.
Signs & Songs requires that explicit description; Forced does not imply it.
Existing flags and audio/subtitle language codes are never changed by these rules.
Ambiguous legacy descriptions must be reviewed rather than assumed correct.

## Customising

Edit a rule's title template or overwrite policy, enable/disable rules and assign
them to profiles. Duplicate a profile to obtain independent copies of its rules.
Use Make active and Save library to choose your own defaults; turn off automatic
staging if desired. Rules can be tested against the open file without writing it.
Manual editor overrides take precedence over proposals.

Add standard profile inserts a fresh factory profile without replacing your
existing library. It is a draft until Save library, and is not activated implicitly.

Technical references: [Matroska codec mappings](https://www.matroska.org/technical/codec_specs.html),
[MediaInfo channel labels](https://mediaarea.net/AudioChannelLayout),
[Dolby Atmos presentation](https://professional.dolby.com/siteassets/tv/home/dolby-atmos/dolby-atmos-for-home-theater.pdf).
