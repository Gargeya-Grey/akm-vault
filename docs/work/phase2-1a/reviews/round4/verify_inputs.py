import difflib
import hashlib
import json
from pathlib import Path
import re
import zipfile

ROOT = Path(r'W:\API Keys Manager')
REVIEW = ROOT / 'docs/work/phase2-1a/reviews/round4'
INPUTS = REVIEW / 'inputs'
OUT = Path(__file__).parent

def sha(data):
    return hashlib.sha256(data).hexdigest()

def digest(path):
    return sha(path.read_bytes())

def load(path):
    return json.loads(path.read_text(encoding='utf-8-sig'))

index = load(REVIEW / 'input-sha256.json')
failures = []
for item in index['files']:
    actual = digest(REVIEW / item['frozen'])
    if actual != item['sha256']:
        failures.append({'frozen': item['frozen'], 'actual': actual})

def check_archive(zip_path, manifest_path, label):
    manifest = load(manifest_path)
    with zipfile.ZipFile(zip_path) as z:
        names = set(z.namelist())
        paths = set(manifest)
        differences = []
        for path in sorted(paths & names):
            value = sha(z.read(path))
            if value != manifest[path]:
                differences.append(path)
        return {'label': label, 'archiveSha256': digest(zip_path), 'manifestSha256': digest(manifest_path),
                'manifestPaths': len(paths), 'archivePaths': len(names), 'missing': sorted(paths-names),
                'extra': sorted(names-paths), 'mismatched': differences}

base = INPUTS / 'docs/work/phase2-1a'
cycle = base / 'corrections3'
source = cycle / 'source-sha256.json'
archives = [
    check_archive(base / 'before.zip', base / 'before-sha256.json', 'unit baseline'),
    check_archive(cycle / 'before-cycle.zip', cycle / 'before-cycle-sha256.json', 'cycle baseline'),
    check_archive(REVIEW / 'current-source.zip', source, 'current cycle'),
]
full_manifest = load(base / 'source-sha256.json')
current_manifest = load(source)
full_vs_current = {k: [v, current_manifest.get(k)] for k,v in full_manifest.items() if current_manifest.get(k) != v}
live_differences = []
for path, expected in current_manifest.items():
    p = ROOT / path
    actual = digest(p) if p.is_file() else None
    if actual != expected:
        live_differences.append({'path': path, 'expected': expected, 'actual': actual})

artifacts = {}
for name, expected in load(cycle / 'artifacts-sha256.json').items():
    p = ROOT / name
    actual = digest(p) if p.is_file() else None
    artifacts[name] = {'expected': expected, 'actual': actual, 'matches': expected == actual}

def diff_summary(diffpath, oldzip, oldmanifest, newmanifest):
    text = diffpath.read_text(encoding='utf-8')
    paths = re.findall(r'^--- before/(.*)$', text, flags=re.M)
    after = re.findall(r'^\+\+\+ after/(.*)$', text, flags=re.M)
    old = load(oldmanifest)
    new = load(newmanifest)
    changed = sorted(p for p in set(old)|set(new) if old.get(p) != new.get(p))
    with zipfile.ZipFile(oldzip) as a, zipfile.ZipFile(REVIEW / 'current-source.zip') as b:
        an, bn = set(a.namelist()), set(b.namelist())
        reproduced = []
        nontext = []
        for path in sorted(set(paths)):
            try:
                aa = a.read(path).decode('utf-8').splitlines(keepends=True) if path in an else []
                bb = b.read(path).decode('utf-8').splitlines(keepends=True) if path in bn else []
            except UnicodeDecodeError:
                nontext.append(path)
                continue
            reproduced.extend(difflib.unified_diff(aa, bb, fromfile='before/'+path, tofile='after/'+path))
        reconstructed = ''.join(reproduced)
    return {'sha256': digest(diffpath), 'lines': len(text.splitlines()), 'diffPaths': len(paths),
            'headersMatch': paths == after, 'duplicatePaths': len(paths) != len(set(paths)),
            'changedManifestPaths': len(changed), 'manifestChangesNotInDiff': sorted(set(changed)-set(paths)),
            'diffPathsNotManifestChanges': sorted(set(paths)-set(changed)), 'nontext': nontext,
            'reconstructedExactText': reconstructed == text,
            'reconstructedNormalizedNewlines': reconstructed.replace('\r\n','\n') == text.replace('\r\n','\n')}

diffs = {
    'combined': diff_summary(base/'changes.diff', base/'before.zip', base/'before-sha256.json', base/'source-sha256.json'),
    'cycle': diff_summary(cycle/'changes.diff', cycle/'before-cycle.zip', cycle/'before-cycle-sha256.json', source),
}
result = {'frozenFilesChecked': len(index['files']), 'frozenFailures': failures,
          'expectedCurrentZipSha256': index['currentSourceZipSha256'],
          'expectedSourceManifestSha256': index['sourceManifestSha256'],
          'archives': archives, 'fullUnitManifestPaths': len(full_manifest),
          'fullManifestDifferencesFromCycle': full_vs_current, 'liveDifferences': live_differences,
          'artifacts': artifacts, 'diffs': diffs}
(OUT/'input-verification.json').write_text(json.dumps(result, indent=2)+'\n', encoding='utf-8')
print(json.dumps(result, indent=2))
