# Spec Delta

## Purpose

Lets a ported mod force chunks to load per mod, with the behaviour of `TicketController` and `RegisterTicketControllersEvent`, and keeps the tickets in the world save.

## ADDED Requirements

### Requirement: Vanilla forced chunks save
The server SHALL save the vanilla forced chunks of each world and load them again. Status: M4 wave 10 (#190).

#### Scenario: A forced chunk
- **WHEN** an operator forces a chunk with `/forceload add`, the server stops, and the server starts again
- **THEN** the chunk is still forced

### Requirement: Register a ticket controller
A mod SHALL register a ticket controller with a unique id in `RegisterTicketControllersEvent`. A controller MAY have a validation callback. A second controller with the same id SHALL fail the registration. Status: planned (wave 2).

#### Scenario: Register
- **WHEN** a mod registers the controller `examplemod:loaders`
- **THEN** the mod gets a controller handle

#### Scenario: A duplicate id
- **WHEN** two mods register the controller id `examplemod:loaders`
- **THEN** the second registration fails

### Requirement: Force a chunk
A controller SHALL add and remove a ticket for a chunk with a block position owner, an entity owner or a UUID owner. Each call SHALL take the `forceTicks` flag. A chunk with a ticket SHALL stay loaded and SHALL tick when no player is in range. A chunk with a `forceTicks` ticket SHALL count for natural spawning, as a player chunk does. Status: planned (wave 2).

#### Scenario: A loader block
- **WHEN** a mod adds a ticket for a chunk with a block owner and all players leave
- **THEN** the chunk stays loaded and its block entities tick

#### Scenario: A forceTicks ticket
- **WHEN** a ticket with `forceTicks` holds a chunk and no player is in range
- **THEN** `count_natural_spawn_chunks` includes that chunk

#### Scenario: Remove
- **WHEN** the mod removes the ticket and no other ticket or player holds the chunk
- **THEN** the chunk unloads

#### Scenario: Two owners
- **WHEN** two owners hold a ticket on one chunk and one owner removes its ticket
- **THEN** the chunk stays loaded

### Requirement: Save and load
The server SHALL save the tickets of each controller with the world in the vanilla ticket format, under the NeoForge `neo_ticket_data` key. A ticket of an absent mod SHALL stay in the save. It SHALL NOT keep a chunk loaded. Status: planned (wave 2).

#### Scenario: Reload
- **WHEN** a mod holds tickets, the server stops, and the server starts again
- **THEN** the validation callback receives the saved tickets for each world

#### Scenario: A removed mod
- **WHEN** a world with tickets of `examplemod` loads in a modpack without it
- **THEN** the world loads, no chunk stays loaded for those tickets, and the next save keeps them

### Requirement: Validation callback
On load, the controller callback SHALL receive the saved tickets and MAY remove a ticket. The server SHALL activate only the tickets that remain. A controller without a callback SHALL keep all its saved tickets. Status: planned (wave 2).

#### Scenario: Remove a stale ticket
- **WHEN** the callback removes the ticket of a block that no longer exists
- **THEN** that ticket does not load its chunk

### Requirement: Tickets are server-only
A `Display` connection and a `Real` connection SHALL receive no data of a ticket. Status: planned (wave 2).

#### Scenario: A client joins
- **WHEN** a client joins a world with mod tickets
- **THEN** the join succeeds and the client sees the chunks that load
