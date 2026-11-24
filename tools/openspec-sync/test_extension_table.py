"""Tests for extension_methods.py and extension_table.py.

Run: python3 -I tools/openspec-sync/test_extension_table.py
"""

import contextlib
import io
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import extension_methods as em  # noqa: E402
import extension_table as et  # noqa: E402

WIDGET = """\
package net.neoforged.neoforge.common.extensions;

/** Javadoc with a (fake) method(int x) { inside. */
public interface IWidgetExtension {
    // default void commented() {}
    default String label() { return "a { b"; }
    default int size(int a) { return a; }
    default int size(int a, int b) { return a + b; }
    boolean open();
    static int create(Supplier<List<String>> s, int x) { return x; }
    private Widget self() { return (Widget) this; }
    default <T extends Foo, K> Map<T, K> pick(Map<T, K> map, BiFunction<T, K, K> f) { return map; }
    public static enum Mode { A, B }
}
"""
GADGET = """\
package net.neoforged.neoforge.common.extensions;
public interface IGadgetExtension {
    default void ping() {}
    void make();
    default void make(int n) {}
    static void make(String s) {}
}
"""
EMPTY = """\
package net.neoforged.neoforge.common.extensions;
public interface IEmptyExtension {
//    private Thing self() { return null; }
}
"""
INJECTED = '{"net/minecraft/Widget": ["net/neoforged/neoforge/common/extensions/IWidgetExtension"]}'
ROWS = """\
# comment
IWidgetExtension#*\tplanned\tboth\t`Widget` hook\tM5 engine extension: no hook\t
IWidgetExtension#open\tsupported\tboth\t`Widget::open`\tcalled at the vanilla site\ta_test_that_exists
IWidgetExtension#label\tclient-only\tclient\t-\tonly the client calls it\t
IGadgetExtension#*\tnot supported\tboth\t-\tdata generation\t
IEmptyExtension\tnot supported\tboth\t-\tno methods\t
IWidgetExtension.Mode\tnot supported\tboth\t-\ta Java enum\t
"""
DEMAND = "interface\\tmethod\\tparams\\tcalled_by\\toverridden_by\\tmods\n".replace("\\t", "\t") + (
    "IWidgetExtension\topen\t0\t3\t1\t4\n"
    "IWidgetExtension\tsize\t1\t2\t0\t2\n"
    "IWidgetExtension\tsize\t2\t7\t0\t7\n"
    "IGadgetExtension\tping\t0\t1\t0\t1\n"
)
DESIGN = """\
## Inventory

### Summary per family

| Family | Rows | Supported | Planned | Client-only | Not supported |
|:--|:--|:--|:--|:--|:--|
| Events | 2 | 1 | 1 | 0 | 0 |
| Extension interfaces | 1 | 0 | 1 | 0 | 0 |
| Total | 3 | 1 | 2 | 0 | 0 |

### Extension interfaces NeoForge adds to `net.minecraft`

old text

### Other packages

text
"""
MAPPING = """\
## Inventory rows

### Extension interfaces NeoForge adds to `net.minecraft`

| NeoForge item | Mods | Section | Counterpart | Status |
|:--|--:|:--|:--|:--|
| `Old` | 1 | x | y | planned |

### Other packages

text
"""


class Fixture:
    def __init__(self):
        self.tmp = tempfile.TemporaryDirectory()
        root = Path(self.tmp.name)
        ext = root / "nf" / em.EXTENSIONS
        ext.mkdir(parents=True)
        for name, text in (("IWidgetExtension", WIDGET), ("IGadgetExtension", GADGET),
                           ("IEmptyExtension", EMPTY)):
            (ext / f"{name}.java").write_text(text)
        (ext / "package-info.java").write_text("package x;\n")
        (root / "nf/META-INF").mkdir()
        (root / "nf" / em.INJECTED).write_text(INJECTED)
        (root / "crates").mkdir()
        (root / "crates/lib.rs").write_text("#[test]\nfn a_test_that_exists() {}\n")
        (root / "doc").mkdir()
        (root / "doc/design.md").write_text(DESIGN)
        (root / "doc/mapping-table.md").write_text(MAPPING)
        self.root = root
        self.write("rows.tsv", ROWS)
        self.write("demand.tsv", DEMAND)

    def write(self, name, text):
        (self.root / name).write_text(text)

    def run(self, check=False):
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            code = et.run(self.root / "doc", self.root / "nf", self.root / "rows.tsv",
                          self.root / "demand.tsv", self.root / "crates", check)
        return code

    def text(self, name):
        return (self.root / "doc" / name).read_text()

    def close(self):
        self.tmp.cleanup()


