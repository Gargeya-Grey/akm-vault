from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path

scratch = Path(__file__).resolve().parent
root = Path(r'W:\API Keys Manager')
review = root / 'docs/work/phase2-1a/reviews/round4'
inputs = review / 'inputs/docs/work/phase2-1a'

def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()

manifest = json.loads((inputs / 'corrections3/source-sha256.json').read_text(encoding='utf-8-sig'))
old = json.loads((inputs / 'corrections3/before-cycle-sha256.json').read_text(encoding='utf-8-sig'))
verification = json.loads((scratch / 'input-verification.json').read_text(encoding='utf-8'))
core_paths = sorted(name for name in old if name.startswith(('crates/akm-core/', 'crates/akm-cli/')) or name in ('Cargo.toml', 'Cargo.lock'))
core_changed = [name for name in core_paths if old[name] != manifest.get(name)]

scratch_copies = {}
for name in ['clipboard', 'session', 'pending', 'policy', 'error']:
    rel = f'desktop/src-tauri/src/{name}.rs'
    actual = sha(scratch / f'probe-source/{name}.rs')
    scratch_copies[rel] = {'sha256': actual, 'matchesFrozen': actual == manifest[rel]}
for rel, expected in manifest.items():
    if rel.startswith('desktop/src/') or rel in ('desktop/package.json', 'desktop/tsconfig.json'):
        path = scratch / 'frontend-probe' / rel.removeprefix('desktop/')
        actual = sha(path)
        scratch_copies[rel] = {'sha256': actual, 'matchesFrozen': actual == expected}

files = [
    'review-result.md', 'verify_inputs.py', 'input-verification.json', 'input-verification-final.log',
    'probe_backup_clock_skew.py', 'probe_backup_clock_skew.log', 'probe_backup_validate.py', 'probe_backup_validate.log',
    'probe_partial_cleanup.rs', 'run_probe_partial_cleanup.py', 'probe_partial_cleanup-build-final.log',
    'probe_partial_cleanup-final.log', 'probe_partial_cleanup.exe',
    'frontend-probe/vitest.config.ts', 'frontend-probe/src/probe-stale-native-event.test.tsx',
    'frontend-probe/probe-stale-native-event-final.log', 'write_provenance.py',
]
deps = Path(r'W:\API Keys Manager\target\debug\deps')
libraries = ['libakm_core-7c5c09b231fa42f3.rlib', 'libserde-aa63687df4fd0aed.rlib',
             'libsha2-aa896634fe35136b.rlib', 'libuuid-1e0e0f481680fde2.rlib',
             'libzeroize-89d0f21f858ecf35.rlib', 'libwindows_sys-7dd0390b325cd753.rlib']
