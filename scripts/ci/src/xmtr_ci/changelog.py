import re
from pathlib import Path

from markdown_it import MarkdownIt


def section(changelog: Path, version: str) -> str:
    text = changelog.read_text()
    lines = text.splitlines()
    tokens = MarkdownIt("commonmark").parse(text)
    headings = [
        (token.map[0], tokens[index + 1].content)
        for index, token in enumerate(tokens)
        if token.type == "heading_open" and token.tag == "h2" and token.map is not None
    ]
    title = re.compile(rf"^\[?v?{re.escape(version)}\]?(?:\s|$)")
    for position, (line, content) in enumerate(headings):
        if title.match(content):
            end = headings[position + 1][0] if position + 1 < len(headings) else len(lines)
            body = "\n".join(lines[line + 1 : end]).strip("\n")
            if body:
                return body + "\n"
    raise SystemExit(f"{changelog.name} has no notes for {version}")
