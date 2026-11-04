import hashlib
import os
import platform
import shutil
import tomllib
from functools import cache
from importlib.resources import files
from pathlib import Path

import httpx
from pydantic import TypeAdapter

from xmtr_ci import web
from xmtr_ci.openwrt import validate_binary
from xmtr_ci.project import ROOT
from xmtr_ci.shell import output, run
from xmtr_ci.targets import Target

CACHE = ROOT / "target" / "xmtr-ci" / "openwrt"
LLVM_REPOSITORY = "https://github.com/rust-lang/llvm-project.git"


@cache
def toolchains() -> dict[str, dict[str, str]]:
    table = tomllib.loads(files(__package__).joinpath("openwrt-toolchains.toml").read_text())
    return TypeAdapter(dict[str, dict[str, str]]).validate_python(table, strict=True)


def download(url: str, expected: str) -> Path:
    path = CACHE / "downloads" / Path(url).name
    if path.is_file():
        with path.open("rb") as stream:
            actual = hashlib.file_digest(stream, "sha256").hexdigest()
    else:
        try:
            data = web.client.get(url).raise_for_status().content
        except httpx.HTTPError as error:
            raise RuntimeError(f"{url}: download failed: {error}") from error
        actual = hashlib.sha256(data).hexdigest()
        if actual == expected:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(data)
    if actual != expected:
        raise ValueError(f"{url}: SHA-256 is {actual}, expected {expected}")
    return path


def rust_toolchain(settings: dict[str, str]) -> Path:
    prefix = CACHE / f"rust-nightly-{settings['snapshot']}"
    marker = prefix / ".complete"
    if marker.is_file():
        return prefix
    prefix.mkdir(parents=True, exist_ok=True)
    host = settings["host"]
    components = (
        (f"rustc-nightly-{host}", "rustc", "rustc_sha256"),
        (f"cargo-nightly-{host}", "cargo", "cargo_sha256"),
        (f"rust-std-nightly-{host}", f"rust-std-{host}", "host_std_sha256"),
        ("rust-src-nightly", "rust-src", "rust_src_sha256"),
    )
    for archive, component, checksum in components:
        url = f"https://static.rust-lang.org/dist/{settings['snapshot']}/{archive}.tar.xz"
        path = download(url, settings[checksum])
        run(
            "timeout",
            "120",
            "tar",
            "--extract",
            "--xz",
            "--file",
            path,
            "--directory",
            prefix,
            "--strip-components=2",
            f"{archive}/{component}",
        )
    run(prefix / "bin" / "rustc", "--version", "--verbose")
    marker.write_text(settings["snapshot"] + "\n")
    return prefix


def compiler(settings: dict[str, str]) -> Path:
    url = (
        f"https://downloads.openwrt.org/releases/{settings['release']}/targets/"
        f"{settings['target']}/{settings['archive']}"
    )
    archive = download(url, settings["sha256"])
    unpacked = CACHE / "toolchains" / archive.name.removesuffix(".tar.zst")
    prefix = unpacked / settings["directory"] / "bin" / settings["prefix"]
    if not prefix.with_name(prefix.name + "-gcc").is_file():
        unpacked.parent.mkdir(parents=True, exist_ok=True)
        run(
            "timeout",
            "120",
            "tar",
            "--extract",
            "--zstd",
            "--file",
            archive,
            "--directory",
            unpacked.parent,
        )
    run(prefix.with_name(prefix.name + "-gcc"), "--version")
    return prefix


def standard_library(target: Target, settings: dict[str, str], rust: Path) -> Path:
    archive = f"rust-std-nightly-{target.triple}"
    snapshot = toolchains()["rust"]["snapshot"]
    url = f"https://static.rust-lang.org/dist/{snapshot}/{archive}.tar.xz"
    path = download(url, settings["rust_std_sha256"])
    run(
        "timeout",
        "120",
        "tar",
        "--extract",
        "--xz",
        "--file",
        path,
        "--directory",
        rust,
        "--strip-components=2",
        f"{archive}/rust-std-{target.triple}",
    )
    installed = CACHE / "unwind" / f"rust-nightly-{snapshot}" / target.triple / "lib"
    installed.mkdir(parents=True, exist_ok=True)
    source = rust / "lib" / "rustlib" / target.triple / "lib" / "self-contained" / "libunwind.a"
    shutil.copy2(source, installed / "libunwind.a")
    return installed


def llvm_source(commit: str) -> Path:
    source = CACHE / "llvm" / commit
    marker = source / ".complete"
    if not marker.is_file():
        source.mkdir(parents=True, exist_ok=True)
        run("git", "init", source)
        run("git", "-C", source, "config", "remote.origin.url", LLVM_REPOSITORY)
        run("timeout", "60", "git", "-C", source, "sparse-checkout", "init", "--cone")
        run(
            "timeout",
            "120",
            "git",
            "-C",
            source,
            "sparse-checkout",
            "set",
            "libunwind",
            "cmake",
            "runtimes",
            "libcxx/cmake",
            "llvm/cmake",
        )
        run(
            "timeout",
            "240",
            "git",
            "-C",
            source,
            "fetch",
            "--depth",
            "1",
            "--filter=blob:none",
            "origin",
            commit,
        )
        run("timeout", "240", "git", "-C", source, "checkout", "--detach", "FETCH_HEAD")
        marker.write_text(commit + "\n")
    actual = output("git", "-C", source, "rev-parse", "HEAD")
    if actual != commit:
        raise ValueError(f"{source}: LLVM commit is {actual}, expected {commit}")
    return source


