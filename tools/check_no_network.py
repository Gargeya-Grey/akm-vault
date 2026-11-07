#!/usr/bin/env python3
"""Fail when AKM could make a network connection.

AKM promises that it opens no network connection: no telemetry and no update
check. This check cannot prove that promise. It looks for the ways a network
connection usually enters this repository:

1. a networking crate in the Rust dependency tree (shipped Windows target, then
   all targets);
2. the Tauri configuration: content security policy, updater and plugins;
3. the Tauri capabilities: http, shell, opener, updater, websocket or remote URLs;
4. the frontend dependencies (package.json and the production part of
   package-lock.json) and the frontend and Rust sources.

Every allowlist entry below carries the reason that a person read from
`cargo tree -i <crate>` or from the file. Add a reason, never only a name.

Usage:
    python tools/check_no_network.py [--root DIR] [--tree-file FILE]

`--tree-file` replaces the live `cargo tree` run for the shipped target with a
recorded output. The tests use it. Exit code 0: no finding. Exit code 1: at
least one FAIL line.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
from pathlib import Path

SHIPPED_TARGET = "x86_64-pc-windows-msvc"

CARGO_TREE = [
    "cargo",
    "tree",
    "--workspace",
    "--edges",
    "normal,build",
    "--prefix",
    "none",
    "--offline",
    "--locked",
]

# Crates that open sockets, speak HTTP or TLS, or exist to do so.
NETWORK_CRATES = {
    "reqwest",
    "hyper",
    "hyper-util",
    "hyper-tls",
    "hyper-rustls",
    "ureq",
    "curl",
    "curl-sys",
    "isahc",
    "surf",
    "attohttpc",
    "minreq",
    "tungstenite",
    "tokio-tungstenite",
    "soketto",
    "h2",
    "h3",
    "quinn",
    "quinn-proto",
    "quinn-udp",
    "rustls",
    "native-tls",
    "openssl",
    "openssl-sys",
    "schannel",
    "security-framework",
    "webpki",
    "webpki-roots",
    "rustls-native-certs",
    "mio",
    "socket2",
    "ipnet",
    "httparse",
    "tower-http",
    "tiny_http",
    "axum",
    "warp",
    "actix-web",
    "awc",
    "trust-dns-resolver",
    "hickory-resolver",
    "hickory-proto",
    "sentry",
    "tauri-plugin-http",
    "tauri-plugin-updater",
    "tauri-plugin-shell",
    "tauri-plugin-opener",
    "tauri-plugin-websocket",
    "tauri-plugin-upload",
    "tauri-plugin-deep-link",
    "tauri-plugin-localhost",
    "tauri-plugin-single-instance",
    # Not network code by themselves. Listed so that each one needs a reason in
    # ALLOWED_CRATES, and a new one in the tree is read by a person.
    "http",
    "http-body",
    "http-body-util",
    "url",
    "tokio",
    "tokio-util",
}
NETWORK_CRATE_PREFIXES = ("hyper-", "hickory-", "trust-dns-", "rustls-", "aws-sdk-")

# Crates that look like network crates by name or by role but do no network I/O
# in this tree. Each entry was read with `cargo tree -i <crate>` (2026-11-07,
# tauri 2.12.5, tauri-plugin-dialog 2.8.4). Update the reason when a crate
# changes its role.
ALLOWED_CRATES = {
    "http": (
        "type crate: request and response types, no I/O. Reached only through "
        "tauri-utils (and tauri-runtime) for header and URL types."
    ),
    "http-body": "type crate: the body trait of the http types, no I/O.",
    "http-body-util": "helpers for the http-body trait, no I/O.",
    "url": (
        "URL parser. Reached through tauri-utils, schemars and tauri-plugin for "
        "configuration parsing. It parses text and opens no connection."
    ),
    "tokio": (
        "async runtime used by tauri for tasks. This check also requires that "
        "the tokio features `net` and `full` stay off, so tokio has no sockets "
        "and mio does not enter the tree."
    ),
    "tokio-util": "tokio helper types; no sockets without the tokio net feature.",
}

# Crates that appear only when the dependency tree is built for targets AKM does
# not ship. `cargo tree --target all` lists them. tauri 2.12.5 declares
# `reqwest` under `cfg(any(target_os = "android", all(target_vendor = "apple",
# not(target_os = "macos"))))` (its Cargo.toml). AKM ships Windows only
# (PROJECT.md non-goals: mobile platforms are out of scope).
OFF_TARGET_REASON = (
    "off-target only: tauri declares reqwest for Android and iOS, and `cargo tree -i` "
    "showed every path to this crate going through reqwest (2026-11-07); AKM builds "
    "for Windows only"
)

NPM_NETWORK_PACKAGES = {
    "axios",
    "node-fetch",
    "cross-fetch",
    "isomorphic-fetch",
    "whatwg-fetch",
    "undici",
    "got",
    "ky",
    "superagent",
    "request",
    "needle",
    "socket.io",
    "socket.io-client",
    "engine.io-client",
    "sockjs-client",
    "ws",
    "posthog-js",
    "mixpanel-browser",
    "analytics",
    "firebase",
    "logrocket",
    "rollbar",
    "bugsnag-js",
}
NPM_NETWORK_PREFIXES = (
    "@tauri-apps/plugin-http",
    "@tauri-apps/plugin-updater",
    "@tauri-apps/plugin-shell",
    "@tauri-apps/plugin-opener",
    "@tauri-apps/plugin-websocket",
    "@tauri-apps/plugin-upload",
    "@tauri-apps/plugin-deep-link",
    "@sentry/",
    "@datadog/",
    "@amplitude/",
    "@segment/",
    "@vercel/analytics",
)

# Capability permission prefixes that grant network, remote-open or update access.
DENIED_PERMISSION_PREFIXES = (
    "http:",
    "shell:",
    "opener:",
    "updater:",
    "websocket:",
    "upload:",
    "deep-link:",
    "localhost:",
)

# Content security policy sources that need no network. `ipc:` and
# `http://ipc.localhost` are the Tauri IPC scheme on Windows: WebView2 sends the
# request to the app's own custom-protocol handler and no socket is opened.
CSP_ALLOWED_SOURCES = {
    "'self'": "same origin (the bundled app)",
    "'none'": "nothing",
    "'unsafe-inline'": "inline styles; not a network source",
    "data:": "inline data URI",
    "blob:": "in-memory object URL",
    "ipc:": "Tauri IPC scheme, handled in process",
    "http://ipc.localhost": "Tauri IPC origin on Windows, handled in process by the custom protocol handler",
}

# Frontend source files may hold URL text only where it is never fetched.
FRONTEND_URL_ALLOWED = [
    (
        "KeyForm.tsx",
        "placeholder=",
        "placeholder text for the console URL field; a form hint, never fetched",
    ),
]

FRONTEND_NETWORK_PATTERNS = [
    (re.compile(r"\bfetch\s*\("), "fetch("),
    (re.compile(r"\bXMLHttpRequest\b"), "XMLHttpRequest"),
    (re.compile(r"\bnew\s+WebSocket\b"), "WebSocket"),
    (re.compile(r"\bEventSource\b"), "EventSource"),
    (re.compile(r"\bsendBeacon\b"), "sendBeacon"),
    (re.compile(r"\bimportScripts\b"), "importScripts"),
]
URL_LITERAL = re.compile(r"(?:https?|wss?|ftp)://[^\s\"'`<>)]+", re.IGNORECASE)

RUST_NETWORK_PATTERNS = [
    (re.compile(r"\bstd::net\b"), "std::net"),
    (re.compile(r"\bTcpStream\b"), "TcpStream"),
    (re.compile(r"\bTcpListener\b"), "TcpListener"),
    (re.compile(r"\bUdpSocket\b"), "UdpSocket"),
    (re.compile(r"\bToSocketAddrs\b"), "ToSocketAddrs"),
    (re.compile(r"\b(reqwest|hyper|ureq|tungstenite|isahc|attohttpc)::"), "networking crate path"),
    (re.compile(r"\b(WinHttp|WinINet|InternetOpen|URLDownloadToFile|WSAStartup|getaddrinfo)\w*"), "Windows networking API"),
    (re.compile(r"Win32_Networking"), "windows-sys networking feature"),
]

CRATE_LINE = re.compile(r"^(?P<name>[A-Za-z0-9_\-]+) v(?P<version>\S+)")
TOKIO_FEATURE = re.compile(r'^tokio feature "(?P<feature>[^"]+)"', re.MULTILINE)


class Report:
    def __init__(self) -> None:
        self.lines: list[tuple[str, str]] = []

    def fail(self, message: str) -> None:
        self.lines.append(("FAIL", message))

    def note(self, message: str) -> None:
        self.lines.append(("NOTE", message))

    def ok(self, message: str) -> None:
        self.lines.append(("OK", message))

    @property
    def failed(self) -> bool:
        return any(level == "FAIL" for level, _ in self.lines)

    def print(self) -> None:
        for level, message in self.lines:
            print(f"{level:4} {message}")


def is_network_crate(name: str) -> bool:
    lowered = name.lower()
    return lowered in NETWORK_CRATES or lowered.startswith(NETWORK_CRATE_PREFIXES)


def crate_names(tree_text: str) -> dict[str, set[str]]:
    found: dict[str, set[str]] = {}
    for line in tree_text.splitlines():
        match = CRATE_LINE.match(line.strip())
        if match:
            found.setdefault(match["name"], set()).add(match["version"])
    return found


def check_tree(tree_text: str, label: str, report: Report, allowed: dict[str, str] | None = None) -> set[str]:
    """Fail for every networking crate in `tree_text`. Returns the set of hits."""
    allowed = allowed if allowed is not None else {}
    names = crate_names(tree_text)
    if not names:
        report.fail(f"{label}: the cargo tree output holds no crate line; the check cannot judge it")
        return set()
    hits = {name for name in names if is_network_crate(name) and name not in allowed}
    for name in sorted(hits):
        versions = ", ".join(sorted(names[name]))
        report.fail(
            f"{label}: networking crate `{name}` ({versions}) is in the dependency tree. "
            f"Run `cargo tree -i {name}` and remove it, or allowlist it in tools/check_no_network.py with a reason."
        )
    for name in sorted(names):
        if name in allowed and is_network_crate(name):
            report.note(f"{label}: allowlisted `{name}`: {allowed[name]}")
    if not hits:
        report.ok(f"{label}: no networking crate among {len(names)} crates")
    return hits


def tokio_net_features(features_text: str) -> set[str]:
    return {m["feature"] for m in TOKIO_FEATURE.finditer(features_text)} & {"net", "full"}


def run(command: list[str], cwd: Path) -> tuple[int, str, str]:
    env = dict(os.environ, CARGO_TERM_COLOR="never")
    done = subprocess.run(command, cwd=cwd, capture_output=True, text=True, env=env)
    return done.returncode, done.stdout, done.stderr


def check_cargo(root: Path, tree_file: Path | None, report: Report) -> None:
    allowed = {**ALLOWED_CRATES}
    if tree_file is not None:
        check_tree(tree_file.read_text(encoding="utf-8"), "rust tree (recorded)", report, allowed)
        return

    code, out, err = run(CARGO_TREE + ["--target", SHIPPED_TARGET], root)
    if code != 0:
        report.fail(f"`cargo tree` for {SHIPPED_TARGET} failed (exit {code}): {err.strip()[-400:]}")
        return
    shipped_hits = check_tree(out, f"rust tree ({SHIPPED_TARGET}, shipped)", report, allowed)

    code, features, err = run(
        ["cargo", "tree", "--workspace", "--edges", "normal,build,features", "--prefix", "none",
         "--offline", "--locked", "--target", SHIPPED_TARGET, "-i", "tokio"],
        root,
    )
    if code != 0:
        if "tokio" in out:
            report.fail(f"`cargo tree -i tokio` failed (exit {code}): {err.strip()[-300:]}")
    else:
        net = tokio_net_features(features)
        if net:
            report.fail(f"tokio feature(s) {sorted(net)} are enabled: tokio can open sockets and pulls in mio")
        else:
            report.ok("tokio has no `net` or `full` feature on the shipped target")

    code, out_all, err = run(CARGO_TREE + ["--target", "all"], root)
    if code != 0:
        report.fail(f"`cargo tree --target all` failed (exit {code}): {err.strip()[-400:]}")
        return
    off_target = {name: OFF_TARGET_REASON for name in crate_names(out_all) if is_network_crate(name) and name not in shipped_hits}
    check_tree(out_all, "rust tree (all targets)", report, {**allowed, **off_target})


def csp_sources(csp: object) -> list[tuple[str, str]]:
    pairs: list[tuple[str, str]] = []
    if isinstance(csp, str):
        for directive in csp.split(";"):
            parts = directive.split()
            if parts:
                pairs.extend((parts[0], source) for source in parts[1:])
    elif isinstance(csp, dict):
        for name, value in csp.items():
            values = value.split() if isinstance(value, str) else list(value)
            pairs.extend((name, str(source)) for source in values)
    return pairs


def csp_directives(csp: object) -> set[str]:
    if isinstance(csp, str):
        return {d.split()[0] for d in csp.split(";") if d.split()}
    if isinstance(csp, dict):
        return set(csp)
    return set()


def check_tauri_conf(conf: dict, report: Report) -> None:
    security = conf.get("app", {}).get("security", {})
    csp = security.get("csp")
    if not csp:
        report.fail("tauri.conf.json: app.security.csp is missing; the webview would run without a policy")
    else:
        directives = csp_directives(csp)
        for required in ("default-src", "connect-src"):
            if required not in directives:
                report.fail(f"tauri.conf.json: CSP has no `{required}` directive")
        bad = [(d, s) for d, s in csp_sources(csp) if s not in CSP_ALLOWED_SOURCES and not s.startswith("'sha256-")]
        for directive, source in bad:
            report.fail(f"tauri.conf.json: CSP `{directive}` allows `{source}`, which is not on the no-network allowlist")
        if not bad and {"default-src", "connect-src"} <= directives:
            report.ok("tauri.conf.json: CSP has no remote origin")
    if security.get("devCsp") is None:
        report.note("tauri.conf.json: devCsp is null; development only, not used by a release build")
    for flag in ("dangerousRemoteDomainIpcAccess", "assetProtocol"):
        if security.get(flag):
            report.fail(f"tauri.conf.json: app.security.{flag} is set")

    plugins = conf.get("plugins", {}) or {}
    for name in plugins:
        if name in {"updater", "http", "shell", "opener", "websocket", "upload", "deep-link", "localhost"}:
            report.fail(f"tauri.conf.json: plugins.{name} is configured")
    bundle = conf.get("bundle", {}) or {}
    if bundle.get("createUpdaterArtifacts"):
        report.fail("tauri.conf.json: bundle.createUpdaterArtifacts is set; that is an updater")
    if not plugins.keys() & {"updater"} and not bundle.get("createUpdaterArtifacts"):
        report.ok("tauri.conf.json: no updater configuration")

    for window in conf.get("app", {}).get("windows", []) or []:
        url = str(window.get("url", ""))
        if re.match(r"(?i)(https?|wss?)://", url):
            report.fail(f"tauri.conf.json: window `{window.get('label', '?')}` loads remote URL {url}")
    dev_url = str(conf.get("build", {}).get("devUrl", ""))
    if dev_url and not re.match(r"(?i)http://(localhost|127\.0\.0\.1)(:\d+)?(/|$)", dev_url):
        report.fail(f"tauri.conf.json: build.devUrl `{dev_url}` is not local")
    elif dev_url:
        report.note(f"tauri.conf.json: build.devUrl {dev_url} is the local Vite server; development only")

    mode = ((bundle.get("windows") or {}).get("webviewInstallMode") or {}).get("type")
    if mode == "downloadBootstrapper":
        report.note(
            "tauri.conf.json: webviewInstallMode downloadBootstrapper. The installer, not AKM, "
            "downloads the Microsoft WebView2 bootstrapper at install time when WebView2 is missing. "
            "AKM runtime makes no such request. The promise 'no connection' must say 'at run time'."
        )


def walk_strings(value: object):
    if isinstance(value, str):
        yield value
    elif isinstance(value, dict):
        for key, item in value.items():
            if key == "$schema":
                continue
            yield from walk_strings(item)
    elif isinstance(value, list):
        for item in value:
            yield from walk_strings(item)


def check_capabilities(directory: Path, report: Report) -> None:
    files = sorted(p for p in directory.glob("*") if p.is_file()) if directory.is_dir() else []
    if not files:
        report.fail(f"{directory}: no capability file found; the check cannot judge permissions")
        return
    clean = True
    for path in files:
        text = path.read_text(encoding="utf-8")
        if path.suffix.lower() != ".json":
            report.fail(f"{path.name}: capability file is not JSON; this check reads JSON only")
            clean = False
            continue
        data = json.loads(text)
        documents = data if isinstance(data, list) else [data]
        for document in documents:
            if "remote" in document:
                report.fail(f"{path.name}: capability has a `remote` block (remote URLs may call IPC)")
                clean = False
            for permission in document.get("permissions", []):
                identifier = permission if isinstance(permission, str) else permission.get("identifier", "")
                if identifier.startswith(DENIED_PERMISSION_PREFIXES):
                    report.fail(f"{path.name}: permission `{identifier}` grants network, update or remote-open access")
                    clean = False
            for string in walk_strings(document):
                if re.search(r"(?i)\b(https?|wss?)://", string):
                    report.fail(f"{path.name}: capability holds a URL: {string}")
                    clean = False
    if clean:
        report.ok(f"capabilities: no http, shell, opener, updater or remote permission in {len(files)} file(s)")


def is_network_npm(name: str) -> bool:
    return name in NPM_NETWORK_PACKAGES or name.startswith(NPM_NETWORK_PREFIXES)


def check_frontend_packages(desktop: Path, report: Report) -> None:
    manifest = desktop / "package.json"
    if not manifest.is_file():
        report.fail(f"{manifest} is missing")
        return
    package = json.loads(manifest.read_text(encoding="utf-8"))
    declared = {**package.get("dependencies", {}), **package.get("optionalDependencies", {})}
    hits = sorted(name for name in declared if is_network_npm(name))
    for name in hits:
        report.fail(f"package.json: dependency `{name}` is a network client or telemetry package")

    lock = desktop / "package-lock.json"
    locked = 0
    if lock.is_file():
        for key, entry in json.loads(lock.read_text(encoding="utf-8")).get("packages", {}).items():
            if not key or entry.get("dev"):
                continue
            locked += 1
            name = key.rsplit("node_modules/", 1)[-1]
            if is_network_npm(name):
                report.fail(f"package-lock.json: production package `{name}` is a network client or telemetry package")
                hits.append(name)
    else:
        report.fail(f"{lock} is missing; transitive production packages cannot be checked")
    if not hits:
        report.ok(f"frontend packages: no network client among {len(declared)} direct and {locked} locked production packages")


def source_files(base: Path, suffixes: tuple[str, ...], skip_tests: bool):
    if not base.is_dir():
        return
    for path in sorted(base.rglob("*")):
        if not path.is_file() or path.suffix not in suffixes:
            continue
        if any(part in {"node_modules", "dist", "target", "gen"} for part in path.parts):
            continue
        if skip_tests and (".test." in path.name or ".spec." in path.name):
            continue
        yield path


def check_frontend_sources(desktop: Path, report: Report) -> None:
    findings = 0
    files = list(source_files(desktop / "src", (".ts", ".tsx", ".js", ".css", ".html"), True))
    for extra in (desktop / "index.html", *(desktop / "public").glob("*")):
        if extra.is_file():
            files.append(extra)
    for path in files:
        for number, line in enumerate(path.read_text(encoding="utf-8", errors="replace").splitlines(), 1):
            for pattern, label in FRONTEND_NETWORK_PATTERNS:
                if pattern.search(line):
                    report.fail(f"{path.name}:{number}: frontend uses {label}")
                    findings += 1
            for match in URL_LITERAL.finditer(line):
                reason = next((why for name, marker, why in FRONTEND_URL_ALLOWED if path.name == name and marker in line), None)
                if reason:
                    report.note(f"{path.name}:{number}: URL text `{match.group(0)}` allowlisted: {reason}")
                else:
                    report.fail(f"{path.name}:{number}: URL literal `{match.group(0)}`")
                    findings += 1
    if not findings:
        report.ok(f"frontend sources: no network API or URL literal in {len(files)} file(s)")


def check_rust_sources(root: Path, report: Report) -> None:
    bases = [root / "desktop" / "src-tauri" / "src", *sorted((root / "crates").glob("*/src"))]
    manifests = [root / "Cargo.toml", root / "desktop" / "src-tauri" / "Cargo.toml", *sorted((root / "crates").glob("*/Cargo.toml"))]
    findings = 0
    count = 0
    for path in [p for base in bases for p in source_files(base, (".rs",), False)] + [m for m in manifests if m.is_file()]:
        count += 1
        for number, line in enumerate(path.read_text(encoding="utf-8", errors="replace").splitlines(), 1):
            stripped = line.strip()
            if stripped.startswith("//"):
                continue
            for pattern, label in RUST_NETWORK_PATTERNS:
                if pattern.search(line):
                    report.fail(f"{path.relative_to(root)}:{number}: Rust source uses {label}")
                    findings += 1
    if not findings:
        report.ok(f"rust sources: no socket or HTTP API in {count} file(s)")


def run_all(root: Path, tree_file: Path | None) -> Report:
    report = Report()
    check_cargo(root, tree_file, report)
    conf = root / "desktop" / "src-tauri" / "tauri.conf.json"
    if conf.is_file():
        check_tauri_conf(json.loads(conf.read_text(encoding="utf-8")), report)
    else:
        report.fail(f"{conf} is missing")
    check_capabilities(root / "desktop" / "src-tauri" / "capabilities", report)
    check_frontend_packages(root / "desktop", report)
    check_frontend_sources(root / "desktop", report)
    check_rust_sources(root, report)
    return report


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parent.parent)
    parser.add_argument("--tree-file", type=Path, help="recorded `cargo tree` output; skips the live cargo runs")
    args = parser.parse_args(argv)
    report = run_all(args.root.resolve(), args.tree_file)
    report.print()
    if report.failed:
        print("RESULT: FAIL (a networking path may exist)")
        return 1
    print("RESULT: PASS (no networking path found; this is not a proof of no network)")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
