"""One executable smoke check: embedded UI + authenticated backend, with isolated config."""
import argparse
import json
from pathlib import Path
import os
import queue
import re
import socket
import shutil
import subprocess
import tempfile
import threading
import urllib.request
import urllib.error


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable', type=Path)
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix='tracksmith-smoke-') as directory:
        executable = Path(directory) / 'tracksmith.exe'
        shutil.copy2(args.executable.resolve(), executable)
        with socket.socket() as sock:
            sock.bind(('127.0.0.1', 0))
            port = sock.getsockname()[1]
        env = {**os.environ, 'TRACKSMITH_CONFIG_DIR': directory, 'RUST_LOG': 'error'}
        proc = subprocess.Popen([str(executable), '--background', '--no-tray', '--port', str(port)], env=env, cwd=directory,
                                stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, encoding='utf-8')
        lines = queue.Queue()
        def read_lines():
            for line in proc.stdout:
                lines.put(line)
            lines.put(None)
        threading.Thread(target=read_lines, daemon=True).start()
        try:
            token = None
            for _ in range(10):
                line = lines.get(timeout=10)
                if line is None:
                    raise RuntimeError(f'Tracksmith exited before becoming ready: {proc.poll()}')
                match = re.search(r'#token=([a-f0-9-]+)', line)
                if match:
                    token = match[1]
                    break
            if token is None:
                raise RuntimeError('No session URL received')
            origin = f'http://127.0.0.1:{port}'
            opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
            def get(path, authenticated=False):
                headers = {'Authorization': f'Bearer {token}'} if authenticated else {}
                return opener.open(urllib.request.Request(origin + path, headers=headers), timeout=5)
            with get('/') as response:
                html = response.read().decode()
                assert response.headers['Content-Security-Policy']
            asset = re.search(r'src="(/assets/[^\"]+\.js)"', html)
            assert asset, 'Compiled JavaScript reference missing'
            with get(asset[1]) as response:
                assert len(response.read()) > 1000, 'Embedded JavaScript missing'
            with get('/api/status', True) as response:
                status = json.load(response)
            assert status['name'] == 'Tracksmith'
            assert status['media_root_count'] == 0, 'No folder access should be enabled by default'
            assert Path(status['config_dir']) == Path(directory)
            with get('/api/updates', True) as response:
                updates = json.load(response)
            assert updates['current'] == status['version'] and updates['phase'] == 'not_checked'
            assert updates['kind'] == 'portable'
            def post(path, body, expected=200):
                request = urllib.request.Request(origin + '/api/' + path, data=json.dumps(body).encode(),
                    headers={'Authorization': f'Bearer {token}', 'Content-Type': 'application/json'})
                try:
                    with opener.open(request, timeout=5) as response:
                        assert response.status == expected
                        return json.load(response)
                except urllib.error.HTTPError as error:
                    assert error.code == expected, error.read().decode()
                    return json.load(error)
            assert 'drafts' in post('updates/install', {'version': '1.0.0', 'drafts_clear': False}, 422)['message']
            assert 'Check for updates' in post('updates/install', {'version': '1.0.0', 'drafts_clear': True}, 422)['message']
            media = Path(directory) / 'media'
            media.mkdir()
            (media / 'nested').mkdir()
            (media / 'ignore.txt').write_text('not media', encoding='utf-8')
            for index in range(205):
                (media / f'media-{index:03}.mkv').write_bytes(b'')
            denied = post('browser', {'path': str(media)}, 422)
            assert 'Choose folder' in denied['message']
            post('locations', {'roots': [str(media)]})
            page = post('browser', {'path': str(media)})
            assert page['total_entries'] == 206 and len(page['entries']) == 200
            assert page['entries'][0]['kind'] == 'folder' and page['next_offset'] == 200
            assert page['parent'] is None
            last = post('browser', {'path': str(media), 'offset': page['next_offset']})
            assert last['offset'] == 200 and len(last['entries']) == 6 and last['next_offset'] is None
            filtered = post('browser', {'path': str(media), 'search': 'MEDIA-204'})
            assert filtered['total_entries'] == 1
            canonical = page['path']
            stored_config = json.loads((Path(directory) / 'config.json').read_text(encoding='utf-8'))
            assert canonical in stored_config['recent_locations']
            post('favourite', {'path': canonical, 'favourite': True})
            home = post('browser', {})
            assert any(location['path'] == canonical and location['favourite'] for location in home['locations'])
            post('favourite', {'path': canonical, 'favourite': False})
            denied_recent = post('open-recent', {'path': str(media / 'media-001.mkv')}, 422)
            assert 'not in Tracksmith' in denied_recent['message']
            print('PASS: embedded interface, authenticated status, isolated config, scoped browser, pagination/filter, favourites, unknown-recent refusal')
        finally:
            proc.terminate()
            proc.wait(timeout=10)


if __name__ == '__main__':
    main()
