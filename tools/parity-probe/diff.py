#!/usr/bin/env python3
"""Compare two parity probe traces and print the differences.

Usage:
    python3 -I tools/parity-probe/diff.py REFERENCE CANDIDATE
    python3 -I tools/parity-probe/diff.py --slice2 REFERENCE CANDIDATE
    python3 -I tools/parity-probe/diff.py --self-test

REFERENCE is the trace of the Java probe on NeoForge, CANDIDATE the trace of the Rust probe on
IronPumpkin. The trace format is in README.md. The script aligns the two event sequences by name
and ordinal, then compares the fields of each aligned pair. It prints one line per difference:

    missing  an event of the reference that the candidate does not have
    extra    an event of the candidate that the reference does not have
    moved    an event that both have with the same fields, at a different place in the order
    fields   an aligned pair whose fields differ

With --slice2, only the events with the field `step` (the index of the script step) count. The
script compares them step by step, and adds one kind:

    step     an event that both have with the same fields, in another step

Exit status: 0 without differences, 1 with differences, 2 on an unreadable or invalid trace.
"""

import contextlib
import difflib
import io
import json
import sys
import tempfile
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
        # `type(..) is int` rejects a bool, which Python compares equal to 0 and 1, and a float.
        if type(ordinal) is not int:
            raise TraceError(f"{source}:{number}: ordinal must be an integer")
        if ordinal != len(events):
            raise TraceError(f"{source}:{number}: ordinal {ordinal}, expected {len(events)}")
        if not isinstance(name, str) or not name:
            raise TraceError(f"{source}:{number}: name must be a non-empty string")
        if not isinstance(fields, dict):
            raise TraceError(f"{source}:{number}: fields must be an object")
        for key, value in fields.items():
            if not isinstance(value, (str, bool, int)):
                raise TraceError(
                    f"{source}:{number}: field {key!r} must be a string, a boolean or an integer"
                )
        events.append((ordinal, name, fields))
    return events


def load(path):
    try:
        text = Path(path).read_text(encoding="utf-8")
    except OSError as err:
        raise TraceError(f"{path}: {err.strerror}") from err
    except UnicodeDecodeError as err:
        raise TraceError(f"{path}: not UTF-8 ({err.reason} at byte {err.start})") from err
    return parse(text, path)


def show(fields):
    """The canonical text of the fields. Two events have the same fields when this text is equal:
    Python compares True with 1, the text does not."""
    return json.dumps(fields, sort_keys=True, separators=(",", ":"))


def place(events, event):
    """The place of `event` in `events`: the name of the event before it, or "first". In a slice 2
    comparison `events` holds the events of one step, so the place is inside the step."""
    index = events.index(event)
    return f"after {events[index - 1][1]}" if index else "first"


def compare(reference, candidate, prefix=""):
    """Return the differences as (kind, text) tuples, in the order of the reference. `prefix`
    starts each text."""
    matcher = difflib.SequenceMatcher(
        None, [e[1] for e in reference], [e[1] for e in candidate], autojunk=False
    )
    missing, extra, changed = [], [], []
    for tag, r1, r2, c1, c2 in matcher.get_opcodes():
        if tag == "equal":
            for ref, cand in zip(reference[r1:r2], candidate[c1:c2]):
                if show(ref[2]) != show(cand[2]):
                    changed.append((ref, cand))
        else:
            missing.extend(reference[r1:r2])
            extra.extend(candidate[c1:c2])

    moved = []
    for ref in list(missing):
        match = next((c for c in extra if c[1] == ref[1] and show(c[2]) == show(ref[2])), None)
        if match is not None:
            missing.remove(ref)
            extra.remove(match)
            moved.append((ref, match))

    found = []
    for ref in missing:
        found.append((ref[0], "missing", f"{prefix}reference #{ref[0]} {ref[1]} {show(ref[2])}"))
    for ref, cand in moved:
        found.append(
            (
                ref[0],
                "moved",
                f"{prefix}{ref[1]}: reference #{ref[0]} {place(reference, ref)}, "
                f"candidate #{cand[0]} {place(candidate, cand)}",
            )
        )
    for ref, cand in changed:
        # A parsed trace has no null value, so a field that an event lacks shows as null.
        keys = sorted(set(ref[2]) | set(cand[2]))
        texts = [(key, json.dumps(ref[2].get(key)), json.dumps(cand[2].get(key))) for key in keys]
        parts = [f"{key}: {old} != {new}" for key, old, new in texts if old != new]
        found.append(
            (
                ref[0],
                "fields",
                f"{prefix}{ref[1]}: reference #{ref[0]}, candidate #{cand[0]}: "
                + "; ".join(parts),
            )
        )
    found.sort(key=lambda item: item[0])
    found += [(None, "extra", f"{prefix}candidate #{c[0]} {c[1]} {show(c[2])}") for c in extra]
    return [(kind, text) for _, kind, text in found]


