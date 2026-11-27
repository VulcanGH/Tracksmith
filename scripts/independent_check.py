"""Check independent batch verification using disposable audio and a test DLL.

Requires Windows, FFmpeg, MKVToolNix and GCC on PATH.
"""

import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import tempfile
import time

from metadata_check import Service


def settled(service: Service) -> dict:
    """Wait for the read-only batch worker to complete."""
    deadline = time.monotonic() + 25
    while time.monotonic() < deadline:
        view = service.call('batch')
        if view['phase'] != 'planning':
            assert view['phase'] == 'review', view
            return view
        time.sleep(0.05)
    raise AssertionError('Batch verification did not finish')


def main() -> None:
    """Verify warning resolution, failed confirmation and source preservation."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable', type=Path)
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix='tracksmith-independent-') as tmp:
        root = Path(tmp)
        source = root / 'audio.mka'
        subprocess.run([
            shutil.which('ffmpeg'), '-v', 'error', '-f', 'lavfi', '-i',
            'sine=duration=1', '-c:a', 'eac3', '-metadata:s:a:0',
            'title=Korean Dolby Digital Plus 1.0 + Atmos',
            '-metadata:s:a:0', 'language=kor', str(source),
        ], check=True, capture_output=True, timeout=20)
        original = hashlib.sha256(source.read_bytes()).digest()
        identified = json.loads(subprocess.check_output([
            shutil.which('mkvmerge'), '-J', str(source),
        ], timeout=20))
        track = identified['tracks'][0]['properties']
        dll = root / 'mediainfo-test.dll'
        fixture = Path(__file__).resolve().parent.parent / 'tests/fixtures'
        subprocess.run([
            shutil.which('gcc'), '-shared', '-O2', '-o', str(dll),
            str(fixture / 'mediainfo_stub.c'),
        ], check=True, capture_output=True, timeout=30)

        for scenario in ('confirmed', 'conflicting', 'unavailable'):
            config = root / scenario
            config.mkdir()
            (config / 'config.json').write_text(json.dumps({
                'version': 1, 'port': 7476, 'media_roots': [str(root)],
                'tools': {'mediainfo_dll': str(dll),
                          'ffprobe': shutil.which('ffprobe')},
            }), encoding='utf-8')
            log = config / 'calls.txt'
            facts = {'media': {'track': [{
                '@type': 'Audio', 'ID': str(track['number']),
                'UniqueID': str(track['uid']), 'Format': 'E-AC-3',
                'Channels': '1', 'ChannelLayout': 'C',
                'Format_AdditionalFeatures': (
                    'JOC' if scenario == 'confirmed' else ''),
            }] if scenario != 'unavailable' else []}}
            environment = {
                'TRACKSMITH_TEST_MI_JSON': json.dumps(facts),
                'TRACKSMITH_TEST_MI_LOG': str(log),
                'TRACKSMITH_TEST_MI_DELAY': '0',
            }
            with Service(args.executable, config, environment) as service:
                service.call('batch/preview', {
                    'sources': [str(source)], 'recursive': False,
                    'filter': '', 'profile_id': 'general',
                })
                preview = settled(service)
                row = preview['rows'][0]
                assert any('immersive' in w for w in row['warnings']), row
                assert not log.exists(), 'Routine scan invoked MediaInfo'
                request = {'job_id': preview['id'], 'row_id': row['id']}
                # The existing retry keeps its short-probe behaviour.
                service.call('batch/recheck', request)
                normal = settled(service)['rows'][0]
                assert any('immersive' in w for w in normal['warnings'])
                assert not log.exists()
                for attempt in range(2):
                    service.call('batch/recheck', {
                        **request, 'independent': True,
                    })
                    checked = settled(service)['rows'][0]
                    assert checked['id'] == row['id']
                    if scenario == 'confirmed':
                        assert not checked['warnings'], checked
                        assert checked['status'] != 'error', checked
                    else:
                        assert checked['warnings'], checked
                    assert len(log.read_text().splitlines()) == attempt + 1
                    assert hashlib.sha256(source.read_bytes()).digest() == (
                        original)
                # An unreadable file must remain an error after verification.
                broken = root / f'{scenario}-broken.mkv'
                broken.write_bytes(b'not a matroska file')
                service.call('batch/preview', {
                    'sources': [str(broken)], 'recursive': False,
                    'filter': '', 'profile_id': 'general',
                })
                failed = settled(service)
                assert failed['rows'][0]['status'] == 'error', failed
                service.call('batch/recheck', {
                    'job_id': failed['id'],
                    'row_id': failed['rows'][0]['id'], 'independent': True,
                })
                retried = settled(service)['rows'][0]
                assert retried['status'] == 'error' and retried['warnings']
                assert broken.read_bytes() == b'not a matroska file'
        print('PASS: independent confirmation clears resolved warnings; '
              'conflicts, missing facts and errors remain; repeated checks '
              'bypass cache; media bytes unchanged')


if __name__ == '__main__':
    main()
