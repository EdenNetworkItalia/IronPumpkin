#!/usr/bin/env python3
"""Sync the Status column and the Coverage table of mapping-table.md with design.md.

design.md holds the hand-written statuses. mapping-table.md derives from it for the Status column
and the first three rows of the Coverage table only. Every other cell stays hand-written.

    python3 -I tools/openspec-sync/mapping_status.py            # write mapping-table.md
    python3 -I tools/openspec-sync/mapping_status.py --check    # write nothing, exit 1 on a mismatch

Standard library only. See tools/openspec-sync/README.md for the join rule and the Coverage rules.
"""

import argparse
import collections
import re
import sys
from pathlib import Path

DEFAULT_DIR = Path(__file__).resolve().parents[2] / "openspec/changes/neoforge-shaped-plugin-api"
STATUSES = ("supported", "planned", "client-only", "not supported")
# The inventory rows of mapping-table.md: it lists no client-only row.
COVERAGE = (
    ("Inventory, supported", "supported"),
    ("Inventory, planned", "planned"),
    ("Inventory, not supported", "not supported"),
)
SUMMARY_COLUMN = {"supported": "Supported", "planned": "Planned", "client-only": "Client-only",
                  "not supported": "Not supported"}

# cols holds the column numbers of Mods, Counterpart and Status of a mapping-table.md row.
Row = collections.namedtuple("Row", "line family key status cells cols", defaults=(None,))


class ParseError(Exception):
    pass


def split_row(line):
    return [c.strip() for c in re.split(r"(?<!\\)\|", line.strip()[1:-1])]


def tables(lines, start, end):
    """Yield (h3, h4, header, rows) for every pipe table in lines[start:end].

    h3 and h4 are the text of the nearest ### and #### headings. rows holds (line number, cells),
    line numbers start at 1.
    """
    h3 = h4 = ""
    i = start
    while i < end:
        line = lines[i]
        if line.startswith("## "):
            h3 = h4 = ""
        elif line.startswith("### "):
            h3, h4 = line[4:].strip(), ""
        elif line.startswith("#### "):
            h4 = line[5:].strip()
        if line.startswith("|") and i + 1 < end and re.fullmatch(r"\|[:\- |]+\|", lines[i + 1]):
            header = split_row(line)
            rows = []
            j = i + 2
            while j < end and lines[j].startswith("|"):
                cells = split_row(lines[j])
                if len(cells) != len(header):
                    raise ParseError(f"line {j + 1}: {len(cells)} cells, the header has {len(header)}")
                rows.append((j + 1, cells))
                j += 1
            yield h3, h4, header, rows
            i = j
            continue
        i += 1


def section(lines, heading):
    """Return (start, end) of the ## section that begins with heading."""
    start = next((i for i, l in enumerate(lines) if l.startswith(heading)), None)
    if start is None:
        raise ParseError(f"heading not found: {heading}")
    end = next((i for i in range(start + 1, len(lines)) if lines[i].startswith("## ")), len(lines))
    return start, end


def column(header, name, where):
    if name not in header:
        raise ParseError(f"{where}: the table has no {name} column")
    return header.index(name)


def set_cell(line, index, text):
    """Return line with the cell at index replaced by text. The other cells stay byte for byte."""
    line = line.rstrip()
    bars = [m.start() for m in re.finditer(r"(?<!\\)\|", line)]
    return line[: bars[index] + 1] + f" {text} " + line[bars[index + 1]:]


def clean_key(cell):
    """Strip backticks, a trailing "(abstract)" and a trailing package suffix "(`pkg`)"."""
    key = re.sub(r"^`(.*)`$", r"\1", cell.strip())
    return re.fullmatch(r"(.*?)(?: \(abstract\))?(?: \(`[^`]+`\))?", key).group(1)


def check_status(status, where):
    if status not in STATUSES:
        raise ParseError(f"{where}: unknown status {status!r}")


