"""Lists the synchronous v0.2 host imports whose host code reaches a synchronous event fire.

A synchronous (`func`) v0.2 import runs while the guest call holds the root permit of the Wasm
plugin stores. If its host code reaches `PluginManager::fire_blocking` and a Wasm plugin handles
that event, the call hangs. Such an import must defer the work through `DeferHost::defer` (see
`crates/pumpkin-wasm-host-common/src/deferred.rs`) or be recorded in `sync_fires.tsv`.

The call graph matches function names, not types: see README.md for what it finds and misses.
"""

import argparse
import collections
import csv
import os
import re
import sys

HOST = "crates/pumpkin-wasm-host-v0_2/src"
SCANNED = [
    "crates/pumpkin-core/src",
    "crates/pumpkin-inventory/src",
    "crates/pumpkin-world/src",
    "crates/pumpkin-wasm-host-common/src",
    HOST,
]
# The blocking fires, and the reads of an item stack that fire an event through the item hooks of
# `crates/pumpkin-core/src/plugin/item_events.rs`.
TARGETS = {
    "fire_blocking",
    "fire_blocking_with",
    "get_enchantment_level",
    "get_all_enchantments",
    "get_attribute_modifiers",
    "has_prevent_armor_change",
}
# The send paths of the Java and Bedrock connections. Inside a guest call they defer the send when
# a Wasm plugin handles its sent event (`deferring_guest_call`), so they are no path to a hang.
SENDS = {
    "try_enqueue_packet_data",
    "try_enqueue_packet_data_inner",
    "try_enqueue_packet",
    "try_enqueue_packet_data_unobserved",
    "translate_outgoing",
    "reserve_pending_bytes",
    "send_and_wait",
    "try_kick",
    "kick_explicit",
    "try_enqueue_raw_packet_direct",
    "queue_extra_packets",
    "deferring_guest_call",
    "defer_in_guest_call",
    "packet_sent_target",
    "prepare_play_sent_event",
    "try_queue_packet_data",
    "enqueue_packet_data",
    "send_packet_now_data",
    "queue_translated_packet",
}
# A name with more definitions than this is too ambiguous to follow: `set_velocity` of a player
# and of a projectile have the same name.
MAX_DEFINITIONS = 4
# Names of the standard library and of common traits, which a name match would join to an
# unrelated function of the server.
COMMON_NAMES = set(
    """
    new get get_mut set clone cloned from into try_from try_into iter iter_mut into_iter collect
    contains contains_key take replace write read empty file pending create store load swap
    fetch_add fetch_sub compare_exchange add sub mul div drop insert remove push push_back pop
    pop_front len is_empty map map_err and_then unwrap_or unwrap_or_else unwrap_or_default ok_or
    ok_or_else lock try_lock as_ref as_mut to_owned to_string default fmt hash eq ne cmp
    partial_cmp next send try_send recv extend filter filter_map find any all position first last
    sort sort_by max min abs floor ceil round sqrt with entry or_insert or_default upgrade
    downgrade cast_any get_entity get_living_entity as_any from_name to_name from_id to_id parse
    format build finish run call apply update tick handle process close cancel
    """.split()
)
KEYWORDS = {
    "if", "while", "match", "for", "return", "loop", "in", "as", "fn", "let", "move", "else",
    "where", "impl", "use", "mut", "ref",
}
FN = re.compile(r"\b(async\s+)?(?:unsafe\s+)?fn\s+([A-Za-z_][A-Za-z0-9_]*)")
CALL = re.compile(r"\b([a-z_][a-z0-9_]*)\s*(?:::\s*<[^()]*?>\s*)?\(")
IMPL = re.compile(r"\bimpl\s+([A-Za-z0-9_:<>, ]+?)\s+for\s+PluginHostState\s*\{")
# The async imports. wasmtime polls their futures outside the re-entry context of the guest call,
# so a blocking fire in them hangs like one in a synchronous import, unless the work runs through
# `run_blocking` or `in_active_context`.
ASYNC_IMPL = re.compile(
    r"\bimpl\s+([A-Za-z0-9_:]+?)WithStore<PluginHostState>\s+for\s+HasSelf<PluginHostState>\s*\{"
)
IN_CONTEXT = [re.compile(r"\brun_blocking\s*\("), re.compile(r"\bin_active_context\s*\(")]
# Code that runs after the import returns, on another task: deferred work, spawned work and
# async blocks.
LATER = [
    re.compile(pattern)
    for pattern in (
        r"\.defer\s*\(",
        r"\bspawn_blocking\s*\(",
        r"\bspawn\s*\(",
        r"\basync\s+(?:move\s+)?\{",
    )
]
CHAR = re.compile(r"'(?:\\(?:x[0-9a-fA-F]{2}|u\{[0-9a-fA-F]+\}|.)|[^'\\\n])'")
RAW_STRING = re.compile(r'b?r(#*)"')


