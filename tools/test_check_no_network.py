"""Tests for tools/check_no_network.py. Run: python tools/test_check_no_network.py"""

import importlib.util
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

TOOLS = Path(__file__).resolve().parent
SCRIPT = TOOLS / "check_no_network.py"
spec = importlib.util.spec_from_file_location("check_no_network", SCRIPT)
check = importlib.util.module_from_spec(spec)
spec.loader.exec_module(check)

CLEAN_TREE = """\
akm-cli v0.1.0 (C:\\repo\\crates\\akm-cli)
akm-core v0.1.0 (C:\\repo\\crates\\akm-core)
chrono v0.4.42
serde v1.0.228
tokio v1.54.1
http v1.5.0
url v2.5.8
"""

PLANTED_TREE = CLEAN_TREE + "reqwest v0.13.5\nhyper v1.12.0\n"

CLEAN_CONF = {
    "app": {
        "windows": [{"label": "main"}],
        "security": {
            "csp": "default-src 'self'; connect-src ipc: http://ipc.localhost; img-src 'self' data:",
            "devCsp": None,
        },
    },
    "build": {"devUrl": "http://localhost:1420"},
    "bundle": {},
}


def messages(report, level):
    return [message for found, message in report.lines if found == level]


class TreeTests(unittest.TestCase):
    def test_a_planted_reqwest_fails(self):
        report = check.Report()
        hits = check.check_tree(PLANTED_TREE, "planted", report, dict(check.ALLOWED_CRATES))
        self.assertEqual(hits, {"reqwest", "hyper"})
        self.assertTrue(report.failed)
        self.assertTrue(any("`reqwest`" in m for m in messages(report, "FAIL")))

    def test_a_clean_tree_passes_and_allowlisted_crates_are_noted(self):
        report = check.Report()
        hits = check.check_tree(CLEAN_TREE, "clean", report, dict(check.ALLOWED_CRATES))
        self.assertEqual(hits, set())
        self.assertFalse(report.failed)
        self.assertTrue(any("`tokio`" in m for m in messages(report, "NOTE")))

    def test_every_listed_name_and_prefix_fails(self):
        for name in ["ureq", "curl", "isahc", "surf", "attohttpc", "tungstenite", "tokio-tungstenite",
                     "h2", "rustls", "native-tls", "openssl", "mio", "hyper-tls", "hickory-client",
                     "rustls-webpki", "tauri-plugin-updater", "tauri-plugin-http"]:
            with self.subTest(name=name):
                report = check.Report()
                check.check_tree(CLEAN_TREE + f"{name} v1.0.0\n", "x", report, {})
                self.assertTrue(report.failed)

    def test_an_empty_tree_is_not_a_pass(self):
        report = check.Report()
        check.check_tree("error: something\n", "empty", report, {})
        self.assertTrue(report.failed)

    def test_tokio_net_feature_is_detected(self):
        self.assertEqual(check.tokio_net_features('tokio feature "rt"\ntokio feature "net"\n'), {"net"})
        self.assertEqual(check.tokio_net_features('tokio feature "rt"\ntokio feature "sync"\n'), set())


class CommandLineTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.root = Path(self.directory.name)
        desktop = self.root / "desktop"
        (desktop / "src-tauri" / "capabilities").mkdir(parents=True)
        (desktop / "src").mkdir()
        (self.root / "crates" / "akm-core" / "src").mkdir(parents=True)
        (desktop / "src-tauri" / "tauri.conf.json").write_text(json.dumps(CLEAN_CONF))
        (desktop / "src-tauri" / "capabilities" / "default.json").write_text(
            json.dumps({"identifier": "default", "permissions": ["core:default", "dialog:default"]})
        )
        (desktop / "package.json").write_text(json.dumps({"dependencies": {"react": "^19"}}))
        (desktop / "package-lock.json").write_text(
            json.dumps({"packages": {"": {}, "node_modules/react": {}, "node_modules/vite": {"dev": True}}})
        )
        (desktop / "src" / "App.tsx").write_text("export const App = () => null;\n")
        (self.root / "crates" / "akm-core" / "src" / "lib.rs").write_text("pub fn f() {}\n")
        self.tree = self.root / "tree.txt"

    def tearDown(self):
        self.directory.cleanup()

    def run_check(self, tree_text):
        self.tree.write_text(tree_text)
        return subprocess.run(
            [sys.executable, str(SCRIPT), "--root", str(self.root), "--tree-file", str(self.tree)],
            capture_output=True,
            text=True,
        )

    def test_planted_reqwest_makes_the_command_fail(self):
        done = self.run_check(PLANTED_TREE)
        self.assertEqual(done.returncode, 1, done.stdout)
        self.assertIn("networking crate `reqwest`", done.stdout)
        self.assertIn("RESULT: FAIL", done.stdout)

    def test_clean_input_makes_the_command_pass(self):
        done = self.run_check(CLEAN_TREE)
        self.assertEqual(done.returncode, 0, done.stdout)
        self.assertIn("RESULT: PASS", done.stdout)

    def test_a_remote_csp_origin_fails(self):
        conf = json.loads(json.dumps(CLEAN_CONF))
        conf["app"]["security"]["csp"] += "; script-src https://cdn.example.test"
        (self.root / "desktop" / "src-tauri" / "tauri.conf.json").write_text(json.dumps(conf))
        done = self.run_check(CLEAN_TREE)
        self.assertEqual(done.returncode, 1, done.stdout)
        self.assertIn("https://cdn.example.test", done.stdout)

    def test_a_missing_csp_fails(self):
        conf = json.loads(json.dumps(CLEAN_CONF))
        del conf["app"]["security"]["csp"]
        (self.root / "desktop" / "src-tauri" / "tauri.conf.json").write_text(json.dumps(conf))
        self.assertEqual(self.run_check(CLEAN_TREE).returncode, 1)

    def test_updater_configuration_fails(self):
        conf = json.loads(json.dumps(CLEAN_CONF))
        conf["plugins"] = {"updater": {"endpoints": ["https://updates.example.test"]}}
        (self.root / "desktop" / "src-tauri" / "tauri.conf.json").write_text(json.dumps(conf))
        self.assertEqual(self.run_check(CLEAN_TREE).returncode, 1)

    def test_network_capabilities_fail(self):
        for permission in ["http:default", "shell:allow-open", "updater:default", "opener:default", "websocket:default"]:
            with self.subTest(permission=permission):
                (self.root / "desktop" / "src-tauri" / "capabilities" / "default.json").write_text(
                    json.dumps({"identifier": "default", "permissions": ["core:default", {"identifier": permission}]})
                )
                done = self.run_check(CLEAN_TREE)
                self.assertEqual(done.returncode, 1, done.stdout)
                self.assertIn(permission, done.stdout)

    def test_a_remote_capability_block_fails(self):
        (self.root / "desktop" / "src-tauri" / "capabilities" / "default.json").write_text(
            json.dumps({"identifier": "x", "permissions": [], "remote": {"urls": ["https://example.test"]}})
        )
        self.assertEqual(self.run_check(CLEAN_TREE).returncode, 1)

    def test_a_network_client_in_package_json_or_the_lock_fails(self):
        manifest = self.root / "desktop" / "package.json"
        manifest.write_text(json.dumps({"dependencies": {"axios": "^1"}}))
        self.assertEqual(self.run_check(CLEAN_TREE).returncode, 1)
        manifest.write_text(json.dumps({"dependencies": {"react": "^19"}}))
        (self.root / "desktop" / "package-lock.json").write_text(
            json.dumps({"packages": {"": {}, "node_modules/react/node_modules/undici": {}}})
        )
        done = self.run_check(CLEAN_TREE)
        self.assertEqual(done.returncode, 1, done.stdout)
        self.assertIn("undici", done.stdout)

    def test_a_dev_only_network_package_does_not_fail(self):
        (self.root / "desktop" / "package-lock.json").write_text(
            json.dumps({"packages": {"": {}, "node_modules/undici": {"dev": True}}})
        )
        self.assertEqual(self.run_check(CLEAN_TREE).returncode, 0)

    def test_frontend_network_calls_and_url_literals_fail(self):
        source = self.root / "desktop" / "src" / "App.tsx"
        for text in ["fetch('x');", "const s = new WebSocket('x');", "const u = 'https://example.test/a';"]:
            with self.subTest(text=text):
                source.write_text(text + "\n")
                self.assertEqual(self.run_check(CLEAN_TREE).returncode, 1)

    def test_rust_socket_use_fails(self):
        source = self.root / "crates" / "akm-core" / "src" / "lib.rs"
        for text in ["use std::net::TcpStream;", "fn f() { let _ = UdpSocket::bind(\"0.0.0.0:0\"); }"]:
            with self.subTest(text=text):
                source.write_text(text + "\n")
                self.assertEqual(self.run_check(CLEAN_TREE).returncode, 1)


if __name__ == "__main__":
    unittest.main()
