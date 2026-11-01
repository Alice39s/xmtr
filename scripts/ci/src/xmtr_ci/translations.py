from pathlib import Path

import opencc

from xmtr_ci.project import ROOT

DOCS = ROOT / "docs"
SOURCE = "zh-CN.md"
# OpenCC has no Macau conversion; zh-MO.md uses the Hong Kong character forms.
CONFIGS = {"zh-TW.md": "s2twp.json", "zh-HK.md": "s2hk.json", "zh-MO.md": "s2hk.json"}


def generate(docs: Path = DOCS) -> dict[Path, str]:
    source = (docs / SOURCE).read_text(encoding="utf-8")
    return {docs / name: opencc.OpenCC(config).convert(source) for name, config in CONFIGS.items()}


def write(docs: Path = DOCS) -> list[Path]:
    generated = generate(docs)
    for path, text in generated.items():
        path.write_text(text, encoding="utf-8", newline="\n")
    return list(generated)


def check(docs: Path = DOCS) -> None:
    stale = [
        path.name
        for path, text in generate(docs).items()
        if not path.is_file() or path.read_text(encoding="utf-8") != text
    ]
    if stale:
        message = f"{', '.join(stale)} out of date with {SOURCE}; run xmtr-ci docs"
        raise SystemExit(message)
