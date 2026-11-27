"""Read-only Windows metadata benchmarks; never edit source media.

DLL ParseSpeed options are equivalent to MediaInfo CLI --ParseSpeed values.
Each native benchmark runs in an isolated process with a deadline. Samples
are disposable stream copies, not transcodes. No new Python packages needed.
"""

import argparse
import ctypes
import json
from pathlib import Path
import shutil
import statistics
import subprocess
import sys
import tempfile
import time


def counters(handle: int) -> dict:
    """Return CPU seconds and process I/O, including cached reads."""
    from ctypes import wintypes

    class Io(ctypes.Structure):
        _fields_ = [(name, ctypes.c_ulonglong) for name in (
            'read_ops', 'write_ops', 'other_ops', 'read_bytes',
            'write_bytes', 'other_bytes')]

    kernel = ctypes.WinDLL('kernel32', use_last_error=True)
    kernel.GetProcessIoCounters.argtypes = [wintypes.HANDLE,
                                           ctypes.POINTER(Io)]
    kernel.GetProcessTimes.argtypes = [wintypes.HANDLE] + [
        ctypes.POINTER(ctypes.c_ulonglong)] * 4
    values = Io()
    times = [ctypes.c_ulonglong() for _ in range(4)]
    if not kernel.GetProcessIoCounters(handle, ctypes.byref(values)):
        raise ctypes.WinError(ctypes.get_last_error())
    if not kernel.GetProcessTimes(handle, *[ctypes.byref(t) for t in times]):
        raise ctypes.WinError(ctypes.get_last_error())
    return {'cpu_s': (times[2].value + times[3].value) / 10**7,
            'read_bytes': values.read_bytes, 'read_ops': values.read_ops,
            'write_bytes': values.write_bytes}


def launch(command: list[str | Path], timeout: float) -> dict:
    """Measure an isolated command, terminating it at its deadline."""
    started = time.perf_counter()
    process = subprocess.Popen(
        list(map(str, command)), stdout=subprocess.PIPE,
        stderr=subprocess.PIPE, creationflags=0x08000000,
    )
    expired = False
    try:
        stdout, stderr = process.communicate(timeout=timeout)
    except subprocess.TimeoutExpired:
        expired = True
        process.kill()
        stdout, stderr = process.communicate()
    row = {'wall_s': time.perf_counter() - started,
           'timeout': expired, 'exit': process.returncode,
           **counters(process._handle)}
    row['output_bytes'] = len(stdout)
    if stderr:
        row['stderr'] = stderr.decode('utf-8', errors='replace')[:3000]
    if not expired and process.returncode == 0:
        started = time.perf_counter()
        try:
            row['data'] = json.loads(stdout)
        except (ValueError, UnicodeError):
            pass
        row['json_s'] = time.perf_counter() - started
    return row


def facts(data: dict) -> list[dict]:
    """Retain evidence fields without titles or positional codec inference."""
    mi = data.get('media', {}).get('track')
    if mi is not None:
        keys = ('@type', 'ID', 'UniqueID', 'Format', 'Format_Profile',
                'Format_AdditionalFeatures', 'Format_Commercial_IfAny',
                'Channels', 'ChannelLayout', 'BitDepth', 'SamplingRate',
                'Channels_Original', 'ChannelLayout_Original',
                'HDR_Format', 'HDR_Format_Profile', 'HDR_Format_Compatibility',
                'NumberOfDynamicObjects', 'BedChannelCount')
        return [{k: t[k] for k in keys if k in t} for t in mi
                if t.get('@type') in ('Audio', 'Video')]
    if 'streams' in data:
        keys = ('index', 'id', 'codec_type', 'codec_name', 'profile',
                'channels', 'channel_layout', 'bits_per_raw_sample',
                'bits_per_sample', 'sample_rate', 'color_transfer',
                'color_primaries', 'side_data_list')
        return [{k: t[k] for k in keys if k in t}
                for t in data['streams']
                if t.get('codec_type') in ('audio', 'video')]
    return [{'id': t['id'], 'type': t['type'], 'codec': t['codec'],
             'properties': {k: v for k, v in t['properties'].items()
                            if k != 'track_name'}}
            for t in data.get('tracks', [])
            if t['type'] in ('audio', 'video')]


def native_worker(args: argparse.Namespace) -> None:
    """Measure DLL loading, parsing and serialisation separately."""
    started = time.perf_counter()
    lib = ctypes.WinDLL(str(args.dll))
    lib.MediaInfo_New.restype = ctypes.c_void_p
    lib.MediaInfo_Delete.argtypes = [ctypes.c_void_p]
    lib.MediaInfo_Option.argtypes = [ctypes.c_void_p, ctypes.c_wchar_p,
                                   ctypes.c_wchar_p]
    lib.MediaInfo_Option.restype = ctypes.c_wchar_p
    lib.MediaInfo_Open.argtypes = [ctypes.c_void_p, ctypes.c_wchar_p]
    lib.MediaInfo_Open.restype = ctypes.c_size_t
    lib.MediaInfo_Inform.argtypes = [ctypes.c_void_p, ctypes.c_size_t]
    lib.MediaInfo_Inform.restype = ctypes.c_wchar_p
    loaded = time.perf_counter() - started
    rows = []
    for _ in range(args.repeats):
        handle = lib.MediaInfo_New()
        if not handle:
            raise RuntimeError('MediaInfo allocation failed')
        try:
            lib.MediaInfo_Option(handle, 'Output', 'JSON')
            lib.MediaInfo_Option(handle, 'Complete', '1')
            default = lib.MediaInfo_Option(handle, 'ParseSpeed_Get', '')
            if args.speed != 'default':
                lib.MediaInfo_Option(handle, 'ParseSpeed', args.speed)
            started = time.perf_counter()
            if not lib.MediaInfo_Open(handle, str(args.file[0])):
                raise RuntimeError('MediaInfo open failed')
            opened = time.perf_counter() - started
            started = time.perf_counter()
            output = lib.MediaInfo_Inform(handle, 0)
            informed = time.perf_counter() - started
            started = time.perf_counter()
            data = json.loads(output)
            parsed = time.perf_counter() - started
            rows.append({'open_s': opened, 'inform_s': informed,
                         'json_s': parsed, 'default_speed': default,
                         'facts': facts(data)})
        finally:
            lib.MediaInfo_Delete(handle)
        if opened > 3:
            break
    print(json.dumps({'load_s': loaded, 'runs': rows}))


