# Proposal

## Why

A custom entry can join only generated tags: a tag name of a mod, such as `mymod:ores`, fails with `UnknownTag`. NeoForge mods define their own block, item and entity type tags, so the neo `TagKey` API needs tag names that mods register. The content manifest also stores no explicit tags, so a placeholder loses the tags of its missing mod.

## What Changes

- A mod registers a tag name per registry, with generated and custom members. Custom entries list it in their `tags`. Generated tag lists, the datapack tag loader and the client tag sync stay unchanged.
- The content manifest stores the explicit tags of each entry and the mod tags with their declared members. A start without the mod restores them on the placeholders.
- `ironpumpkin-mods` gets a tag builder and `ModInit::register_tag`.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `content-registry`: the custom tag table holds mod tags, and a new requirement covers mod tag names and members.

## Impact

`crates/pumpkin-data/src/dynamic.rs` (registration, freeze, tag queries), `crates/pumpkin-core/src/content/mod.rs` (manifest), `crates/ironpumpkin-mods` (builder API). No generated file, codegen template or packet changes. Clients learn about mod tags only with the registry sync of phase 3.
