#!/usr/bin/env python3
"""Homebrew formula writer, using fixture archives instead of GitHub."""

from __future__ import annotations

import hashlib
import os
import subprocess
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "publish-homebrew-formula.sh"


def _archive(directory: Path, name: str, payload: bytes) -> str:
    path = directory / name
    path.write_bytes(payload)
    return hashlib.sha256(payload).hexdigest()


class PublishHomebrewFormulaTests(unittest.TestCase):
    def test_writes_formula_from_fixtures(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            fixture = Path(tmp) / "fixtures"
            fixture.mkdir()
            arm = _archive(fixture, "minibuild-v0.3.0-aarch64-apple-darwin.tar.gz", b"arm")
            intel = _archive(fixture, "minibuild-v0.3.0-x86_64-apple-darwin.tar.gz", b"intel")
            linux = _archive(fixture, "minibuild-v0.3.0-x86_64-unknown-linux-gnu.tar.gz", b"linux")
            out = Path(tmp) / "Formula" / "minibuild.rb"
            env = os.environ.copy()
            env.update(
                {
                    "TAG": "v0.3.0",
                    "FIXTURE_DIR": str(fixture),
                    "FORMULA_OUT": str(out),
                    "GITHUB_REPOSITORY": "SebTardif/MiniBuildRust",
                }
            )
            proc = subprocess.run(
                ["bash", str(SCRIPT)],
                cwd=tmp,
                env=env,
                capture_output=True,
                text=True,
                check=False,
            )
            self.assertEqual(proc.returncode, 0, proc.stderr)
            text = out.read_text(encoding="utf-8")
            self.assertIn("/releases/download/v0.3.0/minibuild-v0.3.0-", text)
            self.assertNotIn("version ", text)
            self.assertIn(f'sha256 "{arm}"', text)
            self.assertIn(f'sha256 "{intel}"', text)
            self.assertIn(f'sha256 "{linux}"', text)
            self.assertIn("minibuild-v0.3.0-aarch64-apple-darwin.tar.gz", text)
            self.assertNotIn("windows", text)
            self.assertNotIn("PUSH", proc.stdout)

    def test_rejects_empty_tag(self) -> None:
        env = os.environ.copy()
        env["TAG"] = ""
        proc = subprocess.run(
            ["bash", str(SCRIPT)],
            env=env,
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(proc.returncode, 1)
        self.assertIn("TAG is empty", proc.stderr)

    def test_push_requires_a_key(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            fixture = Path(tmp) / "fixtures"
            fixture.mkdir()
            for name, payload in (
                ("minibuild-v9.9.9-aarch64-apple-darwin.tar.gz", b"a"),
                ("minibuild-v9.9.9-x86_64-apple-darwin.tar.gz", b"b"),
                ("minibuild-v9.9.9-x86_64-unknown-linux-gnu.tar.gz", b"c"),
            ):
                (fixture / name).write_bytes(payload)
            env = os.environ.copy()
            env.update(
                {
                    "TAG": "v9.9.9",
                    "FIXTURE_DIR": str(fixture),
                    "FORMULA_OUT": str(Path(tmp) / "minibuild.rb"),
                    "PUSH": "1",
                    "TAP_KEY": "",
                }
            )
            proc = subprocess.run(
                ["bash", str(SCRIPT)],
                cwd=tmp,
                env=env,
                capture_output=True,
                text=True,
                check=False,
            )
            self.assertEqual(proc.returncode, 1, proc.stdout)
            self.assertIn("TAP_KEY is empty", proc.stderr)


if __name__ == "__main__":
    unittest.main()
