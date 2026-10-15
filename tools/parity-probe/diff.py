#!/usr/bin/env python3
"""Compare two parity probe traces and print the differences.

Usage:
    python3 -I tools/parity-probe/diff.py REFERENCE CANDIDATE
    python3 -I tools/parity-probe/diff.py --self-test

REFERENCE is the trace of the Java probe on NeoForge, CANDIDATE the trace of the Rust probe on
IronPumpkin. The trace format is in README.md. The script aligns the two event sequences by name
and ordinal, then compares the fields of each aligned pair. It prints one line per difference:

    missing  an event of the reference that the candidate does not have
    extra    an event of the candidate that the reference does not have
    moved    an event that both have with the same fields, at a different place in the order
    fields   an aligned pair whose fields differ

Exit status: 0 without differences, 1 with differences, 2 on an unreadable or invalid trace.
"""

import difflib
import io
import json
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
KEYS = ["ordinal", "name", "fields"]


class TraceError(Exception):
    pass


def parse(text, source):
    """Return the events of a trace as a list of (ordinal, name, fields)."""
    events = []
    for number, raw in enumerate(text.splitlines(), start=1):
        if not raw.strip():
            continue
        try:
            entry = json.loads(raw)
        except json.JSONDecodeError as err:
            raise TraceError(f"{source}:{number}: not JSON: {err}") from err
        if not isinstance(entry, dict) or sorted(entry) != sorted(KEYS):
            raise TraceError(f"{source}:{number}: keys must be {KEYS}")
        ordinal, name, fields = entry["ordinal"], entry["name"], entry["fields"]
        if ordinal != len(events):
            raise TraceError(f"{source}:{number}: ordinal {ordinal}, expected {len(events)}")
        if not isinstance(name, str) or not name:
            raise TraceError(f"{source}:{number}: name must be a non-empty string")
        if not isinstance(fields, dict):
            raise TraceError(f"{source}:{number}: fields must be an object")
        events.append((ordinal, name, fields))
    return events


def load(path):
    try:
        text = Path(path).read_text(encoding="utf-8")
    except OSError as err:
        raise TraceError(f"{path}: {err.strerror}") from err
    return parse(text, path)


def show(fields):
    return json.dumps(fields, sort_keys=True, separators=(",", ":"))


def place(events, ordinal):
    return f"after {events[ordinal - 1][1]}" if ordinal else "first"


def compare(reference, candidate):
    """Return the differences as (kind, text) tuples, in the order of the reference."""
    matcher = difflib.SequenceMatcher(
        None, [e[1] for e in reference], [e[1] for e in candidate], autojunk=False
    )
    missing, extra, changed = [], [], []
    for tag, r1, r2, c1, c2 in matcher.get_opcodes():
        if tag == "equal":
            for ref, cand in zip(reference[r1:r2], candidate[c1:c2]):
                if ref[2] != cand[2]:
                    changed.append((ref, cand))
        else:
            missing.extend(reference[r1:r2])
            extra.extend(candidate[c1:c2])

    moved = []
    for ref in list(missing):
        match = next((c for c in extra if c[1] == ref[1] and c[2] == ref[2]), None)
        if match is not None:
            missing.remove(ref)
            extra.remove(match)
            moved.append((ref, match))

    found = []
    for ref in missing:
        found.append((ref[0], "missing", f"reference #{ref[0]} {ref[1]} {show(ref[2])}"))
    for ref, cand in moved:
        found.append(
            (
                ref[0],
                "moved",
                f"{ref[1]}: reference #{ref[0]} {place(reference, ref[0])}, "
                f"candidate #{cand[0]} {place(candidate, cand[0])}",
            )
        )
    for ref, cand in changed:
        keys = sorted(set(ref[2]) | set(cand[2]))
        parts = [
            f"{key}: {json.dumps(ref[2].get(key))} != {json.dumps(cand[2].get(key))}"
            for key in keys
            if ref[2].get(key, ...) != cand[2].get(key, ...)
        ]
        found.append(
            (
                ref[0],
                "fields",
                f"{ref[1]}: reference #{ref[0]}, candidate #{cand[0]}: " + "; ".join(parts),
            )
        )
    found.sort(key=lambda item: item[0])
    found += [(None, "extra", f"candidate #{c[0]} {c[1]} {show(c[2])}") for c in extra]
    return [(kind, text) for _, kind, text in found]


