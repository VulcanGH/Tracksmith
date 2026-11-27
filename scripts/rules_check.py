"""Narrow production API check for rule staging; never calls the media save endpoint."""
import argparse
import copy
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
    args = parser.parse_args()
    ffmpeg = shutil.which('ffmpeg')
    if not ffmpeg:
        raise RuntimeError('FFmpeg is required to generate the disposable audio fixture')
    with tempfile.TemporaryDirectory(prefix='tracksmith-rules-') as directory:
        root = Path(directory)
        fixture = root / 'generated.mka'
        subprocess.run([ffmpeg, '-v', 'error', '-f', 'lavfi', '-i', 'sine=frequency=440:duration=1',
                        '-c:a', 'flac', str(fixture)], check=True, capture_output=True, timeout=20)
        original = hashlib.sha256(fixture.read_bytes()).digest()
        config = root / 'config'
        config.mkdir()
        (config / 'config.json').write_text(json.dumps({'version': 1, 'port': 7476, 'media_roots': [str(root)], 'tools': {}}), encoding='utf-8')
        with socket.socket() as sock:
            sock.bind(('127.0.0.1', 0))
            port = sock.getsockname()[1]
        process = subprocess.Popen([str(args.executable.resolve()), '--background', '--no-tray', '--port', str(port)],
            env={**os.environ, 'TRACKSMITH_CONFIG_DIR': str(config), 'RUST_LOG': 'error'},
            stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, encoding='utf-8')
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
                if line is None:
                    raise RuntimeError('Backend exited during startup')
                found = re.search(r'#token=([a-f0-9-]+)', line)
                if found:
                    token = found[1]
                    break
            if token is None:
                raise RuntimeError('No authenticated session URL received')
            opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
            def request(path, body=None, expected=200):
                headers = {'Authorization': f'Bearer {token}'}
                if body is not None:
                    headers['Content-Type'] = 'application/json'
                req = urllib.request.Request(f'http://127.0.0.1:{port}/api/{path}',
                    data=None if body is None else json.dumps(body).encode(), headers=headers)
                try:
                    with opener.open(req, timeout=20) as response:
                        assert response.status == expected
                        return json.load(response)
                except urllib.error.HTTPError as error:
                    details = error.read().decode()
                    assert error.code == expected, details
                    return json.loads(details)
            def inspect_ready(path):
                doc = request('inspect', {'path': str(path)})
                for _ in range(400):
                    if doc.get('analysis', {}).get('status') != 'pending' and not doc.get('sections_pending'):
                        return doc
                    doc = request('enrichment', {'id': doc['id']})
                    time.sleep(0.05)
                raise AssertionError('Technical analysis did not finish')
            library = request('rules')
            assert library['revision'] == 0 and len(library['rules']) >= 29
            assert library['profiles'][0]['auto_apply']
            assert all(a['property'] == 'title' or
                       (r['scope'] == 'video' and a['property'] == 'language')
                       for r in library['rules'] for a in r['actions'])
            standard = request('rules/standard')
            assert standard == library
            default_doc = inspect_ready(fixture)
            default_preview = request('rules/preview', {'inspection_id': default_doc['id'], 'profile_id': 'general', 'manual_edits': [], 'test_rule': None, 'test_library': None})
            assert any(p['edit']['value'] == 'Unknown FLAC 1.0' for p in default_preview['proposals']), default_preview
            assert hashlib.sha256(fixture.read_bytes()).digest() == original
            subtitles = root / 'commentary.srt'
            subtitles.write_text(
                '1\n00:00:00,000 --> 00:00:00,900\nCommentary fixture\n',
                encoding='utf-8',
            )
            commentary = root / 'commentary.mkv'
            subprocess.run([
                ffmpeg, '-v', 'error', '-i', str(fixture), '-i',
                str(subtitles), '-map', '0:a:0', '-map', '1:s:0',
                '-c:a', 'copy', '-c:s', 'srt', '-metadata',
                'title=commentary', '-metadata:s:a:0', 'language=eng',
                '-metadata:s:a:0', 'title=English FLAC 1.0 (Commentary)',
                '-metadata:s:s:0', 'language=eng', '-metadata:s:s:0',
                'title=English (SRT, Commentary)', str(commentary),
            ], check=True, capture_output=True, timeout=20)
            commentary_hash = hashlib.sha256(commentary.read_bytes()).digest()
            for _ in range(3):
                doc = inspect_ready(commentary)
                preview = request('rules/preview', {
                    'inspection_id': doc['id'], 'profile_id': 'general',
                    'manual_edits': [], 'test_rule': None,
                    'test_library': None,
                })
                assert not preview['proposals'], preview
            assert (
                hashlib.sha256(commentary.read_bytes()).digest()
                == commentary_hash
            )
            group = {'mode': 'all', 'conditions': [], 'groups': []}
            def rule(id, priority, property, source, value, overwrite):
                return {'id': id, 'name': id, 'description': '', 'enabled': True, 'priority': priority,
                    'scope': 'audio', 'conditions': group, 'actions': [{'property': property, 'source': source,
                        'value': value, 'missing': 'skip_action'}], 'overwrite': overwrite, 'replace_when': None}
            library['rules'] = [rule('language', 0, 'language', 'preferred_language', None, 'when_undefined'),
                                rule('title', 1, 'title', 'template', '{language_name} {channel_count}ch', 'always')]
            library['profiles'][0].update(rule_ids=['language', 'title'], preferred_languages=['fr-CA'], auto_apply=True)
            saved = request('rules', library)
            assert saved['revision'] == 1
            assert 'changed since' in request('rules', library, 422)['message']
            assert request('rules/validate', saved)['profiles'] == saved['profiles']
            doc = inspect_ready(fixture)
            audio = doc['tracks'][0]
            payload = {'inspection_id': doc['id'], 'profile_id': 'general', 'manual_edits': [], 'test_rule': None, 'test_library': None}
            preview = request('rules/preview', payload)
            assert len(preview['proposals']) == 2, preview
            assert any(p['edit']['value'] == 'French 1ch' for p in preview['proposals'])
            assert any(p['edit']['value'] == 'fr-CA' for p in preview['proposals'])
            protected = request('rules/preview', {**payload, 'manual_edits': [{'track_uid': audio['uid'], 'property': 'title', 'value': None}]})
            assert protected['preserved_manual'] == 1
            assert not any(p['edit']['property'] == 'title' for p in protected['proposals'])
            draft = copy.deepcopy(saved)
            draft['profiles'][0]['preferred_languages'] = ['de']
            test = copy.deepcopy(draft['rules'][1])
            test['actions'][0]['value'] = '{preferred_language_name} staged'
            tested = request('rules/preview', {**payload, 'test_rule': test, 'test_library': draft})
            assert any(p['edit']['value'] == 'German staged' for p in tested['proposals'])
            assert request('rules')['profiles'][0]['preferred_languages'] == ['fr-CA']
            assert hashlib.sha256(fixture.read_bytes()).digest() == original, 'Rule preview modified media bytes'
            stamp = fixture.stat()
            os.utime(fixture, ns=(stamp.st_atime_ns, stamp.st_mtime_ns + 5_000_000_000))
            assert 'changed' in request('rules/preview', payload, 422)['message']
            print('PASS: fresh standard defaults and factory endpoint, real-fixture Unknown FLAC 1.0 preview, editable profile persistence, manual-original protection, unsaved test context, stale-file refusal; media bytes unchanged')
        finally:
            # This scenario never starts a metadata write.
            process.terminate()
            process.wait(timeout=10)


if __name__ == '__main__':
    main()
