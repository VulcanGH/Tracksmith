//! Audio facts from FFprobe, bound to actual Matroska blocks rather than stream order.
use super::{model::Inspection, process};
use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs::File,
    io::{BufReader, Read, Seek, SeekFrom},
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

const PROBE_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_BLOCK: u64 = 1024 * 1024;
const MAX_HEADER_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Default, Debug)]
struct TrueHd {
    headers: usize,
    atmos: bool,
    layout: String,
}

#[derive(Debug)]
struct Block {
    number: u64,
    truehd: TrueHd,
}

fn vint(input: &[u8], keep_marker: bool) -> Result<(u64, usize, bool)> {
    let first = *input.first().context("Truncated EBML integer")?;
    ensure!(first != 0, "Invalid EBML integer");
    let width = first.leading_zeros() as usize + 1;
    ensure!(width <= 8 && input.len() >= width, "Truncated EBML integer");
    let mut number = u64::from(u16::from(first) & (0xff_u16 >> width));
    for byte in &input[1..width] {
        number = (number << 8) | u64::from(*byte);
    }
    let unknown = number == (1_u64 << (width * 7)) - 1;
    if keep_marker {
        number |= 1_u64 << (width * 7);
    }
    Ok((number, width, unknown))
}

fn read_vint(reader: &mut impl Read, keep_marker: bool) -> Result<(u64, bool)> {
    let mut bytes = [0; 8];
    reader.read_exact(&mut bytes[..1])?;
    ensure!(bytes[0] != 0, "Invalid EBML integer");
    let width = bytes[0].leading_zeros() as usize + 1;
    reader.read_exact(&mut bytes[1..width])?;
    let (number, _, unknown) = vint(&bytes[..width], keep_marker)?;
    Ok((number, unknown))
}

/// Validate all three Matroska lacing modes before inspecting codec bytes.
fn frames(block: &[u8]) -> Result<Vec<&[u8]>> {
    let (_, width, _) = vint(block, false)?;
    ensure!(block.len() >= width + 3, "Truncated block header");
    let mode = (block[width + 2] >> 1) & 3;
    let mut offset = width + 3;
    if mode == 0 {
        return Ok(vec![&block[offset..]]);
    }
    let count = usize::from(*block.get(offset).context("Missing lace count")?) + 1;
    offset += 1;
    let mut sizes = Vec::with_capacity(count);
    if mode == 2 {
        ensure!(
            (block.len() - offset).is_multiple_of(count),
            "Invalid fixed lacing"
        );
        sizes.resize(count, (block.len() - offset) / count);
    } else {
        for index in 0..count - 1 {
            let size = if mode == 1 {
                let mut size = 0_usize;
                loop {
                    let byte = *block.get(offset).context("Truncated Xiph lace")?;
                    offset += 1;
                    size = size
                        .checked_add(usize::from(byte))
                        .context("Lace overflow")?;
                    if byte != 255 {
                        break;
                    }
                }
                size
            } else {
                let (number, width, unknown) = vint(&block[offset..], false)?;
                ensure!(!unknown, "Reserved lace size");
                offset += width;
                if index == 0 {
                    usize::try_from(number)?
                } else {
                    let delta = i128::from(number) - ((1_i128 << (7 * width - 1)) - 1);
                    usize::try_from(sizes[index - 1] as i128 + delta)?
                }
            };
            ensure!(size <= block.len(), "Lace exceeds block");
            sizes.push(size);
        }
        let used: usize = sizes.iter().sum();
        sizes.push(
            (block.len() - offset)
                .checked_sub(used)
                .context("Invalid lace sizes")?,
        );
    }
    let mut result = Vec::with_capacity(count);
    for size in sizes {
        let end = offset.checked_add(size).context("Lace overflow")?;
        result.push(block.get(offset..end).context("Truncated lace")?);
        offset = end;
    }
    ensure!(offset == block.len(), "Unaccounted block bytes");
    Ok(result)
}

