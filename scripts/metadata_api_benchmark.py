"""Read-only UI-load/enrichment/cache timings through the production API."""

import argparse
import json
from pathlib import Path
import statistics
import tempfile
import time

from metadata_check import Service


def main() -> None:
    """Measure fresh and cached inspections without saving any source media."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable', type=Path)
    parser.add_argument('--file', type=Path, action='append', required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    stamps = {path: (path.stat().st_size, path.stat().st_mtime_ns)
              for path in args.file}
    results = []
    with tempfile.TemporaryDirectory(prefix='tracksmith-api-bench-') as tmp:
        config = Path(tmp)
        (config / 'config.json').write_text(json.dumps({
            'version': 1, 'port': 7476,
            'media_roots': list(dict.fromkeys(
                str(path.resolve().parent) for path in args.file)),
            'tools': {},
        }), encoding='utf-8')
        with Service(args.executable, config, {}) as service:
            for path in args.file:
                started = time.perf_counter()
                doc = service.inspect(path)
                listed = time.perf_counter() - started
                initial = doc['analysis'].copy()
                doc = service.ready(doc)
                complete = time.perf_counter() - started
                preview = service.rules(doc, [])
                cached = []
                for _ in range(3):
                    started = time.perf_counter()
                    reopened = service.inspect(path)
                    cached.append(time.perf_counter() - started)
                    if doc['mediainfo'] is not None:
                        assert reopened['analysis']['cached'], reopened
                result = {
                    'file': path.name, 'tracks': len(doc['tracks']),
                    'first_tracks_s': listed, 'initial_analysis': initial,
                    'technical_ready_s': complete,
                    'final_analysis': doc['analysis'],
                    'cached_reopen_s': cached,
                    'median_cached_s': statistics.median(cached),
                    'warnings': doc['warnings'],
                    'audio': [{
                        'uid': t['uid'], 'codec': t['codec'],
                        'mapping': t['mapping'],
                        'layout': (t['mediainfo'] or {}).get('ChannelLayout'),
                        'features': (t['mediainfo'] or {}).get(
                            'Format_AdditionalFeatures'),
                        'format': (t['mediainfo'] or {}).get(
                            'Format_Commercial_IfAny'),
                    } for t in doc['tracks'] if t['kind'] == 'audio'],
                    'proposals': preview['proposals'],
                }
                results.append(result)
                print(f'{path.name}: tracks {listed:.3f}s; technical '
                      f'{complete:.3f}s; cached '
                      f'{result["median_cached_s"]:.3f}s', flush=True)
        with Service(args.executable, config, {}) as service:
            for path, result in zip(args.file, results):
                started = time.perf_counter()
                doc = service.inspect(path)
                result['restart_s'] = time.perf_counter() - started
                result['restart_analysis'] = doc['analysis']
                assert doc['analysis']['cached'], doc
        for path in args.file:
            assert stamps[path] == (path.stat().st_size,
                                    path.stat().st_mtime_ns), path
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(results, indent=2), encoding='utf-8')
    print('PASS: cache survives restart; source sizes/mtimes unchanged; '
          'no media save endpoint called')


if __name__ == '__main__':
    main()
