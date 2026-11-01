from pathlib import Path
import hashlib
import json
import os
import shutil
import subprocess
import sys
import uuid

scratch = Path(__file__).resolve().parent
work = scratch / 'fake-backup-clock-skew'
work.mkdir(exist_ok=False)
cli = Path(r'W:\API Keys Manager\target\release\akm.exe')
vault = work / 'fake.kdbx'
legacy = Path(str(vault) + '.bak')
env = os.environ.copy()
env['AKM_PASSWORD'] = 'fake-only-password-12345'
env['AKM_VAULT'] = str(vault)

def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def run(*args):
    result = subprocess.run([str(cli), *args], cwd=work, env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    if result.returncode:
        raise RuntimeError((args, result.returncode, result.stdout.decode(errors='replace'), result.stderr.decode(errors='replace')))
    return {'command': list(args), 'exit': result.returncode,
            'stdout': result.stdout.decode('utf-8', errors='replace'),
            'stderr': result.stderr.decode('utf-8', errors='replace')}

init = run('init', '--name', 'Fake backup skew')
add = run('add', 'fake-key-to-delete', '--secret', 'fake-only-secret')
original_hash = sha(vault)
assert legacy.is_file()
earlier_hash = sha(legacy)
assert original_hash != earlier_hash

future = []
for i in range(5):
    name = f'fake.kdbx.bak.29990101T00000{i}000000000Z_{uuid.UUID(int=i + 1)}.kdbx'
    dest = work / name
    shutil.copyfile(legacy, dest)
    future.append(name)
legacy.unlink()
legacy.mkdir()

deletion = run('rm', 'fake-key-to-delete', '--yes')
remaining = sorted(work.glob('fake.kdbx.bak.*.kdbx'))
hashes = {p.name: sha(p) for p in remaining}
result = {
    'cliSha256': sha(cli),
    'init': init,
    'add': add,
    'deletion': deletion,
    'predeleteVaultSha256': original_hash,
    'futureBackupsAreOlderVaultBytes': all(value == earlier_hash for value in hashes.values()),
    'predeleteVaultRetainedInDatedBackups': original_hash in hashes.values(),
    'legacyBackupIsDirectory': legacy.is_dir(),
    'remainingBackupCount': len(remaining),
    'remainingBackupHashes': hashes,
    'futureNames': future,
}
(scratch / 'probe_backup_clock_skew.log').write_text(json.dumps(result, indent=2) + '\n', encoding='utf-8')
print(json.dumps(result, indent=2))
assert len(remaining) == 5
assert result['futureBackupsAreOlderVaultBytes']
assert not result['predeleteVaultRetainedInDatedBackups']
assert result['legacyBackupIsDirectory']
assert 'retained backup' in deletion['stderr']