def steps(events, source):
    """Group the events that have the field `step` by its value, without the field. The other
    events belong to slice 1 and are left out."""
    groups = {}
    for ordinal, name, fields in events:
        if "step" not in fields:
            continue
        step = fields["step"]
        if type(step) is not int or step < 0:
            raise TraceError(f"{source}: event #{ordinal}: step must be a non-negative integer")
        rest = {key: value for key, value in fields.items() if key != "step"}
        groups.setdefault(step, []).append((ordinal, name, rest))
    return groups


def compare_steps(reference, candidate):
    """Compare two slice 2 traces step by step. An event that one step misses and another step
    has with the same name and fields is one `step` difference instead of a missing and an extra."""
    found = []
    for step in sorted(set(reference) | set(candidate)):
        ref, cand = reference.get(step, []), candidate.get(step, [])
        for kind, text in compare(ref, cand, f"step {step} "):
            found.append([step, kind, text])
        # compare() writes the event of a missing or extra line as "<name> <fields>" at the end.
        for entry in found:
            if entry[0] == step and entry[1] in ("missing", "extra"):
                entry.append(entry[2].split(" ", 4)[4])

    for entry in found:
        if entry[1] != "missing":
            continue
        match = next(
            (o for o in found if o[1] == "extra" and o[0] != entry[0] and o[3] == entry[3]),
            None,
        )
        if match is not None:
            name = entry[3].split(" ", 1)[0]
            entry[1:] = ["step", f"{name}: reference step {entry[0]}, candidate step {match[0]}"]
            match[1] = None
    return [(entry[1], entry[2]) for entry in found if entry[1] is not None]


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
        # Python compares True with 1 and False with 0, the rendered fields do not.
        (
            "true against 1",
            trace(("A", {"x": True})),
            trace(("A", {"x": 1})),
            ["fields"],
        ),
        (
            "false against 0",
            trace(("A", {"x": False})),
            trace(("A", {"x": 0})),
            ["fields"],
        ),
        (
            "true against 1 in another place",
            trace(a, ("B", {"x": True})),
            trace(("B", {"x": 1}), a),
            ["missing", "extra"],
        ),
        (
            "equal integers in another place",
            trace(a, ("B", {"x": 1})),
            trace(("B", {"x": 1}), a),
            ["moved"],
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
        ("boolean ordinal", '{"ordinal":false,"name":"A","fields":{}}'),
        ("float ordinal", '{"ordinal":0.0,"name":"A","fields":{}}'),
        ("float field", '{"ordinal":0,"name":"A","fields":{"x":1.5}}'),
        ("null field", '{"ordinal":0,"name":"A","fields":{"x":null}}'),
        ("list field", '{"ordinal":0,"name":"A","fields":{"x":[1]}}'),
        ("object field", '{"ordinal":0,"name":"A","fields":{"x":{}}}'),
    ]:
        try:
            parse(bad, "bad")
        except TraceError:
            continue
        print(f"FAIL {title}: accepted")
        failures += 1

    # A trace that is not UTF-8 is an invalid trace (exit status 2), not a crash.
    with tempfile.TemporaryDirectory() as scratch:
        binary = Path(scratch) / "binary.jsonl"
        binary.write_bytes(b'{"ordinal":0,"name":"\xff","fields":{}}\n')
        with contextlib.redirect_stderr(io.StringIO()):
            status = main([str(binary), str(binary)])
        if status != 2:
            print(f"FAIL trace that is not UTF-8: exit status {status}, expected 2")
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

    failures += self_test_slice2()

    if failures:
        print(f"self-test: {failures} failures")
        return 1
    print("self-test: ok")
    return 0


