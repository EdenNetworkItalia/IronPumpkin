#!/usr/bin/env python3
"""Per-method demand for the NeoForge extension interfaces in a folder of NeoForge mod jars.

    python3 -I extension_scan.py <mods-dir> <out-file> <index-jar>...

The index jars give the class hierarchy and the extension interfaces of the NeoForge version that
the mods target (21.1.248): the vanilla jar with Mojang names, the patched client jar and the
universal jar, in this order. A later jar replaces a class of an earlier one.

For each method of an extension interface (`net/neoforged/neoforge/common/extensions`), the scan
counts the mods (top-level jars, Jar-in-Jar jars included) that:

- call the method: a method reference whose owner has the extension interface among its supertypes,
  and the interface declares the same name and descriptor (a call through the patched vanilla class
  counts);
- override the method: a class of the mod that declares a method with the same name and descriptor
  as an inherited extension method.

The output is a tab-separated file, sorted, with no paths and no time stamps. One row per method
name (the overloads are merged). Columns: interface, method, the mods that call it and the mods that
override it, each a `;` separated list of jar names.

The jars are data. The scanner reads them with zipfile and its own class file parser and never loads
or runs anything from them. Standard library only.
"""

import collections
import concurrent.futures
import io
import multiprocessing
import struct
import sys
import zipfile
import zlib

EXTENSIONS = "net/neoforged/neoforge/common/extensions/"
MAX_ENTRY = 16 * 1024 * 1024
MAX_NESTED_JAR = 256 * 1024 * 1024
MAX_DEPTH = 3
READ_ERRORS = (zipfile.BadZipFile, zlib.error, EOFError, OSError, NotImplementedError)
ACC_PRIVATE, ACC_STATIC, ACC_SYNTHETIC = 0x0002, 0x0008, 0x1000
WORKERS = 6


class ClassFormatError(Exception):
    pass


def mutf8(raw):
    try:
        return raw.decode("utf-8")
    except UnicodeDecodeError:
        return raw.replace(b"\xc0\x80", b"\x00").decode("utf-8", "replace")


def param_count(desc):
    """Number of parameters of a method descriptor such as `(ILjava/lang/String;[J)V`."""
    n, i = 0, 1
    while desc[i] != ")":
        while desc[i] == "[":
            i += 1
        i = desc.index(";", i) + 1 if desc[i] == "L" else i + 1
        n += 1
    return n


def parse_class(data, names=None):
    """Return the hierarchy, the methods and the method references of a class file.

    A method or a reference is kept when its name is in `names` (every name when it is None).
    Returns {"name", "super", "interfaces", "methods": [(name, desc, access)], "refs": set of
    (owner, name, desc)}.
    """
    pos = 0

    def take(n):
        nonlocal pos
        if pos + n > len(data):
            raise ClassFormatError("truncated")
        pos += n
        return data[pos - n:pos]

    def u2():
        return struct.unpack(">H", take(2))[0]

    if take(4) != b"\xca\xfe\xba\xbe":
        raise ClassFormatError("bad magic")
    take(4)
    count = u2()
    pool = [None] * count
    i = 1
    while i < count:
        tag = take(1)[0]
        if tag == 1:
            pool[i] = mutf8(take(u2()))
        elif tag in (3, 4):
            take(4)
        elif tag in (5, 6):
            take(8)
            i += 1
        elif tag in (7, 8, 16, 19, 20):
            pool[i] = (tag, u2())
        elif tag in (9, 10, 11, 12, 17, 18):
            pool[i] = (tag, u2(), u2())
        elif tag == 15:
            take(3)
        else:
            raise ClassFormatError("bad constant tag %d" % tag)
        i += 1

    def text(idx):
        v = pool[idx]
        return pool[v[1]] if isinstance(v, tuple) else v

    u2()  # access
    name = text(u2())
    sup = u2()
    sup = text(sup) if sup else None
    interfaces = [text(u2()) for _ in range(u2())]
    methods = []
    for _kind in ("fields", "methods"):
        for _ in range(u2()):
            access, mname, mdesc = u2(), text(u2()), text(u2())
            for _ in range(u2()):
                u2()
                take(struct.unpack(">I", take(4))[0])
            if _kind == "methods" and (names is None or mname in names):
                methods.append((mname, mdesc, access))
    refs = set()
    for entry in pool:
        if isinstance(entry, tuple) and entry[0] in (10, 11):
            nt = pool[entry[2]]
            if names is None or pool[nt[1]] in names:
                refs.add((text(entry[1]), pool[nt[1]], pool[nt[2]]))
    return {"name": name, "super": sup, "interfaces": interfaces, "methods": methods, "refs": refs}


def walk_jar(zf, depth, visit):
    """Call visit(entry name, bytes) for each class file of a jar and of its nested jars."""
    for info in sorted(zf.infolist(), key=lambda i: i.filename):
        name = info.filename
        if info.is_dir():
            continue
        is_jar = name.endswith(".jar") and depth < MAX_DEPTH
        if info.file_size > (MAX_NESTED_JAR if is_jar else MAX_ENTRY):
            continue
        if not (is_jar or name.endswith(".class")):
            continue
        try:
            raw = zf.read(info)
        except READ_ERRORS:
            continue
        if is_jar:
            try:
                walk_jar(zipfile.ZipFile(io.BytesIO(raw)), depth + 1, visit)
            except READ_ERRORS:
                pass
        else:
            visit(name, raw)


