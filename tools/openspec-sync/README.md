# openspec-sync

`mapping_status.py` keeps the Status column and the Coverage table of
`openspec/changes/neoforge-shaped-plugin-api/mapping-table.md` in sync with the inventory tables of
`design.md` in the same folder. The statuses of design.md are hand-written and win. The script never
changes the counterpart, Section and Mods columns of the mapping table: they are hand-written.

## Command

```sh
python3 -I tools/openspec-sync/mapping_status.py            # write mapping-table.md
python3 -I tools/openspec-sync/mapping_status.py --check    # write nothing, exit 1 on a mismatch
python3 -I -B tools/openspec-sync/test_mapping_status.py    # unit tests
```

Run it after each change to a status in design.md. `--dir DIR` selects another folder that holds a
`design.md` and a `mapping-table.md`. The script uses the standard library only. It was tested with
Python 3.14.

Exit codes:

- `0`: the files agree and nothing is unmatched.
- `1`: with `--check`, the run would change `mapping-table.md`. In every mode, an unmatched row or a
  `problem:` line (see "Coverage") remains.
- `2`: a file cannot be read or parsed. Examples: a table row with a wrong number of cells, a table
  without a `Mods`, `Counterpart` or `Status` column, a non-numeric cell in the Total row. The
  message names the line or the table and is on stderr.

The output lists the status changes, the number of matched rows, the unmatched rows with their reason
and the three computed Coverage rows. Two runs in a row give the same file: the second run writes
nothing.

## Join rule

The script joins a row of design.md to a row of mapping-table.md when both have the same family and
the same key. It never matches by suffix, by simple name or by similarity.

- **Family**: the text of the nearest `###` heading above the table. The 11 server-side families
  have the same heading in both files. The headings "Summary per family" and "Client-only subsystems
  (families with counts)" of design.md have no twin: they hold no row that the mapping table lists.
- **Key**: the first column of the row (`Event`, `API`, `Registry`, `Package` and so on in design.md,
  `NeoForge item` in mapping-table.md), after these steps:
  1. Remove the backticks around the cell.
  2. Remove a package suffix at the end: ``Gamma (`registries`)`` becomes `Gamma`. The table "Events
     in other packages" of design.md writes the package this way.
  3. Remove `(abstract)` at the end: design.md marks an abstract event this way, the mapping table
     does not.
  4. In the table of `neoforge.mods.toml` keys, the key is `<Section> <Key>`: the cells `[[mods]]` and
     `modId` become `[[mods]] modId`, the name that the mapping table uses.

A nested class keeps the dot (`GrindstoneEvent.OnPlaceItem`) and a member keeps its `#`
(`DeferredRegister#register`). Both files write them the same way, so the script does not translate
them. The tables under "Classes outside the inventory" and "Top 30 gaps and verdicts" use full class
names with `$`. They have no Status column and the script does not read them.

## What the script writes

- **Status column**: a matched row takes the status of design.md. The script finds the column by
  the header name `Status`. Only that cell of the line changes. The other cells stay byte for byte
  as they are.
- **Coverage table, rows "Inventory, supported", "Inventory, planned" and "Inventory, not
  supported"**: computed from the final statuses of the inventory rows of the mapping table. A row
  goes to the first column of the next table whose rule applies. The Rows column counts every row of
  the status.
- **Coverage table, the last two rows** (modpack classes outside the inventory): hand-written. They
  depend on no status. The script does not touch them.

| Column, in test order | A row goes there when |
|:--|:--|
| Contract only | the counterpart cell contains `contract only` |
| Partial | the counterpart cell contains `partial` |
| Gap or not supported | the status is `not supported` |
| Counterpart | no rule above applies. A `supported` or `planned` row has a counterpart by definition, also when its counterpart cell is a gap note. |

The columns "Used by 1 or more mods" and "Used by 50 or more mods" count the rows whose Mods cell
starts with a number of at least 1, or at least 50. The cell `52 (21.1 BlockEvent$BreakEvent)`
counts as 52.

After the sync, the script checks the totals. It compares the count of rows per status in design.md
with the Total row of its "Summary per family" table. It compares the three Coverage rows with the
same Total row. A difference prints a `problem:` line and sets exit code 1.

## Unmatched rows

The script lists every row that it cannot join and leaves the Status cell of that row as it is. A
reason is one of these:

| Reason | What to do |
|:--|:--|
| `no design.md row with this key` | A mapping-table.md row has no twin. Rename the row or remove it, by hand. |
| `no mapping-table.md row with this key` | A design.md row, not `client-only`, has no twin. Add the row to the mapping table, by hand. |
| `duplicate key in design.md` or `in mapping-table.md` | Two rows of one family have the same key. The script cannot choose. Make one key unique. |
| `design.md status is client-only, mapping-table.md lists no client-only row` | Remove the row from the mapping table, by hand. |

A `client-only` row of design.md has no row in the mapping table by design. The script counts these
rows and does not list them.

Current result: 883 inventory rows of the mapping table, 883 matched, 0 unmatched. design.md has
210 `client-only` rows without a mapping row:

- 189 client events and 9 client-only subsystems, in tables that the mapping table does not have.
- 12 rows in server-side families: `ModConfig.Type.CLIENT`, `ModContainer#getCustomExtension`,
  `ModContainer#registerExtensionPoint`, `IClientCommonPacketListenerExtension` and 8 keys of
  `neoforge.mods.toml` (`[[mods]]` `logoFile`, `logoBlur`, `credits`, `licenseURL`, `bannerFile`,
  `iconFile`, `iconBlur`, `displayURL`).

## Safety

The script reads two markdown files and writes one. It uses no network and runs no code from the
files. Run it with `python3 -I`, so that Python does not import modules from the working directory.
