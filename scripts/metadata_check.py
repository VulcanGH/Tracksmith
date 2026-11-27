"""Regression checks for asynchronous metadata and cache reuse on fixtures.

Requires Windows, GCC, FFmpeg and MKVToolNix. A test-only DLL provides
deterministic delays and records analysis calls; all media is disposable.
"""

import argparse
import json
import os
from pathlib import Path
import shutil
import socket
import subprocess
import tempfile
import time
import urllib.error
import urllib.request


class Service:
    """Own an isolated local server and its authenticated test requests."""

    def __init__(self, executable: Path, config: Path, environment: dict):
        self.executable = executable.resolve()
        self.config = config
        self.environment = environment

    def __enter__(self):
        with socket.socket() as sock:
            sock.bind(('127.0.0.1', 0))
            self.port = sock.getsockname()[1]
        self.process = subprocess.Popen(
            [str(self.executable), '--background', '--no-tray', '--port',
             str(self.port)],
            env={**os.environ, **self.environment,
                 'TRACKSMITH_CONFIG_DIR': str(self.config)},
            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
            creationflags=0x08000000,
        )
        self.opener = urllib.request.build_opener(
            urllib.request.ProxyHandler({}))
        session = self.config / 'session.json'
        deadline = time.monotonic() + 15
        while time.monotonic() < deadline:
            if session.exists():
                try:
                    data = json.loads(session.read_text(encoding='utf-8'))
                except (PermissionError, json.JSONDecodeError):
                    time.sleep(0.05)
                    continue
                if data['port'] == self.port:
                    self.token = data['token']
                    return self
            if self.process.poll() is not None:
                raise RuntimeError('Test service exited during startup')
            time.sleep(0.05)
        self.process.terminate()
        self.process.wait(timeout=10)
        raise RuntimeError('Test service startup timed out')

    def __exit__(self, *_):
        try:
            self.call('desktop/stop', {})
        except (OSError, urllib.error.URLError):
            self.process.terminate()
        self.process.wait(timeout=45)

    def call(self, endpoint: str, body: dict | None = None) -> dict:
        request = urllib.request.Request(
            f'http://127.0.0.1:{self.port}/api/{endpoint}',
            data=None if body is None else json.dumps(body).encode(),
            headers={'Authorization': f'Bearer {self.token}',
                     'Content-Type': 'application/json'},
        )
        try:
            with self.opener.open(request, timeout=20) as response:
                return json.load(response)
        except urllib.error.HTTPError as error:
            raise AssertionError(error.read().decode()) from error

    def inspect(self, path: Path) -> dict:
        return self.call('inspect', {'path': str(path)})

    def ready(self, document: dict) -> dict:
        deadline = time.monotonic() + 25
        while (document['analysis']['status'] == 'pending'
               or document.get('sections_pending')):
            assert time.monotonic() < deadline, 'Enrichment did not finish'
            document = self.call('enrichment', {'id': document['id']})
            time.sleep(0.05)
        return document

    def rules(self, document: dict, manual: list) -> dict:
        return self.call('rules/preview', {
            'inspection_id': document['id'], 'profile_id': 'general',
            'manual_edits': manual, 'test_rule': None, 'test_library': None,
        })


