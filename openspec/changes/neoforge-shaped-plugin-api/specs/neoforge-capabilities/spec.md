# Spec Delta

## Purpose

Lets mods expose and look up item, fluid and energy handlers on blocks, entities and items with the behaviour of NeoForge capabilities, across mods and over Pumpkin inventories. It is the `capabilities` and `transfer` modules of `ironpumpkin-mods`.

## ADDED Requirements

### Requirement: Capability creation by name
`BlockCapability::create`, `EntityCapability::create` and `ItemCapability::create` (and the `create_sided` and `create_void` forms) SHALL return the capability with that name, creating it on first use, and SHALL fail with `CapabilityError::TypeMismatch` when the name exists with another handler or context type. Status: supported (task 2.7, #153, tests `creating_the_item_handler_by_name_returns_the_built_in_capability`, `creating_a_name_with_other_types_fails_with_type_mismatch`).

#### Scenario: Built-in capability
- **WHEN** a mod creates the block capability `neoforge:item_handler` with `dyn ResourceHandler<ItemResource>` and `Option<BlockDirection>`
- **THEN** it gets the built-in item handler capability `capabilities::item::BLOCK`

#### Scenario: Conflicting types
- **WHEN** a mod creates `neoforge:item_handler` with `dyn EnergyHandler`
- **THEN** the call fails with `CapabilityError::TypeMismatch`

### Requirement: Providers from mods
A provider registered through `RegisterCapabilitiesEvent` (`NativeMod::register_capabilities`) SHALL answer lookups of other mods with its handler. The host SHALL call the method once per `EventPriority`, from `Highest` to `Lowest`, for every mod in load order before the next priority, with its own steps first in each priority. Status: supported (task 2.7, #153).

#### Scenario: Mod block exposes items
- **WHEN** another mod calls `get_capability` on a block whose mod registered an item provider
- **THEN** it gets the providing mod's handler, and `insert` on it reaches that handler (tests `a_provider_of_one_mod_answers_the_lookup_of_another`, `a_second_mod_and_vanilla_blocks_use_the_item_handler_of_the_test_block`)

#### Scenario: Fallback in a later phase
- **WHEN** a mod checks `is_block_registered` for a block in the `Low` phase
- **THEN** the answer includes the `Normal` providers of every mod, whatever the load order (test `every_mod_gets_each_phase_in_order_after_the_host`)

### Requirement: Pumpkin inventories as providers
The built-in item capabilities SHALL return a handler over the Pumpkin block entity and entity inventories that vanilla exposes, with the vanilla slots of each face for furnaces, blast furnaces, smokers, brewing stands and shulker boxes. Status: supported for the containers that Pumpkin has (task 2.7, #153, tests `a_chest_has_27_slots_and_a_double_chest_54_with_the_right_half_first`, `a_furnace_exposes_the_vanilla_slots_of_each_face`).

#### Scenario: Chest
- **WHEN** a mod looks up `capabilities::item::BLOCK` on a chest
- **THEN** `size` returns 27, and 54 on a double chest

#### Scenario: Furnace faces
- **WHEN** a mod looks up `capabilities::item::BLOCK` on a furnace from below
- **THEN** the handler has the result and fuel slots, and extracts fuel only as a bucket

### Requirement: Transactions
`insert` and `extract` SHALL take an open transaction; changes SHALL take effect when the root transaction commits and SHALL roll back when it is dropped without commit, including changes of mod handlers. A transaction SHALL belong to its thread, with one root per thread (test `a_second_root_on_the_same_thread_fails`). Status: supported (task 2.7, #153).

#### Scenario: Aborted extract
- **WHEN** a mod extracts 10 items in a transaction and drops it without commit
- **THEN** the inventory still holds the 10 items (tests `an_aborted_extract_leaves_the_chest_untouched`, `a_mod_handler_rolls_back_with_its_root_transaction`)

#### Scenario: Two threads on one journal
- **WHEN** a transaction of thread A changed an index and thread B inserts into or extracts from that index before A's root closes
- **THEN** B transfers nothing and A's commit or abort stays intact (test `a_journal_owned_by_another_thread_transfers_nothing`)

#### Scenario: Concurrent write
- **WHEN** another thread writes a slot while a transaction that changed it is open, and the transaction aborts
- **THEN** the rollback gives back only what the transaction moved, and keeps the slot when the other write replaced its item (tests `an_aborted_extract_after_another_write_gives_back_its_items`, `a_rollback_keeps_a_slot_that_another_write_changed`)

### Requirement: Cache invalidation
A `BlockCapabilityCache` that was read since its last invalidation SHALL call its invalidation listener when a block entity at its position is placed or removed, a chest there changes its type, its chunk loads or unloads, or a mod calls `invalidate_capabilities` for the position. Status: supported (task 2.7, #153, test `a_chunk_unload_invalidates_its_caches`).

#### Scenario: Block entity removed
- **WHEN** the block entity of a cache is removed
- **THEN** the host calls the invalidation listener of the cache (test `a_cache_listener_runs_when_the_block_entity_goes`)

#### Scenario: Plain block
- **WHEN** a mod drops the handler of a block without a block entity and calls `invalidate_capabilities`
- **THEN** the cache calls its listener and the next lookup returns the new handler (test `a_mod_invalidates_the_cache_of_a_plain_block`)

### Requirement: Hoppers and droppers use item capabilities
A hopper or a dropper SHALL fall back to `capabilities::item::BLOCK` of the block it faces when that block has no vanilla inventory, as `VanillaInventoryCodeHooks` does: a hopper pushes one item to the side it faces and pulls one from the block above, looked up from below; a dropper drops one item through the side that faces it. Status: supported (task 2.7, #153).

#### Scenario: Hopper into a mod block
- **WHEN** a hopper faces a mod block with an item provider
- **THEN** one item per transfer moves into the mod handler (tests `a_hopper_pushes_into_the_item_capability_of_a_plain_block`, `a_hopper_pulls_from_the_item_capability_of_a_plain_block`)

#### Scenario: Dropper into a mod block
- **WHEN** a dropper faces a mod block with an item provider
- **THEN** one item moves into the mod handler (test `a_dropper_inserts_into_the_item_capability_of_a_plain_block`)

### Requirement: Registration closes
Calls on `RegisterCapabilitiesEvent` after its last phase SHALL fail with `CapabilityError::RegistrationClosed`. Status: supported (task 2.7, #153, test `registration_closes_after_the_event`).

#### Scenario: Late provider
- **WHEN** a mod registers a provider after the event
- **THEN** the call fails with `CapabilityError::RegistrationClosed`
