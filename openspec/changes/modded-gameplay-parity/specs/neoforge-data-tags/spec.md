# Spec Delta

## Purpose

Makes the tag files of data packs, and of the data folders of mods, change the tags of the static registries for every tag check and for the client, as vanilla 26.3 and NeoForge do.

## ADDED Requirements

### Requirement: Data pack tags apply to tag checks
The server SHALL build the tags of each static registry from the generated tags, the mod tags registered in code and the tag files of the enabled data packs, in that order. Every tag check SHALL answer from these tags. The server SHALL rebuild them at each data pack load and reload. Status: planned (wave 2).

#### Scenario: A block added to a vanilla tag
- **WHEN** a data pack has `data/minecraft/tags/block/mineable/pickaxe.json` with `{"values": ["minecraft:dirt"]}`
- **THEN** dirt has the tag `minecraft:mineable/pickaxe`, and the other members of the tag keep it

#### Scenario: A reload removes the pack
- **WHEN** the pack is disabled and `/reload` runs
- **THEN** dirt does not have the tag

### Requirement: Merge rules
The merge SHALL follow vanilla `TagLoader` and the NeoForge `remove` list: the files of one tag apply in pack order; `replace` clears the entries of the earlier packs; an entry with `"required": false` that names a missing element is left out; a `#` entry adds the members of the other tag; a `remove` entry takes a member out. Status: planned (wave 2).

#### Scenario: A tag that names a changed tag
- **WHEN** a data pack adds `minecraft:dirt` to `minecraft:oak_logs`
- **THEN** dirt also has `minecraft:logs_that_burn`, which names `#minecraft:oak_logs`, and the charcoal recipe takes dirt

#### Scenario: Replace
- **WHEN** a data pack replaces `minecraft:planks` with the list `["minecraft:oak_planks"]`
- **THEN** only oak planks have the tag

#### Scenario: Remove
- **WHEN** a data pack removes `minecraft:dirt` from `minecraft:dirt` with the NeoForge `remove` list
- **THEN** dirt does not have that tag

### Requirement: Missing references
A tag with a required entry that names no element or no tag SHALL NOT load, so it has no members, and the server SHALL log the vanilla line that names the tag and the missing references. A tag in a reference cycle SHALL NOT load. The other tags SHALL load. Status: planned (wave 2).

#### Scenario: A missing item
- **WHEN** a tag file names `examplemod:missing` as a required entry
- **THEN** the tag has no members, also not the generated ones, because vanilla builds all files of one tag together; the log names the tag and `examplemod:missing`, and the server starts

### Requirement: The tags packet
The tags packet SHALL send the tags that the tag checks use, at the join and to every player after a reload. A `Real` connection SHALL get the mod ids. A `Display` connection SHALL get only the vanilla ids of each tag. Status: planned (wave 2).

#### Scenario: A vanilla client after a reload
- **WHEN** a data pack adds a mod block and dirt to `minecraft:mineable/pickaxe` and a vanilla client is connected during `/reload`
- **THEN** the client receives the tag with dirt and without the mod block

### Requirement: Cost of tag checks
A tag check for a tag that no data pack changed SHALL cost at most one atomic load more than the generated lookup. Status: planned (wave 2).

#### Scenario: No changed tag
- **WHEN** the `has_tag` bench runs with the vanilla data pack only
- **THEN** it reports no significant regression against master