def main() -> None:
    """Exercise delayed analysis, identity invalidation and metadata writes."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable', type=Path)
    args = parser.parse_args()
    merge = Path('C:/Program Files/MKVToolNix/mkvmerge.exe')
    with tempfile.TemporaryDirectory(prefix='tracksmith-metadata-') as tmp:
        root = Path(tmp)
        source = root / 'fixture.mka'
        subprocess.run([
            shutil.which('ffmpeg'), '-v', 'error', '-f', 'lavfi', '-i',
            'sine=frequency=440:duration=1', '-c:a', 'flac',
            '-metadata:s:a:0', 'title=Original audio', str(source),
        ], check=True, capture_output=True, timeout=20)
        raw = json.loads(subprocess.check_output(
            [str(merge), '-J', str(source)], timeout=10))
        uid = str(raw['tracks'][0]['properties']['uid'])
        number = str(raw['tracks'][0]['properties']['number'])
        dll = root / 'mediainfo-test.dll'
        fixture = Path(__file__).resolve().parent.parent / 'tests/fixtures'
        subprocess.run([
            shutil.which('gcc'), '-shared', '-O2', '-o', str(dll),
            str(fixture / 'mediainfo_stub.c'),
        ], check=True, capture_output=True, timeout=30)
        log = root / 'calls.txt'
        environment = {
            'TRACKSMITH_TEST_MI_LOG': str(log),
            'TRACKSMITH_TEST_MI_DELAY': '1200',
            'TRACKSMITH_TEST_MI_JSON': json.dumps({'media': {'track': [{
                '@type': 'Audio', 'ID': number, 'UniqueID': uid,
                'Format': 'FLAC', 'Channels': '1', 'ChannelLayout': 'M',
                'BitDepth': '16', 'SamplingRate': '44100',
                'Title': 'Must never be cached as editable metadata',
            }]}}),
        }

        def configuration(name: str) -> Path:
            config = root / name
            config.mkdir()
            (config / 'config.json').write_text(json.dumps({
                'version': 1, 'port': 7476, 'media_roots': [str(root)],
                'tools': {'mediainfo_dll': str(dll),
                          'ffprobe': str(root / 'disabled-ffprobe.exe')},
            }), encoding='utf-8')
            return config

        def calls() -> int:
            return len(log.read_text().splitlines()) if log.exists() else 0

        def await_calls(count: int) -> None:
            deadline = time.monotonic() + 5
            while calls() < count:
                assert time.monotonic() < deadline
                time.sleep(0.01)

        config = configuration('config')
        with Service(args.executable, config, environment) as service:
            started = time.perf_counter()
            doc = service.inspect(source)
            assert time.perf_counter() - started < 1.2
            assert doc['analysis']['status'] == 'pending', doc
            assert doc['tracks'][0]['title'] == 'Original audio'
            pending = service.rules(doc, [])
            assert not any(p['edit']['track_uid'] == uid
                           for p in pending['proposals']), pending
            service.call('enrichment', {'id': doc['id']})
            await_calls(1)
            manual = [{'track_uid': uid, 'property': 'title',
                       'value': 'Manual audio'}]
            started = time.perf_counter()
            service.call('plans', {
                'inspection_id': doc['id'], 'edits': manual,
            })
            assert time.perf_counter() - started < 0.5
            doc = service.ready(doc)
            assert doc['analysis']['status'] == 'ready', doc
            assert 'Title' not in doc['tracks'][0]['mediainfo']
            preview = service.rules(doc, manual)
            assert preview['preserved_manual'] == 1, preview
            assert not any(p['edit']['track_uid'] == uid
                           for p in preview['proposals']), preview
            assert calls() == 1
            assert service.inspect(source)['analysis']['cached']
            assert calls() == 1
        with Service(args.executable, config, environment) as service:
            doc = service.inspect(source)
            assert doc['analysis']['cached']
            plan = service.call('plans', {
                'inspection_id': doc['id'], 'edits': manual,
            })
            saved = service.call('save', {'plan_id': plan['id']})
            assert saved['success'], saved
            assert saved['inspection']['analysis']['cached'], saved
            doc = service.inspect(source)
            assert doc['tracks'][0]['title'] == 'Manual audio'
            assert doc['analysis']['cached'] and calls() == 1

            stamp = source.stat()
            os.utime(source, ns=(stamp.st_atime_ns,
                                stamp.st_mtime_ns + 5_000_000_000))
            doc = service.inspect(source)
            assert doc['analysis']['status'] == 'pending'
            service.call('enrichment', {'id': doc['id']})
            await_calls(2)
            stamp = source.stat()
            os.utime(source, ns=(stamp.st_atime_ns,
                                stamp.st_mtime_ns + 5_000_000_000))
            doc = service.ready(doc)
            assert doc['analysis']['status'] == 'failed', doc
            doc = service.ready(service.inspect(source))
            assert calls() == 3 and doc['analysis']['status'] == 'ready'
            stamp = dll.stat()
            os.utime(dll, ns=(stamp.st_atime_ns,
                             stamp.st_mtime_ns + 5_000_000_000))
            doc = service.ready(service.inspect(source))
            assert calls() == 4

            service.call('batch/preview', {
                'sources': [str(source)], 'recursive': False,
                'filter': '', 'profile_id': 'general',
            })
            deadline = time.monotonic() + 10
            while True:
                view = service.call('batch')
                if view['phase'] != 'planning':
                    break
                assert time.monotonic() < deadline
                time.sleep(0.05)
            assert view['phase'] == 'review', view
            selected = [row['id'] for row in view['rows']
                        if row['status'] == 'ready']
            assert selected, view
            service.call('batch/save', {
                'job_id': view['id'], 'selected': selected,
            })
            while True:
                view = service.call('batch')
                if view['phase'] != 'saving':
                    break
                assert time.monotonic() < deadline
                time.sleep(0.05)
            assert all(row['status'] == 'verified'
                       for row in view['rows']), view
            assert calls() == 4, 'Batch preview/save reanalysed bitstreams'

        bulk = configuration('bulk-config')
        copies = [root / f'bulk-{i}.mka' for i in range(3)]
        for copy in copies:
            shutil.copy2(source, copy)
        with Service(args.executable, bulk, environment) as service:
            count = calls()
            payload = {
                'sources': [str(p) for p in copies], 'recursive': False,
                'filter': '', 'profile_id': 'general',
            }
            service.call('batch/preview', payload)
            await_calls(count + 1)
            view = service.call('batch')
            assert view['phase'] == 'planning', view
            assert len(view['rows']) == view['total_files'] == 3, view
            assert not any(r['status'] == 'ready' for r in view['rows'])
            try:
                service.call('batch/save', {
                    'job_id': view['id'],
                    'selected': [view['rows'][0]['id']],
                })
            except AssertionError as error:
                assert 'Preview this batch again' in str(error), error
            else:
                raise AssertionError('Unfinished batch allowed a save')
            deadline = time.monotonic() + 15
            while view['phase'] == 'planning':
                assert time.monotonic() < deadline, view
                time.sleep(0.05)
                view = service.call('batch')
            assert view['phase'] == 'review', view
            assert all(r['status'] == 'ready' for r in view['rows']), view
            assert calls() == count + 3, 'Each uncached file analysed once'
            service.call('batch/preview', payload)
            view = service.call('batch')
            while view['phase'] == 'planning':
                assert time.monotonic() < deadline, view
                time.sleep(0.05)
                view = service.call('batch')
            assert view['phase'] == 'review', view
            assert calls() == count + 3, 'Cached batch reanalysed audio'
            for copy in copies:
                stamp = copy.stat()
                os.utime(copy, ns=(stamp.st_atime_ns,
                                  stamp.st_mtime_ns + 5_000_000_000))
            service.call('batch/preview', payload)
            await_calls(count + 4)
            service.call('batch/cancel', {})
            view = service.call('batch')
            deadline = time.monotonic() + 6
            while view['phase'] == 'planning':
                assert time.monotonic() < deadline, view
                time.sleep(0.05)
                view = service.call('batch')
            assert view['phase'] == 'cancelled', view
            assert sum(r['status'] == 'skipped' for r in view['rows']) == 2
            assert calls() == count + 4, 'Stop analysed an additional file'

        fallback = configuration('fallback-config')
        compressed = root / 'compressed.mka'
        subprocess.run([
            shutil.which('ffmpeg'), '-v', 'error', '-f', 'lavfi', '-i',
            'sine=frequency=440:duration=1', '-c:a', 'eac3', str(compressed),
        ], check=True, capture_output=True, timeout=20)
        raw = json.loads(subprocess.check_output(
            [str(merge), '-J', str(compressed)], timeout=10))
        sampled_facts = json.loads(environment['TRACKSMITH_TEST_MI_JSON'])
        audio = sampled_facts['media']['track'][0]
        audio['UniqueID'] = str(raw['tracks'][0]['properties']['uid'])
        audio['ID'] = str(raw['tracks'][0]['properties']['number'])
        audio['Format'] = 'E-AC-3'
        environment['TRACKSMITH_TEST_MI_JSON'] = json.dumps(sampled_facts)
        environment['TRACKSMITH_TEST_MI_DELAY'] = '10000'
        with Service(args.executable, fallback, environment) as service:
            original = compressed.stat()
            count = calls()
            doc = service.ready(service.inspect(compressed))
            assert doc['analysis']['status'] == 'partial', doc
            assert doc['analysis']['scope'] == 'audio_sample', doc
            assert doc['tracks'][0]['mapping'] == 'uid_number_type_verified'
            assert calls() == count + 2, 'Expected full attempt then sample'
            assert service.inspect(compressed)['analysis']['cached']
            assert calls() == count + 2
            assert compressed.stat().st_mtime_ns == original.st_mtime_ns
            doc = service.call('enrichment', {'id': doc['id'], 'retry': True})
            doc = service.ready(doc)
            assert doc['analysis']['status'] == 'ready', doc
            assert doc['analysis']['scope'] == 'full'
            assert calls() == count + 3

        cancelled = configuration('cancellation-config')
        environment['TRACKSMITH_TEST_MI_DELAY'] = '6000'
        with Service(args.executable, cancelled, environment) as service:
            count = calls()
            doc = service.inspect(compressed)
            service.call('enrichment', {'id': doc['id']})
            await_calls(count + 1)
            service.call('enrichment', {'id': doc['id'], 'cancel': True})
            started = time.monotonic()
            service.call('enrichment', {'id': doc['id'], 'retry': True})
            await_calls(count + 2)
            assert time.monotonic() - started < 2
            stopped = service.call('enrichment', {
                'id': doc['id'], 'cancel': True,
            })
            assert stopped['analysis']['status'] == 'failed'
        unavailable = configuration('unavailable-sections')
        settings = json.loads((unavailable / 'config.json').read_text())
        settings['tools']['mkvextract'] = str(args.executable.resolve())
        (unavailable / 'config.json').write_text(json.dumps(settings))
        with Service(args.executable, unavailable, environment) as service:
            before = compressed.read_bytes()
            doc = service.inspect(compressed)
            assert doc['sections_pending']
            plan = service.call('plans', {
                'inspection_id': doc['id'],
                'edits': [{'track_uid': doc['tracks'][0]['uid'],
                           'property': 'title', 'value': 'Must not be saved'}],
            })
            try:
                service.call('save', {'plan_id': plan['id']})
                raise AssertionError('Missing preservation facts allowed save')
            except AssertionError as error:
                assert 'no write was started' in str(error), error
            assert compressed.read_bytes() == before
        print('PASS: responsive loading, released engine lock, safe pending '
              'naming, manual edits, persistent cache, own-write reuse, '
              'stale-result rejection, provider invalidation, batch queue, '
              'read-ahead cancellation, batch reuse, '
              'bounded audio fallback, full retries and active cancellation')


if __name__ == '__main__':
    main()
