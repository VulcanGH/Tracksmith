"""Focused generated-fixture check: reviewed batch, stale refusal, stop, handoff, graceful exit."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import queue
import re
import shutil
import socket
import subprocess
import tempfile
import threading
import time
import urllib.error
import urllib.request


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable', type=Path)
    parser.add_argument(
        '--benchmark-files', type=int,
        help='Only time three standard-profile previews of N generated files',
    )
    args = parser.parse_args()
    if args.benchmark_files is not None and args.benchmark_files < 3:
        parser.error('--benchmark-files must be at least 3')
    executable = str(args.executable.resolve())
    with tempfile.TemporaryDirectory(prefix='tracksmith-batch-') as directory:
        root = Path(directory)
        media = root / 'media'
        media.mkdir()
        first = media / '1.mka'
        subprocess.run([shutil.which('ffmpeg'), '-v', 'error', '-f', 'lavfi', '-i',
                        'sine=frequency=440:duration=1', '-c:a', 'flac', str(first)],
                       check=True, capture_output=True, timeout=20)
        second, third = media / '2.mka', media / '3.mka'
        shutil.copy2(first, second)
        shutil.copy2(first, third)
        if args.benchmark_files:
            for index in range(4, args.benchmark_files + 1):
                shutil.copy2(first, media / f'{index}.mka')
        original = hashlib.sha256(first.read_bytes()).digest()
        config = root / 'config'
        config.mkdir()
        (config / 'config.json').write_text(json.dumps({'version': 1, 'port': 7476,
            'media_roots': [str(media)], 'tools': {}}), encoding='utf-8')
        with socket.socket() as sock:
            sock.bind(('127.0.0.1', 0))
            port = sock.getsockname()[1]
        env = {**os.environ, 'TRACKSMITH_CONFIG_DIR': str(config), 'RUST_LOG': 'error'}
        process = subprocess.Popen([executable, '--background', '--no-tray', '--port', str(port)],
            env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, encoding='utf-8')
        lines = queue.Queue()
        def reader():
            for line in process.stdout:
                lines.put(line)
            lines.put(None)
        threading.Thread(target=reader, daemon=True).start()
        try:
            token = None
            for _ in range(10):
                line = lines.get(timeout=20)
                assert line is not None, 'Service exited during startup'
                found = re.search(r'#token=([a-f0-9-]+)', line)
                if found:
                    token = found[1]
                    break
            assert token
            opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
            def request(path, body=None, expected=200):
                headers = {'Authorization': f'Bearer {token}'}
                if body is not None:
                    headers['Content-Type'] = 'application/json'
                req = urllib.request.Request(f'http://127.0.0.1:{port}/api/{path}',
                    data=None if body is None else json.dumps(body).encode(), headers=headers)
                try:
                    with opener.open(req, timeout=30) as response:
                        assert response.status == expected
                        return json.load(response)
                except urllib.error.HTTPError as error:
                    detail = error.read().decode()
                    assert error.code == expected, detail
                    return json.loads(detail) if detail.startswith('{') else detail
            def settled():
                for _ in range(300):
                    view = request('batch')
                    if view['phase'] not in ('planning', 'saving'):
                        return view
                    time.sleep(.1)
                raise AssertionError('Batch did not settle')
            assert request('desktop/launch', {'path': str(third)}, 403) is not None
            forwarded = subprocess.run([executable, '--background', '--no-tray', str(third)],
                env=env, capture_output=True, text=True, timeout=15)
            assert forwarded.returncode == 0, forwarded.stderr
            pending = request('desktop/pending')
            assert len(pending) == 1 and pending[0]['path'].endswith('3.mka')
            request('desktop/dismiss', {'id': pending[0]['id']})
            assert process.poll() is None and not request('desktop/pending')
            library = request('rules')
            if args.benchmark_files:
                durations = []
                for _ in range(3):
                    request('batch/preview', {
                        'sources': [str(media)], 'recursive': False,
                        'filter': '', 'profile_id': library['active_profile_id'],
                    })
                    benchmark = settled()
                    assert benchmark['phase'] == 'review', benchmark
                    assert len(benchmark['rows']) == args.benchmark_files
                    assert all(row['status'] == 'ready'
                               for row in benchmark['rows']), benchmark
                    durations.append(benchmark['elapsed_ms'])
                print(json.dumps({'files': args.benchmark_files,
                                  'preview_ms': durations}))
                return
            # Track-only batch preview must agree with full editor inspection.
            payload = {'sources': [str(media)], 'recursive': False,
                       'filter': '', 'profile_id': 'general'}
            request('batch/preview', payload)
            standard = settled()
            assert standard['phase'] == 'review', standard
            inspected = request('inspect', {'path': str(first)})
            preview = request('rules/preview', {
                'inspection_id': inspected['id'], 'profile_id': 'general',
                'manual_edits': [], 'test_rule': None, 'test_library': None,
            })
            plan = request('plans', {
                'inspection_id': inspected['id'],
                'edits': [p['edit'] for p in preview['proposals']],
            })
            assert standard['rows'][0]['changes'] == plan['changes']
            request('batch/clear', {})
            assert not request('batch')['id']
            request('batch/save', {
                'job_id': standard['id'],
                'selected': [standard['rows'][0]['id']],
            }, 422)
            video_folder = media / 'video'
            video_folder.mkdir()
            movie = video_folder / 'Movie.2026.release.mkv'
            subprocess.run([
                shutil.which('ffmpeg'), '-v', 'error', '-f', 'lavfi', '-i',
                'color=size=16x16:duration=1', '-f', 'lavfi', '-i',
                'sine=frequency=440:duration=1', '-c:v', 'ffv1', '-c:a',
                'flac', '-metadata:s:v:0', 'language=und',
                '-metadata:s:a:0', 'language=fre', str(movie),
            ], check=True, capture_output=True, timeout=20)
            request('batch/preview', {**payload, 'sources': [str(movie)]})
            video_preview = settled()
            assert video_preview['phase'] == 'review', video_preview
            request('batch/save', {
                'job_id': video_preview['id'],
                'selected': [video_preview['rows'][0]['id']],
            })
            video_saved = settled()
            assert video_saved['rows'][0]['status'] == 'verified', video_saved
            movie_doc = request('inspect', {'path': str(movie)})
            assert movie_doc['title'] == movie.stem
            video_track = next(t for t in movie_doc['tracks']
                               if t['kind'] == 'video')
            assert video_track['title'] == movie.stem
            assert video_track['language_ietf'] == 'fr', video_track
            request('batch/preview', {**payload, 'sources': [str(movie)]})
            assert settled()['rows'][0]['status'] == 'unchanged'
            request('batch/clear', {})
            library['rules'] = [{'id': 'batch-title', 'name': 'Batch title', 'description': '',
                'enabled': True, 'priority': 0, 'scope': 'audio',
                'conditions': {'mode': 'all', 'conditions': [], 'groups': []},
                'actions': [{'property': 'title', 'source': 'literal', 'value': 'Batch checked',
                             'missing': 'skip_action'}], 'overwrite': 'always', 'replace_when': None}]
            library['profiles'][0]['rule_ids'] = ['batch-title']
            request('rules', library)
            payload = {'sources': [str(media)], 'recursive': False, 'filter': '', 'profile_id': 'general'}
            request('batch/preview', payload)
            view = settled()
            assert view['phase'] == 'review' and len(view['rows']) == 3, view
            assert all(r['status'] == 'ready' and len(r['changes']) == 1 for r in view['rows'])
            assert all(hashlib.sha256(p.read_bytes()).digest() == original for p in (first, second, third))
            # The reviewed library is frozen even if the saved profile is subsequently edited.
            library = request('rules')
            library['rules'][0]['actions'][0]['value'] = 'Newer library'
            request('rules', library)
            stamp = second.stat()
            os.utime(second, ns=(stamp.st_atime_ns, stamp.st_mtime_ns + 5_000_000_000))
            request('batch/save', {'job_id': view['id'], 'selected': [r['id'] for r in view['rows']]})
            saved = settled()
            assert [r['status'] for r in saved['rows']] == ['verified', 'error', 'skipped'], saved
            assert saved['messages'] and saved['rows'][0]['operation_id']
            assert all(hashlib.sha256(p.read_bytes()).digest() == original for p in (second, third))
            inspected = request('inspect', {'path': str(first)})
            assert inspected['tracks'][0]['title'] == 'Batch checked', inspected['tracks'][0]
            history = request('batch/history')
            assert history[0]['id'] == saved['id']
            library = request('rules')
            library['rules'][0]['actions'][0]['value'] = 'Batch checked'
            request('rules', library)
            request('batch/preview', {**payload, 'sources': [str(first)]})
            unchanged = settled()
            assert unchanged['rows'][0]['status'] == 'unchanged', unchanged
            request('batch/clear', {})
            assert request('batch/history')[0]['id'] == saved['id']
            library = request('rules')
            library['rules'][0]['actions'][0]['value'] = 'Newer library'
            request('rules', library)
            # Cancel a fresh preview while native inspection is in flight; it must never write.
            untouched = hashlib.sha256(third.read_bytes()).digest()
            request('batch/preview', {**payload, 'sources': [str(third)]})
            request('batch/cancel', {})
            cancelled = settled()
            assert cancelled['phase'] == 'cancelled', cancelled
            assert hashlib.sha256(third.read_bytes()).digest() == untouched
            # Request cancellation after a file has started; its write/verification must finish.
            request('batch/preview', payload)
            reviewed = settled()
            assert reviewed['phase'] == 'review'
            request('batch/save', {'job_id': reviewed['id'], 'selected': [r['id'] for r in reviewed['rows'] if r['status'] == 'ready']})
            for _ in range(300):
                active = request('batch')
                if active['phase'] == 'saving' and active['current']:
                    break
                assert active['phase'] == 'saving', active
                time.sleep(.01)
            else:
                raise AssertionError('No active batch file observed')
            request('batch/cancel', {})
            stopped = settled()
            assert stopped['phase'] == 'cancelled', stopped
            assert stopped['rows'][0]['status'] == 'verified', stopped
            assert any(r['status'] == 'skipped' for r in stopped['rows']), stopped
            assert all(r['status'] in ('verified', 'skipped') for r in stopped['rows']), stopped
            request('desktop/stop', {})
            assert process.wait(timeout=15) == 0
            assert not (config / 'session.json').exists()
            print('PASS: preview unchanged bytes; frozen profile; verified first save; stale second refused; remaining skipped; report; preview cancellation; active-file cancellation finishes verification; authenticated single-instance handoff; graceful exit')
        finally:
            if process.poll() is None:
                # Emergency cleanup only after the job has finished; never terminate an active writer.
                try:
                    request('desktop/stop', {})
                    process.wait(timeout=30)
                except Exception:
                    raise RuntimeError(f'Backend {process.pid} still running; inspect it before cleanup')


if __name__ == '__main__':
    main()