fn truehd_header(header: &[u8]) -> Option<(bool, String)> {
    if header.len() < 28 || header[..4] != [0xf8, 0x72, 0x6f, 0xba] {
        return None;
    }
    let size = 28
        + if header[25] & 1 != 0 {
            2 + 2 * usize::from(header[26] >> 4)
        } else {
            0
        };
    let header = header.get(..size)?;
    // TrueHD major-sync CRC-16 (polynomial 0x002d) and trailing checksum word.
    let mut crc = 0_u16;
    for byte in &header[..size - 4] {
        crc ^= u16::from(*byte) << 8;
        for _ in 0..8 {
            crc = (crc << 1) ^ if crc & 0x8000 != 0 { 0x002d } else { 0 };
        }
    }
    let checksum =
        crc.swap_bytes() ^ u16::from_le_bytes(header[size - 4..size - 2].try_into().ok()?);
    if checksum != u16::from_le_bytes(header[size - 2..].try_into().ok()?) {
        return None;
    }
    let channels = u32::from_be_bytes(header[4..8].try_into().ok()?) & 0x1fff;
    let positions = [
        "L R", "C", "LFE", "Ls Rs", "Tfl Tfr", "Lc Rc", "Lb Rb", "Cs", "Tc", "Lss Rss", "Lw Rw",
        "Tfc", "LFE2",
    ];
    let layout = positions
        .iter()
        .enumerate()
        .filter(|(bit, _)| channels & (1 << bit) != 0)
        .map(|(_, value)| *value)
        .collect::<Vec<_>>()
        .join(" ");
    Some((header[16] >> 4 == 4 && header[17] & 0x80 != 0, layout))
}

fn inspect_truehd(block: &[u8]) -> Result<TrueHd> {
    let mut result = TrueHd::default();
    for frame in frames(block)? {
        let mut offset = 0;
        while offset + 8 <= frame.len() {
            let size =
                usize::from(u16::from_be_bytes([frame[offset], frame[offset + 1]]) & 0xfff) * 2;
            if size < 8 || size > frame.len() - offset {
                break;
            }
            if let Some((atmos, layout)) = truehd_header(&frame[offset + 4..offset + size]) {
                result.headers += 1;
                result.atmos |= atmos;
                if layout.split_whitespace().count() > result.layout.split_whitespace().count() {
                    result.layout = layout;
                }
            }
            offset += size;
        }
    }
    Ok(result)
}

/// Walk real element boundaries, rejecting guessed packet offsets inside codec bytes.
fn blocks(
    path: &Path,
    targets: &BTreeMap<u64, u64>,
    truehd_streams: &HashSet<u64>,
    cancel: &AtomicBool,
) -> Result<BTreeMap<u64, Block>> {
    let mut reader = BufReader::new(File::open(path)?);
    let file_size = reader.get_ref().metadata()?.len();
    let last = *targets
        .keys()
        .next_back()
        .context("No audio packet positions")?;
    ensure!(last < file_size, "Packet position exceeds file");
    let mut parents: Vec<(u64, u64, bool)> = Vec::new();
    let mut result = BTreeMap::new();
    let mut read_bytes = 0;
    let started = Instant::now();
    for _ in 0..100_000 {
        ensure!(!cancel.load(Ordering::SeqCst), "Audio inspection cancelled");
        ensure!(
            started.elapsed() < Duration::from_secs(5),
            "Block mapping exceeded its budget"
        );
        let position = reader.stream_position()?;
        if position > last || position >= file_size {
            break;
        }
        while parents.last().is_some_and(|(end, _, _)| position >= *end) {
            parents.pop();
        }
        let (id, _) = read_vint(&mut reader, true)?;
        let (size, unknown) = read_vint(&mut reader, false)?;
        let payload = reader.stream_position()?;
        if id == 0x1f43b675
            && parents
                .last()
                .is_some_and(|(_, kind, unknown)| *kind == 0x1f43b675 && *unknown)
        {
            parents.pop();
        }
        let parent = parents.last().map(|(_, kind, _)| *kind);
        let limit = parents.last().map(|(end, _, _)| *end).unwrap_or(file_size);
        let end = if unknown {
            limit
        } else {
            payload.checked_add(size).context("Element size overflow")?
        };
        ensure!(
            end <= limit && end <= file_size,
            "Element exceeds its container"
        );
        let master = matches!(
            (id, parent),
            (0x18538067, None) | (0x1f43b675, Some(0x18538067)) | (0xa0, Some(0x1f43b675))
        );
        if master {
            ensure!(parents.len() < 4, "Unexpected container nesting");
            parents.push((end, id, unknown));
            continue;
        }
        ensure!(!unknown, "Unknown-sized non-container element");
        if let Some(stream) = targets.get(&payload) {
            ensure!(
                matches!((id, parent), (0xa3, Some(0x1f43b675)) | (0xa1, Some(0xa0))),
                "Packet is not a Matroska audio block"
            );
            let (number, _) = read_vint(&mut reader, false)?;
            ensure!(
                reader.stream_position()?.saturating_add(3) <= end,
                "Truncated block header"
            );
            let mut truehd = TrueHd::default();
            if truehd_streams.contains(stream)
                && size <= MAX_BLOCK
                && read_bytes + size <= MAX_HEADER_BYTES
            {
                reader.seek(SeekFrom::Start(payload))?;
                let mut bytes = vec![0; usize::try_from(size)?];
                reader.read_exact(&mut bytes)?;
                read_bytes += size;
                truehd = inspect_truehd(&bytes)?;
            }
            result.insert(payload, Block { number, truehd });
        }
        reader.seek(SeekFrom::Start(end))?;
    }
    Ok(result)
}