tauri_sources = [
    Path(r'C:\Users\Gargeya\.cargo\registry\src\index.crates.io-1949cf8c6b5b557f\tauri-2.12.5\src\webview\mod.rs'),
    Path(r'C:\Users\Gargeya\.cargo\registry\src\index.crates.io-1949cf8c6b5b557f\tauri-runtime-wry-2.12.3\src\lib.rs'),
]
result = {
    'reviewedAtUtc': datetime.now(timezone.utc).isoformat(),
    'role': 'one fresh independent read-only reviewer/collector',
    'requestedModelVariant': 'openai/gpt-6.1-terra#max',
    'harnessDeclaredModel': 'openai/gpt-6.1-terra',
    'catalogModelActive': True,
    'catalogListsMaxVariant': True,
    'independentlyObservedActiveVariant': None,
    'independentlyObservedEffort': None,
    'independentlyObservedTokenUsage': None,
    'sourceAcceptance': 'NO',
    'overallWorkUnitRuntimeCompletion': 'NO',
    'findings': [
        {'id': '1A-R4-01', 'priority': 'P2', 'classification': 'reproduced exact CLI with fake vault; GUI consequence inspection-supported', 'summary': 'Future-sorting dated backups can cause pruning of the just-created pre-deletion backup while refresh fails and success/warnings say retained.'},
        {'id': '1A-R4-02', 'priority': 'P2', 'classification': 'reproduced against frozen App/API with mocked native IPC and deliberately delayed event', 'summary': 'Old untagged native lock notification can false-lock frontend B; actual native event ordering is not established.'},
        {'id': '1A-R4-03', 'priority': 'P3', 'classification': 'reproduced with frozen Session/ClipboardLease and fake backend', 'summary': 'Failed immediate cleanup after partial publication bypasses lease episode warning accounting.'},
        {'id': '1A-R4-04', 'priority': 'P2 work-unit proof gate', 'classification': 'coverage gap, not product defect', 'summary': 'Exact-GUI native delayed outcome handoff not reached after two failed minimize observations.'},
    ],
    'inputIndexSha256': sha(review / 'input-sha256.json'),
    'currentSourceZipSha256': verification['archives'][2]['archiveSha256'],
    'cycleSourceManifestSha256': verification['archives'][2]['manifestSha256'],
    'fullUnitSourceManifestPaths': verification['fullUnitManifestPaths'],
    'diffs': verification['diffs'],
    'liveDifferences': verification['liveDifferences'],
    'artifactHashes': verification['artifacts'],
    'beforeCycleCoreCliPathsChecked': len(core_paths),
    'beforeCycleCoreCliDifferences': core_changed,
    'scratchSourceCopies': scratch_copies,
    'allScratchSourceCopiesMatchFrozen': all(v['matchesFrozen'] for v in scratch_copies.values()),
    'independentChecks': [
        {'name': 'frozen inputs, baseline/current archives, exact diff reconstruction, live source and artifacts', 'command': 'python verify_inputs.py', 'exit': 0, 'result': '19 frozen files and three archives matched; diffs exact; four artifact hashes matched; only allowed docs/STATE.md differed'},
        {'name': 'clock-skew backup fake vault', 'command': 'python probe_backup_clock_skew.py; python probe_backup_validate.py', 'exit': 0, 'result': 'exact pinned CLI reproduced deletion with no retained backup containing the deleted fake key'},
        {'name': 'partial publication cleanup fake backend', 'command': 'python run_probe_partial_cleanup.py', 'compileExit': 0, 'exit': 0, 'result': 'actual copied Session/ClipboardLease skipped warning both initially and after an earlier pending episode'},
        {'name': 'native event delay mocked tests', 'command': 'node W:/API Keys Manager/desktop/node_modules/vitest/vitest.mjs run src/probe-stale-native-event.test.tsx --config vitest.config.ts', 'cwd': str(scratch / 'frontend-probe'), 'exit': 0, 'testsPassed': 2, 'result': 'tests assert the current false-lock behavior under mocked event ordering'},
    ],
    'mainSessionEvidenceReportedNotRerun': {
        'rustUniquePasses': 163,
        'rustProvenance': '156 completed before harness timeout plus seven same-workspace selective resume with completed names skipped; no uninterrupted full-workspace exit0 claim',
        'rustGatedIgnored': 2,
        'frontendPasses': 105,
        'mockedNativeBrowserPasses': 18,
        'strictLintTypeAndGuiBundles': 'reported pass',
        'nativeJournalAndExport': 'reported pass on exact GUI; logs inspected',
        'nativeOutcome': 'two failed minimize observations before handoff assertions; coverage incomplete',
        'layoutChecker': 'exit1; existing Activity5 and Keys21 findings per theme; outcome fixture same21 Keys findings',
    },
    'realSecretReadiness': 'not established and not evaluated',
    'projectSourceDocsTestProofChangesByReviewer': False,
    'realClipboardGuiSettingsOsSessionInstallationGitPublicationOrRealSecrets': False,
    'nextAction': 'Main archives verdict and returns rejected acceptance to owner; no automatic correction or extra review cycle.',
    'scratchArtifactSha256': {name: sha(scratch / name) for name in files},
    'debugLibrariesUsedForFakeNativeProbeSha256': {name: sha(deps / name) for name in libraries},
    'externalTauriSourceSha256': {str(path): sha(path) for path in tauri_sources},
}
assert not verification['frozenFailures']
assert not core_changed
assert result['allScratchSourceCopiesMatchFrozen']
(scratch / 'provenance-result.json').write_text(json.dumps(result, indent=2) + '\n', encoding='utf-8')
print(json.dumps({'provenance': str(scratch / 'provenance-result.json'), 'reviewSha256': result['scratchArtifactSha256']['review-result.md'], 'scratchSourcesMatched': len(scratch_copies), 'allScratchSourceCopiesMatchFrozen': result['allScratchSourceCopiesMatchFrozen']}, indent=2))
