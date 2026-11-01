import datetime
import hashlib
import json
import re
import zipfile
from pathlib import Path

ROOT = Path(r"W:\API Keys Manager")
SCRATCH = Path(r"C:\Users\Gargeya\AppData\Local\Temp\opencode\akm-unit1a-acceptance-round3")
ROUND = ROOT / "docs/work/phase2-1a/reviews/round3"

def digest(data):
    return hashlib.sha256(data).hexdigest()

def file_hash(path):
    return digest(path.read_bytes()) if path.is_file() else None

def category(path):
    if path.startswith("docs/") or path in {"AGENTS.md", "README.md", "REVIEW.md", "PHASE1.md", "PHASE2.md", "crates/akm-cli/README.md", "crates/akm-core/tests/fixtures/README.md"}:
        return "documentation"
    if path.startswith("tools/"):
        return "proof-harness"
    return "application-build-test"

manifest = json.loads((ROUND / "source-sha256.json").read_text(encoding="utf-8"))
inputs = json.loads((ROUND / "input-sha256.json").read_text(encoding="utf-8"))
before = json.loads((ROOT / "docs/work/phase2-1a/before-sha256.json").read_text(encoding="utf-8"))
source = []
for name, expected in manifest.items():
    actual = file_hash(ROOT / name)
    source.append({"path": name, "category": category(name), "expected": expected, "actual": actual, "matches": actual == expected})

with zipfile.ZipFile(ROOT / "docs/work/phase2-1a/before.zip") as z:
    zip_names = set(z.namelist())
    baseline_bytes = {name: z.read(name) for name in zip_names}
    baseline = {name: digest(baseline_bytes[name]) == expected for name, expected in before.items() if name in zip_names}
    zip_missing = sorted(set(before) - zip_names)
    zip_extra = sorted(zip_names - set(before))

diff = (ROUND / "changes.diff").read_text(encoding="utf-8")
diff_paths = set(re.findall(r"^\+\+\+ after/(.+)$", diff, re.MULTILINE))
manifest_changed = {p for p in set(before) | set(manifest) if before.get(p) != manifest.get(p)}

def verify_diff():
    lines = diff.splitlines(keepends=True)
    failures = []
    verified = []
    index = 0
    while index < len(lines):
        header = lines[index]
        if not header.startswith("--- before/"):
            raise ValueError(f"unexpected diff header at {index + 1}")
        name = header.removeprefix("--- before/").rstrip("\n")
        index += 1
        if lines[index] != f"+++ after/{name}\n":
            raise ValueError(f"unmatched after header for {name}")
        index += 1
        original = baseline_bytes.get(name, b"").decode("utf-8").splitlines(keepends=True)
        output = []
        cursor = 0
        while index < len(lines) and not lines[index].startswith("--- before/"):
            hunk = re.match(r"@@ -(\d+)(?:,\d+)? \+\d+(?:,\d+)? @@", lines[index])
            if not hunk:
                raise ValueError(f"invalid hunk for {name} at {index + 1}")
            start = max(int(hunk.group(1)) - 1, 0)
            output.extend(original[cursor:start])
            cursor = start
            index += 1
            while index < len(lines) and not lines[index].startswith(("@@ ", "--- before/")):
                part = lines[index]
                if part[0] in " -":
                    if cursor >= len(original) or original[cursor] != part[1:]:
                        raise ValueError(f"baseline context mismatch in {name} at diff line {index + 1}")
                    cursor += 1
                if part[0] in " +":
                    output.append(part[1:])
                elif part[0] != "-":
                    raise ValueError(f"invalid diff line {index + 1}")
                index += 1
        output.extend(original[cursor:])
        actual = digest("".join(output).encode("utf-8"))
        if actual != manifest.get(name):
            failures.append({"path": name, "reconstructed": actual, "expected": manifest.get(name)})
        else:
            verified.append(name)
    return {"verifiedCount": len(verified), "reconstructionMismatches": failures}

inventory = set()
for relative in ["crates", "desktop/src", "desktop/src-tauri/src", "desktop/e2e", "desktop/public"]:
    directory = ROOT / relative
    for item in directory.rglob("*"):
        if item.is_file():
            inventory.add(item.relative_to(ROOT).as_posix())

copies = {}
for p, expected in manifest.items():
    if p.startswith("desktop/src/"):
        copied = SCRATCH / p.removeprefix("desktop/")
        copies[p] = {"expected": expected, "actual": file_hash(copied), "matches": file_hash(copied) == expected}

artifact_manifest = json.loads((ROOT / "docs/work/phase2-1a/corrections2/artifacts-sha256.json").read_text(encoding="utf-8"))
artifacts = {p: {"expected": expected, "actual": file_hash(ROOT / p)} for p, expected in artifact_manifest.items()}
for value in artifacts.values():
    value["matches"] = value["expected"] == value["actual"]

result = {
    "observedUtc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
    "inputManifestSha256": file_hash(ROUND / "input-sha256.json"),
    "inputs": {p: {"expected": v, "actual": file_hash(ROUND / p), "matches": file_hash(ROUND / p) == v} for p, v in inputs.items()},
    "pinnedSourceCount": len(source),
    "pinnedSource": source,
    "mismatchesByCategory": {c: [x["path"] for x in source if x["category"] == c and not x["matches"]] for c in ["application-build-test", "documentation", "proof-harness"]},
    "applicationInventoryUnpinned": sorted(inventory - set(manifest)),
    "baselineArchive": {"sha256": file_hash(ROOT / "docs/work/phase2-1a/before.zip"), "manifestEntries": len(before), "archiveEntries": len(zip_names), "hashMismatches": sorted(k for k, v in baseline.items() if not v), "missing": zip_missing, "extra": zip_extra},
    "diffCoverage": {"manifestChangedCount": len(manifest_changed), "diffPathCount": len(diff_paths), "missingFromDiff": sorted(manifest_changed - diff_paths), "extraInDiff": sorted(diff_paths - manifest_changed), **verify_diff()},
    "probeCopy": {"count": len(copies), "mismatches": [p for p, v in copies.items() if not v["matches"]], "files": copies},
    "artifacts": artifacts,
    "probeTestSha256": file_hash(SCRATCH / "probe.test.tsx"),
    "probeOutputSha256": file_hash(SCRATCH / "probe-output-final.txt"),
}
(SCRATCH / "source-integrity.json").write_text(json.dumps(result, indent=2), encoding="utf-8")
print(json.dumps({k: result[k] for k in ["observedUtc", "inputManifestSha256", "pinnedSourceCount", "mismatchesByCategory", "applicationInventoryUnpinned", "baselineArchive", "diffCoverage", "artifacts"]}, indent=2))
print("probeCopyMismatches", result["probeCopy"]["mismatches"])