def scan_jar(args):
    """Worker: parse every class of one jar. Returns a list of parsed classes."""
    path, names, only_prefix = args
    out = []

    def visit(entry, raw):
        if only_prefix is not None and not entry.startswith(only_prefix):
            return
        if b"\xca\xfe\xba\xbe" != raw[:4]:
            return
        try:
            out.append(parse_class(raw, names))
        except (ClassFormatError, struct.error, IndexError, KeyError, TypeError):
            pass

    try:
        with zipfile.ZipFile(path) as zf:
            walk_jar(zf, 0, visit)
    except READ_ERRORS:
        pass
    return out


class Hierarchy:
    """Class map with the supertype closure of a class."""

    def __init__(self):
        self.classes = {}
        self._ancestors = {}

    def add(self, cls):
        self.classes[cls["name"]] = (cls["super"], cls["interfaces"])

    def ancestors(self, start):
        """`start` and every superclass and interface above it that the class map knows."""
        known = self._ancestors.get(start)
        if known is None:
            known, queue = [], collections.deque([start])
            seen = set()
            while queue:
                cur = queue.popleft()
                if cur in seen:
                    continue
                seen.add(cur)
                known.append(cur)
                if cur in self.classes:
                    sup, interfaces = self.classes[cur]
                    queue.extend(([sup] if sup else []) + interfaces)
            self._ancestors[start] = known
        return known


def scan(mods, index_jars):
    """Return ({(interface, method): [call mods, override mods]}, [(interface, method)]).

    A call counts for an extension interface when the owner of the reference has the interface among
    its supertypes and the interface declares the same name and descriptor. That includes a call
    through a patched vanilla class that implements or overrides the method. An override counts when
    a class of the mod declares a method that an extension interface above it declares too.
    """
    hierarchy = Hierarchy()
    extension_methods = {}  # (interface, name, desc) -> parameter count
    # Stage 0: the extension interfaces name the methods that are worth keeping.
    for jar in index_jars:
        for cls in scan_jar((jar, None, EXTENSIONS)):
            for n, d, access in cls["methods"]:
                if not access & (ACC_PRIVATE | ACC_STATIC | ACC_SYNTHETIC):
                    extension_methods[(cls["name"].rsplit("/", 1)[1], n, d)] = param_count(d)
    names = {n for _, n, _ in extension_methods}
    for jar in index_jars:
        for cls in scan_jar((jar, names, None)):
            hierarchy.add(cls)
    interface_of = {}  # (class path, name, desc) -> interface simple name
    for iface, n, d in extension_methods:
        interface_of[(EXTENSIONS + iface, n, d)] = iface

    hits = collections.defaultdict(lambda: ([set(), set()]))
    with concurrent.futures.ProcessPoolExecutor(
            max_workers=WORKERS, mp_context=multiprocessing.get_context("fork")) as pool:
        parsed = list(pool.map(scan_jar, [(m, names, None) for m in mods], chunksize=1))
    for classes in parsed:
        for cls in classes:
            hierarchy.add(cls)
    for label, classes in zip(mods, parsed):
        mod = label.rsplit("/", 1)[-1]
        for cls in classes:
            for owner, name, desc in cls["refs"]:
                for ancestor in hierarchy.ancestors(owner):
                    key = interface_of.get((ancestor, name, desc))
                    if key:
                        hits[(key, name)][0].add(mod)
            for name, desc, access in cls["methods"]:
                if access & (ACC_PRIVATE | ACC_STATIC | ACC_SYNTHETIC):
                    continue
                for ancestor in hierarchy.ancestors(cls["name"])[1:]:
                    key = interface_of.get((ancestor, name, desc))
                    if key:
                        hits[(key, name)][1].add(mod)
    return hits, sorted({(i, n) for (i, n, _d) in extension_methods})


def render(hits, methods):
    """One row per method name. The mods are listed, so a reader can union rows."""
    lines = ["interface\tmethod\tcalled_by\toverridden_by"]
    for iface, name in methods:
        called, overridden = hits.get((iface, name), (set(), set()))
        for mod in called | overridden:
            if ";" in mod or "\t" in mod:
                raise ValueError(f"the jar name {mod!r} has a separator character")
        lines.append(f"{iface}\t{name}\t{';'.join(sorted(called))}\t{';'.join(sorted(overridden))}")
    return "\n".join(lines) + "\n"


def main(argv):
    if len(argv) < 4:
        print(__doc__)
        return 2
    import os
    mods_dir, out = argv[1], argv[2]
    mods = sorted(os.path.join(mods_dir, f) for f in os.listdir(mods_dir) if f.endswith(".jar"))
    hits, methods = scan(mods, argv[3:])
    with open(out, "w", encoding="utf-8") as f:
        f.write(render(hits, methods))
    print(f"{len(mods)} jars, {len(methods)} extension method names, "
          f"{sum(1 for h in hits.values() if h[0] | h[1])} with demand")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
