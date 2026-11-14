# Spec Delta

## Purpose

Lets a ported mod open container menus for players with its own slots, data slots and menu logic, with the behaviour of `AbstractContainerMenu`, `MenuProvider` and `IPlayerExtension#openMenu`, as the `menus` module of `ironpumpkin-mods`.

## ADDED Requirements

### Requirement: Open a menu
`menus::open_menu` SHALL close the open menu of the player, create a server menu of a vanilla menu type from the `MenuDefinition`, send it to the player and return its `Menu` handle. It SHALL fail with `MenuError::Layout` when the definition does not have the slot and data slot counts that the vanilla client builds for that type. It SHALL fire the Pumpkin `InventoryOpenEvent`, so that a cancel from a v0.2 plugin or from a native handler stops the open; the cancel is a Pumpkin addition, because `PlayerContainerEvent.Open` of NeoForge cannot be cancelled. Status: supported (task 2.9, #154, tests `a_vanilla_menu_type_opens_a_chest_screen`, `a_layout_mismatch_fails_and_opens_nothing`, `a_cancelled_open_event_stops_the_open`).

#### Scenario: A vanilla menu type
- **WHEN** a mod opens a `minecraft:generic_9x3` menu with 27 `SlotSource::MenuContainer` slots and the 36 player inventory slots
- **THEN** a vanilla client shows a chest screen and `get_open_menu` returns the menu

### Requirement: Open a menu of a mod menu type
`menus::open_menu` SHALL open a menu of a menu type of the mod, with the extra data of `openMenu(provider, extraDataWriter)`, through `neoforge:advanced_open_screen`, and SHALL fail with an unsupported-client error for a client without that type. Status: planned (M5, `neoforge:advanced_open_screen`).

#### Scenario: A mod menu type on a vanilla client
- **WHEN** a mod opens a menu of its own menu type for a player on a vanilla client
- **THEN** the call fails with the unsupported-client error and the player's open menu does not change

### Requirement: Menu logic runs in the mod
The host SHALL call `MenuLogic::clicked` before handling a click, `quick_move_stack` on a shift-click, `still_valid` every tick, `click_button` on a button click, `slots_changed` when the menu's own container changes, `can_take_item_for_pick_all` for each slot of a double click, `slot_may_place` before an item enters a filtered slot, `slot_on_take` after the player takes an item, and `removed` on close. Status: supported (task 2.9, #154, tests `shift_click_moves_the_stack_into_the_machine`, `a_ghost_slot_keeps_the_cursor`, `the_output_slot_refuses_items_and_reports_takes`, `an_invalid_menu_closes_on_the_tick`, `a_double_click_skips_the_slot_that_the_logic_excludes`, `a_close_inside_a_button_closes_at_once`).

#### Scenario: Shift-click into the machine
- **WHEN** the player shift-clicks a stack in the player inventory part of a mod menu, and the mod's `quick_move_stack` calls `MenuAccess::move_item_stack_to` for its input slots
- **THEN** the stack moves into the input slots as far as they accept it, and the client sees the result

#### Scenario: Ghost slot
- **WHEN** the player clicks a filter slot with an item on the cursor and the mod's `clicked` returns true after it stores a copy of the item as the filter
- **THEN** the cursor keeps its item and the vanilla click handling does not run

#### Scenario: Output slot refuses items
- **WHEN** a slot has a place filter whose `slot_may_place` returns false and the player drops an item on it
- **THEN** the item stays on the cursor

### Requirement: Slots view their source
A slot SHALL read and write the item of its `SlotSource`: the player inventory, the menu's own container, a container block entity, or a resource handler of a mod through its handler methods. Data slots SHALL reach the client after `MenuAccess::broadcast_changes` or an accepted button, 16 bits per slot as in vanilla. Status: supported (task 2.9, #154, tests `a_handler_slot_extracts_and_inserts_in_transactions`, `a_capped_extract_rolls_the_shift_click_back`, `a_refused_place_keeps_the_cursor`, `data_slots_reach_the_client_as_16_bits`, `an_accepted_button_broadcasts_the_changes`).

#### Scenario: A handler slot
- **WHEN** a slot has the source `SlotSource::Handler` with a resource handler of a mod and an index, and the player takes the item
- **THEN** the host calls `extract` on that handler in a nested transaction of the root transaction of the click, and commits the root when the click ends

#### Scenario: A handler refuses part of a write
- **WHEN** a click writes a handler slot and the handler moves less than the click needs
- **THEN** the root transaction of the click rolls back, the cursor and the other slots get their stacks from before the click, and the client gets the whole menu again
