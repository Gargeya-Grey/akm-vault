import hashlib
import json
from pathlib import Path

root = Path(r"W:\API Keys Manager")
review = root / "docs/work/phase2-1a/reviews/round2"
scratch = Path(__file__).parent

def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

source_manifest = json.loads((review / "source-sha256.json").read_text(encoding="utf-8-sig"))
input_manifest = json.loads((review / "input-sha256.json").read_text(encoding="utf-8-sig"))
changes = []
for relative, expected in source_manifest.items():
    path = root / relative
    actual = digest(path) if path.is_file() else None
    if actual != expected:
        category = "documentation_or_harness" if relative.startswith(("docs/", "tools/")) or relative in {"AGENTS.md", "PHASE1.md", "PHASE2.md", "README.md", "REVIEW.md"} or relative.endswith("/README.md") else "application_or_build_input"
        changes.append({"path": relative, "category": category, "expected": expected, "actual": actual})

input_changes = []
for relative, expected in input_manifest.items():
    path = review / relative
    actual = digest(path) if path.is_file() else None
    if actual != expected:
        input_changes.append({"path": relative, "expected": expected, "actual": actual})

source_dirs = [root / "crates/akm-cli/src", root / "crates/akm-cli/tests", root / "crates/akm-core/src", root / "crates/akm-core/tests", root / "desktop/src", root / "desktop/src-tauri/src", root / "desktop/e2e"]
extensions = {".rs", ".ts", ".tsx", ".js", ".cjs", ".css"}
unpinned = []
for directory in source_dirs:
    for path in directory.rglob("*"):
        if path.is_file() and path.suffix in extensions:
            relative = path.relative_to(root).as_posix()
            if relative not in source_manifest:
                unpinned.append(relative)

output = {
    "source_paths_pinned": len(source_manifest),
    "source_manifest_sha256": digest(review / "source-sha256.json"),
    "input_manifest_sha256": digest(review / "input-sha256.json"),
    "source_mismatches": changes,
    "input_mismatches": input_changes,
    "unpinned_code_in_inspected_source_directories": sorted(unpinned),
    "release_cli_sha256": digest(root / "target/release/akm.exe"),
    "release_gui_sha256": digest(root / "target/release/akm-desktop.exe"),
}
(scratch / "source-integrity.json").write_text(json.dumps(output, indent=2) + "\n", encoding="utf-8")
print(json.dumps(output, indent=2))
