from pathlib import Path
from typing import Annotated

import typer

from xmtr_ci import alpine, check, github, glitchtip, publish, release, repos, translations
from xmtr_ci import build as builder
from xmtr_ci import matrix as matrices
from xmtr_ci import smoke as smoke_test
from xmtr_ci import verify as channels
from xmtr_ci.checksums import read_sums
from xmtr_ci.packages import chocolatey, docker, install, linux, managers, npm, wheels
from xmtr_ci.project import version
from xmtr_ci.secrets import Secrets
from xmtr_ci.shell import console
from xmtr_ci.targets import Triple, target

app = typer.Typer(no_args_is_help=True, add_completion=False, pretty_exceptions_show_locals=False)
check_app = typer.Typer(no_args_is_help=True, help="Lint and test the repository.")
package_app = typer.Typer(no_args_is_help=True, help="Build packages from the release archives.")
docker_app = typer.Typer(no_args_is_help=True, help="Build the container image.")
release_app = typer.Typer(no_args_is_help=True, help="Create the GitHub release.")
publish_app = typer.Typer(no_args_is_help=True, help="Publish a release to package registries.")
alpine_app = typer.Typer(no_args_is_help=True, help="Write and build the Alpine aport.")
for name, group in {
    "check": check_app,
    "package": package_app,
    "docker": docker_app,
    "release": release_app,
    "publish": publish_app,
    "alpine": alpine_app,
}.items():
    app.add_typer(group, name=name)

Archives = Annotated[Path, typer.Option(help="Directory with the release archives.")]
Packages = Annotated[Path, typer.Option(help="Directory with the built packages.")]
Version = Annotated[
    str,
    typer.Option(
        default_factory=version, show_default="Cargo.toml version", help="Release version."
    ),
]
Tag = Annotated[str, typer.Option(envvar="GITHUB_REF_NAME", help="Release tag, such as v1.2.3.")]
Repository = Annotated[str, typer.Option(envvar="GITHUB_REPOSITORY")]


def listing(paths: list[Path]) -> None:
    for path in paths:
        console.print(str(path), markup=False)


@app.command(help="Write the build and smoke test matrices as step outputs.")
def matrix() -> None:
    github.set_output(**matrices.matrices())


@app.command(help="Compile one target and pack its archive and installer into the dist directory.")
def build(triple: Triple, dist: Path = Path("dist")) -> None:
    selected = target(triple)
    builder.compile_release(selected)
    listing(builder.pack_release(selected, dist))


@app.command(help="Start the packed binaries of one target, natively or through its runner.")
def smoke(
    triple: Triple,
    dist: Path = Path("dist"),
    trace: Annotated[bool, typer.Option(help="Also trace 127.0.0.1 with raw sockets.")] = False,
) -> None:
    smoke_test.smoke(target(triple), dist, version(), trace=trace)


@check_app.command("rust", help="Run rustfmt, clippy and the test suite.")
def check_rust(fmt: Annotated[bool, typer.Option(help="Also check formatting.")] = True) -> None:
    check.rust(fmt=fmt)


@check_app.command("scripts", help="Lint the shell, Node.js and Python sources and run pytest.")
def check_scripts() -> None:
    check.scripts()


@app.command("docs", help="Write the Traditional Chinese READMEs from docs/zh-CN.md with OpenCC.")
def write_docs() -> None:
    listing(translations.write())


@check_app.command("release", help="Validate the knope versions and the release tag.")
def check_release(ref: Annotated[str, typer.Option(envvar="GITHUB_REF")] = "") -> None:
    release.check()
    if ref.startswith("refs/tags/v"):
        release.check_tag(ref.removeprefix("refs/tags/"))


@package_app.command("npm", help="Build @xmtr/cli and one @xmtr/<platform> package per binary.")
def package_npm(
    version: Version, archives: Archives = Path("dist"), out: Path = Path("packages/npm")
) -> None:
    npm.build(version, archives, out)


@package_app.command("wheels", help="Build one PyPI wheel per platform.")
def package_wheels(
    version: Version, archives: Archives = Path("dist"), out: Path = Path("packages/wheels")
) -> None:
    listing(wheels.build(version, archives, out))


@package_app.command("linux", help="Build the deb, rpm and apk packages with nFPM.")
def package_linux(
    version: Version, archives: Archives = Path("dist"), out: Path = Path("packages/linux")
) -> None:
    listing(linux.build(version, archives, out, Secrets()))


@package_app.command("chocolatey", help="Pack the Chocolatey package with the archives embedded.")
def package_chocolatey(
    version: Version, archives: Archives = Path("dist"), out: Path = Path("packages")
) -> None:
    console.print(str(chocolatey.build(version, archives, out)))


