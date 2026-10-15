"""Tests for mapping_status.py. Run: python3 -I tools/openspec-sync/test_mapping_status.py"""

import io
import sys
import tempfile
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import mapping_status as ms  # noqa: E402

DESIGN = """\
## Inventory

### Summary per family

| Family | Rows | Supported | Planned | Client-only | Not supported |
|:--|:--|:--|:--|:--|:--|
| Total | 5 | 1 | 2 | 1 | 1 |

### Events

#### `event` (3)

| Event | Bus | Side | Cancel | Result | Status | Pumpkin event or concept | Reason |
|:--|:--|:--|:--|:--|:--|:--|:--|
| AlphaEvent (abstract) | game | both | no | - | planned | - | r |
| AlphaEvent.Beta | game | both | no | - | supported | `X` | r |
| ClientOnlyEvent | game | client | no | - | client-only | - | r |

#### Events in other packages (1)

| Event | Bus | Side | Cancel | Result | Status | Pumpkin event or concept | Reason |
|:--|:--|:--|:--|:--|:--|:--|:--|
| Gamma (`registries`) | mod | both | no | - | planned | - | r |

### Lifecycle, entry points and metadata

#### `neoforge.mods.toml` keys (1)

| Section | Key | Read by | Side | Status | Pumpkin concept | Reason |
|:--|:--|:--|:--|:--|:--|:--|
| file | `modLoader` | FML | both | not supported | - | r |

## Risks
"""

MAPPING = """\
## Coverage

| Part | Rows | Counterpart | Partial | Contract only | Gap or not supported | Used by 1 or more mods | Used by 50 or more mods |
|:--|--:|--:|--:|--:|--:|--:|--:|
| Inventory, supported | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| Inventory, planned | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| Inventory, not supported | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| Modpack classes outside the inventory, NeoForge | 7 | 7 | 0 | 0 | 0 | 7 | 7 |
| Modpack classes outside the inventory, Minecraft and Mojang | 8 | 8 | 0 | 0 | 0 | 8 | 8 |

## Inventory rows

### Events

| NeoForge item | Mods | Section | Counterpart | Status |
|:--|--:|:--|:--|:--|
| `AlphaEvent` | 60 | event class | no case (abstract) | supported |
| `AlphaEvent.Beta` | 52 (21.1 `Old`) | event class | `x` | not supported |
| `Gamma` | 0 | event class | `y` (contract only) | not supported |

### Lifecycle, entry points and metadata

| NeoForge item | Mods | Section | Counterpart | Status |
|:--|--:|:--|:--|:--|
| `file modLoader` | 414 | `neoforge.mods.toml` keys | `lifecycle` doc, not supported | planned |

## Modpack coverage
"""


def lines(text):
    return text.split("\n")


