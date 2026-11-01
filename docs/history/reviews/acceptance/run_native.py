"""Compile only current core/helper sources against existing dependency artifacts, then run fake-data probes."""
import json
from pathlib import Path
import subprocess

scratch = Path(__file__).resolve().parent
deps = Path("C:/Users/Gargeya/AppData/Local/Temp/opencode/akm-phase1-correction-review/target/debug/deps")
sources = Path("W:/API Keys Manager")

def artifact(name, explicit=None):
    paths = [deps / explicit] if explicit else list(deps.glob(f"lib{name}-*.rlib"))
    assert len(paths) == 1 and paths[0].is_file(), (name, paths)
    return paths[0]

shared = {
    "serde": artifact("serde"), "serde_json": artifact("serde_json"),
    "sha2": artifact("sha2", "libsha2-dcf00c7ae8040a88.rlib"),
    "uuid": artifact("uuid"), "zeroize": artifact("zeroize"),
}
core_deps = shared | {name: artifact(name) for name in ["chrono", "fs2", "keepass", "hex", "thiserror", "argon2"]}
core = scratch / "libakm_core_acceptance.rlib"
native_deps = shared | {"akm_core": core, "windows_sys": artifact("windows_sys", "libwindows_sys-412daa131efad4b7.rlib")}

def compiler(name, path, output, externs, library=False):
    args = ["rustc", "--edition=2021", "--crate-name", name, str(path), "-L", f"dependency={deps}", "-o", str(output)]
    if library:
        args += ["--crate-type", "rlib"]
    for key, value in externs.items():
        args += ["--extern", f"{key}={value}"]
    return args

records = []
commands = [
    compiler("akm_core", sources / "crates/akm-core/src/lib.rs", core, core_deps, True),
    compiler("akm_acceptance_native", scratch / "native.rs", scratch / "native.exe", native_deps),
    [str(scratch / "native.exe")],
]
for command in commands:
    result = subprocess.run(command, cwd=scratch, text=True, encoding="utf-8", capture_output=True, timeout=120)
    records.append({"command": command, "exit_code": result.returncode, "stdout": result.stdout, "stderr": result.stderr})
    (scratch / "native-results.json").write_text(json.dumps(records, indent=2), encoding="utf-8")
    print(result.stdout, end="")
    print(result.stderr, end="")
    if result.returncode:
        raise SystemExit(result.returncode)
