#!/usr/bin/env python3
"""Method-level scan of the mixin members in a folder of NeoForge mod jars.

Run with `python3 -I mixin_scan.py <mods-dir> <out-dir>`. The jars are data:
the scanner reads them with zipfile and its own class file parser and never
loads or runs anything from them. Standard library only.

Outputs, all deterministic for the same input:
  members.tsv  one row per mixin member, target and injection point
  report.md    the markdown of the "Mixin targets by method" section
"""

import collections
import concurrent.futures
import io
import json
import multiprocessing
import os
import re
import struct
import sys
import tomllib
import zipfile
import zlib

# Size limits keep memory bounded with one worker per core.
MAX_ENTRY = 16 * 1024 * 1024
MAX_NESTED_JAR = 256 * 1024 * 1024
READ_ERRORS = (zipfile.BadZipFile, zlib.error, EOFError, OSError, NotImplementedError)
MAX_DEPTH = 3
MIXIN_DESC = "Lorg/spongepowered/asm/mixin/Mixin;"
MIXIN_MARK = MIXIN_DESC.encode()

# Simple names of the member annotations. Their descriptor must start with one
# of ANNOTATION_PACKAGES: NeoForge ships MixinExtras unrelocated.
MEMBER_ANNOTATIONS = {
    "Inject", "Redirect", "ModifyArg", "ModifyArgs", "ModifyVariable",
    "ModifyConstant", "Overwrite", "Shadow", "Unique", "Accessor", "Invoker",
    "ModifyExpressionValue", "ModifyReturnValue", "ModifyReceiver",
    "WrapOperation", "WrapWithCondition", "WrapMethod",
}
ANNOTATION_PACKAGES = ("Lorg/spongepowered/asm/mixin/", "Lcom/llamalad7/mixinextras/")

ACCESSOR = {"Accessor", "Invoker", "Shadow"}
VALUE = {
    "Redirect", "WrapOperation", "ModifyArg", "ModifyArgs", "ModifyConstant",
    "ModifyExpressionValue", "ModifyVariable", "ModifyReturnValue",
    "ModifyReceiver", "WrapWithCondition",
}
INTENTS = ("pre", "post", "mid", "cancel", "value", "overwrite", "accessor")
PRIMITIVE_RANK = {"accessor": 0, "hook": 1, "call site": 2, "override": 3}
INTENT_PRIMITIVE = {
    "accessor": "accessor", "pre": "hook", "post": "hook", "cancel": "hook",
    "mid": "call site", "value": "call site", "overwrite": "override",
}
REPLACE_NAMED = (
    "net.minecraft.world.item.crafting.RecipeManager",
    "net.minecraft.world.level.storage.loot.LootTable",
    "net.minecraft.world.level.storage.loot.LootPool",
    "net.minecraft.world.level.BaseSpawner",
    "net.minecraft.world.level.chunk.ChunkGenerator",
    "net.minecraft.world.level.Explosion",
    "net.minecraft.world.item.enchantment.EnchantmentHelper",
    "net.minecraft.world.item.alchemy.PotionBrewing",
    "net.minecraft.tags.TagLoader",
    "net.minecraft.server.ReloadableServerResources",
    "net.minecraft.server.players.PlayerList",
)
TOP_METHODS = 200
TOP_OTHER = 25


# ---------------------------------------------------------------- class files

class ClassFormatError(Exception):
    pass


def mutf8(raw):
    try:
        return raw.decode("utf-8")
    except UnicodeDecodeError:
        return raw.replace(b"\xc0\x80", b"\x00").decode("utf-8", "replace")


class Reader:
    __slots__ = ("data", "pos")

    def __init__(self, data):
        self.data = data
        self.pos = 0

    def take(self, n):
        if self.pos + n > len(self.data):
            raise ClassFormatError("truncated")
        out = self.data[self.pos:self.pos + n]
        self.pos += n
        return out

    def u1(self):
        return self.take(1)[0]

    def u2(self):
        return struct.unpack(">H", self.take(2))[0]

    def u4(self):
        return struct.unpack(">I", self.take(4))[0]


def read_pool(r):
    count = r.u2()
    pool = [None] * count
    i = 1
    while i < count:
        tag = r.u1()
        if tag == 1:
            pool[i] = mutf8(r.take(r.u2()))
        elif tag == 3:
            pool[i] = struct.unpack(">i", r.take(4))[0]
        elif tag == 4:
            pool[i] = struct.unpack(">f", r.take(4))[0]
        elif tag == 5:
            pool[i] = struct.unpack(">q", r.take(8))[0]
            i += 1
        elif tag == 6:
            pool[i] = struct.unpack(">d", r.take(8))[0]
            i += 1
        elif tag in (7, 8, 16, 19, 20):
            pool[i] = ("ref", r.u2())
        elif tag in (9, 10, 11, 12, 17, 18):
            r.take(4)
        elif tag == 15:
            r.take(3)
        else:
            raise ClassFormatError("bad constant tag %d" % tag)
        i += 1
    return pool


def utf(pool, idx):
    v = pool[idx]
    if isinstance(v, tuple):
        v = pool[v[1]]
    if not isinstance(v, str):
        raise ClassFormatError("not utf8 at %d" % idx)
    return v


def read_element(r, pool):
    tag = chr(r.u1())
    if tag in "BCDFIJSZ":
        v = pool[r.u2()]
        return bool(v) if tag == "Z" else v
    if tag == "s":
        return utf(pool, r.u2())
    if tag == "e":
        r.u2()
        return utf(pool, r.u2())
    if tag == "c":
        return ("class", utf(pool, r.u2()))
    if tag == "@":
        return read_annotation(r, pool)
    if tag == "[":
        return [read_element(r, pool) for _ in range(r.u2())]
    raise ClassFormatError("bad element tag %r" % tag)


def read_annotation(r, pool):
    desc = utf(pool, r.u2())
    elems = {}
    for _ in range(r.u2()):
        name = utf(pool, r.u2())
        elems[name] = read_element(r, pool)
    return (desc, elems)


def read_attributes(r, pool):
    annos = []
    for _ in range(r.u2()):
        name = utf(pool, r.u2())
        length = r.u4()
        if name in ("RuntimeInvisibleAnnotations", "RuntimeVisibleAnnotations"):
            sub = Reader(r.take(length))
            annos.extend(read_annotation(sub, pool) for _ in range(sub.u2()))
        else:
            r.take(length)
    return annos


