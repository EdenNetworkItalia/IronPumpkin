# Porting data attachments from NeoForge

In NeoForge, a mod stores its own data on an entity, a block entity, a chunk or a level with an attachment type. The mod registers the type in `NeoForgeRegistries.ATTACHMENT_TYPES` with `AttachmentType.builder`, and reads and writes the value with `getData`, `setData` and the other methods of `IAttachmentHolder`. A type with a serializer is saved with its holder, a type with copy on death survives the death of a player, and a type with a sync handler reaches the clients.

In IronPumpkin, the `attachments` module of `ironpumpkin-mods` has the same types, holders, saving, copy rules and sync, with Rust types. The handle `AttachmentType<T>` is typed, so a value of another type does not compile. The state that a mixin adds to a vanilla class with `@Unique` also becomes an attachment.

---

## Key Differences

| Feature | NeoForge | IronPumpkin |
| :--- | :--- | :--- |
| **Registration** | `DeferredRegister` of `NeoForgeRegistries.ATTACHMENT_TYPES` | `ModInit::register_attachment_type(name, builder)` in `init`. It returns the `AttachmentType<T>`; keep it in a `OnceLock`. A failed registration stops the server at the freeze of the attachment types, after the last `RegisterEvent`. |
| **Builder** | `AttachmentType.builder(() -> 0)`, `builder(holder -> ..)`, `serializable(..)` | `AttachmentTypeBuilder::new(\|\| 0)`, `with_constructor(\|holder\| ..)`, `serializable(..)` for a value that implements `NbtSerializable` |
| **Builder checks** | `IllegalStateException` for a second serializer, and for `copyOnDeath` or `copyHandler` without a serializer | Compile errors: `serialize` returns a builder that has `copy_on_death` and `copy_handler`, and no second `serialize` |
| **Serializer** | `IAttachmentSerializer` over `ValueInput` and `ValueOutput`, or a `MapCodec` | `AttachmentSerializer` over the NBT compound of the attachment, `NbtSerializer::new(read, write)`, or `serialize_codec` and `serialize_codec_if` for a value whose `pumpkin_codecs` codec encodes a compound |
| **Sync** | `sync(StreamCodec)`, `sync(predicate, StreamCodec)`, `sync(AttachmentSyncHandler)` | `sync_with(encode)`, `sync_to(predicate, encode)`, `sync(handler)`. The encoder writes the bytes that the stream codec of the client reads. |
| **Holders** | `entity`, `player`, `blockEntity`, `chunk`, `level` | `entity` (players through `player.get_entity()`), `attachments::block_entity(&world, &block_entity)`, `attachments::chunk(&world, chunk_pos)`, `world`. Each implements `AttachmentAccess`. |
| **Read** | `getData` returns the stored object | `get_data` returns a clone. Change a value in place with `with_data(ty, \|value\| ..)`, or store an `Arc` with interior mutability. |
| **Existing value** | `getExistingData`, `getExistingDataOrNull` | `get_existing_data`, an `Option` |
| **Sync without a handler** | `syncData` does nothing | `sync_data` fails with `AttachmentError::NotSynced` |

---

## Code Comparison: a mana value on players

### 1. NeoForge implementation (Java)

```java
public static final DeferredRegister<AttachmentType<?>> ATTACHMENT_TYPES =
        DeferredRegister.create(NeoForgeRegistries.Keys.ATTACHMENT_TYPES, ExampleMod.MOD_ID);

public static final Supplier<AttachmentType<Integer>> MANA = ATTACHMENT_TYPES.register("mana",
        () -> AttachmentType.builder(() -> 0)
                .serialize(Codec.INT.fieldOf("mana"))
                .copyOnDeath()
                .sync(ByteBufCodecs.INT)
                .build());

public static void addMana(ServerPlayer player, int amount) {
    player.setData(MANA, player.getData(MANA) + amount);
}
```

### 2. IronPumpkin implementation (Rust)

