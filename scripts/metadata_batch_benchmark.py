"""Read-only batch timings for file listing and completed technical checks."""

import argparse
import json
from pathlib import Path
import tempfile
import time

from metadata_check import Service


def main() -> None:
    """Time uncached and cached previews without changing source media."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable', type=Path)
    parser.add_argument('--file', type=Path, action='append', required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    stamps = {p: (p.stat().st_size, p.stat().st_mtime_ns) for p in args.file}
    results = []
    with tempfile.TemporaryDirectory(prefix='tracksmith-bulk-bench-') as tmp:
        config = Path(tmp)
        (config / 'config.json').write_text(json.dumps({
            'version': 1, 'port': 7476, 'tools': {},
            'media_roots': list(dict.fromkeys(
                str(p.resolve().parent) for p in args.file)),
        }), encoding='utf-8')
        with Service(args.executable, config, {}) as service:
            for run in ('uncached', 'cached'):
                started = time.monotonic()
                service.call('batch/preview', {
                    'sources': [str(p.resolve()) for p in args.file],
                    'recursive': False, 'filter': '',
                    'profile_id': 'general',
                })
                first_listed = all_listed = first_checked = None
                while True:
                    view = service.call('batch')
                    elapsed = time.monotonic() - started
                    rows = view['rows']
                    if rows and first_listed is None:
                        first_listed = elapsed
                    if len(rows) == len(args.file) and all_listed is None:
                        all_listed = elapsed
                    if first_checked is None and any(
                        r['status'] in ('ready', 'unchanged', 'error')
                        for r in rows
                    ):
                        first_checked = elapsed
                    if view['phase'] != 'planning':
                        break
                    assert elapsed < 300, 'Benchmark exceeded five minutes'
                    time.sleep(0.02)
                assert view['phase'] == 'review', view
                results.append({
                    'run': run, 'files': len(rows),
                    'first_listed_s': first_listed,
                    'all_listed_s': all_listed,
                    'first_checked_s': first_checked,
                    'complete_s': elapsed,
                    'rows': rows,
                })
                print(json.dumps({k: v for k, v in results[-1].items()
                                  if k != 'rows'}), flush=True)
    for path, stamp in stamps.items():
        assert stamp == (path.stat().st_size, path.stat().st_mtime_ns)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(results, indent=2), encoding='utf-8')


if __name__ == '__main__':
    main()