def parse_class(data):
    r = Reader(data)
    if r.u4() != 0xCAFEBABE:
        raise ClassFormatError("bad magic")
    r.take(4)
    pool = read_pool(r)
    access = r.u2()
    name = utf(pool, r.u2())
    r.u2()
    r.take(2 * r.u2())
    members = []
    for kind in ("field", "method"):
        for _ in range(r.u2()):
            r.u2()
            mname = utf(pool, r.u2())
            mdesc = utf(pool, r.u2())
            members.append((kind, mname, mdesc, read_attributes(r, pool)))
    return {"name": name, "access": access, "members": members,
            "annotations": read_attributes(r, pool)}


# ------------------------------------------------------------------ jar reads

def read_toml(raw):
    """Mod id of the first [[mods]] entry and the mixin configs of [[mixins]]."""
    text = mutf8(raw)
    mod_id, configs = None, set()
    try:
        doc = tomllib.loads(text)
        mods = doc.get("mods")
        if isinstance(mods, list) and mods and isinstance(mods[0], dict):
            mid = mods[0].get("modId")
            mod_id = mid if isinstance(mid, str) else None
        for entry in doc.get("mixins") or []:
            if isinstance(entry, dict) and isinstance(entry.get("config"), str):
                configs.add(entry["config"])
    except (tomllib.TOMLDecodeError, ValueError, AttributeError, TypeError):
        m = re.search(r'modId\s*=\s*"([^"]+)"', text)
        mod_id = m.group(1) if m else None
        configs.update(re.findall(r'config\s*=\s*"([^"]+)"', text))
    return mod_id, configs


def manifest_configs(raw):
    text = mutf8(raw).replace("\r\n", "\n").replace("\n ", "")
    m = re.search(r"^MixinConfigs:\s*(.*)$", text, re.M)
    return {x.strip() for x in m.group(1).split(",") if x.strip()} if m else set()


def read_container(zf, label, depth, out):
    """Scan one jar: class names, mixin configs, mixin classes, nested jars."""
    cont = {"label": label, "mod_id": None, "classes": [], "configs": [],
            "mixins": [], "errors": 0, "read_errors": 0, "declared": set()}
    out.append(cont)
    infos = sorted(zf.infolist(), key=lambda i: i.filename)
    tomls = {}

    def read(info, limit=MAX_ENTRY):
        """Entry bytes, or None when the entry is too large or does not decompress."""
        if info.file_size > limit:
            cont["read_errors"] += 1
            return None
        try:
            return zf.read(info)
        except READ_ERRORS:
            cont["read_errors"] += 1
            return None

    for info in infos:
        n = info.filename
        if info.is_dir():
            continue
        nested = n.startswith("META-INF/jarjar/") and n.endswith(".jar")
        wanted = (n in ("META-INF/neoforge.mods.toml", "META-INF/mods.toml", "META-INF/MANIFEST.MF")
                  or n.endswith(".class") or (n.endswith(".json") and "/" not in n)
                  or (nested and depth < MAX_DEPTH))
        if not wanted:
            continue
        data = read(info, MAX_NESTED_JAR if nested else MAX_ENTRY)
        if data is None:
            continue
        if n in ("META-INF/neoforge.mods.toml", "META-INF/mods.toml"):
            tomls[n] = read_toml(data)
        elif n == "META-INF/MANIFEST.MF":
            cont["declared"].update(manifest_configs(data))
        elif n.endswith(".class"):
            cname = n[:-6]
            if cname.startswith("META-INF/versions/"):
                parts = cname.split("/", 3)
                cname = parts[3] if len(parts) == 4 else cname
            cont["classes"].append(cname)
            if MIXIN_MARK not in data:
                continue
            try:
                cls = parse_class(data)
            except (ClassFormatError, IndexError, struct.error, RecursionError):
                cont["errors"] += 1
                continue
            if any(d == MIXIN_DESC for d, _ in cls["annotations"]):
                cont["mixins"].append(cls)
        elif n.endswith(".json") and "/" not in n:
            try:
                doc = json.loads(data)
            except (ValueError, UnicodeDecodeError, RecursionError):
                continue
            if isinstance(doc, dict) and isinstance(doc.get("package"), str) and any(
                    isinstance(doc.get(k), list) for k in ("mixins", "client", "server")):
                cont["configs"].append({
                    "file": n,
                    "package": doc["package"],
                    "plugin": doc.get("plugin") if isinstance(doc.get("plugin"), str) else "",
                    "lists": {k: [x for x in doc.get(k) or [] if isinstance(x, str)]
                              for k in ("mixins", "client", "server")},
                })
        elif nested:
            try:
                inner = zipfile.ZipFile(io.BytesIO(data))
            except READ_ERRORS:
                cont["read_errors"] += 1
                continue
            with inner:
                read_container(inner, label + "!" + n.rsplit("/", 1)[1], depth + 1, out)
    # NeoForge 21.1 reads neoforge.mods.toml; some jars also carry an old mods.toml.
    for name in ("META-INF/neoforge.mods.toml", "META-INF/mods.toml"):
        if name in tomls:
            cont["mod_id"], declared = tomls[name]
            cont["declared"].update(declared)
            break


def scan_jar(path):
    out = []
    try:
        with zipfile.ZipFile(path) as zf:
            read_container(zf, os.path.basename(path), 0, out)
    except READ_ERRORS:
        # The jar itself does not open: keep one empty container that carries the error.
        out = [{"label": os.path.basename(path), "mod_id": None, "classes": [], "configs": [],
                "mixins": [], "errors": 0, "read_errors": 1, "declared": set()}]
    return out


# -------------------------------------------------------------- member model

def short(cls):
    return cls[len("net.minecraft."):] if cls.startswith("net.minecraft.") else cls


def simple(cls):
    return cls.rsplit(".", 1)[-1]


def dotted(internal):
    return internal.replace("/", ".")


