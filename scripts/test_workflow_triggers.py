#!/usr/bin/env python3
"""Lock Recipe A (no push-to-master compile) and notes-branch apply."""

from __future__ import annotations

import re
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
WORKFLOWS = ROOT / ".github" / "workflows"


def _on_block(text: str) -> str:
    start = text.index("\non:")
    rest = text[start + 1 :]
    end = rest.index("\njobs:")
    block = rest[:end]
    lines = []
    for line in block.splitlines():
        stripped = line.split("#", 1)[0].rstrip()
        if stripped:
            lines.append(stripped)
    return "\n".join(lines)


class WorkflowTriggerTests(unittest.TestCase):
    def test_ci_has_no_push_compile(self) -> None:
        on_block = _on_block((WORKFLOWS / "ci.yml").read_text(encoding="utf-8"))
        self.assertIn("pull_request:", on_block)
        self.assertIn("workflow_dispatch:", on_block)
        self.assertIn("merge_group:", on_block)
        self.assertNotIn("push:", on_block)
        self.assertNotIn("tags:", on_block)

    def test_apply_release_notes_is_dispatch_only(self) -> None:
        text = (WORKFLOWS / "apply-release-notes.yml").read_text(encoding="utf-8")
        on_block = _on_block(text)
        self.assertIn("workflow_dispatch:", on_block)
        self.assertNotIn("pull_request:", on_block)
        self.assertNotIn("push:", on_block)

    def test_release_uses_apply_script(self) -> None:
        rel = (WORKFLOWS / "release.yml").read_text(encoding="utf-8")
        self.assertIn("bash scripts/apply-release-notes.sh", rel)

    def test_release_publishes_the_crate_after_the_tag(self) -> None:
        rel = (WORKFLOWS / "release.yml").read_text(encoding="utf-8")
        publish = rel.split("publish-crate:", 1)[1].split("build-binaries:", 1)[0]
        self.assertIn("needs: release-please", publish)
        self.assertIn("needs.release-please.outputs.release_created", publish)
        self.assertIn("id-token: write", publish)
        self.assertIn("persist-credentials: false", publish)
        self.assertIn(
            "rust-lang/crates-io-auth-action@c6f97d42243bad5fab37ca0427f495c86d5b1a18 # v1.0.5",
            publish,
        )
        self.assertIn("cargo publish --dry-run --locked", publish)
        self.assertIn("bash scripts/publish-crate.sh", publish)
        self.assertNotIn("cargo publish || true", publish)
        self.assertTrue((ROOT / "scripts" / "publish-crate.sh").is_file())

    def test_release_please_heads_have_light_path(self) -> None:
        ci = (WORKFLOWS / "ci.yml").read_text(encoding="utf-8")
        self.assertIn("startsWith(github.head_ref, 'release-please')", ci)
        self.assertIn("cargo check --locked --all-targets", ci)
        self.assertIn("os: [ubuntu-latest, macos-latest, windows-latest]", ci)
        self.assertIn("release-please keeps this check without a full test run", ci)

    def test_actions_are_pinned_by_sha(self) -> None:
        for path in sorted(WORKFLOWS.glob("*.yml")):
            for lineno, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
                if "uses:" not in line or line.lstrip().startswith("#"):
                    continue
                self.assertIsNotNone(
                    re.search(r"uses:\s+\S+@[0-9a-f]{40}\b", line),
                    f"{path.name}:{lineno} is not pinned: {line.strip()}",
                )
                self.assertIsNone(
                    re.search(r"uses:\s+\S+@v\d", line),
                    f"{path.name}:{lineno} uses a floating tag: {line.strip()}",
                )

    def test_security_has_no_path_filter(self) -> None:
        text = (WORKFLOWS / "security.yml").read_text(encoding="utf-8")
        on_block = _on_block(text)
        self.assertIn("pull_request:", on_block)
        self.assertIn("merge_group:", on_block)
        self.assertNotIn("paths:", on_block)
        self.assertIn("name: Gitleaks", text)
        self.assertIn("name: CodeQL (${{ matrix.language }})", text)
        self.assertNotIn("build-mode: manual", text)
        self.assertIn("language: rust", text)
        self.assertIn("build-mode: none", text)
        self.assertIn("queries: security-and-quality", text)
        self.assertIn("scripts/report-scheduled-failure.py", text)

    def test_required_check_workflows_exist(self) -> None:
        dco = (WORKFLOWS / "dco.yml").read_text(encoding="utf-8")
        self.assertIn("name: DCO Sign-off", dco)
        self.assertIn("bash scripts/check-dco.sh", dco)
        self.assertIn("cancel-in-progress: false", dco)
        title = (WORKFLOWS / "pr-title.yml").read_text(encoding="utf-8")
        self.assertIn("name: Semantic PR Title", title)
        self.assertIn("cancel-in-progress: false", title)
        self.assertIn("dependabot[bot]", title)
        links = (WORKFLOWS / "links.yml").read_text(encoding="utf-8")
        self.assertIn("name: Check Links", links)
        self.assertNotIn("!RELEASE_NOTES.md", links)
        scorecard = (WORKFLOWS / "scorecard.yml").read_text(encoding="utf-8")
        self.assertIn("branches: [master]", scorecard)
        self.assertIn("results.sarif", scorecard)
        approve = (WORKFLOWS / "auto-approve.yml").read_text(encoding="utf-8")
        self.assertIn("github.actor == 'SebTardif'", approve)
        self.assertIn("minibuild-release[bot]", approve)
        self.assertIn("github-actions[bot]", approve)
        self.assertIn("autorelease: pending", approve)
        self.assertNotIn("dependabot[bot]", approve)
        self.assertIn("actions/create-github-app-token@fee1f7d63c2ff003460e3d139729b119787bc349 # v2", approve)
        self.assertIn("secrets.GITHUB_TOKEN", approve)
        self.assertIn("steps.app-token.outputs.token", approve)
        dependabot = (WORKFLOWS / "dependabot-auto-merge.yml").read_text(encoding="utf-8")
        self.assertIn("pull_request_target:", dependabot)
        self.assertIn("github.event.pull_request.user.login == 'dependabot[bot]'", dependabot)
        self.assertNotIn("github.actor == 'dependabot[bot]'", dependabot)
        self.assertNotIn("actions/checkout", dependabot)
        self.assertIn("version-update:semver-major", dependabot)
        self.assertIn("actions/create-github-app-token@fee1f7d63c2ff003460e3d139729b119787bc349 # v2", dependabot)
        self.assertIn('GH_TOKEN="$APP_TOKEN" gh pr merge --auto --squash', dependabot)
        ci = (WORKFLOWS / "ci.yml").read_text(encoding="utf-8")
        self.assertIn("name: Workflow sanity", ci)
        self.assertIn("scripts/report-scheduled-failure.py", ci)
        self.assertIn("test_check_dco.py", ci)
        self.assertIn("test_publish_homebrew_formula.py", ci)

    def test_homebrew_formula_publishes_only_after_a_release(self) -> None:
        release = (WORKFLOWS / "release.yml").read_text(encoding="utf-8")
        homebrew = release.split("homebrew:", 1)[1]
        self.assertIn("needs: [release-please, build-binaries]", homebrew)
        self.assertIn("needs.release-please.outputs.release_created", homebrew)
        self.assertIn("bash scripts/publish-homebrew-formula.sh", homebrew)
        self.assertIn("secrets.HOMEBREW_TAP_KEY", homebrew)
        release_please = release.split("publish-crate:", 1)[0]
        self.assertIn("actions/create-github-app-token@fee1f7d63c2ff003460e3d139729b119787bc349 # v2", release_please)
        self.assertIn("token: ${{ steps.app-token.outputs.token }}", release_please)
        self.assertNotIn("cargo-dist", release)
        self.assertNotIn("dist-workspace.toml", release)
        self.assertTrue((ROOT / "scripts" / "publish-homebrew-formula.sh").is_file())


if __name__ == "__main__":
    unittest.main()
