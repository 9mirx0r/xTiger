"""Compare the structure files the game ships (common/**/_*.info) with what the Tiger validators accept.

For every `.info` file it finds the validator source of the same item type and lists:

  NEW    names the info file documents that the validator never mentions (possible false positives:
         a field added to the game that the validator does not know yet);
  STALE  names the validator accepts that the info file does not document and that the game's own
         files of that folder never use (possible false negatives: a field the game dropped).

Both lists are suspects, not verdicts: the game as an oracle decides (see AUDIT.md). Run with `python -I`.

    python -I tools/audit_info.py "<game dir>" [out.md]
"""
import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
DATA = REPO / "src" / "ck3" / "data"

GENERIC = {
    # words that show up in every info file or validator and say nothing
    "key", "type", "name", "value", "limit", "trigger", "effect", "desc", "description", "icon", "yes", "no",
    "and", "or", "not", "nor", "nand", "always", "modifier", "base", "add", "multiply", "scope", "root", "prev",
    "this", "from", "example", "default", "true", "false", "if", "else", "else_if", "hidden_effect", "text",
    "weight", "ai_chance", "min", "max", "factor", "opinion", "random", "list", "any", "every", "ordered",
}


def camel(snake: str) -> str:
    return "".join(part.capitalize() for part in snake.split("_"))


def variants_for(leaf: str):
    base = camel(leaf)
    out = {base}
    if base.endswith("ies"):
        out.add(base[:-3] + "y")
    if base.endswith("s"):
        out.add(base[:-1])
    if base.endswith("es"):
        out.add(base[:-2])
    return out


def loader_map():
    """Item variant -> the data files that load it."""
    found = {}
    for path in DATA.glob("*.rs"):
        text = path.read_text(encoding="utf-8", errors="replace")
        for variant in set(re.findall(r"ItemLoader::\w+\(\s*(?:GameFlavor::\w+,\s*)?Item::(\w+)", text)):
            found.setdefault(variant, []).append(path)
        for variant in set(re.findall(r"db\.add(?:_exact_dup_ok)?\(\s*Item::(\w+)", text)):
            found.setdefault(variant, []).append(path)
    return found


def strings_of(paths):
    names = set()
    for path in paths:
        text = path.read_text(encoding="utf-8", errors="replace")
        for s in re.findall(r'"([a-z_][a-z0-9_]*)"', text):
            names.add(s)
        # `field_x("a|b")` style alternations and macro-like lists
        for alt in re.findall(r'"([a-z_][a-z0-9_|]*\|[a-z0-9_|]+)"', text):
            names.update(alt.split("|"))
    return names


def info_names(text: str):
    names = set()
    for line in text.splitlines():
        line = line.strip().lstrip("﻿")
        line = line.lstrip("#").strip()
        m = re.match(r"^([a-z_][a-z0-9_]*)\s*(?:=|\?=|>=|<=|<|>)", line)
        if m:
            names.add(m.group(1))
        m = re.match(r"^-\s*([a-z_][a-z0-9_]*)\b", line)
        if m:
            names.add(m.group(1))
    return names


def vanilla_keys(folder: Path):
    keys = set()
    for path in folder.rglob("*.txt"):
        text = path.read_text(encoding="utf-8-sig", errors="replace")
        text = re.sub(r"#[^\n]*", "", text)
        keys.update(re.findall(r"\b([a-z_][a-z0-9_]*)\s*(?:=|<=|>=|<|>)", text))
        # values used as enum words (`type = holder`) count as seen, too
        keys.update(re.findall(r"=\s*([a-z_][a-z0-9_]*)\b", text))
    return keys


def global_known():
    """Names of triggers, effects and so on, which an info file shows as examples and no validator lists."""
    names = set()
    for path in (REPO / "src" / "ck3" / "tables").glob("*.rs"):
        text = path.read_text(encoding="utf-8", errors="replace")
        names.update(re.findall(r'"([a-z_][a-z0-9_]*)"', text))
    return names


def top_level_keys(folder: Path):
    """Names of the entries of the folder (the first level), which info files use as examples."""
    names = set()
    for path in folder.rglob("*.txt"):
        text = path.read_text(encoding="utf-8-sig", errors="replace")
        text = re.sub(r"#[^\n]*", "", text)
        depth = 0
        for token in re.finditer(r"([A-Za-z_][\w.:]*)\s*=\s*\{|\{|\}", text):
            if token.group(0) == "}":
                depth -= 1
            elif token.group(0) == "{":
                depth += 1
            else:
                if depth == 0:
                    names.add(token.group(1))
                depth += 1
    return names


def main():
    game = Path(sys.argv[1]) / "game" if (Path(sys.argv[1]) / "game").is_dir() else Path(sys.argv[1])
    out = Path(sys.argv[2]) if len(sys.argv) > 2 else None
    loaders = loader_map()
    known = global_known()
    groups = {}
    unmapped = []
    for info in sorted((game / "common").rglob("*.info")):
        folder = info.parent
        files = []
        for variant in variants_for(folder.name):
            files += loaders.get(variant, [])
        files = tuple(sorted(set(files)))
        if not files:
            unmapped.append(folder.relative_to(game).as_posix())
            continue
        groups.setdefault(files, []).append(info)
    lines = ["# .info vs validator audit", "", f"Game: `{game}`", ""]
    summary = 0
    for files, infos in groups.items():
        documented, used, tops = set(), set(), set()
        for info in infos:
            documented |= info_names(info.read_text(encoding="utf-8-sig", errors="replace"))
            used |= vanilla_keys(info.parent)
            tops |= top_level_keys(info.parent)
        accepted = strings_of(files)
        new = sorted(
            n for n in documented
            if n not in accepted and n not in known and n not in GENERIC and n not in tops
        )
        stale = sorted(
            n for n in accepted
            if n not in documented and n not in used and n not in GENERIC and len(n) > 3 and "_" in n
        )
        if not new and not stale:
            continue
        summary += 1
        lines.append("## " + ", ".join(i.parent.relative_to(game).as_posix() for i in infos))
        lines.append("validator: " + ", ".join(p.name for p in files))
        if new:
            lines.append(f"- NEW ({len(new)}): " + ", ".join(f"`{n}`" for n in new))
        if stale:
            lines.append(f"- STALE ({len(stale)}): " + ", ".join(f"`{n}`" for n in stale))
        lines.append("")
    lines.insert(4, f"{summary} validator groups with suspects; {len(unmapped)} folders without a validator file found.")
    lines.append("## Folders without a matching validator file")
    lines += [f"- {u}" for u in unmapped]
    report = "\n".join(lines) + "\n"
    if out:
        out.write_text(report, encoding="utf-8")
        print(f"wrote {out}: {summary} validator groups with suspects, {len(unmapped)} unmapped")
    else:
        print(report)


if __name__ == "__main__":
    main()