def summary_total(header, line, cells):
    try:
        return dict(zip(header[1:], (int(v) for v in cells[1:])))
    except ValueError as e:
        raise ParseError(f"design.md line {line}: the Total row has a cell that is not a number") from e


def design_rows(lines):
    """Return every inventory row of design.md and the Total row of Summary per family."""
    start, end = section(lines, "## Inventory")
    rows, total = [], None
    for family, _h4, header, body in tables(lines, start, end):
        if header[:2] == ["Family", "Rows"]:
            total = next((summary_total(header, line, c) for line, c in body if c[0] == "Total"), None)
        elif "Status" in header:
            status_col = header.index("Status")
            for line, cells in body:
                key = f"{cells[0]} {clean_key(cells[1])}" if header[:2] == ["Section", "Key"] else clean_key(cells[0])
                check_status(cells[status_col], f"design.md line {line}")
                rows.append(Row(line, family, key, cells[status_col], cells))
    return rows, total


def mapping_rows(lines):
    """Return every inventory row of mapping-table.md."""
    start, end = section(lines, "## Inventory rows")
    rows = []
    for family, _h4, header, body in tables(lines, start, end):
        if header[0] != "NeoForge item":
            continue
        where = f"mapping-table.md, table [{family}]"
        cols = tuple(column(header, name, where) for name in ("Mods", "Counterpart", "Status"))
        for line, cells in body:
            check_status(cells[cols[2]], f"mapping-table.md line {line}")
            rows.append(Row(line, family, clean_key(cells[0]), cells[cols[2]], cells, cols))
    return rows


def coverage_cells(rows):
    """Return [rows, counterpart, partial, contract only, gap or not supported, 1+ mods, 50+ mods].

    rows holds (status, counterpart text, mods cell). The columns follow the legend of the file:
    a not supported row has no counterpart (a gap); "contract only" and "partial" come from the
    counterpart text; every other row has a counterpart.
    """
    n = [0] * 7
    for status, counterpart, mods in rows:
        text = counterpart.lower()
        if "contract only" in text:
            column = 3
        elif "partial" in text:
            column = 2
        elif status == "not supported":
            column = 4
        else:
            column = 1
        count = re.match(r"\d+", mods)  # "52 (21.1 `BlockEvent$BreakEvent`)" counts 52
        count = int(count.group()) if count else 0
        n[0] += 1
        n[column] += 1
        n[5] += count >= 1
        n[6] += count >= 50
    return n


