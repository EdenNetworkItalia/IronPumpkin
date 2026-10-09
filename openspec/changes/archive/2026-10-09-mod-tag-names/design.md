# Design

## Context

The custom tag table of the dynamic content registry (#45) holds the custom ids of each generated tag. An explicit tag must name a generated tag, and the content manifest (`<world>/ironpumpkin/content_registry.json`) stores no tags. The design of the registry stays: generated tag lists hold generated ids only, `BlockId::has_tag` and the generated `Taggable` have no custom branch, and the tables are filled once at the freeze.

## Decisions

### Mod tags are keys of the custom tag table

A mod tag is a new key of the per-registry custom tag table, which holds its custom ids. A second per-registry map holds its generated member ids; every mod tag has a key there, so a lookup finds an empty tag too. `tag_ids` and `has_tag_dynamic` try the generated tags first, then the mod tags. Generated tags are not all in the `minecraft` namespace (`c:ores`, `c:ingots`), so a mod tag name that a generated tag of the registry already has returns `DuplicateTag`; otherwise the generated tag would shadow it.

Display inheritance covers generated tags only. Custom content copies its display entry to behave like it (mining speed, tool rules); a mod tag is the mod's own set, and a placeholder or another mod's block with the same display must not join it.

### Late resolution

A mod tag that an entry lists, and a custom member of a mod tag, resolve at the freeze. The order of tag and entry registrations does not matter, and one mod can list another mod's tag whatever the init order. A name in the `minecraft` namespace, or without a namespace, resolves at registration as before, so a typo in a vanilla tag still fails with the mod id. The freeze checks every reference before it leaks anything.

A tag name belongs to one mod: a second registration returns `DuplicateTag`. Shared convention tags (`c:ores`) that several mods fill need a merge rule; the neo `TagKey` work decides it.

### Manifest shape

The manifest format stays 1. New keys are left out when empty, so a manifest without tags is rewritten byte-identical, and an older build ignores them.

```json
{
  "format": 1,
  "blocks": {
    "test:lamp": {
      "properties": { "facing": ["north", "south", "west", "east"], "lit": ["true", "false"] },
      "default": { "facing": "west", "lit": "false" },
      "display": "minecraft:redstone_lamp[lit=false]",
      "tags": ["minecraft:mineable/pickaxe", "test:lamps"]
    }
  },
  "items": { "test:lamp": { "display": "minecraft:redstone_lamp", "tags": ["minecraft:piglin_loved"] } },
  "entity_types": { "test:golem": { "display": "minecraft:iron_golem", "tags": ["minecraft:skeletons"] } },
  "block_tags": { "test:lamps": ["minecraft:redstone_lamp"] },
  "item_tags": { "test:lamps": ["minecraft:glowstone", "test:lamp"] }
}
```

- `tags` of an entry: its explicit tags only, not the tags of its display entry, in name order. Generated tags carry the `minecraft` namespace.
- `block_tags`, `item_tags`, `entity_type_tags`: each mod tag with the members its definition lists, in name order, generated entries with the `minecraft` namespace. An entry that joins through its own `tags` is written on the entry only.
- At a start, the content phase registers each manifest tag that no mod registers (one warning per tag), then the placeholder entries with their `tags`. The written manifest is identical to the one read.
- The manifest is written when the registry holds a custom entry or a mod tag.

### Clients

The datapack tag loader and `CUpdateTags` do not change. A client learns nothing about mod tags until the registry sync of phase 3; server-side queries (recipes, tool rules, commands) see them now.
