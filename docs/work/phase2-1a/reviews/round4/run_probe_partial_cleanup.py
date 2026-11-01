from pathlib import Path
import subprocess
import sys

scratch = Path(__file__).resolve().parent
deps = Path(r'W:\API Keys Manager\target\debug\deps')
crates = {
    'akm_core': 'libakm_core-7c5c09b231fa42f3.rlib',
    'serde': 'libserde-aa63687df4fd0aed.rlib',
    'sha2': 'libsha2-aa896634fe35136b.rlib',
    'uuid': 'libuuid-1e0e0f481680fde2.rlib',
    'zeroize': 'libzeroize-89d0f21f858ecf35.rlib',
    'windows_sys': 'libwindows_sys-7dd0390b325cd753.rlib',
}
exe = scratch / 'probe_partial_cleanup.exe'
command = ['rustc', '--edition=2021', str(scratch / 'probe_partial_cleanup.rs'), '-L', 'dependency=' + str(deps)]
for name, file in crates.items():
    command.extend(['--extern', name + '=' + str(deps / file)])
command.extend(['-o', str(exe)])
compiled = subprocess.run(command, cwd=scratch, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
(scratch / 'probe_partial_cleanup-build-final.log').write_bytes(compiled.stdout)
print('compile exit:', compiled.returncode)
if compiled.returncode:
    print(compiled.stdout.decode('utf-8', errors='replace'))
    sys.exit(compiled.returncode)
result = subprocess.run([str(exe)], cwd=scratch, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
(scratch / 'probe_partial_cleanup-final.log').write_bytes(result.stdout)
print(result.stdout.decode('utf-8', errors='replace'))
print('probe exit:', result.returncode)
sys.exit(result.returncode)