def main() -> None:
    """Benchmark selected inputs and save timing and evidence records."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--file', type=Path, action='append', required=True)
    parser.add_argument('--dll', type=Path, default=Path(
        'C:/Program Files/MediaInfo/MediaInfo.dll'))
    parser.add_argument('--mkvmerge', type=Path, default=Path(
        'C:/Program Files/MKVToolNix/mkvmerge.exe'))
    parser.add_argument('--rust-worker', type=Path, default=Path(
        'C:/Program Files/Tracksmith/tracksmith.exe'))
    parser.add_argument('--output', type=Path)
    parser.add_argument('--timeout', type=float, default=25)
    parser.add_argument('--repeats', type=int, default=3)
    parser.add_argument('--worker', action='store_true')
    parser.add_argument('--speed', default='default')
    parser.add_argument('--only', action='append',
                        help='Run only a named method (repeatable)')
    args = parser.parse_args()
    if args.worker:
        native_worker(args)
        return
    if args.output is None:
        parser.error('--output is required')
    ffprobe = shutil.which('ffprobe')
    report = {'method': 'Sequential runs, uncontrolled OS cache; no cache '
              'flush. Windows I/O counters include cached/logical reads, '
              'not physical disk reads. Timed-out results are censored.',
              'repeats': args.repeats, 'timeout_s': args.timeout,
              'files': []}
    for path in args.file:
        before = path.stat()
        entry = {'file': path.name, 'size_bytes': before.st_size, 'rows': []}
        report['files'].append(entry)

        def bench(name, command, native=False):
            if args.only and name not in args.only:
                return
            rows = []
            for _ in range(1 if native else args.repeats):
                row = launch(command, args.timeout)
                data = row.pop('data', {})
                if native:
                    row['native'] = data
                else:
                    row['facts'] = facts(data)
                rows.append(row)
                if row['timeout'] or row['wall_s'] > 3:
                    break
            result = {'name': name, 'runs': rows,
                      'median_wall_s': statistics.median(
                          r['wall_s'] for r in rows)}
            entry['rows'].append(result)
            print(f'{path.name}: {name} '
                  f'{result["median_wall_s"]:.3f}s', flush=True)
            args.output.parent.mkdir(parents=True, exist_ok=True)
            args.output.write_text(json.dumps(report, indent=2), 'utf-8')
            return result

        bench('mkvmerge', [args.mkvmerge, '-J', path])
        bench('rust_dll_current', [args.rust_worker, '--mediainfo-worker',
                                  args.dll, path])
        for speed in ('default', '0', '0.1', '0.3', '0.5'):
            bench('dll_' + speed, [sys.executable, __file__, '--worker',
                  '--dll', args.dll, '--file', path, '--speed', speed,
                  '--repeats', str(args.repeats)], native=True)
        if ffprobe:
            for name, options in (
                ('default', []),
                ('small', ['-probesize', '262144', '-analyzeduration',
                           '250000', '-max_probe_packets', '128']),
                ('medium', ['-probesize', '5000000', '-analyzeduration',
                            '5000000']),
            ):
                bench('ffprobe_' + name, [ffprobe, '-v', 'error', *options,
                      '-show_streams', '-show_format', '-of', 'json', path])
        with tempfile.TemporaryDirectory(
            prefix='tracksmith-audio-bench-'
        ) as tmp:
            sample = Path(tmp) / 'audio.mka'
            prepared = None
            if not args.only or 'sample_dll_default' in args.only:
                prepared = launch([
                    args.mkvmerge, '-o', sample, '--split',
                    'parts:00:00:00-00:00:30', '--no-video', '--no-subtitles',
                    '--no-buttons', '--no-chapters', '--no-attachments',
                    '--no-global-tags', '--no-track-tags', path,
                ], args.timeout)
            entry['sample_preparation'] = prepared
            if prepared and not prepared['timeout'] and prepared['exit'] == 0:
                entry['sample_bytes'] = sample.stat().st_size
                bench('sample_dll_default', [sys.executable, __file__,
                      '--worker', '--dll', args.dll, '--file', sample,
                      '--repeats', str(args.repeats)], native=True)
        after = path.stat()
        entry['source_unchanged'] = (
            before.st_size == after.st_size
            and before.st_mtime_ns == after.st_mtime_ns)
        assert entry['source_unchanged'], (
            'Source file changed during benchmark'
        )
        args.output.write_text(json.dumps(report, indent=2), 'utf-8')


if __name__ == '__main__':
    main()