class MappingStatusTest(unittest.TestCase):
    def test_only_the_status_cell_and_the_coverage_rows_change(self):
        new, report = ms.sync(lines(DESIGN), lines(MAPPING))
        self.assertEqual(report["unmatched"], [])
        self.assertEqual(report["problems"], [])
        self.assertEqual(report["client_only"], 1)
        for old_line, new_line in zip(lines(MAPPING), new):
            if old_line.startswith("| `"):
                self.assertEqual(ms.split_row(old_line)[:-1], ms.split_row(new_line)[:-1])
        statuses = [ms.split_row(l)[-1] for l in new if l.startswith("| `")]
        self.assertEqual(statuses, ["planned", "supported", "planned", "not supported"])
        self.assertIn("| Modpack classes outside the inventory, NeoForge | 7 | 7 | 0 | 0 | 0 | 7 | 7 |", new)

    def test_coverage_follows_the_new_statuses(self):
        _, report = ms.sync(lines(DESIGN), lines(MAPPING))
        # rows, counterpart, partial, contract only, gap or not supported, 1+ mods, 50+ mods
        self.assertEqual(report["coverage"], {
            "Inventory, supported": [1, 1, 0, 0, 0, 1, 1],
            "Inventory, planned": [2, 1, 0, 1, 0, 1, 1],
            "Inventory, not supported": [1, 0, 0, 0, 1, 1, 1],
        })

    def test_unmatched_rows_are_listed_and_never_guessed(self):
        design = DESIGN.replace("| Gamma (`registries`)", "| DesignOnly | mod | both | no | - | planned | - | r |\n"
                                "| Gamma (`registries`)")
        mapping = MAPPING.replace("| `Gamma`", "| `MappingOnly` | 1 | event class | `z` | planned |\n| `Gamma`")
        _, report = ms.sync(lines(design), lines(mapping))
        self.assertEqual({(u[0], u[3]): u[4] for u in report["unmatched"]}, {
            ("mapping-table.md", "MappingOnly"): "no design.md row with this key",
            ("design.md", "DesignOnly"): "no mapping-table.md row with this key",
        })

    def test_second_run_changes_nothing(self):
        new, _ = ms.sync(lines(DESIGN), lines(MAPPING))
        again, report = ms.sync(lines(DESIGN), new)
        self.assertEqual(again, new)
        self.assertEqual(report["changes"], {})

    def test_status_cell_is_found_by_its_header_not_by_its_position(self):
        mapping = MAPPING.replace(
            "| NeoForge item | Mods | Section | Counterpart | Status |\n|:--|--:|:--|:--|:--|",
            "| NeoForge item | Mods | Status | Section | Counterpart |\n|:--|--:|:--|:--|:--|",
        )
        mapping = mapping.replace("| event class | no case (abstract) | supported |",
                                  "| supported | event class | no case (abstract) |")
        mapping = mapping.replace("| event class | `x` | not supported |", "| not supported | event class | `x` |")
        mapping = mapping.replace("| event class | `y` (contract only) | not supported |",
                                  "| not supported | event class | `y` (contract only) |")
        mapping = mapping.replace("| `neoforge.mods.toml` keys | `lifecycle` doc, not supported | planned |",
                                  "| planned | `neoforge.mods.toml` keys | `lifecycle` doc, not supported |")
        new, report = ms.sync(lines(DESIGN), lines(mapping))
        self.assertEqual(report["unmatched"], [])
        self.assertEqual(report["problems"], [])
        self.assertIn("| `AlphaEvent.Beta` | 52 (21.1 `Old`) | supported | event class | `x` |", new)
        self.assertIn("| `file modLoader` | 414 | not supported | `neoforge.mods.toml` keys | "
                      "`lifecycle` doc, not supported |", new)

    def test_a_non_numeric_total_cell_is_a_parse_error_with_its_line(self):
        design = DESIGN.replace("| Total | 5 |", "| Total | five |")
        with self.assertRaisesRegex(ms.ParseError, r"design\.md line 7: .*not a number"):
            ms.sync(lines(design), lines(MAPPING))

    def test_a_table_without_a_status_column_is_a_parse_error(self):
        mapping = MAPPING.replace("| Counterpart | Status |", "| Counterpart | State |")
        with self.assertRaisesRegex(ms.ParseError, "no Status column"):
            ms.sync(lines(DESIGN), lines(mapping))

    def test_main_reports_a_parse_error_with_exit_code_2(self):
        with tempfile.TemporaryDirectory() as tmp, redirect_stdout(io.StringIO()), redirect_stderr(io.StringIO()):
            Path(tmp, "design.md").write_text(DESIGN.replace("| Total | 5 |", "| Total | five |"), encoding="utf-8")
            Path(tmp, "mapping-table.md").write_text(MAPPING, encoding="utf-8")
            self.assertEqual(ms.main(["--dir", tmp]), 2)

    def test_check_fails_on_a_stale_status_and_passes_after_the_write(self):
        with tempfile.TemporaryDirectory() as tmp, redirect_stdout(io.StringIO()):
            Path(tmp, "design.md").write_text(DESIGN, encoding="utf-8")
            mapping = Path(tmp, "mapping-table.md")
            mapping.write_text(MAPPING, encoding="utf-8")
            self.assertEqual(ms.main(["--check", "--dir", tmp]), 1)
            self.assertEqual(mapping.read_text(encoding="utf-8"), MAPPING)
            self.assertEqual(ms.main(["--dir", tmp]), 0)
            self.assertEqual(ms.main(["--check", "--dir", tmp]), 0)
            mapping.write_text(mapping.read_text(encoding="utf-8").replace("| supported |", "| planned |"),
                               encoding="utf-8")
            self.assertEqual(ms.main(["--check", "--dir", tmp]), 1)


if __name__ == "__main__":
    unittest.main()