def target_classes(anno):
    _, el = anno
    out = []
    for v in el.get("value", []) or []:
        if isinstance(v, tuple) and v[0] == "class":
            d = v[1]
            if d.startswith("L") and d.endswith(";"):
                out.append(dotted(d[1:-1]))
    for v in el.get("targets", []) or []:
        if isinstance(v, str) and v.strip():
            out.append(dotted(v.strip()))
    return sorted(set(out))


SELECTOR = re.compile(r"^(?:L([^;]+);)?([^(:]*)(\([^)]*\)\S*)?(?::(\S+))?$")


def parse_selector(sel):
    """Split a target selector into (owner, name, descriptor)."""
    s = sel.strip()
    if s.startswith("/"):
        return None, s, ""
    m = SELECTOR.match(s)
    if not m:
        return None, s, ""
    owner, name, mdesc, fdesc = m.groups()
    name = name.strip()
    if owner is None and ("." in name or "/" in name):
        owner, name = re.split(r"[./](?=[^./]*$)", name)
    # A trailing `*`, `+` or `{n}` quantifier selects all overloads of the name.
    name = re.sub(r"(?<=.)(\*|\+|\{[0-9,]*\})$", "", name)
    return (dotted(owner) if owner else None), name, (mdesc or fdesc or "")


def desc_selector(el):
    """(owner, name, descriptor) of an `@Desc` target; void.class means the mixin target."""
    def cls(v):
        return v[1] if isinstance(v, tuple) and v[0] == "class" else "V"
    owner = cls(el.get("owner"))
    owner = dotted(owner[1:-1]) if owner.startswith("L") and owner.endswith(";") else None
    args = "".join(cls(a) for a in el.get("args", []) or [])
    return owner, str(el.get("value", "")), "(%s)%s" % (args, cls(el.get("ret")))


def call_target(t):
    """Short form of an @At target: Owner.member."""
    owner, name, _ = parse_selector(t)
    if owner:
        return "%s.%s" % (simple(owner), name)
    return name or t


def decap(s):
    if not s:
        return s
    if s.upper() == s:
        return s
    return s[0].lower() + s[1:]


def accessor_target(kind_name, mname, mdesc, value):
    if value:
        return value
    if kind_name == "Invoker":
        for p in ("call", "invoke"):
            if mname.startswith(p) and len(mname) > len(p):
                return decap(mname[len(p):])
        for p in ("new", "create"):
            if mname.startswith(p):
                return "<init>"
        return mname
    for p in ("get", "set", "is"):
        if mname.startswith(p) and len(mname) > len(p) and mname[len(p)].isupper():
            return decap(mname[len(p):])
    return mname


def at_list(el):
    ats = []
    for key in ("at",):
        v = el.get(key)
        if isinstance(v, tuple):
            ats.append(v)
        elif isinstance(v, list):
            ats.extend(x for x in v if isinstance(x, tuple) and len(x) == 2)
    out = []
    for _, a in ats:
        value = a.get("value", "")
        target = a.get("target", "")
        if isinstance(target, list):
            target = target[0] if target else ""
        out.append((str(value), str(target)))
    consts = el.get("constant")
    if isinstance(consts, tuple):
        consts = [consts]
    for c in consts or []:
        if isinstance(c, tuple) and len(c) == 2:
            items = sorted("%s=%s" % (k, v) for k, v in c[1].items()
                           if k not in ("ordinal", "slice", "log", "expandZeroConditions"))
            out.append(("CONSTANT", ",".join(items)))
    return out


def classify(anno, ats, cancellable):
    if anno in ACCESSOR:
        return "accessor"
    if anno == "Overwrite":
        return "overwrite"
    if anno == "WrapMethod":
        return "cancel"
    if anno in VALUE:
        return "value"
    if anno == "Inject":
        if cancellable:
            return "cancel"
        values = {v.upper() for v, _ in ats}
        if values and values <= {"HEAD"}:
            return "pre"
        if values and values <= {"RETURN", "TAIL"}:
            return "post"
        return "mid"
    return "unique"


def side_kind(cls, own, owners, mod_of):
    if cls.startswith("net.minecraft.client.") or cls.startswith("com.mojang.blaze3d."):
        return "vanilla-client", ""
    if cls.startswith("net.minecraft.") or cls.startswith("com.mojang."):
        return "vanilla", ""
    if cls.startswith("net.neoforged."):
        return "neoforge", ""
    internal = cls.replace(".", "/")
    jars = owners.get(internal, ())
    if own in jars:
        return "own", ""
    if jars:
        return "other", mod_of[min(jars)]
    return "unknown", ""


SIDE_RANK = {"server": 0, "common": 1, "client": 2}


def members_of(mod, jar, cont, configs_outer, declared, owners, mod_of, jar_index, class_out):
    """Yield one row per mixin member, target and injection point.

    Appends (mod, config side, declared, target kinds) of each mixin class to class_out."""
    listed = collections.defaultdict(list)
    for cfg in configs_outer:
        for side_key, side in (("mixins", "common"), ("client", "client"), ("server", "server")):
            for entry in cfg["lists"][side_key]:
                listed[cfg["package"] + "." + entry].append(
                    (cfg["file"] not in declared, SIDE_RANK[side], cfg["file"], side, cfg))
    for cls in sorted(cont["mixins"], key=lambda c: c["name"]):
        mixin = dotted(cls["name"])
        if listed.get(mixin):
            undeclared, _, _, side, cfg = min(listed[mixin], key=lambda c: c[:3])
        else:
            undeclared, side, cfg = True, "unlisted", None
        mixin_anno = next(a for a in cls["annotations"] if a[0] == MIXIN_DESC)
        targets = target_classes(mixin_anno)
        class_out.append((mod, side, not undeclared,
                          tuple(side_kind(t, jar_index, owners, mod_of)[0] for t in targets)))
        for kind, mname, mdesc, annos in cls["members"]:
            for desc, el in annos:
                if not desc.startswith(ANNOTATION_PACKAGES):
                    continue
                anno = desc[1:-1].rsplit("/", 1)[-1]
                if anno not in MEMBER_ANNOTATIONS:
                    continue
                ats = at_list(el)
                cancellable = bool(el.get("cancellable", False))
                intent = classify(anno, ats, cancellable)
                selectors = []
                if anno in ("Accessor", "Invoker"):
                    selectors = [(None, accessor_target(anno, mname, mdesc, el.get("value", "")),
                                  "field" if anno == "Accessor" else mdesc)]
                elif anno == "Shadow":
                    prefix = el.get("prefix", "shadow$")
                    name = mname[len(prefix):] if mname.startswith(prefix) else mname
                    selectors = [(None, name, mdesc if kind == "method" else "field")]
                elif anno == "Overwrite":
                    selectors = [(None, mname, mdesc)]
                elif anno == "Unique":
                    selectors = [(None, "", "")]
                else:
                    sels = el.get("method", [])
                    if isinstance(sels, str):
                        sels = [sels]
                    selectors = [parse_selector(s) for s in sels if isinstance(s, str)]
                    for d in el.get("target", []) or []:
                        if isinstance(d, tuple) and len(d) == 2:
                            selectors.append(desc_selector(d[1]))
                    if not selectors:
                        selectors = [(None, "", "")]
                at_rows = ats or [("", "")]
                for t in targets or [""]:
                    tkind, towner = side_kind(t, jar_index, owners, mod_of) if t else ("none", "")
                    for sel_owner, tname, tdesc in selectors:
                        if sel_owner and len(targets) > 1 and sel_owner != t:
                            continue
                        for at_value, at_target in at_rows:
                            yield (mod, jar, cont["label"], cfg["file"] if cfg else "", side,
                                   cfg["plugin"] if cfg else "", mixin, kind, mname, mdesc, anno,
                                   intent, "true" if cancellable else "false", t, tkind, towner,
                                   tname, tdesc, at_value, at_target,
                                   "no" if undeclared else "yes")


