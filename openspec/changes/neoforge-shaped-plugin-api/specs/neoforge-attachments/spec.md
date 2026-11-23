# Spec Delta

## Purpose

Lets mods store their own data on entities, block entities, chunks and levels with the behaviour of NeoForge attachment types, as the `attachments` module of `ironpumpkin-mods`, with saving, copy on death and client sync handled by the host.

## ADDED Requirements

### Requirement: Attachment types are registry entries
An attachment type SHALL be registered in `neoforge:attachment_types` with `ModInit::register_attachment_type` and an `AttachmentTypeBuilder` (`AttachmentType.builder`): a default value (`new`) or a default constructor that gets the holder (`with_constructor`), an optional serializer (`serialize`, `serialize_codec`, `serialize_codec_if`, or `serializable` for an `NbtSerializable` value), copy on death with an optional copy handler, and an optional sync handler (`sync`, `sync_with`, `sync_to`). The call SHALL return a typed `AttachmentType<T>` handle, so a value of another type does not compile. Copy on death and a copy handler SHALL exist only on a builder with a serializer, the checks that `NeoForge` makes at run time. A name that is not a valid resource location SHALL fail with `AttachmentError::InvalidName`, a name without a namespace or in `minecraft` with `AttachmentError::ReservedNamespace` (`NeoForge` accepts a `minecraft` name), a second registration of a name with `AttachmentError::Duplicate`, and a registration after the freeze with `AttachmentError::RegistryFrozen`; the first error SHALL stop the server at the freeze of the attachment types, after the last `RegisterEvent`. Status: supported (task 2.14, #168, tests `names_follow_the_resource_location_rules`, `a_second_registration_of_a_name_fails`, `a_registration_after_the_freeze_fails`, and the `compile_fail` examples of `AttachmentType` and `AttachmentTypeBuilder`).

#### Scenario: Value of another type
- **WHEN** a mod passes a `&str` to `set_data` with an `AttachmentType<i32>`
- **THEN** the mod does not compile

#### Scenario: Copy on death without a serializer
- **WHEN** a mod calls `copy_on_death` on a builder without a serializer
- **THEN** the mod does not compile

### Requirement: Holders
Every `Entity` (players included), every `World` (the level), the block entity of `attachments::block_entity` and the loaded chunk of `attachments::chunk` SHALL implement `AttachmentAccess` with `has_attachments`, `has_data`, `get_data`, `get_existing_data`, `set_data`, `remove_data` and `sync_data` as `IAttachmentHolder` defines them, and `with_data` for a change in place. `get_data` SHALL return the stored value, or create it from the default and store and sync it; `set_data` and `remove_data` SHALL return the previous value and sync the change; a `set_data`, `remove_data` or `with_data` of a block entity SHALL mark its chunk unsaved, as `setChanged` does, and of a chunk SHALL mark it unsaved. Each type of a holder SHALL have its own lock: a mod callback, `with_data` included, SHALL read and write the other types of the same holder, and a use of the type of the running `with_data` on the same holder SHALL panic with the name of the type instead of a deadlock. A serializer, a copy handler or a sync handler that panics SHALL lose only its own value, which is logged and skipped, as `NeoForge` catches the exception of one attachment. Status: supported (task 2.14, #168, tests `an_entity_saves_its_serialized_attachments_only`, `with_data_reads_another_type_of_the_same_holder`, `with_data_that_reenters_its_own_type_panics`, `an_in_place_chunk_change_marks_the_chunk_unsaved`, `a_panicking_serializer_read_skips_only_its_attachment`, `a_panicking_serializer_write_skips_only_its_attachment`, `a_panicking_callback_skips_only_its_value`).

#### Scenario: First read
- **WHEN** a mod reads a string attachment with default `""` from a new entity
- **THEN** it gets `""` and `has_data` returns true afterwards

### Requirement: No cost without attachments
A holder without attachments SHALL allocate nothing and write no `neoforge:attachments` key, and without a registered attachment type the engine SHALL install no attachment hook. Status: supported (task 2.14, #168, tests `an_entity_saves_its_serialized_attachments_only`, `a_chunk_saves_its_attachments_at_the_top_level`, `a_holder_without_saved_types_writes_nothing`).

#### Scenario: Empty holder
- **WHEN** a mod asks `has_data` and `get_existing_data` of an entity without attachments, and the entity saves
- **THEN** the entity holds no attachment map and its NBT has no `neoforge:attachments` key

### Requirement: Saving
A type with a serializer SHALL be saved with its holder under the `neoforge:attachments` NBT key, a compound of one compound per type name, and restored on load; a type without one SHALL not survive a save. A serializer that declines a value SHALL save no key for it. An invalid key, an unknown type, a type without a serializer and a failed read SHALL be logged and skipped, as `AttachmentHolder#deserializeAttachments` does. A chunk SHALL save the key at the top level of its NBT, and a level SHALL save to `data/neoforge/data_attachments.dat` in its dimension folder as `LevelAttachmentsSavedData` does (`{data: {...}, DataVersion}`). Status: supported (task 2.14, #168, tests `an_entity_saves_its_serialized_attachments_only`, `a_player_keeps_its_attachments_across_a_relog`, `a_block_entity_saves_with_its_chunk_data`, `a_chunk_saves_its_attachments_at_the_top_level`, `a_level_saves_to_its_saved_data_file`, `only_types_with_a_serializer_are_saved_and_read_back`, `unknown_invalid_and_broken_entries_are_skipped_on_read`, `a_serializer_that_skips_a_value_drops_its_key`). The relight downgrade of a chunk to a proto chunk SHALL keep its attachments, as `NeoForge` proto chunks carry them (test `attachments_survive_chunk_data_resume`). The Pump chunk format does not carry chunk attachments and logs one warning when it drops them.

#### Scenario: Round trip
- **WHEN** a player with a serialized attachment logs out and back in
- **THEN** `get_existing_data` returns the same value

### Requirement: Copy on death
When a player respawns after death, a type with copy on death SHALL keep its value, through the copy handler of the type when it has one and else through a write and a read of its serializer, and the other types SHALL be dropped. When a player leaves the End through the exit portal, every type with a serializer SHALL keep its value the same way, as `PlayerList.respawn(player, true)` does after the credits. A type without a serializer SHALL never survive a respawn, and a copy handler that returns `None` or panics SHALL drop only that value, as `AttachmentInternals.copyAttachments` does from `PlayerEvent.Clone`. The copy SHALL run after the respawn position event and the change of world, so a handler of the respawn position sees every type. A mob that converts SHALL give its copy-on-death types to the new mob before the new mob spawns, as `AttachmentInternals.onLivingConvert` does. Status: supported (task 2.14, #168, tests `copy_on_death_keeps_only_its_types_through_the_copy_handler`, `copy_rules_follow_restore_from`, `the_respawn_position_event_sees_every_type_and_a_failed_copy_drops_one`, `the_end_exit_keeps_every_serialized_type`, `a_zombie_that_becomes_a_drowned_keeps_its_copy_on_death_types`).

#### Scenario: Respawn
- **WHEN** a player with a copy-on-death attachment dies and respawns
- **THEN** the respawned player has the value that the copy handler returns

### Requirement: Sync to clients
A type with a sync handler SHALL get an id in the synced registry `neoforge:synced_attachment_types`, in registration order, which the registry sync sends to `NeoForge` clients. Its value SHALL reach the clients that negotiated `neoforge:sync_attachments` and that the sync handler accepts, with the `SyncAttachmentsPayload` wire format, when it is created, set or removed and on `sync_data`: the players that track an entity and the player itself, the players that got a chunk, the players of a level, and for a block entity the players that got its chunk at the start of the next world tick. A holder SHALL send its synced values to a client that starts to see it: an entity on pairing, a player on join, respawn and dimension change, a level with the level info, a chunk and its block entities when the chunk is sent. The sync handler SHALL choose the players with no lock of the holder held, so it can read the holder. A payload SHALL be sent with no lock of a value held, and the payloads of one holder SHALL leave in the order of their snapshots. A Java player SHALL get its own synced values at login before `PlayerJoinEvent`, and a block entity that sleeps in the chunk data with saved attachments SHALL wake when its chunk is sent, so its synced values reach the client. `sync_data` on a type without a sync handler SHALL do nothing, as `AttachmentSync` does. A vanilla client SHALL get nothing. Status: supported (task 2.14, #168, tests `a_synced_change_reaches_neoforge_clients_only`, `a_new_viewer_gets_the_synced_values`, `synced_types_join_the_registry_sync`, `payload_matches_the_neoforge_codec`, `initial_data_leaves_out_types_that_write_nothing`, `a_sync_predicate_reads_the_holder`, `a_java_login_sends_the_initial_player_sync`, `a_sleeping_block_entity_gets_its_initial_sync`, `a_packet_handler_can_read_the_synced_holder`, `targets_pack_as_vanilla_with_negative_coordinates`, `the_synced_types_registry_payload_matches_neoforge`).

#### Scenario: Tracking player
- **WHEN** a synced value of an entity changes
- **THEN** every NeoForge client tracking that entity, and the entity itself when it is a player, receives the new value

#### Scenario: Vanilla client
- **WHEN** a synced value of the entity of a vanilla player changes
- **THEN** the client of that player gets no `neoforge:sync_attachments` payload
