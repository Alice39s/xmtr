from pathlib import Path

from xmtr_ci import changelog, github
from xmtr_ci.checksums import write_sums
from xmtr_ci.project import ROOT, prerelease, version
from xmtr_ci.shell import output, run


def check() -> None:
    run("knope", "--validate", cwd=ROOT)
    knope_version = output("knope", "get-version", cwd=ROOT)
    if knope_version != version():
        raise SystemExit(f"knope reads {knope_version}, Cargo.toml has {version()}")
    if any((ROOT / ".changeset").glob("*.md")):
        pending = output("knope", "release", "--dry-run", cwd=ROOT)
        github.add_summary(f"## Pending release\n```text\n{pending}\n```\n")


def check_tag(tag: str) -> None:
    if tag != f"v{version()}":
        raise SystemExit(f"tag {tag} does not match version {version()}")
    changelog.section(ROOT / "CHANGELOG.md", version())


def prepare(dist: Path, notes: Path) -> None:
    write_sums(dist)
    notes.write_text(changelog.section(ROOT / "CHANGELOG.md", version()))


def publish(tag: str, repository: str, dist: Path, notes: Path) -> None:
    repo = ("--repo", repository)
    flags = ("--prerelease", "--latest=false") if prerelease(tag) else ()
    run(
        "gh", "release", "create", tag, "--draft", *repo,
        "--title", tag, "--notes-file", notes, *flags,
    )  # fmt: skip
    run("gh", "release", "upload", tag, *sorted(dist.iterdir()), *repo)
    run("gh", "release", "edit", tag, "--draft=false", *repo)