COLUMNS = ("mod", "jar", "container", "config", "config_side", "plugin", "mixin_class",
           "member_kind", "member_name", "member_desc", "annotation", "intent", "cancellable",
           "target_class", "target_kind", "target_owner_mod", "target_member", "target_desc",
           "at_value", "at_target", "config_declared")


# ------------------------------------------------------------------ analysis

def esc(s):
    return s.replace("|", "\\|")


def pct(a, b):
    return "%d%%" % round(100.0 * a / b) if b else "0%"


def top_counts(counter, n):
    return ", ".join("`%s` %d" % (esc(k), v) for k, v in
                     sorted(counter.items(), key=lambda kv: (-kv[1], kv[0]))[:n])


def member_id(row):
    return row[0:3] + row[6:11]


def in_scope(row):
    """Common or server mixin class of a config that the jar declares."""
    return row[4] in ("common", "server") and row[20] == "yes"


def analyse(rows):
    srv = [r for r in rows if in_scope(r) and r[14] == "vanilla" and r[11] != "unique"]
    members = collections.defaultdict(set)
    member_intent = {}
    member_mod = {}
    per_key = collections.defaultdict(lambda: {
        "mods": set(), "members": set(), "intents": collections.Counter(),
        "calls": collections.Counter(), "sites": set(), "descs": set()})
    for r in srv:
        mid = member_id(r)
        key = (r[13], r[16] if r[16] else "(no target)", r[7] == "field" or r[17] == "field")
        members[mid].add(key)
        member_intent[mid] = r[11]
        member_mod[mid] = r[0]
        k = per_key[key]
        k["mods"].add(r[0])
        if mid not in k["members"]:
            k["members"].add(mid)
            k["intents"][r[11]] += 1
        if r[17] and r[17] != "field":
            k["descs"].add(r[17])
        if r[11] in ("value", "mid") and r[18]:
            k["sites"].add((r[18], r[19]))
            if r[10] in ("Redirect", "WrapOperation") and r[19]:
                k["calls"][call_target(r[19])] += 1
    return srv, members, member_intent, member_mod, per_key


def key_label(key):
    cls, name, field = key
    return "%s.%s%s" % (short(cls), name, " (field)" if field else "")