@package_app.command("managers", help="Write the Homebrew formula, Scoop manifest and PKGBUILD.")
def package_managers(
    tag: Tag,
    sums: Annotated[Path, typer.Option(help="SHA256SUMS of the release.")] = Path(
        "dist/SHA256SUMS"
    ),
    out: Path = Path("packages"),
) -> None:
    listing(managers.write(tag, read_sums(sums.read_text()), out))


@package_app.command("test", help="Install each package format once and run the installed xmtr.")
def package_test(
    formats: list[install.Format],
    version: Version,
    packages: Packages = Path("packages"),
) -> None:
    for name in formats:
        install.INSTALLERS[name](packages, version)
        console.print(f"[green]{name}[/]: xmtr {version}")


@docker_app.command("context", help="Lay out the build context and write the image metadata.")
def docker_context(
    version: Version, archives: Archives = Path("dist"), out: Path = Path("docker")
) -> None:
    platforms = docker.context(archives, out)
    github.set_output(
        platforms=",".join(platforms), version=version, tags=",".join(docker.tags(version))
    )


@docker_app.command("test", help="Build and run the amd64 image, then build every platform.")
def docker_test(context: Path = Path("docker"), archives: Archives = Path("dist")) -> None:
    docker.test(context, docker.context(archives, context))


@app.command(help="Build the signed apt, dnf and apk repositories.")
def repositories(packages: Packages = Path("packages"), site: Path = Path("site")) -> None:
    listing(repos.build(packages, site, Secrets()))


@release_app.command("prepare", help="Write SHA256SUMS and the release notes.")
def release_prepare(dist: Path = Path("dist"), notes: Path = Path("notes.md")) -> None:
    release.prepare(dist, notes)


@release_app.command("publish", help="Create a draft release, attach every file and publish it.")
def release_publish(
    tag: Tag, repository: Repository, dist: Path = Path("dist"), notes: Path = Path("notes.md")
) -> None:
    release.publish(tag, repository, dist, notes)


@publish_app.command("glitchtip", help="Create the release and its deploy on GlitchTip.")
def publish_glitchtip(
    tag: Tag,
    commit: Annotated[str, typer.Option(envvar="GITHUB_SHA", help="Commit of the release.")],
    token: Annotated[str, typer.Option(envvar="GLITCHTIP_AUTH_TOKEN", show_default=False)],
) -> None:
    glitchtip.release(tag.removeprefix("v"), commit, token)


@publish_app.command("crates", help="Publish xmtr-release and xmtr to crates.io.")
def publish_crates(tag: Tag) -> None:
    publish.crates(tag.removeprefix("v"))


@publish_app.command("npm", help="Publish the platform packages, then @xmtr/cli.")
def publish_npm(tag: Tag, packages: Packages = Path("npm")) -> None:
    publish.npm(tag.removeprefix("v"), packages)


@publish_app.command("homebrew", help="Push the formula to the Homebrew tap.")
def publish_homebrew(tag: Tag) -> None:
    publish.homebrew(tag)


@publish_app.command("scoop", help="Push the manifest to the Scoop bucket.")
def publish_scoop(tag: Tag) -> None:
    publish.scoop(tag)


@publish_app.command("aur", help="Build xmtr-bin in Arch Linux and push it to the AUR.")
def publish_aur(
    tag: Tag,
    push: Annotated[bool, typer.Option(help="Push the package to the AUR.")] = False,
    out: Path = Path("packages/aur"),
) -> None:
    publish.aur(tag, out, push=push)


@publish_app.command("winget-listed", help="Write whether winget-pkgs lists xmtr.")
def publish_winget_listed() -> None:
    publish.winget_listed()


@publish_app.command("chocolatey", help="Push the package once the first version is approved.")
def publish_chocolatey(
    api_key: Annotated[str, typer.Option(envvar="CHOCOLATEY_API_KEY", show_default=False)],
    packages: Packages = Path("packages"),
) -> None:
    publish.chocolatey(packages, api_key)


@app.command(help="Install a published release through each channel and check the version.")
def verify(
    version: Version,
    names: Annotated[
        list[channels.Channel] | None,
        typer.Argument(help="Channels to check; the default depends on the system."),
    ] = None,
    attempts: int = 20,
    wait: float = 30,
) -> None:
    channels.verify(version, names or channels.default_channels(), attempts=attempts, wait=wait)


@alpine_app.command("aport", help="Write the APKBUILD for a published release.")
def alpine_aport(version: Version, out: Path = Path("packages/alpine/testing/xmtr")) -> None:
    console.print(str(alpine.aport(version, out)))


@alpine_app.command("build", help="Lint, build and install the aport in Alpine edge.")
def alpine_build(revision: str = "HEAD") -> None:
    alpine.build(revision)