def report(differences, out):
    for kind, text in differences:
        print(f"{kind:8} {text}", file=out)
    if differences:
        counts = {}
        for kind, _ in differences:
            counts[kind] = counts.get(kind, 0) + 1
        detail = ", ".join(f"{kind} {count}" for kind, count in counts.items())
        noun = "difference" if len(differences) == 1 else "differences"
        print(f"{len(differences)} {noun} ({detail})", file=out)
    else:
        print("no differences", file=out)


def trace(*events):
    return "\n".join(
        json.dumps({"ordinal": i, "name": name, "fields": fields}, separators=(",", ":"))
        for i, (name, fields) in enumerate(events)
    )


def self_test():
    a = ("A", {})
    b = ("B", {"registry": "minecraft:block"})
    c = ("C", {"type": "SYNCED", "enabled": True})
    cases = [
        ("identical traces", trace(a, b, c), trace(a, b, c), []),
        ("event the candidate lacks", trace(a, b, c), trace(a, c), ["missing"]),
        ("event the reference lacks", trace(a, c), trace(a, b, c), ["extra"]),
        ("event in another place", trace(a, b, c), trace(b, a, c), ["moved"]),
        (
            "one changed field",
            trace(a, b, c),
            trace(a, b, ("C", {"type": "SYNCED", "enabled": False})),
            ["fields"],
        ),
        (
            "one added field",
            trace(a, b, c),
            trace(("A", {"x": 1}), b, c),
            ["fields"],
        ),
    ]
    failures = 0
    for title, ref, cand, expected in cases:
        kinds = [kind for kind, _ in compare(parse(ref, "reference"), parse(cand, "candidate"))]
        if kinds != expected:
            print(f"FAIL {title}: {kinds} != {expected}")
            failures += 1

    for title, bad in [
        ("ordinal gap", '{"ordinal":1,"name":"A","fields":{}}'),
        ("missing key", '{"ordinal":0,"name":"A"}'),
        ("extra key", '{"ordinal":0,"name":"A","fields":{},"time":1}'),
        ("not JSON", "{"),
    ]:
        try:
            parse(bad, "bad")
        except TraceError:
            continue
        print(f"FAIL {title}: accepted")
        failures += 1

    # A changed field in the Rust probe: the committed Java trace against itself with one field
    # of one event changed reports exactly that one difference.
    java = HERE / "captures" / "neoforge-26.3.0.64-beta" / "probe-trace.jsonl"
    events = load(java)
    index = next(i for i, event in enumerate(events) if event[2])
    ordinal, name, fields = events[index]
    changed = list(events)
    changed[index] = (ordinal, name, dict(fields, **{min(fields): "changed"}))
    differences = compare(events, changed)
    if [kind for kind, _ in differences] != ["fields"]:
        print(f"FAIL changed field against {java.name}: {differences}")
        failures += 1

    # The diff of the committed traces is the committed diff.txt: a new capture of either side
    # comes with a new diff.txt.
    rust = HERE / "captures" / "ironpumpkin" / "probe-trace.jsonl"
    expected = (HERE / "captures" / "ironpumpkin" / "diff.txt").read_text(encoding="utf-8")
    out = io.StringIO()
    report(compare(events, load(rust)), out)
    if out.getvalue() != expected:
        print("FAIL committed captures: the diff differs from captures/ironpumpkin/diff.txt")
        failures += 1

    if failures:
        print(f"self-test: {failures} failures")
        return 1
    print("self-test: ok")
    return 0


def main(argv):
    if argv == ["--self-test"]:
        return self_test()
    if len(argv) != 2:
        print(__doc__.strip(), file=sys.stderr)
        return 2
    try:
        reference, candidate = load(argv[0]), load(argv[1])
    except TraceError as err:
        print(f"error: {err}", file=sys.stderr)
        return 2
    differences = compare(reference, candidate)
    report(differences, sys.stdout)
    return 1 if differences else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
