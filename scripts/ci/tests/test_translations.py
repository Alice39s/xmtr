from pathlib import Path

import pytest
from inline_snapshot import snapshot

from xmtr_ci import translations


def test_writes_each_variant_from_the_simplified_source(tmp_path: Path) -> None:
    (tmp_path / "zh-CN.md").write_text("默认安装到软件包目录\n", encoding="utf-8")
    written = translations.write(tmp_path)
    assert {path.name: path.read_text(encoding="utf-8") for path in written} == snapshot(
        {
            "zh-TW.md": "預設安裝到軟體包目錄\n",
            "zh-HK.md": "默認安裝到軟件包目錄\n",
            "zh-MO.md": "默認安裝到軟件包目錄\n",
        }
    )


def test_check_names_the_stale_files(tmp_path: Path) -> None:
    source = tmp_path / "zh-CN.md"
    source.write_text("网络\n", encoding="utf-8")
    translations.write(tmp_path)
    translations.check(tmp_path)
    source.write_text("网络与软件\n", encoding="utf-8")
    with pytest.raises(SystemExit, match=r"^zh-TW\.md, zh-HK\.md, zh-MO\.md out of date"):
        translations.check(tmp_path)