class ReaderTest(unittest.TestCase):
    def parse(self, text):
        with tempfile.TemporaryDirectory() as d:
            path = Path(d) / "X.java"
            path.write_text(text)
            return em.parse_interface(path)

    def test_methods_overloads_and_private(self):
        groups = {n: (k, len(o)) for n, k, o in em.groups(self.parse(WIDGET))}
        self.assertEqual(list(groups), ["label", "size", "open", "create", "pick"])
        self.assertEqual(groups["size"], ("default method, 2 overloads", 2))
        self.assertEqual(groups["open"], ("abstract method", 1))
        self.assertEqual(groups["create"], ("static method", 1))

    def test_comments_strings_and_nested_types(self):
        parsed = self.parse(WIDGET)
        self.assertNotIn("commented", [m["name"] for m in parsed["methods"]])
        self.assertEqual([n["name"] for n in parsed["nested"]], ["Mode"])

    def test_same_name_in_two_kinds_is_one_row(self):
        groups = {n: k for n, k, _ in em.groups(self.parse(GADGET))}
        self.assertEqual(groups["make"], "abstract and default and static method, 3 overloads")

    def test_generic_parameters_stay_whole(self):
        pick = next(m for m in self.parse(WIDGET)["methods"] if m["name"] == "pick")
        self.assertEqual(len(pick["params"]), 2)

    def test_injected_targets(self):
        self.assertEqual(em.load_injected(Path(self.fixture.root) / "nf"),
                         {"IWidgetExtension": ["Widget"]})

    def setUp(self):
        self.fixture = Fixture()

    def tearDown(self):
        self.fixture.close()


class TableTest(unittest.TestCase):
    def setUp(self):
        self.f = Fixture()

    def tearDown(self):
        self.f.close()

    def test_rows_and_counts(self):
        self.assertEqual(self.f.run(), 0)
        design = self.f.text("design.md")
        self.assertIn("| `IWidgetExtension#open` | abstract method | both | supported |", design)
        self.assertIn("| `IWidgetExtension#size` | default method, 2 overloads | both | planned |", design)
        self.assertIn("(target `Widget`; 5 rows: 1 supported, 3 planned, 1 client-only)", design)
        self.assertIn("| Extension interfaces | 9 | 1 | 3 | 1 | 4 |", design)
        self.assertIn("| Total | 11 | 2 | 4 | 1 | 4 |", design)
        self.assertIn("| `IEmptyExtension` | interface, no public method |", design)

    def test_mapping_rows_by_demand_without_client_only(self):
        self.f.run()
        mapping = self.f.text("mapping-table.md")
        keys = [line.split("|")[1].strip() for line in mapping.splitlines() if line.startswith("| `")]
        self.assertEqual(keys[:3], ["`IWidgetExtension#size`", "`IWidgetExtension#open`",
                                     "`IGadgetExtension#ping`"])
        self.assertNotIn("IWidgetExtension#label", mapping)
        self.assertIn("| `IWidgetExtension#size` | 7 |", mapping)

    def test_a_second_run_changes_nothing(self):
        self.f.run()
        first = (self.f.text("design.md"), self.f.text("mapping-table.md"))
        self.assertEqual(self.f.run(), 0)
        self.assertEqual(self.f.run(check=True), 0)
        self.assertEqual((self.f.text("design.md"), self.f.text("mapping-table.md")), first)

    def test_check_exits_1_and_writes_nothing(self):
        self.assertEqual(self.f.run(check=True), 1)
        self.assertEqual(self.f.text("design.md"), DESIGN)

    def test_errors(self):
        for name, mutate, text in (
            ("a method without a row", lambda r: r.replace("IGadgetExtension#*", "IGadgetExtension#ping"), "no row for"),
            ("a key without a method", lambda r: r + "IWidgetExtension#gone\tplanned\tboth\t-\tr\t\n", "matches no method"),
            ("a test that is missing", lambda r: r.replace("a_test_that_exists", "no_such_test"), "is not a function"),
            ("a supported row without a test", lambda r: r.replace("\ta_test_that_exists", "\t"), "needs a test"),
            ("an empty reason", lambda r: r.replace("data generation", ""), "empty reason"),
            ("an unknown status", lambda r: r.replace("not supported", "maybe"), "unknown status"),
        ):
            with self.subTest(name):
                self.f.write("rows.tsv", mutate(ROWS))
                with self.assertRaises(et.TableError) as cm:
                    self.f.run()
                self.assertIn(text, str(cm.exception))


if __name__ == "__main__":
    unittest.main()
