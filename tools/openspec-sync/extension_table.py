#!/usr/bin/env python3
"""Write the extension interface rows of design.md and mapping-table.md, one row per method.

    python3 -I tools/openspec-sync/extension_table.py --neoforge DIR            # write both files
    python3 -I tools/openspec-sync/extension_table.py --neoforge DIR --check    # write nothing

DIR is the root of the NeoForge sources (the folder that holds `net/` and `META-INF/`). The methods
come from `net/neoforged/neoforge/common/extensions`. The statuses, sides, concepts and reasons are
hand-written in `extension_rows.tsv`. The demand per method comes from
`tools/modpack-scan/extension_demand.tsv` (written by `extension_scan.py`). Standard library only.
See tools/openspec-sync/README.md.
"""

import argparse
import collections
import re
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import extension_methods as em  # noqa: E402

REPO = HERE.parents[1]
DEFAULT_DIR = REPO / "openspec/changes/neoforge-shaped-plugin-api"
HEADING = "### Extension interfaces NeoForge adds to `net.minecraft`"
FAMILY = "Extension interfaces"
STATUSES = ("supported", "planned", "client-only", "not supported")
ROW_FIELDS = ("key", "status", "side", "concept", "reason", "test")


class TableError(Exception):
    pass


def read_rows(path):
    """Return {key: dict} from the hand-written TSV. Lines that start with # are comments."""
    rows = {}
    for n, line in enumerate(Path(path).read_text(encoding="utf-8").splitlines(), 1):
        if not line.strip() or line.startswith("#"):
            continue
        cells = line.split("\t")
        if len(cells) != len(ROW_FIELDS):
            raise TableError(f"{path} line {n}: {len(cells)} cells, expected {len(ROW_FIELDS)}")
        row = dict(zip(ROW_FIELDS, (c.strip() for c in cells)))
        if row["key"] in rows:
            raise TableError(f"{path} line {n}: duplicate key {row['key']}")
        if row["status"] not in STATUSES:
            raise TableError(f"{path} line {n}: unknown status {row['status']!r}")
        if not row["reason"]:
            raise TableError(f"{path} line {n}: empty reason for {row['key']}")
        if row["status"] == "supported" and not row["test"]:
            raise TableError(f"{path} line {n}: a supported row needs a test")
        rows[row["key"]] = row
    return rows


def read_demand(path):
    """Return {(interface, method): set of mods} from extension_scan.py (call or override)."""
    demand = {}
    lines = Path(path).read_text(encoding="utf-8").splitlines()
    for line in lines[1:]:
        iface, name, called, overridden = line.split("\t")
        mods = {m for m in (called + ";" + overridden).split(";") if m}
        demand[(iface, name)] = demand.get((iface, name), set()) | mods
    return demand


def read_moves(path):
    """Return {26.3 key: [21.1 keys]} from extension_moves.tsv."""
    moves = {}
    for n, line in enumerate(Path(path).read_text(encoding="utf-8").splitlines(), 1):
        if not line.strip() or line.startswith("#"):
            continue
        cells = line.split("\t")
        if len(cells) != 2:
            raise TableError(f"{path} line {n}: expected 2 cells")
        moves[cells[0].strip()] = [k.strip() for k in cells[1].split(";") if k.strip()]
    return moves


def demand_of(demand, moves, key):
    """The number of mods that use `key` (Interface#method) or a 21.1 method that it replaces."""
    mods = set()
    for k in [key] + moves.get(key, []):
        iface, _, method = k.partition("#")
        mods |= demand.get((iface, method), set())
    return len(mods)


def test_names(crates):
    """The set of function names in the Rust sources under crates (for the test check)."""
    names = set()
    for path in Path(crates).rglob("*.rs"):
        names.update(re.findall(r"\bfn (\w+)", path.read_text(encoding="utf-8", errors="replace")))
    return names


def inventory(neoforge):
    """Return the interfaces in name order: dicts with name, target, rows (one per method name)."""
    root = Path(neoforge)
    injected = em.load_injected(root)
    out = []
    for path in sorted((root / em.EXTENSIONS).glob("*.java")):
        parsed = em.parse_interface(path)
        if parsed["name"] == "package-info":
            continue
        targets = injected.get(parsed["name"])
        entry = {"name": parsed["name"], "target": ", ".join(targets) if targets else None,
                 "methods": em.groups(parsed)}
        out.append(entry)
        for nested in parsed["nested"]:
            if em.public_methods(nested):
                raise TableError(f"{parsed['name']}.{nested['name']} declares public methods: "
                                 "the script reads only the top-level interface")
            out.append({"name": f"{parsed['name']}.{nested['name']}", "target": None, "methods": []})
    return out


