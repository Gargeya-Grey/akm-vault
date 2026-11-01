"""Pin the current unit diff without initializing git or copying runtime data."""
import difflib
import hashlib
import json
import zipfile
from pathlib import Path

work = Path(__file__).parent
root = work.parents[2]
with zipfile.ZipFile(work / 'before.zip') as archive:
    before = {item.filename: archive.read(item) for item in archive.infolist()}
files = set(before)
for folder in ['crates', 'desktop/src', 'desktop/e2e', 'desktop/src-tauri/src', 'tools']:
    for path in (root / folder).rglob('*'):
        if path.is_file() and path.suffix in {'.rs', '.ts', '.tsx', '.py', '.ps1', '.toml', '.json'}:
            files.add(path.relative_to(root).as_posix())
manifest, changes = {}, []
for name in sorted(files):
    path = root / name
    after = path.read_bytes() if path.is_file() else b''
    if path.is_file():
        manifest[name] = hashlib.sha256(after).hexdigest()
    old = before.get(name, b'')
    if old == after:
        continue
    try:
        changes.extend(difflib.unified_diff(old.decode('utf-8-sig').splitlines(keepends=True), after.decode('utf-8-sig').splitlines(keepends=True), fromfile=f'before/{name}', tofile=f'after/{name}'))
    except UnicodeDecodeError:
        changes.append(f'Binary difference: {name}\n')
(work / 'changes.diff').write_text(''.join(changes), encoding='utf-8')
(work / 'source-sha256.json').write_text(json.dumps(manifest, indent=2) + '\n', encoding='utf-8')
print(f'Pinned {len(manifest)} current paths and combined unit diff at {work}')