def blank(text):
    """`text` with every character except line breaks replaced by a space."""
    return re.sub(r"[^\n]", " ", text)


def strip(src):
    """`src` with comments, strings and character literals blanked, at the same offsets."""
    out = []
    i, n = 0, len(src)
    while i < n:
        if src.startswith("//", i):
            j = src.find("\n", i)
            j = n if j < 0 else j
            out.append(blank(src[i:j]))
            i = j
            continue
        if src.startswith("/*", i):
            depth, j = 1, i + 2
            while j < n and depth:
                if src.startswith("/*", j):
                    depth, j = depth + 1, j + 2
                elif src.startswith("*/", j):
                    depth, j = depth - 1, j + 2
                else:
                    j += 1
            out.append(blank(src[i:j]))
            i = j
            continue
        raw = RAW_STRING.match(src, i)
        if raw and (i == 0 or not (src[i - 1].isalnum() or src[i - 1] == "_")):
            end = src.find('"' + raw.group(1), raw.end())
            end = n if end < 0 else end + 1 + len(raw.group(1))
            out.append(blank(src[i:end]))
            i = end
            continue
        if src[i] == '"':
            j = i + 1
            while j < n and src[j] != '"':
                j += 2 if src[j] == "\\" else 1
            out.append('"' + blank(src[i + 1 : j]) + '"')
            i = j + 1
            continue
        if src[i] == "'":
            char = CHAR.match(src, i)
            if char:
                out.append(blank(char.group(0)))
                i = char.end()
                continue
        out.append(src[i])
        i += 1
    return "".join(out)


def closing(text, start):
    """The offset of the bracket that closes the bracket at `start`."""
    pairs = {"(": ")", "{": "}", "[": "]"}
    opening, close = text[start], pairs[text[start]]
    depth = 0
    for i in range(start, len(text)):
        if text[i] == opening:
            depth += 1
        elif text[i] == close:
            depth -= 1
            if depth == 0:
                return i
    return len(text)


def blank_later(body, patterns=LATER):
    """`body` with the code that runs after the import returns blanked."""
    for pattern in patterns:
        while True:
            match = pattern.search(body)
            if not match:
                break
            start = match.end() - 1
            end = closing(body, start)
            body = body[: match.start()] + blank(body[match.start() : end + 1]) + body[end + 1 :]
    return body


def functions(src):
    """`(name, is_async, start, body)` for each function with a body in stripped `src`."""
    for match in FN.finditer(src):
        i, depth = match.end(), 0
        while i < len(src):
            if src[i] in "([":
                depth += 1
            elif src[i] in ")]":
                depth -= 1
            elif depth == 0 and src[i] in "{;":
                break
            i += 1
        if i < len(src) and src[i] == "{":
            yield match.group(2), bool(match.group(1)), match.start(), src[i : closing(src, i) + 1]