def lookup(rows, key, iface):
    return rows.get(key) or rows.get(f"{iface}#*")


def build(neoforge, rows, demand, moves, tests):
    """Return the list of row dicts, in inventory order, and the used keys."""
    result, used = [], set()
    for iface in inventory(neoforge):
        name = iface["name"]
        entries = iface["methods"]
        if not entries:
            row = rows.get(name)
            if row is None:
                raise TableError(f"no row for the interface {name} in extension_rows.tsv")
            used.add(name)
            result.append({**row, "iface": name, "target": iface["target"], "method": None,
                           "kind": "interface, no public method", "mods": 0, "overload_kinds": []})
            continue
        for mname, kind, overloads in entries:
            key = f"{name}#{mname}"
            row = lookup(rows, key, name)
            if row is None:
                raise TableError(f"no row for {key} and no {name}#* default in extension_rows.tsv")
            used.add(row["key"])
            label = kind
            result.append({**row, "iface": name, "target": iface["target"], "method": mname,
                           "kind": label, "key": key, "mods": demand_of(demand, moves, key),
                           "overload_kinds": [em.kind(m).split()[0] for m in overloads]})
    for key in rows:
        if key not in used:
            raise TableError(f"extension_rows.tsv has the key {key}, which matches no method")
    for row in result:
        if row["status"] == "supported":
            for t in row["test"].split(";"):
                if t.strip() not in tests:
                    raise TableError(f"{row['key']}: test {t.strip()!r} is not a function in crates/")
    return result


def counts(rows):
    c = collections.Counter(r["status"] for r in rows)
    return [len(rows)] + [c[s] for s in ("supported", "planned", "client-only", "not supported")]


def rendered_counts(rows):
    c = collections.Counter(r["status"] for r in rows)
    parts = [f"{c[s]} {s}" for s in STATUSES if c[s]]
    return ", ".join(parts)


def esc(text):
    return text.replace("|", "\\|")


def design_block(rows):
    kinds = collections.Counter(k for r in rows for k in r["overload_kinds"])
    named = [r for r in rows if r["method"]]
    by_iface = collections.OrderedDict()
    for r in rows:
        by_iface.setdefault(r["iface"], []).append(r)
    with_methods = sum(1 for g in by_iface.values() if g[0]["method"])
    out = [HEADING, "",
           "One row per public method name of each interface, from "
           "`net/neoforged/neoforge/common/extensions` (overloads are counted in the Kind column). "
           "Targets come from `src/main/resources/META-INF/injected-interfaces.json`. "
           "`python3 -I tools/openspec-sync/extension_table.py` writes these tables and the "
           "family of `mapping-table.md` from the NeoForge sources and "
           "`tools/openspec-sync/extension_rows.tsv`: edit the TSV, not the tables. "
           "A `supported` row names the Pumpkin trait method or function and a test. A `planned` row "
           "with the prefix `M5 engine extension:` waits for an engine call site; the other "
           "`planned` rows wait for a mod API accessor.", ""]
    total = sum(kinds.values())
    out.append(f"{total} public methods in {with_methods} interfaces: {kinds['default']} default, "
               f"{kinds['abstract']} abstract, {kinds['static']} static. They form {len(named)} "
               f"rows, one per method name. {len([r for r in rows if not r['method']])} more rows are types "
               "without a public method. "
               "The two largest, `IBlockExtension` and "
               "`IBlockStateExtension`, hold the block hooks that ported blocks override most.")
    out.append("")
    empty = [r for r in rows if not r["method"]]
    for iface, group in by_iface.items():
        if not group[0]["method"]:
            continue
        target = group[0]["target"]
        where = f"target `{target}`" if target else "not injected (used through a cast or a patch)"
        out += [f"#### `{iface}` ({where}; {len(group)} {"row" if len(group) == 1 else "rows"}: {rendered_counts(group)})", "",
                "| Method | Kind | Side | Status | Pumpkin concept | Reason |",
                "|:--|:--|:--|:--|:--|:--|"]
        for r in group:
            out.append(f"| `{r['key']}` | {r['kind']} | {r['side']} | {r['status']} | "
                       f"{esc(r['concept'])} | {esc(r['reason'])} |")
        out.append("")
    if empty:
        out += [f"#### Types without a public method ({len(empty)}: {rendered_counts(empty)})", "",
                "| Interface | Kind | Side | Status | Pumpkin concept | Reason |",
                "|:--|:--|:--|:--|:--|:--|"]
        for r in empty:
            out.append(f"| `{r['iface']}` | {r['kind']} | {r['side']} | {r['status']} | "
                       f"{esc(r['concept'])} | {esc(r['reason'])} |")
        out.append("")
    return out


