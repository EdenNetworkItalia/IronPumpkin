# Spec Delta

## Purpose

Lets a ported mod register menu types of its own, open them with extra data, and send data slot values with 32 bits. It extends the `menus` module of `neoforge-menus`.

## ADDED Requirements

### Requirement: Register a menu type
A mod SHALL register a menu type by name before the freeze, as an entry of the id table. At the freeze, the type SHALL get an id after the vanilla menu types, in name order. The freeze SHALL build the `minecraft:menu` row when a mod menu type exists. Status: planned (wave 1).

#### Scenario: Register a menu type
- **WHEN** a mod registers `examplemod:grinder_menu`
- **THEN** the sync sends the `minecraft:menu` row with the 25 vanilla names first and `examplemod:grinder_menu` after them

#### Scenario: No mod menu type
- **WHEN** no mod registers a menu type
- **THEN** the sync sends no `minecraft:menu` row

### Requirement: Open a menu of a mod menu type
This requirement replaces the requirement "Open a menu of a mod menu type" of the `neoforge-menus` spec of M4. `menus::open_menu` SHALL open a menu of a mod menu type for a `Real` connection. For a `Display` connection it SHALL fail with a `MenuError` variant that names the menu type. The open menu of the player SHALL NOT change. Status: planned (wave 1).

#### Scenario: A Real connection
- **WHEN** a mod opens a menu of `examplemod:grinder_menu` with no extra data for a `Real` connection
- **THEN** the client receives the vanilla open packet with the menu type id of the synced menu row

#### Scenario: A Display connection
- **WHEN** a mod opens a menu of a mod menu type for a `Display` connection
- **THEN** the call fails with the `MenuError` variant and the open menu of the player does not change

### Requirement: No layout limit for mod menu types
The vanilla layout check of M4 SHALL apply to vanilla menu types only. Status: planned (wave 1).

#### Scenario: A mod type with 5 slots
- **WHEN** a mod defines a menu of a mod type with 5 slots that no vanilla type has
- **THEN** the open call does not fail with a layout error

### Requirement: Extra data
`open_menu` SHALL take an extra data writer. When the writer writes at least one byte, the server SHALL send `neoforge:advanced_open_screen` with the window id, the menu type id, the title and the extra bytes. Otherwise the server SHALL send the vanilla open packet. Status: planned (wave 1).

#### Scenario: Extra data on a vanilla type
- **WHEN** a mod opens a `minecraft:generic_9x3` menu with a writer that writes a block position for a `Real` connection
- **THEN** the client receives `neoforge:advanced_open_screen` with the position bytes

#### Scenario: No extra data
- **WHEN** a mod opens a vanilla menu type with an empty writer
- **THEN** the client receives the vanilla open packet

### Requirement: Policy for Display connections
For a `Display` connection and a vanilla menu type, the server SHALL send the vanilla open packet without the extra data. Status: planned (wave 1).

#### Scenario: Extra data on a Display connection
- **WHEN** a mod opens a vanilla menu type with extra data for a `Display` connection
- **THEN** the client receives the vanilla open packet without the extra data

### Requirement: 32-bit data slots
The server SHALL send each data slot value as `neoforge:advanced_container_set_data` to a client that has the channel. This covers vanilla and mod menus, the first full sync and each change. The server SHALL send the 16-bit vanilla packet to a client without the channel. The value SHALL stay an `i32` from the sync handler to the send. Status: planned (wave 1).

#### Scenario: A large value
- **WHEN** a machine menu sets a data slot to 100000 for a client with the channel
- **THEN** the client receives 100000 in the advanced packet

#### Scenario: A client without the channel
- **WHEN** the same slot changes for a client without the channel
- **THEN** the client receives the 16-bit vanilla packet

### Requirement: Menu logic for a mod menu type
Slots, data slots and `MenuLogic` callbacks of a mod menu type SHALL behave as the requirements of `neoforge-menus` say for vanilla menu types. Status: planned (wave 1).

#### Scenario: Shift-click
- **WHEN** a player shift-clicks a stack in a menu of a mod menu type and the mod logic moves it into the machine slots
- **THEN** the stack moves as far as the slots accept it