def self_test_slice2():
    """The slice 2 comparison: per step, a `step` difference, the events without a step left
    out, an invalid step, a changed field in the committed Java trace, and the committed slice 2
    captures against diff-slice2.txt."""

    def at(step, name, **fields):
        return (name, dict(fields, step=step))

    join = at(0, "PlayerEvent.PlayerLoggedInEvent")
    chat = at(1, "ServerChatEvent", raw_text="hello")
    leave = at(2, "PlayerEvent.PlayerLoggedOutEvent")
    start = ("ServerStartedEvent", {})
    cases = [
        ("identical slice 2 traces", trace(start, join, chat, leave), trace(join, chat, leave), []),
        ("event in another step", trace(join, chat, leave),
         trace(join, at(2, "ServerChatEvent", raw_text="hello"), leave), ["step"]),
        ("event a step lacks", trace(join, chat, leave), trace(join, leave), ["missing"]),
        ("changed field in a step", trace(join, chat, leave),
         trace(join, at(1, "ServerChatEvent", raw_text="other"), leave), ["fields"]),
        ("order inside a step", trace(join, chat, at(1, "X"), leave),
         trace(join, at(1, "X"), chat, leave), ["moved"]),
    ]
    failures = 0
    for title, ref, cand, expected in cases:
        differences = compare_steps(
            steps(parse(ref, "reference"), "reference"),
            steps(parse(cand, "candidate"), "candidate"),
        )
        if [kind for kind, _ in differences] != expected:
            print(f"FAIL {title}: {differences} != {expected}")
            failures += 1

    for title, bad in [
        ("text step", trace(("A", {"step": "1"}))),
        ("negative step", trace(("A", {"step": -1}))),
        ("boolean step", trace(("A", {"step": True}))),
    ]:
        try:
            steps(parse(bad, "bad"), "bad")
        except TraceError:
            continue
        print(f"FAIL {title}: accepted")
        failures += 1

    # A changed field of one play event of the committed Java trace is exactly one difference.
    captures = HERE / "captures"
    java = captures / "neoforge-26.3.0.64-beta" / "probe-trace-slice2.jsonl"
    events = load(java)
    index = next(i for i, event in enumerate(events) if len(event[2]) > 1 and "step" in event[2])
    ordinal, name, fields = events[index]
    key = min(key for key in fields if key != "step")
    changed = list(events)
    changed[index] = (ordinal, name, dict(fields, **{key: "changed"}))
    differences = compare_steps(steps(events, java), steps(changed, java))
    if [kind for kind, _ in differences] != ["fields"]:
        print(f"FAIL changed field against {java.name}: {differences}")
        failures += 1

    expected = (captures / "ironpumpkin" / "diff-slice2.txt").read_text(encoding="utf-8")
    out = io.StringIO()
    report(
        compare_slice2(
            captures / "neoforge-26.3.0.64-beta" / "probe-trace-slice2.jsonl",
            captures / "ironpumpkin" / "probe-trace-slice2.jsonl",
        ),
        out,
    )
    if out.getvalue() != expected:
        print("FAIL committed slice 2 captures: the diff differs from diff-slice2.txt")
        failures += 1
    return failures


def compare_slice2(reference, candidate):
    return compare_steps(steps(load(reference), reference), steps(load(candidate), candidate))


def main(argv):
    if argv == ["--self-test"]:
        return self_test()
    slice2 = argv[:1] == ["--slice2"]
    if slice2:
        argv = argv[1:]
    if len(argv) != 2:
        print(__doc__.strip(), file=sys.stderr)
        return 2
    try:
        if slice2:
            differences = compare_slice2(argv[0], argv[1])
        else:
            differences = compare(load(argv[0]), load(argv[1]))
    except TraceError as err:
        print(f"error: {err}", file=sys.stderr)
        return 2
    report(differences, sys.stdout)
    return 1 if differences else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