def calls(body, extra=()):
    body = blank_later(body)
    if extra:
        body = blank_later(body, extra)
    return set(CALL.findall(body)) - KEYWORDS


def rust_files(root, directory):
    for path, _, files in os.walk(os.path.join(root, directory)):
        for name in sorted(files):
            if name.endswith(".rs"):
                yield os.path.join(path, name)


def reaching(definitions):
    """For each name that reaches a target, the next name on the way to it."""
    ignored = {
        name for name, bodies in definitions.items() if len(bodies) > MAX_DEFINITIONS
    } | COMMON_NAMES | SENDS
    next_name = dict.fromkeys(TARGETS)
    changed = True
    while changed:
        changed = False
        for name, bodies in definitions.items():
            if name in next_name or name in ignored:
                continue
            for called in bodies:
                hit = min((c for c in called if c in next_name and c not in ignored), default=None)
                if hit:
                    next_name[name] = hit
                    changed = True
                    break
    return next_name, ignored


def chain(next_name, name):
    names = [name]
    while next_name.get(names[-1]) and len(names) < 40:
        names.append(next_name[names[-1]])
    return names


def scan(root):
    """`{import: (path, line, chains)}` for each synchronous import that reaches a target: one chain
    for each function that the import calls and that reaches a target."""
    definitions = collections.defaultdict(list)
    for directory in SCANNED:
        for path in rust_files(root, directory):
            with open(path, encoding="utf-8") as file:
                src = strip(file.read())
            for name, is_async, _, body in functions(src):
                # A synchronous caller cannot reach an async function without `block_on`.
                if not is_async:
                    definitions[name].append(calls(body))
    next_name, ignored = reaching(definitions)
    found = {}
    for path in rust_files(root, HOST):
        with open(path, encoding="utf-8") as file:
            src = strip(file.read())
        for pattern, wanted_async, prefix, extra in (
            (IMPL, False, "", ()),
            (ASYNC_IMPL, True, "async ", IN_CONTEXT),
        ):
            for impl in pattern.finditer(src):
                start = impl.end() - 1
                block = src[start : closing(src, start) + 1]
                trait = re.sub(r"<.*", "", impl.group(1)).split("::")[-1]
                for name, is_async, offset, body in functions(block):
                    if is_async != wanted_async:
                        continue
                    hits = sorted(
                        c for c in calls(body, extra) if c in next_name and c not in ignored
                    )
                    if hits:
                        line = src.count("\n", 0, start + offset) + 1
                        found[f"{prefix}{trait}.{name}"] = (
                            os.path.relpath(path, root),
                            line,
                            [chain(next_name, hit) for hit in hits],
                        )
    return found


def recorded(path):
    with open(path, encoding="utf-8", newline="") as file:
        rows = list(csv.reader(file, delimiter="\t"))
    return {row[0]: row[1] for row in rows[1:] if row and not row[0].startswith("#")}


def main():
    here = os.path.dirname(os.path.abspath(__file__))
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--root", default=os.path.join(here, "..", ".."))
    parser.add_argument("--list", default=os.path.join(here, "sync_fires.tsv"))
    parser.add_argument(
        "--check", action="store_true", help="exit 1 when the scan and the list differ"
    )
    args = parser.parse_args()
    found = scan(args.root)
    known = recorded(args.list)
    new = sorted(set(found) - set(known))
    gone = sorted(set(known) - set(found))
    for name in sorted(found):
        path, line, chains = found[name]
        mark = "new" if name in new else known[name]
        for names in chains:
            print(f"{name}\t{mark}\t{path}:{line}\t{' -> '.join(names)}")
    for name in gone:
        print(f"{name}\tgone: in the list, but the scan does not find it")
    print(f"{len(found)} imports reach a blocking fire, {len(new)} new, {len(gone)} gone")
    if args.check and (new or gone):
        sys.exit(1)


if __name__ == "__main__":
    main()