def coverage(order, members, member_mod, all_mods):
    """Methods needed (in this order) to cover 50/80/90/100% of members and mods."""
    rank = {k: i for i, k in enumerate(order)}
    need_member = sorted(max(rank[k] for k in ks) + 1 for ks in members.values())
    need_mod = {m: 0 for m in all_mods}
    for mid, ks in members.items():
        m = member_mod[mid]
        need_mod[m] = max(need_mod[m], max(rank[k] for k in ks) + 1)
    mod_needs = sorted(need_mod.values())
    out = {}
    for label, needs in (("members", need_member), ("mods", mod_needs)):
        res = []
        for p in (50, 80, 90, 100):
            idx = max(0, -(-len(needs) * p // 100) - 1)
            res.append(needs[idx])
        out[label] = res
    return out


def greedy_mod_order(members, member_mod, base_order):
    """Order of target members that adds, at each step, the mod that needs the fewest new methods."""
    mod_keys = collections.defaultdict(set)
    for mid, ks in members.items():
        mod_keys[member_mod[mid]].update(ks)
    base_rank = {k: i for i, k in enumerate(base_order)}
    have = set()
    order = []
    left = dict(mod_keys)
    while left:
        mod = min(left, key=lambda m: (len(left[m] - have), m))
        new = sorted(left.pop(mod) - have, key=lambda k: base_rank[k])
        order.extend(new)
        have.update(new)
    return order


def report(rows, n_mods, stats):
    out = []
    w = out.append
    srv, members, member_intent, member_mod, per_key = analyse(rows)
    keys = sorted(per_key, key=lambda k: (-len(per_key[k]["mods"]), -len(per_key[k]["members"]),
                                          key_label(k)))
    by_members = sorted(per_key, key=lambda k: (-len(per_key[k]["members"]), -len(per_key[k]["mods"]),
                                                key_label(k)))
    srv_mods = sorted({member_mod[m] for m in members})
    all_members = {member_id(r) for r in rows}
    srv_all = {member_id(r) for r in rows if in_scope(r)}
    unique_srv = {member_id(r) for r in rows if in_scope(r) and r[11] == "unique"
                  and r[14] == "vanilla"}
    intents_total = collections.Counter(member_intent.values())
    anno_total = collections.Counter()
    seen = set()
    for r in srv:
        if member_id(r) not in seen:
            seen.add(member_id(r))
            anno_total[r[10]] += 1
    target_kinds = collections.Counter()
    seen = set()
    for r in rows:
        if in_scope(r) and r[11] != "unique":
            k = (member_id(r), r[14])
            if k not in seen:
                seen.add(k)
                target_kinds[r[14]] += 1
    multi = sum(1 for ks in members.values() if len(ks) > 1)
    undeclared = {member_id(r) for r in rows if r[4] in ("common", "server") and r[20] == "no"
                  and r[14] == "vanilla" and r[11] != "unique"}
    undeclared_cfg = {(r[1], r[3]) for r in rows if r[20] == "no" and r[3]}
    plugins = {r[0] for r in srv if r[5]}
    classes = {k[0] for k in per_key}

    w("## 9. Mixin targets by method\n")
    w("This section sizes the two primitives of the native mod channel (the NeoForge-shaped API "
      "and compile-time source patches) by the members of the mixin classes, not by the classes. "
      "Section 8 counts the mixin classes; this section reads every member of every mixin class "
      "and its target member.\n")
    w("### 9.1 Method\n")
    w("The scanner is `tools/modpack-scan/mixin_scan.py` (its README gives the command). It reads "
      "the %d jars of the folder and their Jar-in-Jar jars with `zipfile` and its own class file "
      "parser, and it does not load or run code from the jars. A class is a mixin class when it "
      "carries `@Mixin`. The scanner reads `RuntimeInvisibleAnnotations` and "
      "`RuntimeVisibleAnnotations` of the class, its fields and its methods, with all element "
      "values. The mixin configs are the JSON files at the root of a jar that have a `package` "
      "and a `mixins`, `client` or `server` list. %d class files with the `@Mixin` descriptor "
      "did not parse.\n" % (n_mods, stats["errors"]))
    w("Read errors: %d jars or entries were skipped because they are larger than the size limit or "
      "do not decompress.\n" % stats["read_errors"])
    w("For each member the scanner records the annotation, the target class (from `value` and "
      "`targets` of `@Mixin`), the target member, the `@At` value and target, `cancellable`, and "
      "the mixin plugin class of the config. The target member comes from `method` of the "
      "injector (a selector with or without descriptor; overloads merge by name), from `value` "
      "of `@Accessor` and `@Invoker` or from the method name without the `get`, `set`, `is`, "
      "`call` or `invoke` prefix, and from the member name for `@Shadow` and `@Overwrite`. "
      "Names are Mojang names: NeoForge runs Mojang mappings in production.\n")
    w("Scope: members of mixin classes listed in the `mixins` (common) or `server` list of a "
      "config that the jar declares (in `[[mixins]]` of its `mods.toml` or in `MixinConfigs` of its "
      "manifest), with a vanilla target (`net.minecraft` or `com.mojang`) outside "
      "`net.minecraft.client` and `com.mojang.blaze3d`. `@Unique` members add a new member to the "
      "target and target no member: they are counted apart. `@Mutable` is a modifier of "
      "`@Shadow` and is not a member. A member with two target classes or two selectors counts "
      "once, and it needs all its target members.\n")
    w("Intent of a member:\n")
    w("- **pre**: `@Inject` at `HEAD`, not cancellable.\n"
      "- **post**: `@Inject` at `RETURN` or `TAIL`, not cancellable.\n"
      "- **mid**: `@Inject` at a point inside the method (`INVOKE`, `FIELD`, `NEW` and others), "
      "not cancellable.\n"
      "- **cancel**: `@Inject` with `cancellable = true` at any point, and `@WrapMethod` (it can "
      "skip the original method).\n"
      "- **value**: a value modifier: `@Redirect`, `@WrapOperation`, `@ModifyArg`, `@ModifyArgs`, "
      "`@ModifyConstant`, `@ModifyExpressionValue`, `@ModifyVariable`, `@ModifyReturnValue`, "
      "`@ModifyReceiver`, `@WrapWithCondition`.\n"
      "- **overwrite**: `@Overwrite`, a whole-method override.\n"
      "- **accessor**: `@Accessor`, `@Invoker`, `@Shadow`.\n")
    w("The analysis groups the intents in four classes: accessor (accessor), hook (pre, post, "
      "cancel), call site (mid, value) and override (overwrite). The classes describe what the "
      "mixin does. The two primitives of the native channel serve them as follows:\n")
    w("- **accessor**: a source patch, or the NeoForge-shaped API when it exposes the field or "
      "method.\n"
      "- **hook**: a NeoForge event when one fires on that method, otherwise a source patch.\n"
      "- **call site**: a source patch.\n"
      "- **override**: a source patch.\n")

    w("### 9.2 Totals\n")
    w("| Measure | Value |\n|:--|--:|")
    w("| Mixin members, all sides and targets (`@Unique` included) | %d |" % len(all_members))
    w("| Members of common or server mixin classes of declared configs (`@Unique` included) | %d |" % len(srv_all))
    w("| Server-side members with a vanilla target (the scope) | %d |" % len(members))
    w("| Mods with at least one member in the scope | %d of %d |" % (len(srv_mods), n_mods))
    w("| Distinct target members in the scope (methods, and fields for accessors) | %d |" % len(per_key))
    w("| Vanilla target classes in the scope | %d |" % len(classes))
    w("| Members with more than one target member | %d |" % multi)
    w("| `@Unique` members in common or server mixin classes with a vanilla target | %d |" % len(unique_srv))
    w("| Mods in the scope whose config names a mixin plugin | %d |" % len(plugins))
    w("| Configs with mixin members that the jar does not declare | %d |" % len(undeclared_cfg))
    w("| Common or server members with a vanilla target in those configs (left out) | %d |" % len(undeclared))
    w("")
    fabric_cfg = {c for c in undeclared_cfg if "fabric" in c[1].lower()}
    fabric_left = {member_id(r) for r in rows if (r[1], r[3]) in fabric_cfg and member_id(r) in undeclared}
    # Section 8 counts a mixin class as server-side when its config does not list it as client.
    class_mods = {c[0] for c in stats["mixin_classes"] if c[1] != "client" and "vanilla" in c[3]}
    w("The mixins section counts %d mods with a common or server mixin class into vanilla server "
      "code; this scope counts %d, because the other %d mods have only mixin classes with no "
      "members in the scope or mixin classes in configs that the jar does not declare. A config "
      "that the jar does not declare does not load through the jar metadata. %d of the %d "
      "have `fabric` in the file name: Fabric configs that multi-loader jars carry, some with Fabric "
      "intermediary names (`net.minecraft.class_1309`). %d of the %d members left out are in them. "
      "A mixin plugin or mod code can still add a config at run time; the scan does not follow "
      "code.\n" % (len(class_mods), len(srv_mods), len(class_mods) - len(srv_mods), len(fabric_cfg),
                   len(undeclared_cfg), len(fabric_left), len(undeclared)))
    w("Members of common or server mixin classes of declared configs by target kind (`@Unique` left out): %s.\n" % ", ".join(
        "%s %d" % (k, target_kinds[k]) for k in ("vanilla", "vanilla-client", "neoforge", "other", "own",
                                                  "unknown", "none") if target_kinds[k]))
    w("Members in the scope by intent: %s.\n" % ", ".join("%s %d (%s)" % (i, intents_total[i], pct(
        intents_total[i], len(members))) for i in INTENTS))
    cancel_at = collections.defaultdict(set)
    for r in srv:
        if r[11] == "cancel":
            cancel_at[member_id(r)].add("WrapMethod" if r[10] == "WrapMethod" else r[18].upper())
    cancel_kind = collections.Counter(
        "`@WrapMethod`" if v == {"WrapMethod"} else "at `HEAD`" if v <= {"HEAD"}
        else "at `RETURN` or `TAIL`" if v <= {"RETURN", "TAIL"} else "at another point"
        for v in cancel_at.values())
    w("Cancel members by point: %s.\n" % ", ".join(
        "%s %d" % (k, n) for k, n in sorted(cancel_kind.items(), key=lambda kv: (-kv[1], kv[0]))))
    w("Members in the scope by annotation: %s.\n" % ", ".join(
        "%s %d" % (a, n) for a, n in sorted(anno_total.items(), key=lambda kv: (-kv[1], kv[0]))))

    w("### 9.3 Coverage curve\n")
    n_cov_mods = len(srv_mods)
    hooks = {mid: ks for mid, ks in members.items() if member_intent[mid] != "accessor"}
    hook_count = collections.Counter(k for ks in hooks.values() for k in ks)
    hook_keys = sorted(hook_count, key=lambda k: (-hook_count[k], -len(per_key[k]["mods"]), key_label(k)))
    w("A set of target members covers a member when the set holds all target members of that "
      "member. It covers a mod when it covers every server-side member of the mod with a vanilla "
      "target (%d mods). Mixins into NeoForge and into other mods are out of this count (section "
      "9.6). Three orders of the target members:\n" % n_cov_mods)
    w("- **by members**: target members ordered by the number of mixin members, highest first.\n"
      "- **by mods**: the order of the table in section 9.4.\n"
      "- **cheapest mod first**: at each step the set adds all target members of the mod that "
      "needs the fewest new ones. This order covers the most mods for a number of target "
      "members.\n")
    w("The rows `all` count all members. The rows `not accessors` count the members of the hook, "
      "call site and override classes only. A mod with accessor members only is covered at 0 in "
      "those rows.\n")
    w("Target members needed to cover a share of the members or of the %d mods:\n" % n_cov_mods)
    w("| Members counted | Order | Covers | Total | 50% | 80% | 90% | 100% |")
    w("|:--|:--|:--|--:|--:|--:|--:|--:|")
    hook_by_mods = sorted(hook_count, key=lambda k: keys.index(k))
    for label, subset, order, table_order in (("all", members, by_members, keys),
                                              ("not accessors", hooks, hook_keys, hook_by_mods)):
        runs = (("by members", coverage(order, subset, member_mod, srv_mods)),
                ("by mods", coverage(table_order, subset, member_mod, srv_mods)),
                ("cheapest mod first", coverage(greedy_mod_order(subset, member_mod, order), subset,
                                                member_mod, srv_mods)))
        for what, total in (("members", len(subset)), ("mods", n_cov_mods)):
            for name, cov in runs:
                w("| %s | %s | %s | %d | %s |" % (label, name, what, total,
                                                 " | ".join(str(x) for x in cov[what])))
    w("")
    top_cover = sum(1 for ks in members.values() if all(k in set(keys[:TOP_METHODS]) for k in ks))
    top_set = set(keys[:TOP_METHODS])
    mod_cov = collections.defaultdict(lambda: True)
    for mid, ks in members.items():
        mod_cov[member_mod[mid]] &= all(k in top_set for k in ks)
    w("The %d target members of the table in section 9.4 cover %d of %d members (%s) and %d of %d "
      "mods (%s).\n" % (TOP_METHODS, top_cover, len(members), pct(top_cover, len(members)),
                        sum(mod_cov.values()), n_cov_mods, pct(sum(mod_cov.values()), n_cov_mods)))
    hook_cover = sum(1 for ks in hooks.values() if ks <= top_set)
    hook_mod_cov = {m: True for m in srv_mods}
    for mid, ks in hooks.items():
        hook_mod_cov[member_mod[mid]] &= ks <= top_set
    w("With accessors left out, the same %d target members cover %d of %d members that are not "
      "accessors "
      "(%s) and %d of %d mods (%s).\n" % (
          TOP_METHODS, hook_cover, len(hooks), pct(hook_cover, len(hooks)), sum(hook_mod_cov.values()),
          n_cov_mods, pct(sum(hook_mod_cov.values()), n_cov_mods)))

    member_classes = {mid: {k[0] for k in ks} for mid, ks in members.items()}
    class_count = collections.Counter(c for cs in member_classes.values() for c in cs)
    class_order = sorted(class_count, key=lambda c: (-class_count[c], c))
    rank = {c: i + 1 for i, c in enumerate(class_order)}
    need = sorted(max(rank[c] for c in cs) for cs in member_classes.values())
    marks = [need[max(0, -(-len(need) * p // 100) - 1)] for p in (50, 80, 90, 100)]
    w("By class: the vanilla classes ordered by members, highest first, cover 50%%, 80%%, 90%% and "
      "100%% of the members with %s classes.\n" % ", ".join(str(m) for m in marks))
    w("### 9.4 Target members by number of mods\n")
    w("The %d most targeted vanilla server-side members, by mods, then by mixin members. Class names "
      "drop the `net.minecraft.` prefix. **Desc** is the number of distinct descriptors that "
      "selectors name (0: the selectors give the name only). The intent columns count members "
      "(section 9.1). **Call targets** are the most common `@At` targets of `@Redirect` and "
      "`@WrapOperation`, with their count.\n" % TOP_METHODS)
    w("| # | Target member | Mods | Members | Desc | pre | post | mid | cancel | value | overwrite | accessor | Call targets |")
    w("|--:|:--|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|:--|")
    for i, k in enumerate(keys[:TOP_METHODS], 1):
        d = per_key[k]
        w("| %d | `%s` | %d | %d | %d | %s | %s |" % (
            i, esc(key_label(k)), len(d["mods"]), len(d["members"]), len(d["descs"]),
            " | ".join(str(d["intents"][x]) for x in INTENTS), top_counts(d["calls"], 3) or "-"))
    w("")
    tail = keys[TOP_METHODS:]
    tail_by_class = collections.defaultdict(lambda: {"keys": 0, "members": set(), "mods": set(),
                                                     "intents": collections.Counter()})
    for k in tail:
        t = tail_by_class[k[0]]
        t["keys"] += 1
        t["members"].update(per_key[k]["members"])
        t["mods"].update(per_key[k]["mods"])
        t["intents"].update(per_key[k]["intents"])
    tail_members = set()
    for k in tail:
        tail_members.update(per_key[k]["members"])
    multi_mod = [c for c in tail_by_class if len(tail_by_class[c]["mods"]) > 1]
    single = [c for c in tail_by_class if len(tail_by_class[c]["mods"]) == 1]
    w("Long tail: %d more target members in %d classes, with %d members. The classes of the long "
      "tail that 2 or more mods target, by mods (target members, members, mods, intents):\n" % (
          len(tail), len(tail_by_class), len(tail_members)))
    w("| Class | Target members | Members | Mods | Intents |\n|:--|--:|--:|--:|:--|")
    for cls in sorted(multi_mod, key=lambda c: (-len(tail_by_class[c]["mods"]),
                                                -len(tail_by_class[c]["members"]), c)):
        t = tail_by_class[cls]
        w("| `%s` | %d | %d | %d | %s |" % (esc(short(cls)), t["keys"], len(t["members"]), len(t["mods"]),
                                            ", ".join("%s %d" % (x, t["intents"][x]) for x in INTENTS
                                                      if t["intents"][x])))
    w("")
    pkg = collections.defaultdict(lambda: [0, 0, set()])
    for cls in single:
        name = short(cls)
        group = ".".join(name.split(".")[:2]) if name.startswith("world.") else name.split(".")[0]
        pkg[group][0] += 1
        pkg[group][1] += len(tail_by_class[cls]["members"])
        pkg[group][2].update(tail_by_class[cls]["mods"])
    w("The other %d classes of the long tail have one mod each (%d members). By package "
      "(classes, members, mods): %s.\n" % (
          len(single), sum(len(tail_by_class[c]["members"]) for c in single),
          ", ".join("`%s` %d/%d/%d" % (g, v[0], v[1], len(v[2])) for g, v in
                    sorted(pkg.items(), key=lambda gv: (-gv[1][1], gv[0])))))

    w("### 9.5 Intent classes per target member\n")
    w("For each target member of section 9.4: the members in each intent class (section 9.1), and "
      "the strongest class. **Accessor**: `@Accessor`, `@Invoker`, `@Shadow`; a source patch serves "
      "it, or the NeoForge-shaped API when it exposes the field or method. **Hook**: pre, post and "
      "cancel, at the start or the end of the method; a NeoForge event serves it when one fires on "
      "that method, otherwise a source patch. **Call site**: value modifiers and mid-method "
      "injections, at an identified call, field access or constant inside the method; a source "
      "patch serves it. **Sites** is the number of distinct `@At` points. **Override**: "
      "`@Overwrite`, a whole-method override; a source patch serves it. **Strongest** is the "
      "strongest class of the members of the target member, in the order accessor, hook, call "
      "site, override. A target member whose strongest class is call site or override needs a "
      "source patch.\n")
    w("| # | Target member | Mods | Accessor | Hook | Call site | Sites | Override | Strongest |")
    w("|--:|:--|--:|--:|--:|--:|--:|--:|:--|")
    need_count = collections.Counter()
    for i, k in enumerate(keys[:TOP_METHODS], 1):
        d = per_key[k]
        prim = collections.Counter()
        for x, n in d["intents"].items():
            prim[INTENT_PRIMITIVE[x]] += n
        need = max((p for p in prim if prim[p]), key=lambda p: PRIMITIVE_RANK[p])
        need_count[need] += 1
        w("| %d | `%s` | %d | %d | %d | %d | %d | %d | %s |" % (
            i, esc(key_label(k)), len(d["mods"]), prim["accessor"], prim["hook"], prim["call site"],
            len(d["sites"]), prim["override"], need))
    w("")
    w("Strongest class over the %d target members: %s.\n" % (
        TOP_METHODS, ", ".join("%s %d" % (p, need_count[p]) for p in PRIMITIVE_RANK)))
    site_members = collections.defaultdict(set)
    for r in srv:
        if r[11] in ("value", "mid") and r[18]:
            site_members[(r[13], r[16], r[18].upper(), r[19])].add(member_id(r))
    w("Call-site points over all target members: %d distinct points (target member, `@At` value and "
      "target); %d of them have one member.\n" % (
          len(site_members), sum(1 for v in site_members.values() if len(v) == 1)))

    w("#### Classes that mods replace\n")
    w("These are the classes whose logic mods replace instead of hooking it: the classes that #51 "
      "lists, and every other class in the scope whose members are mostly `@Overwrite` or "
      "`@Redirect` (half or more of its members that are not accessors, with at least 2 mods). "
      "Their members become a source patch, or a NeoForge API row where the inventory has one. "
      "Columns: mods (all members), accessor members, members that are not accessors, "
      "`@Overwrite`, `@Redirect`, `@WrapOperation`, the share of `@Overwrite` and `@Redirect`, "
      "and the most targeted members.\n")
    cls_stats = collections.defaultdict(lambda: {"mods": set(), "members": set(), "anno": collections.Counter(),
                                                 "keys": collections.Counter(), "accessors": 0})
    seen = set()
    for r in srv:
        mid = member_id(r)
        c = cls_stats[r[13]]
        c["mods"].add(r[0])
        if (mid, r[13]) in seen:
            continue
        seen.add((mid, r[13]))
        if r[11] == "accessor":
            c["accessors"] += 1
            continue
        c["members"].add(mid)
        c["anno"][r[10]] += 1
        c["keys"][r[16]] += 1
    cands = []
    for cls, c in cls_stats.items():
        n = len(c["members"])
        share = (c["anno"]["Overwrite"] + c["anno"]["Redirect"]) / n if n else 0.0
        if cls in REPLACE_NAMED or (share >= 0.5 and len(c["mods"]) >= 2):
            cands.append((cls, share))
    w("| Class | Mods | Accessors | Members | Overwrite | Redirect | WrapOperation | Share | Top members |")
    w("|:--|--:|--:|--:|--:|--:|--:|--:|:--|")
    for cls, share in sorted(cands, key=lambda cs: (-len(cls_stats[cs[0]]["mods"]), cs[0])):
        c = cls_stats[cls]
        w("| `%s` | %d | %d | %d | %d | %d | %d | %s | %s |" % (
            esc(short(cls)), len(c["mods"]), c["accessors"], len(c["members"]), c["anno"]["Overwrite"],
            c["anno"]["Redirect"], c["anno"]["WrapOperation"],
            pct(c["anno"]["Overwrite"] + c["anno"]["Redirect"], len(c["members"])) if c["members"] else "-",
            top_counts(c["keys"], 3) or "-"))
    for cls in REPLACE_NAMED:
        if cls not in cls_stats:
            w("| `%s` | 0 | 0 | 0 | 0 | 0 | 0 | - | - |" % short(cls))
    w("")

    w("### 9.6 Mixins into other mods\n")
    other = [r for r in rows if in_scope(r) and r[14] == "other" and r[11] != "unique"]
    o_stats = collections.defaultdict(lambda: {"mods": set(), "members": set(), "owner": "",
                                               "intents": collections.Counter(), "keys": collections.Counter()})
    seen = set()
    for r in other:
        mid = member_id(r)
        o = o_stats[r[13]]
        o["mods"].add(r[0])
        o["owner"] = r[15]
        if (mid, r[13]) in seen:
            continue
        seen.add((mid, r[13]))
        o["members"].add(mid)
        o["intents"][r[11]] += 1
        o["keys"][r[16]] += 1
    o_members = {member_id(r) for r in other}
    o_mods = {r[0] for r in other}
    o_owners = collections.Counter()
    for cls, o in o_stats.items():
        o_owners[o["owner"]] += len(o["members"])
    w("Members of common or server mixin classes whose target class is in another jar of the pack: "
      "%d members in %d mods, into %d classes of %d mods. Most targeted mods by members: %s.\n" % (
          len(o_members), len(o_mods), len(o_stats), len(o_owners), top_counts(o_owners, 8)))
    w("The %d most targeted classes of other mods, by mods, then by members:\n" % TOP_OTHER)
    w("| # | Class | Owner | Mods | Members | Intents | Top members |\n|--:|:--|:--|--:|--:|:--|:--|")
    for i, cls in enumerate(sorted(o_stats, key=lambda c: (-len(o_stats[c]["mods"]),
                                                           -len(o_stats[c]["members"]), c))[:TOP_OTHER], 1):
        o = o_stats[cls]
        w("| %d | `%s` | %s | %d | %d | %s | %s |" % (
            i, esc(cls), o["owner"], len(o["mods"]), len(o["members"]),
            ", ".join("%s %d" % (x, o["intents"][x]) for x in INTENTS if o["intents"][x]),
            top_counts(o["keys"], 3)))
    w("")
    return "\n".join(out) + "\n"


# ---------------------------------------------------------------------- main

def main(argv):
    if len(argv) != 3:
        sys.stderr.write("usage: python3 -I mixin_scan.py <mods-dir> <out-dir>\n")
        return 2
    mods_dir, out_dir = argv[1], argv[2]
    jars = sorted(os.path.join(mods_dir, f) for f in os.listdir(mods_dir) if f.endswith(".jar"))
    # fork needs no socket, so the scan also runs inside a sandbox without AF_UNIX.
    ctx = multiprocessing.get_context("fork" if "fork" in multiprocessing.get_all_start_methods() else None)
    with concurrent.futures.ProcessPoolExecutor(max_workers=min(32, os.cpu_count() or 1), mp_context=ctx) as ex:
        scans = list(ex.map(scan_jar, jars, chunksize=1))
    mod_of = []
    owners = collections.defaultdict(set)
    errors = 0
    read_errors = 0
    for i, (jar, conts) in enumerate(zip(jars, scans)):
        mid = conts[0]["mod_id"] or next((c["mod_id"] for c in conts if c["mod_id"]), None)
        mod_of.append(mid or os.path.basename(jar)[:-4])
        for c in conts:
            errors += c["errors"]
            read_errors += c["read_errors"]
            for cname in c["classes"]:
                owners[cname].add(i)
    dup = collections.Counter(mod_of)
    mod_of = [m if dup[m] == 1 else "%s (%s)" % (m, os.path.basename(j)) for m, j in zip(mod_of, jars)]
    rows = []
    mixin_classes = []
    for i, (jar, conts) in enumerate(zip(jars, scans)):
        configs_outer = [cfg for c in conts for cfg in c["configs"]]
        declared = set().union(*(c["declared"] for c in conts))
        for c in conts:
            rows.extend(members_of(mod_of[i], os.path.basename(jar), c, configs_outer, declared,
                                   owners, mod_of, i, mixin_classes))
    rows = sorted(set(rows))
    os.makedirs(out_dir, exist_ok=True)
    with open(os.path.join(out_dir, "members.tsv"), "w", encoding="utf-8", newline="\n") as f:
        f.write("\t".join(COLUMNS) + "\n")
        for r in rows:
            f.write("\t".join(str(x).replace("\t", " ").replace("\n", " ") for x in r) + "\n")
    with open(os.path.join(out_dir, "report.md"), "w", encoding="utf-8", newline="\n") as f:
        f.write(report(rows, len(jars), {"errors": errors, "read_errors": read_errors,
                                                 "mixin_classes": mixin_classes}))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
