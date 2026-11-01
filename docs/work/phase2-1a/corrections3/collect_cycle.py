"""Pin the renewed cycle without adding runtime files or initializing git."""
import difflib
import hashlib
import json
from pathlib import Path
import zipfile

folder = Path(__file__).resolve().parent
root = folder.parents[3]
with zipfile.ZipFile(folder / "before-cycle.zip") as archive:
    before = {entry.filename: archive.read(entry) for entry in archive.infolist()}
paths = set(before)
for base in ["crates", "desktop/src", "desktop/e2e", "desktop/src-tauri/src", "tools"]:
    paths.update(p.relative_to(root).as_posix() for p in (root / base).rglob("*") if p.is_file() and p.suffix in {".rs", ".ts", ".tsx", ".py", ".ps1", ".toml", ".json"})
for name in ["run_check.py", "run_native.py", "collect_cycle.py"]:
    paths.add((folder / name).relative_to(root).as_posix())
paths.update(["docs/work/phase2-1a/check_layout.mjs", "docs/work/phase2-1a/graph.json", "docs/work/phase2-1a/render_graph.py"])
manifest, diff, changed = {}, [], []
for name in sorted(paths):
    path = root / name
    new = path.read_bytes() if path.is_file() else b""
    if path.is_file():
        manifest[name] = hashlib.sha256(new).hexdigest()
    old = before.get(name, b"")
    if old == new:
        continue
    changed.append(name)
    try:
        diff.extend(difflib.unified_diff(old.decode("utf-8-sig").splitlines(keepends=True), new.decode("utf-8-sig").splitlines(keepends=True), fromfile="before/" + name, tofile="after/" + name))
    except UnicodeDecodeError:
        diff.append("Binary difference: " + name + "\n")
(folder / "source-sha256.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
(folder / "changes.diff").write_text("".join(diff), encoding="utf-8")
print(json.dumps({"paths": len(manifest), "changedFromBeforeCycle": changed}, indent=2))
