#!/usr/bin/env python3
"""DCO checker against temporary git repositories."""

from __future__ import annotations

import os
import subprocess
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "check-dco.sh"


def _git(repo: Path, *args: str) -> str:
    env = os.environ.copy()
    env.update(
        {
            "GIT_AUTHOR_NAME": "Test User",
            "GIT_AUTHOR_EMAIL": "test@example.com",
            "GIT_COMMITTER_NAME": "Test User",
            "GIT_COMMITTER_EMAIL": "test@example.com",
        }
    )
    proc = subprocess.run(
        ["git", "-C", str(repo), "-c", "commit.gpgsign=false", *args],
        check=True,
        capture_output=True,
        text=True,
        env=env,
    )
    return proc.stdout.strip()


def _repo() -> tuple[Path, str]:
    repo = Path(tempfile.mkdtemp(prefix="dco-"))
    _git(repo, "init", "-b", "master")
    _git(repo, "commit", "--allow-empty", "-s", "-m", "base")
    base = _git(repo, "rev-parse", "HEAD")
    return repo, base


def _check(repo: Path, base: str) -> subprocess.CompletedProcess[str]:
    env = os.environ.copy()
    env["BASE_SHA"] = base
    return subprocess.run(
        ["bash", str(SCRIPT)],
        cwd=repo,
        capture_output=True,
        text=True,
        check=False,
        env=env,
    )


class CheckDcoTests(unittest.TestCase):
    def test_signed_commit_passes(self) -> None:
        repo, base = _repo()
        _git(repo, "commit", "--allow-empty", "-s", "-m", "feat: signed")
        proc = _check(repo, base)
        self.assertEqual(proc.returncode, 0, proc.stderr)
        self.assertIn("All commits have DCO sign-off", proc.stdout)

    def test_missing_signoff_fails(self) -> None:
        repo, base = _repo()
        _git(repo, "commit", "--allow-empty", "-m", "feat: unsigned")
        proc = _check(repo, base)
        self.assertEqual(proc.returncode, 1, proc.stdout)
        self.assertIn("missing Signed-off-by", proc.stdout)

    def test_bot_commit_is_skipped(self) -> None:
        repo, base = _repo()
        _git(
            repo,
            "commit",
            "--allow-empty",
            "-m",
            "build: bot",
            "--author=dependabot[bot] <49699333+dependabot[bot]@users.noreply.github.com>",
        )
        proc = _check(repo, base)
        self.assertEqual(proc.returncode, 0, proc.stderr + proc.stdout)
        self.assertIn("Skipping bot commit", proc.stdout)

    def test_merge_commit_is_skipped(self) -> None:
        repo, base = _repo()
        _git(repo, "checkout", "-b", "feature")
        _git(repo, "commit", "--allow-empty", "-s", "-m", "feat: on branch")
        _git(repo, "checkout", "master")
        _git(repo, "merge", "--no-ff", "feature", "-m", "Merge branch feature")
        proc = _check(repo, base)
        self.assertEqual(proc.returncode, 0, proc.stderr + proc.stdout)

    def test_anthropic_coauthor_fails(self) -> None:
        repo, base = _repo()
        _git(
            repo,
            "commit",
            "--allow-empty",
            "-s",
            "-m",
            "feat: credited\n\nCo-authored-by: Claude <noreply@anthropic.com>",
        )
        proc = _check(repo, base)
        self.assertEqual(proc.returncode, 1, proc.stdout)
        self.assertIn("Anthropic co-author", proc.stdout)

    def test_human_coauthor_passes(self) -> None:
        repo, base = _repo()
        _git(
            repo,
            "commit",
            "--allow-empty",
            "-s",
            "-m",
            "feat: credited\n\nCo-authored-by: Ada Lovelace <ada@example.com>",
        )
        proc = _check(repo, base)
        self.assertEqual(proc.returncode, 0, proc.stderr)

    def test_claude_session_trailer_fails(self) -> None:
        repo, base = _repo()
        _git(
            repo,
            "commit",
            "--allow-empty",
            "-s",
            "-m",
            "feat: session\n\nClaude-Session: https://claude.ai/code/session_abc",
        )
        proc = _check(repo, base)
        self.assertEqual(proc.returncode, 1, proc.stdout)
        self.assertIn("Claude-Session", proc.stdout)

    def test_empty_base_fails(self) -> None:
        repo, _base = _repo()
        env = os.environ.copy()
        env["BASE_SHA"] = ""
        proc = subprocess.run(
            ["bash", str(SCRIPT)],
            cwd=repo,
            capture_output=True,
            text=True,
            check=False,
            env=env,
        )
        self.assertEqual(proc.returncode, 1)
        self.assertIn("BASE_SHA is empty", proc.stderr)


if __name__ == "__main__":
    unittest.main()
