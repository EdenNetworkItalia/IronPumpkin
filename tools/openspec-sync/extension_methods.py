#!/usr/bin/env python3
"""Read the public methods of the NeoForge extension interfaces from the NeoForge sources.

The module has no command of its own. `extension_table.py` builds the inventory rows from it.
Standard library only.
"""

import re
from pathlib import Path

EXTENSIONS = "net/neoforged/neoforge/common/extensions"
INJECTED = "META-INF/injected-interfaces.json"
TYPE_KEYWORDS = ("interface", "class", "enum", "record", "@interface")


def strip_java(text):
    """Remove comments, and blank out string and char literals, keeping the line structure."""
    out, i, n = [], 0, len(text)
    while i < n:
        two = text[i:i + 2]
        if two == "//":
            j = text.find("\n", i)
            i = n if j < 0 else j
        elif two == "/*":
            j = text.find("*/", i + 2)
            j = n if j < 0 else j + 2
            out.append("\n" * text.count("\n", i, j))
            i = j
        elif text.startswith('"""', i):
            j = text.find('"""', i + 3)
            j = n if j < 0 else j + 3
            out.append('""' + "\n" * text.count("\n", i, j))
            i = j
        elif text[i] in "\"'":
            q, j = text[i], i + 1
            while j < n and text[j] != q:
                j += 2 if text[j] == "\\" else 1
            out.append(q + q)
            i = j + 1
        else:
            out.append(text[i])
            i += 1
    return "".join(out)


def split_params(params):
    """Split a parameter list at the top-level commas, so generic arguments stay whole."""
    parts, depth, cur = [], 0, ""
    for ch in params:
        depth += ch in "<(" and 1 or ch in ">)" and -1 or 0
        if ch == "," and depth == 0:
            parts.append(cur)
            cur = ""
        else:
            cur += ch
    if cur.strip():
        parts.append(cur)
    return [re.sub(r"@\w+(\([^)]*\))?\s*", "", p).strip() for p in parts]


def members(body, owner):
    """Yield the member declarations at the top level of an interface body.

    A member is (kind, header, block). kind is "method", "type" or "field". A method header ends at
    its parameter list; block is the text of its body, or None for an abstract method.
    """
    depth, start, i, n = 0, 0, 0, len(body)
    while i < n:
        ch = body[i]
        if ch == "{" and depth == 0:
            j, d = i, 0
            while j < n:
                d += body[j] == "{"
                d -= body[j] == "}"
                j += 1
                if d == 0:
                    break
            header = body[start:i].strip()
            yield header, body[i + 1:j - 1]
            start, i = j, j
            continue
        if ch == ";" and depth == 0:
            yield body[start:i].strip(), None
            start = i + 1
        elif ch in "([":
            depth += 1
        elif ch in ")]":
            depth -= 1
        i += 1


def parse_interface(path):
    """Return {"name", "type_params", "methods": [...], "nested": [...]} for one source file.

    A method is a dict: name, modifiers (set), params (list of str), returns (str), line_order.
    The file may hold an interface with nested types. Only the top-level type and its nested types
    are read, because the extension files declare nothing deeper.
    """
    text = strip_java(Path(path).read_text(encoding="utf-8"))
    m = re.search(r"\b(interface|class|enum|record)\s+(\w+)", text)
    if not m:
        return {"name": Path(path).stem, "methods": [], "nested": []}
    brace = text.index("{", m.end())
    depth, j = 0, brace
    while j < len(text):
        depth += text[j] == "{"
        depth -= text[j] == "}"
        j += 1
        if depth == 0:
            break
    return parse_type(m.group(2), text[brace + 1:j - 1])


def parse_type(name, body):
    methods, nested = [], []
    for header, block in members(body, name):
        header = re.sub(r"@\w+(\.\w+)*(\([^)]*\))?\s*", "", header).strip()
        if not header:
            continue
        tm = re.match(r"((?:\w+\s+)*)(interface|class|enum|record)\s+(\w+)", header)
        if tm and "(" not in header.split(tm.group(2), 1)[0]:
            nested.append(parse_type(tm.group(3), block or ""))
            continue
        paren = header.find("(")
        if paren < 0 or "=" in header[:paren]:
            continue
        head = header[:paren].strip()
        words = re.sub(r"<[^()]*?>(?=\s|$)", "", head).split()
        mm = re.match(r"(.*?)(\w+)$", head, re.S)
        if not mm:
            continue
        returns = re.sub(r"\b(default|static|private|public|abstract|final)\b\s*", "", mm.group(1)).strip()
        end = header.rfind(")")
        params = split_params(header[paren + 1:end])
        mods = {w for w in words if w in ("default", "static", "private", "public", "abstract")}
        methods.append({"name": mm.group(2), "modifiers": mods, "params": params,
                        "returns": returns, "has_body": block is not None})
    return {"name": name, "methods": methods, "nested": nested}


def public_methods(parsed):
    """The public methods of an interface: every method that is not private.

    Interface methods are public without the keyword. A constructor-like or `self()` helper is
    private in the sources and does not count.
    """
    return [m for m in parsed["methods"] if "private" not in m["modifiers"]]


def kind(method):
    mods = method["modifiers"]
    if "static" in mods:
        return "static method"
    return "default method" if "default" in mods else "abstract method"


def groups(parsed):
    """Return [(method name, kind label, [overload dict, ...])] in source order.

    One group per method name. The label is the kind of the overloads, `default method` or
    `abstract method` or `static method`, or "default and abstract method" for a mix, plus the
    number of overloads when it is more than one.
    """
    order, by_name = [], {}
    for m in public_methods(parsed):
        if m["name"] not in by_name:
            by_name[m["name"]] = []
            order.append(m["name"])
        by_name[m["name"]].append(m)
    out = []
    for name in order:
        overloads = by_name[name]
        kinds = sorted({kind(m).split()[0] for m in overloads})
        label = " and ".join(kinds) + " method"
        if len(overloads) > 1:
            label += f", {len(overloads)} overloads"
        out.append((name, label, overloads))
    return out


def load_injected(root):
    """Return {interface simple name: [target class names]} from injected-interfaces.json."""
    import json
    data = json.loads((Path(root) / INJECTED).read_text(encoding="utf-8"))
    out = {}
    for target, interfaces in data.items():
        for iface in interfaces if isinstance(interfaces, list) else [interfaces]:
            simple = iface.rsplit("/", 1)[-1].split("<")[0]
            out.setdefault(simple, []).append(target.rsplit("/", 1)[-1].replace("$", "."))
    return out