fn layout(value: &str) -> Option<String> {
    let value = match value {
        "mono" => "M",
        "stereo" => "L R",
        "2.1" => "L R LFE",
        "3.0" => "L R C",
        "3.0(back)" => "L R Cs",
        "4.0" => "L R C Cs",
        "quad" => "L R Lb Rb",
        "quad(side)" => "L R Ls Rs",
        "3.1" => "L R C LFE",
        "5.0" => "L R C Lb Rb",
        "5.0(side)" => "L R C Ls Rs",
        "4.1" => "L R C LFE Cs",
        "5.1" => "L R C LFE Lb Rb",
        "5.1(side)" => "L R C LFE Ls Rs",
        "6.1" => "L R C LFE Cs Ls Rs",
        "6.1(back)" => "L R C LFE Lb Rb Cs",
        "7.1" => "L R C LFE Lb Rb Ls Rs",
        "7.1(wide)" => "L R C LFE Lb Rb Lc Rc",
        "5.1.2" => "L R C LFE Ls Rs Tfl Tfr",
        "7.1.2" => "L R C LFE Lb Rb Ls Rs Tfl Tfr",
        "7.1.4" => "L R C LFE Lb Rb Ls Rs Tfl Tfr Tbl Tbr",
        _ => return None,
    };
    Some(value.into())
}

fn text(value: &Value) -> Option<String> {
    value
        .as_str()
        .map(str::to_owned)
        .or_else(|| value.as_u64().map(|v| v.to_string()))
}

fn codec_matches(codec: &str, source: &str) -> bool {
    match source {
        "A_TRUEHD" => codec == "truehd",
        "A_EAC3" => codec == "eac3",
        "A_AC3" => codec == "ac3",
        "A_DTS" => codec == "dts",
        "A_FLAC" => codec == "flac",
        "A_ALAC" => codec == "alac",
        "A_OPUS" => codec == "opus",
        "A_VORBIS" => codec == "vorbis",
        "A_MPEG/L3" => codec == "mp3",
        "A_MPEG/L2" => codec == "mp2",
        "A_MPEG/L1" => codec == "mp1",
        v if v.starts_with("A_AAC") => codec == "aac",
        v if v.starts_with("A_PCM/") => codec.starts_with("pcm_"),
        _ => false,
    }
}

fn deepen_layout(
    exe: &Path,
    path: &Path,
    index: u64,
    cancel: &AtomicBool,
) -> Result<Option<String>> {
    let bytes = process::run_cancellable(
        exe,
        &[
            "-v".into(),
            "error".into(),
            "-select_streams".into(),
            index.to_string().into(),
            "-read_intervals".into(),
            "%+2".into(),
            "-show_frames".into(),
            "-show_entries".into(),
            "frame=channels,channel_layout".into(),
            "-of".into(),
            "json".into(),
            path.as_os_str().into(),
        ],
        false,
        PROBE_TIMEOUT,
        Some(cancel),
    )?
    .checked()?;
    let data: Value = serde_json::from_slice(&bytes)?;
    let mut candidates = HashSet::new();
    let mut largest = 0;
    for frame in data["frames"].as_array().into_iter().flatten() {
        if let Some(value) = frame["channel_layout"].as_str().and_then(layout) {
            let count = value.split_whitespace().count() as u64;
            if frame["channels"].as_u64() != Some(count) || count < largest {
                continue;
            }
            if count > largest {
                candidates.clear();
                largest = count;
            }
            candidates.insert(value);
        }
    }
    Ok((candidates.len() == 1).then(|| candidates.into_iter().next().unwrap()))
}