def mapping_block(rows):
    ordered = sorted(((i, r) for i, r in enumerate(rows) if r["status"] != "client-only"),
                     key=lambda p: (-p[1]["mods"], p[0]))
    out = [HEADING, "", "| NeoForge item | Mods | Section | Counterpart | Status |",
           "|:--|--:|:--|:--|:--|"]
    for _, r in ordered:
        key = r["key"] if r["method"] else r["iface"]
        section = f"`{r['iface']}`" + (f" ({r['target']})" if r["target"] else "")
        out.append(f"| `{key}` | {r['mods']} | {esc(section)} | {esc(r['concept'])} | {r['status']} |")
    out.append("")
    return out


def replace_block(lines, new):
    start = next((i for i, l in enumerate(lines) if l.strip() == HEADING), None)
    if start is None:
        raise TableError(f"heading not found: {HEADING}")
    end = next((i for i in range(start + 1, len(lines)) if lines[i].startswith(("### ", "## "))),
               len(lines))
    return lines[:start] + new + lines[end:]


def update_summary(lines, family_counts):
    """Write the family row and recompute the Total row of Summary per family."""
    fam = next((i for i, l in enumerate(lines) if l.startswith(f"| {FAMILY} |")), None)
    tot = next((i for i, l in enumerate(lines) if l.startswith("| Total |")), None)
    if fam is None or tot is None:
        raise TableError("Summary per family has no Extension interfaces or Total row")
    lines[fam] = f"| {FAMILY} | " + " | ".join(map(str, family_counts)) + " |"
    start = fam
    while lines[start - 1].startswith("|"):
        start -= 1
    body = [l for l in lines[start:tot] if re.match(r"\| [^|]+ \| \d", l)]
    sums = [sum(int(re.split(r"\s*\|\s*", l.strip("| \n"))[k]) for l in body) for k in range(1, 6)]
    lines[tot] = "| Total | " + " | ".join(map(str, sums)) + " |"
    return lines


def run(directory, neoforge, rows_path, demand_path, moves_path, crates, check):
    rows = build(neoforge, read_rows(rows_path), read_demand(demand_path),
                 read_moves(moves_path), test_names(crates))
    design_path, mapping_path = Path(directory) / "design.md", Path(directory) / "mapping-table.md"
    design = design_path.read_text(encoding="utf-8").split("\n")
    mapping = mapping_path.read_text(encoding="utf-8").split("\n")
    new_design = "\n".join(update_summary(replace_block(design, design_block(rows)), counts(rows)))
    new_mapping = "\n".join(replace_block(mapping, mapping_block(rows)))
    changed = []
    for path, text in ((design_path, new_design), (mapping_path, new_mapping)):
        if path.read_text(encoding="utf-8") != text:
            changed.append(path.name)
            if not check:
                path.write_text(text, encoding="utf-8")
    print(f"{len(rows)} rows: " + rendered_counts(rows))
    print("changed: " + (", ".join(changed) if changed else "nothing"))
    return 1 if check and changed else 0


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--neoforge", required=True, help="root of the NeoForge sources")
    ap.add_argument("--dir", default=str(DEFAULT_DIR))
    ap.add_argument("--rows", default=str(HERE / "extension_rows.tsv"))
    ap.add_argument("--demand", default=str(REPO / "tools/modpack-scan/extension_demand.tsv"))
    ap.add_argument("--moves", default=str(REPO / "tools/modpack-scan/extension_moves.tsv"))
    ap.add_argument("--crates", default=str(REPO / "crates"))
    ap.add_argument("--check", action="store_true")
    args = ap.parse_args(argv)
    try:
        return run(args.dir, args.neoforge, args.rows, args.demand, args.moves, args.crates, args.check)
    except TableError as e:
        print(f"error: {e}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
