# Tasks

Each item mirrors one GitHub issue on EdenNetworkItalia/IronPumpkin.

## 1. Mod tags

- [ ] 1.1 Mod-defined tag names and explicit tags in the content manifest (#69): `register_tag` with generated and custom members, mod tag names in entry `tags`, `has_tag_dynamic` and `tag_ids` for mod tags, the tag builder in `ironpumpkin-mods`, explicit tags and mod tags in the manifest and on placeholders; verify: unit tests in `dynamic.rs`, `crates/pumpkin-core/tests/content_manifest.rs` restores the tags on placeholders and rewrites the manifest byte-identical
