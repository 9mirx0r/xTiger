"""Run Tiger over the game's own files and group what it reports.

The game loads its own files without complaint, so every report Tiger makes about a vanilla file is either a
Tiger bug (a false positive that mod authors will hit too) or a real flaw of the game. This is the cheapest
way to find "the validator is not updated to the new game version" problems before a mod author does.

    python -I tools/vanilla_selfcheck.py <ck3-tiger binary> "<game root>" <out dir> [severity,list] [path,prefixes]

It writes `<out dir>/vanilla_raw.txt` (the complete output) and `<out dir>/vanilla_groups.md`: the reports
grouped by key and message with two examples each, biggest group first. Default severities: error,fatal.
Compare two runs with `diff` after a validator change: the groups that disappear are the fixed ones, and
anything new is a regression.
"""
import collections
import re
import subprocess
import sys
from pathlib import Path


def main():
    binary, game, out = sys.argv[1], sys.argv[2], Path(sys.argv[3])
    severities = (sys.argv[4] if len(sys.argv) > 4 else "error,fatal").split(",")
    prefixes = (sys.argv[5] if len(sys.argv) > 5 else "").split(",")
    out.mkdir(parents=True, exist_ok=True)
    mod = out / "emptymod"
    (mod / "common").mkdir(parents=True, exist_ok=True)
    modfile = out / "empty.mod"
    modfile.write_text(
        'name="empty"\nversion="1"\nsupported_version="1.20.*"\n' f'path="{mod.as_posix()}"\n', encoding="utf-8"
    )
    raw = out / "vanilla_raw.txt"
    with raw.open("wb") as handle:
        subprocess.run(
            [binary, "--game", game, "--show-vanilla", "--no-color", str(modfile)],
            stdout=handle,
            stderr=subprocess.STDOUT,
            check=False,
        )
    text = raw.read_text(encoding="utf-8", errors="replace")
    pattern = r"^(fatal|error|warning|untidy|tips)\(([\w-]+)\): (.*)\n\s*--> \[CK3\] ([^\n]+)\n(\d+) \|([^\n]*)"
    groups = collections.defaultdict(list)
    for sev, key, msg, path, line, src in re.findall(pattern, text, re.M):
        if sev in severities and any(path.startswith(p) for p in prefixes):
            groups[(sev, key, re.sub(r"`[^`]*`", "`X`", msg)[:120])].append((path, line, src.strip()[:90]))
    lines = [f"# Vanilla self-check ({','.join(severities)})", "", f"Binary: `{binary}`", ""]
    for (sev, key, msg), items in sorted(groups.items(), key=lambda kv: -len(kv[1])):
        lines.append(f"## {len(items)} x {sev}({key}): {msg}")
        for path, line, src in items[:2]:
            lines.append(f"- `{path}:{line}`  `{src}`")
        lines.append("")
    report = out / "vanilla_groups.md"
    report.write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(f"{sum(len(v) for v in groups.values())} reports in {len(groups)} groups -> {report}")


if __name__ == "__main__":
    main()