def sync(design_lines, mapping_lines):
    """Return (new mapping-table.md lines, report)."""
    drows, total = design_rows(design_lines)
    mrows = mapping_rows(mapping_lines)

    design = collections.defaultdict(list)
    for r in drows:
        design[(r.family, r.key)].append(r)
    mapping = collections.Counter((r.family, r.key) for r in mrows)

    unmatched = []  # (file, line, family, key, reason)
    new_lines = list(mapping_lines)
    changes = collections.Counter()
    final = collections.defaultdict(list)  # status -> (status, counterpart, mods) of each mapping row
    for m in mrows:
        found = design.get((m.family, m.key), [])
        status, reason = m.status, None
        if mapping[(m.family, m.key)] > 1:
            reason = "duplicate key in mapping-table.md"
        elif not found:
            reason = "no design.md row with this key"
        elif len(found) > 1:
            reason = "duplicate key in design.md"
        elif found[0].status == "client-only":
            reason = "design.md status is client-only, mapping-table.md lists no client-only row"
        if reason:
            unmatched.append(("mapping-table.md", m.line, m.family, m.key, reason))
        elif found[0].status != m.status:
            status = found[0].status
            changes[(m.status, status)] += 1
            new_lines[m.line - 1] = set_cell(mapping_lines[m.line - 1], m.cols[2], status)
        final[status].append((status, m.cells[m.cols[1]], m.cells[m.cols[0]]))

    client_only = 0
    for key, rows in design.items():
        if key in mapping:
            continue
        if len(rows) > 1:
            unmatched.append(("design.md", rows[0].line, key[0], key[1], "duplicate key in design.md"))
        elif rows[0].status == "client-only":
            client_only += 1
        else:
            unmatched.append(("design.md", rows[0].line, key[0], key[1], "no mapping-table.md row with this key"))

    coverage = {label: coverage_cells(final[status]) for label, status in COVERAGE}
    start, end = section(new_lines, "## Coverage")
    done = set()
    for _h3, _h4, header, body in tables(new_lines, start, end):
        if header[0] != "Part":
            continue
        for line, cells in body:
            if cells[0] in coverage:
                done.add(cells[0])
                new_lines[line - 1] = "| " + " | ".join([cells[0]] + [str(v) for v in coverage[cells[0]]]) + " |"
    if done != set(coverage):
        raise ParseError("Coverage table has no row for: " + ", ".join(sorted(set(coverage) - done)))

    design_counts = collections.Counter(r.status for r in drows)
    problems = []
    if total is None:
        problems.append("design.md has no Total row in Summary per family")
    else:
        for status, column in SUMMARY_COLUMN.items():
            if design_counts[status] != total[column]:
                problems.append(f"design.md has {design_counts[status]} {status} rows, "
                                f"Summary per family has {total[column]}")
        for label, status in COVERAGE:
            if coverage[label][0] != total[SUMMARY_COLUMN[status]]:
                problems.append(f"Coverage has {coverage[label][0]} {status} rows, "
                                f"Summary per family has {total[SUMMARY_COLUMN[status]]}")
    return new_lines, {"changes": changes, "unmatched": unmatched, "client_only": client_only,
                       "matched": len(mrows) - sum(u[0] == "mapping-table.md" for u in unmatched),
                       "coverage": coverage, "problems": problems}


def print_report(report):
    print("status changes in mapping-table.md:")
    for (old, new), n in sorted(report["changes"].items()):
        print(f"  {old} -> {new}: {n}")
    if not report["changes"]:
        print("  none")
    print(f"matched mapping rows: {report['matched']}")
    print(f"design.md client-only rows with no mapping row (by design): {report['client_only']}")
    print(f"unmatched rows: {len(report['unmatched'])}")
    for file, line, family, key, reason in report["unmatched"]:
        print(f"  {file}:{line} [{family}] {key}: {reason}")
    print("Coverage (rows, counterpart, partial, contract only, gap or not supported, 1+ mods, 50+ mods):")
    for label, cells in report["coverage"].items():
        print(f"  {label}: {', '.join(str(c) for c in cells)}")
    for p in report["problems"]:
        print(f"problem: {p}")


def read_lines(path):
    with open(path, encoding="utf-8", newline="") as f:
        return f.read().split("\n")


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--check", action="store_true", help="write nothing, exit 1 when the files disagree")
    ap.add_argument("--dir", type=Path, default=DEFAULT_DIR, help="folder with design.md and mapping-table.md")
    args = ap.parse_args(argv)

    path = args.dir / "mapping-table.md"
    try:
        old_lines = read_lines(path)
        new_lines, report = sync(read_lines(args.dir / "design.md"), old_lines)
    except (OSError, ParseError) as e:
        print(f"error: {e}", file=sys.stderr)
        return 2

    print_report(report)
    stale = new_lines != old_lines
    if stale and not args.check:
        with open(path, "w", encoding="utf-8", newline="") as f:
            f.write("\n".join(new_lines))
        print(f"wrote {path}")
    elif stale:
        print("check: mapping-table.md differs from the result of the sync")
    unresolved = bool(report["unmatched"] or report["problems"])
    if args.check:
        print("check: FAIL" if stale or unresolved else "check: OK")
    return 1 if (stale and args.check) or unresolved else 0


if __name__ == "__main__":
    sys.exit(main())
