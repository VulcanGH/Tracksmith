"""Production-engine verification helper, only called with feasibility.py's generated fixture."""
import hashlib
import json
import os
from pathlib import Path
import queue
import re
import socket
import stat
import subprocess
import threading
import time
import urllib.error
import urllib.request
from feasibility import run


def verify(executable, fixture, root, mkvmerge, mkvextract, expected_hashes):
    config = root / 'app-config'
    config.mkdir()
    (config / 'config.json').write_text(json.dumps({
        'version': 1, 'port': 7476, 'media_roots': [str(root)],
        'tools': {'mkvmerge': str(mkvmerge), 'mkvpropedit': str(mkvmerge.with_name('mkvpropedit.exe')),
                  'mkvextract': str(mkvextract)},
    }), encoding='utf-8')
    with socket.socket() as sock:
        sock.bind(('127.0.0.1', 0))
        port = sock.getsockname()[1]
    proc = subprocess.Popen([str(executable.resolve()), '--background', '--no-tray', '--port', str(port)],
                            env={**os.environ, 'TRACKSMITH_CONFIG_DIR': str(config), 'RUST_LOG': 'error'},
                            stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, encoding='utf-8')
    lines = queue.Queue()
    def reader():
        for line in proc.stdout:
            lines.put(line)
        lines.put(None)
    threading.Thread(target=reader, daemon=True).start()
    try:
        token = None
        for _ in range(10):
            line = lines.get(timeout=20)
            if line is None:
                raise RuntimeError(f'Backend exited during startup: {proc.poll()}')
            match = re.search(r'#token=([a-f0-9-]+)', line)
            if match:
                token = match[1]
                break
        if token is None:
            raise RuntimeError('Backend did not start')
        opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
        def post(path, body):
            request = urllib.request.Request(f'http://127.0.0.1:{port}/api/{path}',
                data=json.dumps(body).encode(), headers={'Content-Type': 'application/json', 'Authorization': f'Bearer {token}'})
            try:
                with opener.open(request, timeout=180) as response:
                    return json.load(response)
            except urllib.error.HTTPError as error:
                raise RuntimeError(f'{path}: {error.read().decode()}') from error
        doc = post('inspect', {'path': str(fixture)})
        deadline = time.monotonic() + 30
        while (doc['analysis']['status'] == 'pending'
               or doc.get('sections_pending')):
            if time.monotonic() >= deadline:
                raise RuntimeError('Fixture enrichment did not finish')
            time.sleep(0.05)
            doc = post('enrichment', {'id': doc['id']})
        assert doc['write_available'], doc['warnings']
        assert len(doc['tracks']) == 3
        assert doc['chapters']['available'] and doc['tags']['available']
        assert len(doc['attachments']) == 1
        assert all(isinstance(t['uid'], str) for t in doc['tracks'])
        audio = next(t for t in doc['tracks'] if t['kind'] == 'audio')
        assert audio['mapping'] == 'uid_number_type_verified', doc['warnings']
        assert {t['uid'] for t in doc['tracks']} == set(expected_hashes)
        edits = [
            {'track_uid': None, 'property': 'title', 'value': 'Tracksmith API verified'},
            {'track_uid': audio['uid'], 'property': 'title', 'value': 'Audio â€” API & Unicode'},
            {'track_uid': audio['uid'], 'property': 'language', 'value': 'fr-CA'},
            {'track_uid': audio['uid'], 'property': 'default', 'value': True},
            {'track_uid': audio['uid'], 'property': 'hearing_impaired', 'value': True},
        ]
        plan = post('plans', {'inspection_id': doc['id'], 'edits': edits})
        assert len(plan['changes']) == len(edits), plan
        result = post('save', {'plan_id': plan['id']})
        assert result['success'], json.dumps({k: result[k] for k in ('status', 'discrepancies', 'warnings')} | {'track_properties': [t['technical'] for t in result.get('inspection', {}).get('tracks', [])]}, indent=2)
        assert result['restore_available']
        after = result['inspection']
        assert after['title'] == 'Tracksmith API verified'
        changed = next(t for t in after['tracks'] if t['uid'] == audio['uid'])
        assert changed['language'] == 'fre' and changed['language_ietf'] == 'fr-CA'
        assert changed['flags']['hearing_impaired'] is True
        unchanged = post('plans', {'inspection_id': after['id'], 'edits': edits})
        assert not unchanged['changes'], 'Repeated metadata proposal is not idempotent'
        restoration = post('restore-plan', {'operation_id': result['operation_id']})
        restored = post('save', {'plan_id': restoration['id']})
        assert restored['success'], json.dumps({k: restored[k] for k in ('status', 'discrepancies', 'warnings')}, indent=2)
        assert restored['inspection']['title'] == doc['title']
        final_audio = next(t for t in restored['inspection']['tracks'] if t['uid'] == audio['uid'])
        assert final_audio['language'] == audio['language'] and final_audio['language_ietf'] == audio['language_ietf']
        assert final_audio['flags'] == audio['flags']
        # Access refusal and a timestamp change must both fail before any write.
        read_only = post('plans', {'inspection_id': restored['inspection']['id'], 'edits': [edits[0]]})
        fixture.chmod(stat.S_IREAD)
        try:
            try:
                post('save', {'plan_id': read_only['id']})
            except RuntimeError as error:
                assert 'read-only' in str(error)
            else:
                raise AssertionError('Read-only write was not rejected')
        finally:
            fixture.chmod(stat.S_IREAD | stat.S_IWRITE)
        stale = post('plans', {'inspection_id': restored['inspection']['id'], 'edits': [edits[0]]})
        file_stat = fixture.stat()
        os.utime(fixture, ns=(file_stat.st_atime_ns, file_stat.st_mtime_ns + 10_000_000_000))
        try:
            post('save', {'plan_id': stale['id']})
        except RuntimeError as error:
            assert 'changed' in str(error)
        else:
            raise AssertionError('Concurrent modification was not rejected')
        try:
            post('inspect', {'path': str(Path(__file__).resolve())})
        except RuntimeError as error:
            assert 'outside configured' in str(error)
        else:
            raise AssertionError('Out-of-scope path was accepted')
        # Verify production save + restoration preserve extracted stream bytes too.
        identified = json.loads(run([mkvmerge, '-J', fixture]))
        for track in identified['tracks']:
            output = root / f"engine-{track['id']}.stream"
            run([mkvextract, fixture, 'tracks', f"{track['id']}:{output}"])
            assert hashlib.sha256(output.read_bytes()).hexdigest() == expected_hashes[str(track['properties']['uid'])]
        return {'result': 'passed', 'verified': ['rich inspection', 'lossless UID transport', 'verified audio facts',
                'staged save', 'language representations', 'flags', 'idempotent plan', 'metadata restoration',
                'read-only rejection', 'stale-file rejection', 'filesystem scope', 'stream payload preservation']}
    finally:
        # Tests terminate the server only after a completed operation, never an active write.
        proc.terminate()
        proc.wait(timeout=10)