```rust
use std::sync::OnceLock;

use ironpumpkin_mods::{
    ModInit, NativeMod,
    attachments::{
        AttachmentAccess, AttachmentType, AttachmentTypeBuilder, NbtCompound, NbtSerializer,
    },
    entity::{EntityBase, player::Player},
};

static MANA: OnceLock<AttachmentType<i32>> = OnceLock::new();

# struct ExampleMod;
#
impl NativeMod for ExampleMod {
    // id, display_name and version as usual.
#     fn id(&self) -> &'static str {
#         "examplemod"
#     }
#     fn display_name(&self) -> &'static str {
#         "Example Mod"
#     }
#     fn version(&self) -> &'static str {
#         "1.0.0"
#     }

    fn init(&self, cx: &mut ModInit) {
        // A failed registration stops the server at the freeze of the attachment types.
        if let Ok(mana) = cx.register_attachment_type(
            "examplemod:mana",
            AttachmentTypeBuilder::new(|| 0)
                .serialize(NbtSerializer::new(
                    |nbt: &NbtCompound| nbt.get_int("mana").ok_or_else(|| "no mana".to_owned()),
                    |mana: &i32, nbt: &mut NbtCompound| nbt.put_int("mana", *mana),
                ))
                .copy_on_death()
                // `ByteBufCodecs.INT`: four bytes, big endian.
                .sync_with(|buf, mana: &i32| buf.extend_from_slice(&mana.to_be_bytes())),
        ) {
            let _ = MANA.set(mana);
        }
    }
}

fn add_mana(player: &Player, amount: i32) {
    let Some(&mana) = MANA.get() else {
        return;
    };
    let entity = player.get_entity();
    // `set_data` stores the value and sends it to the NeoForge clients that see the player.
    entity.set_data(mana, entity.get_data(mana) + amount);
}
```

The other holders take the world, because a Pumpkin block entity and a chunk do not know it:

```rust
use std::sync::Arc;

use ironpumpkin_mods::{
    attachments::{self, AttachmentAccess, AttachmentType},
    math::vector2::Vector2,
    pumpkin_core::block::entities::BlockEntity,
    world::World,
};

fn mark_visited(
    world: &World,
    block_entity: &Arc<dyn BlockEntity>,
    chunk_pos: Vector2<i32>,
    visited: AttachmentType<bool>,
) {
    attachments::block_entity(world, block_entity).set_data(visited, true);
    // `None` when the chunk is not loaded.
    if let Some(chunk) = attachments::chunk(world, chunk_pos) {
        chunk.set_data(visited, true);
    }
    world.set_data(visited, true);
}
```

`crates/ironpumpkin-mods/tests/attachments.rs` runs these holders on a server: saving, copy on death and the sync payloads.

---

## Saving

A type with a serializer is saved with its holder under the `neoforge:attachments` key, one compound per type name, as on NeoForge. An entity saves it in its NBT, a player in its player data, a block entity in its NBT inside the chunk, and a chunk at the top level of the chunk NBT. A level saves to `data/neoforge/data_attachments.dat` in its dimension folder, the file of `LevelAttachmentsSavedData`. A type without a serializer does not survive a save. A serializer that returns `false` from `write` saves nothing for the value.

A key of an unknown type, a type without a serializer and a value that fails to read are logged and skipped, so a world keeps loading when a mod is removed. A holder without attachments allocates nothing and writes no key, and a server without attachment types runs no attachment code.

## Copy on death

Pumpkin keeps the same `Player` when it respawns, where NeoForge creates a new `ServerPlayer` and fires `PlayerEvent.Clone`. IronPumpkin applies the copy rules of that event to the player:

- After death, only the types with copy on death keep their value.
- When a player leaves the End, every type with a serializer keeps its value.
- A type without a serializer never survives a respawn.

A value goes through the copy handler of its type, or else through a write and a read of its serializer, and a copy handler that returns `None` drops the value. The copy handler gets the player with its old attachments taken out, as NeoForge gives it the new player.

## Sync

A type with a sync handler gets an id in `neoforge:synced_attachment_types`, in registration order. The server sends that registry in the registry sync, so a NeoForge client uses the ids of the server. A change of the value, its creation by `get_data`, a removal and `sync_data` send a `neoforge:sync_attachments` payload to the clients that see the holder and that the sync handler accepts:

- an entity: the players that track it, and the player itself;
- a chunk: the players that got the chunk;
- a block entity: the players that got its chunk, at the start of the next world tick;
- a level: the players of the level.

A client that starts to see a holder gets its synced values: an entity when the client starts to track it, a player on join, respawn and dimension change, a level with the level info, and a chunk with its block entities when the chunk is sent. A vanilla client gets nothing.

## Differences from NeoForge

- No `RegisterEvent` fires for `neoforge:attachment_types`: register the types in `init`.
- `with_data` and the sync handlers run while the holder holds the lock of its attachments. They must not read or write the attachments of the same holder.
- The holder that a default constructor, a serializer or a copy handler gets for a chunk has the chunk data only: a chunk that loads from disk has no world yet.
- A proto chunk has no holder, so there is nothing to copy when it becomes a full chunk.
- The sync of a block entity sends no block update with it, and the payloads of a sent chunk and its block entities are not bundled.
- The conversion of a mob, such as a zombie that becomes a drowned, does not copy attachments.
- The Pump chunk format does not save chunk attachments. Use the Anvil or the Linear format.
- A level that never saved an attachment writes no `data_attachments.dat`.

See the rustdoc of [`ironpumpkin_mods::attachments`](../../crates/ironpumpkin-mods/src/attachments/mod.rs) for every type and method.
