# Spec Delta

## Purpose

Lets a ported mod open container menus for players with its own slots, data slots and menu logic, in the shape of `AbstractContainerMenu`, `MenuProvider` and `IPlayerExtension#openMenu`.

## ADDED Requirements

### Requirement: Open a menu
`menus.open-menu` SHALL close the open menu of the player, create a server menu from the `menu-definition`, send it to the player and return its `menu` handle, and SHALL fire the Pumpkin `InventoryOpenEvent`, so that a cancel from a v0.2 plugin or a neo listener of `player-container-event-open` stops the open. A mod menu type SHALL fail with `unsupported-client` for a client without that type. Status: planned (task 2.9).

#### Scenario: A vanilla menu type
- **WHEN** a mod opens a `minecraft:generic_9x3` menu with 27 `menu-container` slots and the 36 player inventory slots
- **THEN** a vanilla client shows a chest screen and `get-open-menu` returns the menu

#### Scenario: A mod menu type on a vanilla client
- **WHEN** a mod opens a menu of its own menu type for a player on a vanilla client
- **THEN** the call fails with `menu-error::unsupported-client` and the player's open menu does not change

### Requirement: Menu logic runs in the mod
The host SHALL call `menu-clicked` before handling a click, `menu-quick-move-stack` on a shift-click, `menu-still-valid` every tick, `menu-click-button` on a button click, `menu-slots-changed` when the menu's own container changes, `menu-slot-may-place` before an item enters a filtered slot, `menu-slot-on-take` after the player takes an item, and `menu-removed` on close. Status: planned (task 2.9).

#### Scenario: Shift-click into the machine
- **WHEN** the player shift-clicks a stack in the player inventory part of a mod menu, and the mod's `menu-quick-move-stack` calls `move-item-stack-to` for its input slots
- **THEN** the stack moves into the input slots as far as they accept it, and the client sees the result

#### Scenario: Ghost slot
- **WHEN** the player clicks a filter slot with an item on the cursor and the mod's `menu-clicked` returns true after it stores a copy of the item as the filter
- **THEN** the cursor keeps its item and the vanilla click handling does not run

#### Scenario: Output slot refuses items
- **WHEN** a slot has a place filter whose `menu-slot-may-place` returns false and the player drops an item on it
- **THEN** the item stays on the cursor

### Requirement: Slots view their source
A slot SHALL read and write the item of its `slot-source`: the player inventory, the menu's own container, a container block entity, or a resource handler of a mod through its handler methods. Data slots SHALL reach the client after `broadcast-changes`, 16 bits per slot as in vanilla. Status: planned (task 2.9).

#### Scenario: A handler slot
- **WHEN** a slot has the source `handler` with a resource handler of a mod and an index, and the player takes the item
- **THEN** the host calls `extract` on that handler inside a transaction and commits it
