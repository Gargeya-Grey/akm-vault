from pathlib import Path
import json
import os
import subprocess

scratch = Path(__file__).resolve().parent
work = scratch / 'fake-backup-clock-skew'
cli = Path(r'W:\API Keys Manager\target\release\akm.exe')
env = os.environ.copy()
env['AKM_PASSWORD'] = 'fake-only-password-12345'
paths = [work / 'fake.kdbx', *sorted(work.glob('fake.kdbx.bak.*.kdbx'))]
observations = []
for path in paths:
    result = subprocess.run([str(cli), '--vault', str(path), 'list', '--json'], cwd=work, env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    assert result.returncode == 0, (path, result.stderr.decode(errors='replace'))
    entries = json.loads(result.stdout)
    observations.append({'path': path.name, 'exit': result.returncode, 'entryCount': len(entries), 'fakeDeletedEntryPresent': any(entry['title'] == 'fake-key-to-delete' for entry in entries)})
assert len(observations) == 6
assert all(not entry['fakeDeletedEntryPresent'] for entry in observations)
(scratch / 'probe_backup_validate.log').write_text(json.dumps(observations, indent=2) + '\n', encoding='utf-8')
print(json.dumps(observations, indent=2))
