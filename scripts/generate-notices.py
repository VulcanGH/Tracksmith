"""Generate redistribution notices from resolved Windows Rust and installed npm packages."""
import argparse
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parent.parent

def texts(directory):
    files = []
    for path in directory.iterdir():
        if path.is_file() and path.name.lower().startswith(('license', 'licence', 'notice', 'copying', 'copyright', 'unlicense')):
            files.append(path)
        elif path.is_dir() and path.name.lower() in ('licenses', 'licences', 'license', 'notices'):
            files.extend(p for p in path.rglob('*') if p.is_file())
    result = []
    for path in sorted(files):
        if path.stat().st_size > 512000:
            raise RuntimeError(f'Oversize licence file: {path.name}')
        result.append((path.relative_to(directory).as_posix(), path.read_text(encoding='utf-8-sig', errors='replace').strip()))
    return result

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, default=ROOT / 'docs/notices/THIRD-PARTY-NOTICES.txt')
    args = parser.parse_args()
    metadata = json.loads(subprocess.check_output(['cargo', 'metadata', '--locked', '--format-version', '1', '--filter-platform', 'x86_64-pc-windows-msvc'], cwd=ROOT))
    entries = []
    for package in metadata['packages']:
        if package['name'] == 'tracksmith':
            continue
        entries.append((f"Rust: {package['name']} {package['version']}", package['license'] or 'See upstream', package.get('repository') or f"https://crates.io/crates/{package['name']}/{package['version']}", texts(Path(package['manifest_path']).parent)))
    lock = json.loads((ROOT / 'frontend/package-lock.json').read_text())
    for name, package in lock['packages'].items():
        if not name:
            continue
        folder = ROOT / 'frontend' / name
        manifest = folder / 'package.json'
        if not manifest.is_file():
            continue  # Optional platform-specific build tools absent from this build.
        info = json.loads(manifest.read_text(encoding='utf-8'))
        repository = info.get('repository', '')
        if isinstance(repository, dict):
            repository = repository.get('url', '')
        entries.append((f"npm: {info['name']} {info['version']}" + (' (build tool)' if package.get('dev') else ''), str(info.get('license', package.get('license', 'See upstream'))), repository or f"https://www.npmjs.com/package/{info['name']}/v/{info['version']}", texts(folder)))
    lines = ['Tracksmith — third-party software notices', '', 'Generated from Cargo.lock and installed package-lock.json dependencies.', 'Includes runtime dependencies and build tools for completeness; not all listed', 'tools are distributed in the executable. Media utilities are installed separately.', 'XFold adaptation and language-data attribution are supplied in separate notices.', '']
    missing = []
    for name, licence, source, files in sorted(entries):
        lines.extend(['=' * 78, name, f'Licence expression: {licence}', f'Upstream: {source}', ''])
        if not files:
            missing.append(name)
            lines.append('No standalone licence file was included in this installed package. See upstream.')
        for filename, body in files:
            lines.extend([f'--- {filename} ---', body, ''])
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text('\n'.join(lines) + '\n', encoding='utf-8')
    print(f'Notices: {len(entries)} dependencies, {args.output.stat().st_size:,} bytes')
    if missing:
        print('Packages without standalone licence files: ' + ', '.join(missing))

if __name__ == '__main__':
    main()
