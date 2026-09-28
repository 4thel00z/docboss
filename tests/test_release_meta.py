"""Release automation metadata: versions agree across the manifests and the
release workflow keeps Cargo.lock in sync. Parses committed files only."""

import json
import tomllib
from pathlib import Path

import yaml

REPO = Path(__file__).resolve().parent.parent


def workspace_version() -> str:
    return tomllib.loads((REPO / "Cargo.toml").read_text())["workspace"]["package"]["version"]


def test_lockfile_pins_current_workspace_version() -> None:
    version = workspace_version()
    lock = tomllib.loads((REPO / "Cargo.lock").read_text())
    members = {pkg["name"]: pkg["version"] for pkg in lock["package"] if pkg["name"].startswith("docboss")}
    assert members, "no docboss workspace members found in Cargo.lock"
    stale = {name: v for name, v in members.items() if v != version}
    assert not stale, f"Cargo.lock pins {stale} but the workspace is at {version}; run `cargo update --workspace`"


def test_versions_agree() -> None:
    version = workspace_version()
    assert tomllib.loads((REPO / "pyproject.toml").read_text())["project"]["version"] == version
    assert json.loads((REPO / ".release-please-manifest.json").read_text())["."] == version


def test_release_workflow_syncs_lockfile_on_release_pr() -> None:
    workflow = (REPO / ".github" / "workflows" / "release-please.yaml").read_text()
    assert "cargo update --workspace" in workflow
    jobs = yaml.safe_load(workflow)["jobs"]
    assert {"release-please", "sync-lockfile", "publish-pypi", "publish-crates"} <= jobs.keys()


def test_release_bumps_cargo_toml() -> None:
    config = json.loads((REPO / "release-please-config.json").read_text())
    assert "Cargo.toml" in config["packages"]["."]["extra-files"]