/// Inspect only audio packets. Native gap checks run only for unconfirmed TrueHD.
pub(super) fn inspect(exe: &Path, document: &Inspection, cancel: &AtomicBool) -> Result<Value> {
    if !document.tracks.iter().any(|t| t.kind == "audio") {
        return Ok(
            json!({"media":{"track":[]}, "tracksmith_analysis":{"scope":"audio", "provider_name":"container"}}),
        );
    }
    let path = Path::new(&document.path);
    let bytes = process::run_cancellable(exe, &[
        "-v".into(), "error".into(), "-select_streams".into(), "a".into(),
        "-read_intervals".into(), "%+0.5".into(), "-show_streams".into(),
        "-show_packets".into(), "-show_entries".into(),
        "stream=index,codec_name,profile,channels,channel_layout,sample_rate,bits_per_sample,bits_per_raw_sample,bit_rate:packet=stream_index,pos".into(),
        "-of".into(), "json".into(), path.as_os_str().into(),
    ], false, PROBE_TIMEOUT, Some(cancel))?.checked()?;
    let raw: Value = serde_json::from_slice(&bytes).context("Malformed FFprobe output")?;
    let streams = raw["streams"]
        .as_array()
        .context("FFprobe returned no streams")?;
    ensure!(streams.len() <= 1024, "Too many audio streams");
    let gap_streams: HashSet<_> = streams
        .iter()
        .filter(|s| {
            s["codec_name"] == "truehd" && !s["profile"].as_str().unwrap_or("").contains("Atmos")
        })
        .filter_map(|s| s["index"].as_u64())
        .collect();
    let mut targets = BTreeMap::new();
    for packet in raw["packets"].as_array().into_iter().flatten() {
        if let (Some(index), Some(position)) = (
            packet["stream_index"].as_u64(),
            text(&packet["pos"]).and_then(|v| v.parse::<u64>().ok()),
        ) && let Some(previous) = targets.insert(position, index)
        {
            ensure!(
                previous == index,
                "Audio streams share an ambiguous packet position"
            );
        }
    }
    let indexed = if targets.is_empty() {
        BTreeMap::new()
    } else {
        blocks(path, &targets, &gap_streams, cancel)?
    };
    let mut bindings: HashMap<u64, HashSet<u64>> = HashMap::new();
    for (position, block) in &indexed {
        bindings
            .entry(targets[position])
            .or_default()
            .insert(block.number);
    }
    let mut tracks = vec![];
    let mut warnings = vec![];
    for stream in streams {
        let index = stream["index"]
            .as_u64()
            .context("Invalid audio stream index")?;
        let Some(numbers) = bindings.get(&index).filter(|v| v.len() == 1) else {
            warnings.push(format!(
                "Audio stream {index}: track identity unconfirmed; technical naming preserved."
            ));
            continue;
        };
        let number = *numbers.iter().next().unwrap();
        ensure!(
            bindings.iter().filter(|(_, n)| n.contains(&number)).count() == 1,
            "Multiple FFprobe streams map to one track"
        );
        let Some(track) = document
            .tracks
            .iter()
            .find(|t| t.kind == "audio" && t.number == number.to_string())
        else {
            bail!("FFprobe packet points to an unknown audio track");
        };
        let codec = stream["codec_name"].as_str().unwrap_or("");
        if !codec_matches(codec, track.technical["codec_id"].as_str().unwrap_or("")) {
            warnings.push(format!(
                "Track {}: codec agreement is unconfirmed; technical naming preserved.",
                track.uid
            ));
            continue;
        }
        let profile = stream["profile"].as_str().unwrap_or("");
        let native: Vec<_> = indexed
            .iter()
            .filter(|(position, _)| targets[*position] == index)
            .map(|(_, block)| &block.truehd)
            .collect();
        let native_layout = native
            .iter()
            .map(|n| n.layout.as_str())
            .max_by_key(|v| v.split_whitespace().count())
            .unwrap_or("");
        let mut channel_layout = if native_layout.is_empty() {
            layout(stream["channel_layout"].as_str().unwrap_or(""))
        } else {
            Some(native_layout.into())
        };
        let expected_channels = track.technical["audio_channels"].as_u64();
        let mut used_frames = false;
        if channel_layout.is_none()
            || (codec == "eac3"
                && expected_channels
                    .zip(stream["channels"].as_u64())
                    .is_some_and(|(a, b)| a > b))
        {
            match deepen_layout(exe, path, index, cancel) {
                Ok(Some(value)) => {
                    channel_layout = Some(value);
                    used_frames = true;
                }
                Ok(None) => {}
                Err(error) => warnings.push(format!(
                    "Track {}: deeper audio inspection unavailable: {error:#}",
                    track.uid
                )),
            }
        }
        let channels = channel_layout
            .as_ref()
            .map(|v| v.split_whitespace().count() as u64)
            .or_else(|| stream["channels"].as_u64());
        let atmos = profile.contains("Atmos")
            || native.iter().any(|n| n.atmos)
            || track.codec.contains("Atmos");
        let mut features = String::new();
        if codec == "dts" && profile.contains("DTS-HD MA") {
            features.push_str("XLL");
        }
        if codec == "dts" && profile.contains("DTS:X") {
            features.push_str(" X");
        }
        if codec == "eac3" && atmos {
            features.push_str("JOC");
        }
        let format = match codec {
            "truehd" => "MLP FBA",
            "eac3" => "E-AC-3",
            "ac3" => "AC-3",
            "dts" => "DTS",
            "aac" => "AAC",
            "flac" => "FLAC",
            "alac" => "ALAC",
            "opus" => "Opus",
            "vorbis" => "Vorbis",
            v if v.starts_with("pcm_") => "PCM",
            _ => codec,
        };
        let mut facts = json!({"@type":"Audio", "ID":track.number, "UniqueID":track.uid, "Format":format,
            "Format_Profile":profile, "Format_AdditionalFeatures":features.trim(),
            "Tracksmith_LayoutProvider":if used_frames { "ffprobe_frames" } else if !native_layout.is_empty() { "tracksmith_truehd" } else { "ffprobe_stream" },
            "Tracksmith_Provider":"ffprobe", "Tracksmith_Coverage":"initial audio packets; TrueHD gap headers checked when needed"});
        let object = facts.as_object_mut().unwrap();
        if atmos {
            object.insert(
                "Format_Commercial_IfAny".into(),
                json!(if codec == "truehd" {
                    "Dolby TrueHD with Dolby Atmos"
                } else {
                    "Dolby Digital Plus with Dolby Atmos"
                }),
            );
        }
        if let Some(channels) = channels {
            object.insert("Channels".into(), json!(channels.to_string()));
        }
        if let Some(value) = &channel_layout {
            object.insert("ChannelLayout".into(), json!(value));
        }
        for (target, source) in [("SamplingRate", "sample_rate"), ("BitRate", "bit_rate")] {
            if let Some(value) = text(&stream[source]) {
                object.insert(target.into(), json!(value));
            }
        }
        let bit_depth = ["bits_per_raw_sample", "bits_per_sample"]
            .iter()
            .filter_map(|key| text(&stream[key]))
            .find(|v| v.parse::<u32>().is_ok_and(|n| n > 0))
            .or_else(|| text(&track.technical["audio_bits_per_sample"]))
            .filter(|v| v.parse::<u32>().is_ok_and(|n| n > 0));
        if let Some(value) = bit_depth {
            object.insert("BitDepth".into(), json!(value));
        }
        let lossless = matches!(codec, "truehd" | "flac" | "alac")
            || codec.starts_with("pcm_")
            || profile.contains("DTS-HD MA");
        let lossy = matches!(
            codec,
            "aac" | "ac3" | "eac3" | "opus" | "vorbis" | "mp1" | "mp2" | "mp3"
        ) || (codec == "dts"
            && matches!(
                profile,
                "DTS" | "DTS-ES" | "DTS 96/24" | "DTS-HD HRA" | "DTS Express"
            ));
        if lossless || lossy {
            object.insert(
                "Compression_Mode".into(),
                json!(if lossless { "Lossless" } else { "Lossy" }),
            );
        }
        let native_missing = gap_streams.contains(&index)
            && !track.codec.contains("Atmos")
            && native.iter().all(|n| n.headers == 0);
        let inconsistent_layout = !used_frames
            && native_layout.is_empty()
            && stream["channels"]
                .as_u64()
                .zip(channels)
                .is_some_and(|(a, b)| a != b);
        let conflicting_channels = codec == "eac3"
            && track.technical["audio_channels"]
                .as_u64()
                .zip(channels)
                .is_some_and(|(a, b)| a > b);
        if channel_layout.is_none() || native_missing || conflicting_channels || inconsistent_layout
        {
            object.insert("Tracksmith_Incomplete".into(), json!(true));
            warnings.push(format!("Track {}: audio layout or extension evidence is incomplete; technical naming preserved. Retry full analysis for this file.", track.uid));
        }
        tracks.push(facts);
    }
    Ok(
        json!({"media":{"track":tracks}, "tracksmith_analysis":{"scope":"audio", "warning":if warnings.is_empty(){Value::Null}else{json!(warnings.join(" "))}, "provider_name":"ffprobe"}}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(value: &str) -> Vec<u8> {
        value
            .as_bytes()
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
            .collect()
    }

    #[test]
    fn truehd_spatial_headers_require_valid_checksum_and_complete_framing() {
        let mut header = hex("f8726fba0017804fb75210000000884243fc000042eee306e3011dc6bc00b044");
        let (atmos, layout) = truehd_header(&header).unwrap();
        assert!(atmos);
        assert_eq!(layout.split_whitespace().count(), 8);
        let mut packet = vec![0, 18, 0, 0];
        packet.extend_from_slice(&header);
        let mut block = vec![0x81, 0, 0, 0];
        block.extend_from_slice(&packet);
        let facts = inspect_truehd(&block).unwrap();
        assert!(facts.atmos && facts.headers == 1);
        header[17] ^= 1;
        assert!(truehd_header(&header).is_none());
        assert!(truehd_header(&header[..20]).is_none());
        block.pop();
        assert_eq!(inspect_truehd(&block).unwrap().headers, 0);
    }

    #[test]
    fn lacing_validates_sizes_and_signed_deltas() {
        for (flags, lace) in [(2, vec![1, 2]), (4, vec![1]), (6, vec![1, 0x82])] {
            let mut block = vec![0x81, 0, 0, flags];
            block.extend(lace);
            block.extend([1, 2, 3, 4]);
            assert_eq!(frames(&block).unwrap(), vec![&[1, 2][..], &[3, 4][..]]);
        }
        assert!(frames(&[0x81, 0, 0, 6, 2, 0x81, 0x80, 1, 2]).is_err());
        assert!(frames(&[0x81, 0, 0, 4, 1, 1]).is_err());
        assert!(frames(&[0x81]).is_err());
        assert_eq!(vint(&[0x40, 0x80], false).unwrap().0, 128);
        assert_eq!(vint(&[1, 0, 0, 0, 0, 0, 0, 1], false).unwrap().0, 1);
    }

    #[test]
    fn actual_element_boundaries_bind_duplicate_tracks_and_reject_fake_offsets() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("fixture.mka");
        // Unknown-sized Segment and Cluster, then a known-sized next Cluster.
        let mut bytes = hex("18538067ff1f43b675ffa38a8100000081a384810000");
        let first = 12;
        let fake = 19;
        let second_cluster = bytes.len();
        bytes.extend(hex("1f43b67589a38740800000010203"));
        let second = second_cluster as u64 + 7;
        std::fs::write(&path, bytes).unwrap();
        let targets = BTreeMap::from([(first, 0), (fake, 0), (second, 1)]);
        let indexed = blocks(&path, &targets, &HashSet::new(), &AtomicBool::new(false)).unwrap();
        assert_eq!(indexed[&first].number, 1);
        assert_eq!(indexed[&second].number, 128);
        assert!(!indexed.contains_key(&fake));
    }

    #[test]
    fn height_layouts_and_codec_identity_are_explicit() {
        assert_eq!(layout("5.1.2").unwrap().split_whitespace().count(), 8);
        assert!(layout("8 channels").is_none());
        assert!(codec_matches("eac3", "A_EAC3"));
        assert!(!codec_matches("ac3", "A_EAC3"));
    }
}
