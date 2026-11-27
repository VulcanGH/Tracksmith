"""Fixture regressions for selective FFprobe audio facts and native gap checks.

Requires Windows, FFmpeg, MKVToolNix and GCC. Optional external sample paths
are read-only inputs; ordinary fixtures and all metadata writes are temporary.
"""

import argparse
import json
from pathlib import Path
import shutil
import subprocess
import tempfile
import time

from metadata_check import Service


def main() -> None:
    """Check identity, naming, caching and that audio avoids MediaInfo."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable', type=Path)
    parser.add_argument('--late-sample', type=Path)
    parser.add_argument('--dtsx-sample', type=Path)
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix='tracksmith-ffprobe-') as tmp:
        root = Path(tmp)
        source = root / 'duplicate.mka'
        subprocess.run([
            shutil.which('ffmpeg'), '-v', 'error', '-f', 'lavfi', '-i',
            'sine=duration=1', '-map', '0:a', '-map', '0:a', '-c:a', 'flac',
            '-metadata:s:a:0', 'title=Identical title',
            '-metadata:s:a:1', 'title=Identical title', str(source),
        ], check=True, capture_output=True, timeout=20)
        dll = root / 'mediainfo-test.dll'
        fixture = Path(__file__).resolve().parent.parent / 'tests/fixtures'
        subprocess.run([
            shutil.which('gcc'), '-shared', '-O2', '-o', str(dll),
            str(fixture / 'mediainfo_stub.c'),
        ], check=True, capture_output=True, timeout=30)
        log = root / 'calls.txt'
        environment = {
            'TRACKSMITH_TEST_MI_LOG': str(log),
            'TRACKSMITH_TEST_MI_DELAY': '6000',
            'TRACKSMITH_TEST_MI_JSON': '{"media":{"track":[]}}',
        }

        def configuration(name: str, ffprobe: str) -> Path:
            config = root / name
            config.mkdir()
            (config / 'config.json').write_text(json.dumps({
                'version': 1, 'port': 7476,
                'media_roots': [str(root)],
                'tools': {'mediainfo_dll': str(dll), 'ffprobe': ffprobe},
            }), encoding='utf-8')
            return config

        config = configuration('config', shutil.which('ffprobe'))
        with Service(args.executable, config, environment) as service:
            before = source.read_bytes()
            doc = service.ready(service.inspect(source))
            assert doc['analysis']['status'] == 'ready', doc
            assert doc['analysis']['scope'] == 'audio', doc
            assert len(doc['tracks']) == 2
            assert len({t['uid'] for t in doc['tracks']}) == 2
            assert all(t['mapping'] == 'uid_number_type_verified'
                       for t in doc['tracks']), doc
            preview = service.rules(doc, [])
            audio = [p for p in preview['proposals']
                     if p['edit']['track_uid'] is not None]
            assert len(audio) == 2, preview
            assert all('FLAC 1.0' in p['edit']['value'] for p in audio)
            assert source.read_bytes() == before
            assert service.inspect(source)['analysis']['cached']
            plan = service.call('plans', {
                'inspection_id': doc['id'], 'edits': [{
                    'track_uid': doc['tracks'][0]['uid'],
                    'property': 'title', 'value': 'Manual probe title',
                }, {'track_uid': None, 'property': 'title',
                    'value': source.stem}],
            })
            saved = service.call('save', {'plan_id': plan['id']})
            assert saved['success'], saved
            assert saved['inspection']['tracks'][0]['title'] == (
                'Manual probe title')
            assert saved['inspection']['analysis']['cached']
            assert not log.exists(), 'Routine audio invoked MediaInfo'

            for sample, expected in [(args.late_sample, 'Atmos'),
                                     (args.dtsx_sample, 'DTS:X')]:
                if sample is None:
                    continue
                copied = root / f'{expected.replace(":", "")}.mka'
                shutil.copy2(sample, copied)
                original = copied.read_bytes()
                inspected = service.ready(service.inspect(copied))
                assert inspected['analysis']['status'] == 'ready', inspected
                facts = inspected['tracks'][0]['mediainfo']
                proposals = service.rules(inspected, [])['proposals']
                assert any(expected in str(p['edit']['value'])
                           for p in proposals), (expected, proposals, facts)
                if expected == 'Atmos':
                    assert facts['Channels'] == '8', facts
                    assert facts['Tracksmith_LayoutProvider'] == (
                        'tracksmith_truehd'), facts
                assert copied.read_bytes() == original
                assert not log.exists(), 'Gap check invoked MediaInfo'

        with Service(args.executable, config, environment) as service:
            assert service.inspect(source)['analysis']['cached']
            assert not log.exists()

        delayed = root / 'delayed.mka'
        subprocess.run([
            shutil.which('ffmpeg'), '-v', 'error', '-i', str(source),
            '-itsoffset', '2', '-i', str(source), '-map', '0:a:0', '-map',
            '1:a:0', '-c:a', 'copy', str(delayed),
        ], check=True, capture_output=True, timeout=20)
        with Service(args.executable, config, environment) as service:
            doc = service.ready(service.inspect(delayed))
            assert doc['analysis']['status'] == 'ready', doc
            assert all(t['mapping'] == 'uid_number_type_verified'
                       for t in doc['tracks']), doc
            assert not log.exists(), 'Delayed track invoked MediaInfo'

        # A broken prober must never be called by a container-only profile.
        light = configuration('light', str(args.executable.resolve()))
        quick_worker = {**environment, 'TRACKSMITH_TEST_MI_DELAY': '20'}
        with Service(args.executable, light, quick_worker) as service:
            library = service.call('rules')
            profile = next(p for p in library['profiles']
                           if p['id'] == library['active_profile_id'])
            profile['rule_ids'] = ['standard-container-title']
            service.call('rules', library)
            doc = service.ready(service.inspect(source))
            assert doc['analysis']['status'] == 'not_requested', doc
            assert not log.exists()
            video = root / 'video.mkv'
            subprocess.run([
                shutil.which('ffmpeg'), '-v', 'error', '-f', 'lavfi', '-i',
                'color=size=16x16:duration=1', '-c:v', 'ffv1', str(video),
            ], check=True, capture_output=True, timeout=20)
            opened = service.ready(service.inspect(video))
            assert opened['analysis']['status'] == 'not_requested', opened
            library = service.call('rules')
            rule = next(r for r in library['rules']
                        if r['id'] == 'standard-movie-title')
            rule['actions'][0].update(source='template',
                                      value='{hdr_format}')
            profile = next(p for p in library['profiles']
                           if p['id'] == library['active_profile_id'])
            profile['rule_ids'].append(rule['id'])
            service.call('rules', library)
            # A video rule on audio-only media still needs no probe.
            untouched = service.ready(service.inspect(source))
            assert untouched['analysis']['status'] == 'not_requested'
            assert not log.exists()
            # Existing open media requests newly required facts on enrichment.
            deeper = service.call('enrichment', {'id': opened['id']})
            deeper = service.ready(deeper)
            assert deeper['analysis']['scope'] == 'full', deeper
            assert len(log.read_text().splitlines()) == 1
            service.call('enrichment', {'id': opened['id']})
            assert len(log.read_text().splitlines()) == 1
        log.unlink()

        # A missing stream layout requests decoded frames only where needed.
        packet_output = subprocess.check_output([
            shutil.which('ffprobe'), '-v', 'error', '-show_packets',
            '-read_intervals', '%+0.02', '-show_entries',
            'packet=stream_index,pos', '-of', 'json', str(source),
        ], timeout=10)
        packets = json.loads(packet_output)['packets']
        frame_probe = root / 'layout.exe'
        frame_source = root / 'layout.c'
        initial = json.dumps({'streams': [{
            'index': index, 'codec_name': 'flac', 'channels': 1,
            'sample_rate': '44100', 'bits_per_raw_sample': '16',
            **({'channel_layout': 'mono'} if index else {}),
        } for index in range(2)], 'packets': packets})
        decoded = json.dumps({'frames': [{
            'channels': 1, 'channel_layout': 'mono',
        }]})
        frame_source.write_text(
            '#include <stdio.h>\n#include <string.h>\n'
            'int main(int n, char **v) { int deep=0; '
            'for(int i=1;i<n;i++) if(!strcmp(v[i],"-show_frames")) deep=1; '
            'puts(deep ? ' + json.dumps(decoded) + ' : '
            + json.dumps(initial) + '); return 0; }\n', encoding='utf-8')
        subprocess.run([
            shutil.which('gcc'), '-O2', '-o', str(frame_probe),
            str(frame_source),
        ], check=True, capture_output=True, timeout=30)
        layout_config = configuration('layout', str(frame_probe))
        with Service(args.executable, layout_config, environment) as service:
            doc = service.ready(service.inspect(source))
            assert doc['analysis']['status'] == 'ready', doc
            assert doc['tracks'][0]['mediainfo'][
                'Tracksmith_LayoutProvider'] == 'ffprobe_frames', doc
            assert doc['tracks'][1]['mediainfo'][
                'Tracksmith_LayoutProvider'] == 'ffprobe_stream', doc
            assert not log.exists()

        # Valid stream facts with no validated packet positions stay unknown.
        fake_source = root / 'unmapped.c'
        fake = root / 'unmapped.exe'
        payload = json.dumps({'streams': [{
            'index': 0, 'codec_name': 'flac', 'channels': 1,
            'channel_layout': 'mono', 'sample_rate': '44100',
        }], 'packets': []})
        unlocked = root / 'unlock-probe'
        mapped = json.loads(initial)
        for stream in mapped['streams']:
            stream['channel_layout'] = 'mono'
        fake_source.write_text(
            '#include <stdio.h>\nint main(void) { FILE *f=fopen('
            + json.dumps(str(unlocked)) + ',"rb"); if(f) { fclose(f); puts('
            + json.dumps(json.dumps(mapped)) + '); } else puts('
            + json.dumps(payload) + '); return 0; }\n', encoding='utf-8')
        subprocess.run([
            shutil.which('gcc'), '-O2', '-o', str(fake), str(fake_source),
        ], check=True, capture_output=True, timeout=30)
        unmapped = configuration('unmapped', str(fake))
        with Service(args.executable, unmapped, environment) as service:
            before = source.read_bytes()
            doc = service.ready(service.inspect(source))
            assert doc['analysis']['status'] == 'partial', doc
            assert not any(p['edit']['track_uid'] is not None
                           for p in service.rules(doc, [])['proposals'])
            assert source.read_bytes() == before
            assert not log.exists(), 'Unmapped audio fell back to MediaInfo'
            broken = root / 'broken.mkv'
            broken.write_bytes(b'not a container')
            service.call('batch/preview', {
                'sources': [str(source), str(broken)], 'recursive': False,
                'filter': '', 'profile_id': 'general',
            })
            deadline = time.monotonic() + 10
            view = service.call('batch')
            while view['phase'] == 'planning':
                assert time.monotonic() < deadline, view
                time.sleep(0.05)
                view = service.call('batch')
            warned = next(r for r in view['rows']
                          if r['path'].endswith('duplicate.mka'))
            other = next(r for r in view['rows'] if r['id'] != warned['id'])
            assert warned['status'] == 'needs_review', warned
            try:
                service.call('batch/recheck', {
                    'job_id': view['id'], 'row_id': 'missing',
                })
            except AssertionError as error:
                assert 'no longer available' in str(error), error
            else:
                raise AssertionError('Unknown batch row allowed recheck')
            # Retain the reviewed profile even if the active profile changes.
            library = service.call('rules')
            library['profiles'][0]['rule_ids'] = []
            service.call('rules', library)
            unlocked.touch()
            service.call('batch/recheck', {
                'job_id': view['id'], 'row_id': warned['id'],
            })
            refreshed = service.call('batch')
            while refreshed['phase'] == 'planning':
                assert time.monotonic() < deadline, refreshed
                time.sleep(0.05)
                refreshed = service.call('batch')
            assert refreshed['id'] == view['id'], refreshed
            fixed = next(r for r in refreshed['rows']
                         if r['id'] == warned['id'])
            assert fixed['status'] == 'ready' and not fixed['warnings'], fixed
            assert next(r for r in refreshed['rows']
                        if r['id'] == other['id']) == other
            assert source.read_bytes() == before
            assert not log.exists()
        samples = sum(p is not None for p in (args.late_sample,
                                             args.dtsx_sample))
        print('PASS: duplicate identity, audio naming, cache restart, manual '
              'save, selective probing, unmapped refusal and no MediaInfo '
              'calls on routine audio; optional sample checks: '
              f'{samples}')


if __name__ == '__main__':
    main()
