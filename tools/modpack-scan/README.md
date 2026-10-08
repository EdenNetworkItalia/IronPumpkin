# modpack-scan

`mixin_scan.py` reads the mixin classes of a folder of NeoForge mod jars and records every mixin
member with its target member and injection point. Its report is section 9 of
`openspec/changes/neoforge-shaped-plugin-api/modpack-usage.md`.

## Command

```sh
M="$HOME/.var/app/org.prismlauncher.PrismLauncher/data/PrismLauncher/instances/FTB StoneBlock 4/.minecraft/mods"
python3 -I tools/modpack-scan/mixin_scan.py "$M" "$TMPDIR/modpack-scan"
```

The input is the `mods` folder of the Prism Launcher instance `FTB StoneBlock 4` (414 jars,
Minecraft 1.21.1, NeoForge 21.1.248). The second argument is the output folder. Keep it outside the
repository.

The scanner needs Python 3.11 or later and the standard library only. On 32 cores it finishes in
about 1 s.

## Outputs

- `members.tsv`: one row per mixin member, target class, target selector and `@At` point. The
  columns are in the header row. A member with two `@At` points or two target classes has two rows.
- `report.md`: the markdown of section 9 "Mixin targets by method", subsections 9.1 to 9.6. Paste
  it over those subsections of `modpack-usage.md`. Subsection 9.7 (findings) is written by hand.

The same input gives byte-identical outputs: jars, entries and rows are sorted, and the outputs hold
no time stamps and no absolute paths.

## Safety

The jars are untrusted data. The scanner reads them with `zipfile` and its own class file parser.
It does not load, import or run code from the jars, it uses no network and no `javap`, and it
extracts nothing to disk: nested Jar-in-Jar jars are read in memory. Run it with `python3 -I`, so
that Python does not import modules from the working directory.