def unwind_library(target: Target, source: Path, prefix: Path, cflags: str) -> Path:
    installed = CACHE / "unwind" / source.name / target.triple
    library = installed / "lib" / "libunwind.a"
    build = CACHE / "unwind-build" / source.name / target.triple
    env = {"STAGING_DIR": str(prefix.parent.parent), "TMPDIR": str(CACHE / "tmp")}
    run(
        "timeout",
        "180",
        "cmake",
        "-S",
        source / "runtimes",
        "-B",
        build,
        "-DLLVM_ENABLE_RUNTIMES=libunwind",
        "-DLLVM_ENABLE_PER_TARGET_RUNTIME_DIR=OFF",
        f"-DLLVM_DEFAULT_TARGET_TRIPLE={target.triple}",
        "-DLLVM_INCLUDE_TESTS=OFF",
        "-DLLVM_INCLUDE_DOCS=OFF",
        "-DCMAKE_SYSTEM_NAME=Linux",
        "-DCMAKE_SYSTEM_PROCESSOR=mips",
        f"-DCMAKE_C_COMPILER={prefix}-gcc",
        f"-DCMAKE_CXX_COMPILER={prefix}-g++",
        f"-DCMAKE_ASM_COMPILER={prefix}-gcc",
        f"-DCMAKE_C_FLAGS={cflags}",
        f"-DCMAKE_CXX_FLAGS={cflags}",
        f"-DCMAKE_ASM_FLAGS={cflags}",
        "-DCMAKE_BUILD_TYPE=MinSizeRel",
        f"-DCMAKE_INSTALL_PREFIX={installed}",
        "-DLIBUNWIND_ENABLE_SHARED=OFF",
        "-DLIBUNWIND_ENABLE_STATIC=ON",
        "-DLIBUNWIND_INCLUDE_TESTS=OFF",
        "-DLIBUNWIND_INCLUDE_DOCS=OFF",
        "-DLIBUNWIND_USE_COMPILER_RT=OFF",
        env=env,
    )
    run("timeout", "180", "cmake", "--build", build, "--parallel", "2", env=env)
    run("timeout", "60", "cmake", "--install", build, env=env)
    if not library.is_file():
        raise FileNotFoundError(f"{target.triple}: CMake did not install {library}")
    return installed / "lib"


def compile_release(target: Target) -> None:
    if (platform.system(), platform.machine()) != ("Linux", "x86_64"):
        raise SystemExit(f"{target.triple}: OpenWrt toolchains require a Linux x86_64 host")
    settings = toolchains()
    selected = settings[target.triple]
    (CACHE / "tmp").mkdir(parents=True, exist_ok=True)
    rust = rust_toolchain(settings["rust"])
    prefix = compiler(selected)
    if "rust_std_sha256" in selected:
        native = standard_library(target, selected, rust)
        cargo_flags = []
    else:
        source = llvm_source(settings["rust"]["llvm_commit"])
        native = unwind_library(target, source, prefix, selected["cflags"])
        cargo_flags = ["-Zbuild-std=std,panic_abort"]
    underscore = target.triple.replace("-", "_")
    build = CACHE / "build"
    env = {
        "PATH": f"{rust / 'bin'}:{os.environ['PATH']}",
        "CARGO_HOME": str(CACHE / "cargo-home"),
        "CARGO_TARGET_DIR": str(build),
        "TMPDIR": str(CACHE / "tmp"),
        "RUSTC": str(rust / "bin" / "rustc"),
        "RUSTDOC": str(rust / "bin" / "rustdoc"),
        "CARGO_BUILD_JOBS": "2",
        "CARGO_ENCODED_RUSTFLAGS": "\x1f".join(
            ("-Ctarget-feature=+crt-static", "-Clink-self-contained=no", f"-Lnative={native}")
        ),
        "STAGING_DIR": str(prefix.parent.parent),
        f"CC_{underscore}": f"{prefix}-gcc",
        f"AR_{underscore}": f"{prefix}-ar",
        f"CFLAGS_{underscore}": selected["cflags"],
        f"CARGO_TARGET_{underscore.upper()}_LINKER": f"{prefix}-gcc",
    }
    run(
        "timeout",
        "600",
        rust / "bin" / "cargo",
        "build",
        *cargo_flags,
        "--release",
        "--locked",
        "--target",
        target.triple,
        "-p",
        "xmtr",
        "-p",
        "xmtr-installer",
        "-j2",
        cwd=ROOT,
        env=env,
    )
    output_directory = ROOT / "target" / target.triple / "release"
    output_directory.mkdir(parents=True, exist_ok=True)
    for name in (target.binary, target.installer_binary):
        binary = build / target.triple / "release" / name
        if not binary.is_file():
            raise FileNotFoundError(f"{target.triple}: Cargo did not produce {binary}")
        validate_binary(binary, target.triple)
        shutil.copy2(binary, output_directory / name)
