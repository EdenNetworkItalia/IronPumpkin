# Porting network payloads from NeoForge

In NeoForge, a mod listens to `RegisterPayloadHandlersEvent` on its mod bus. It gets a `PayloadRegistrar` from the event and registers each payload type with a `StreamCodec` and an `IPayloadHandler`. NeoForge negotiates the channels with a NeoForge client in the configuration phase. A mod sends a payload with `PacketDistributor`, answers in a handler through `IPayloadContext`, and adds configuration tasks in `RegisterConfigurationTasksEvent`.

In IronPumpkin, the two events are Pumpkin events with the same names. A mod registers a handler for them with `ModInit::register_event`. The registrar registers a channel by its id, and a payload is raw bytes: the mod encodes and decodes the body itself. The negotiation, the per-channel dispatch, the checks before a send and the order of the configuration tasks follow NeoForge.

---

## Key Differences

| Feature | NeoForge | IronPumpkin |
| :--- | :--- | :--- |
| **Registration** | `RegisterPayloadHandlersEvent#registrar(version)` on the mod bus | The same event, a Pumpkin event: `ModInit::register_event::<RegisterPayloadHandlersEvent, _>`. It fires once, after `load_complete` and before the synced configs load. |
| **Channel** | A `CustomPacketPayload.Type` and a `StreamCodec` | The channel id as a string. The payload is `&[u8]`. |
| **Registrar methods** | `playToServer`, `playToClient`, `playBidirectional`, the `configuration*` and `common*` methods | `play_to_server`, `play_to_client`, `play_bidirectional`, `configuration_*` and `common_*`. A client-bound method takes no handler: the server has none. |
| **Registrar settings** | `versioned`, `optional`, `executesOn(HandlerThread)` | `versioned`, `optional`, `executes_on(HandlerThread)`. Each returns a copy. |
| **Errors** | An exception | `Err(NetworkError)`: `Duplicate`, `RegistrationClosed`, `InvalidChannel`, `ReservedNamespace` or `BlankVersion`. A failed call registers nothing. |
| **Handler** | `IPayloadHandler<T>` | A closure `Fn(&[u8], &mut PayloadContext<'_>)` |
| **Handler thread** | `MAIN` is the server thread, `NETWORK` the Netty thread | `Main` is the world tick of the player, `Network` the connection task. A configuration payload has no world tick and runs on the connection task. |
| **Context** | `IPayloadContext`: `player`, `reply`, `disconnect`, `flow`, `protocol`, `finishCurrentTask` | `PayloadContext`: `player` (`None` in configuration), `reply`, `disconnect`, `flow`, `protocol`, `finish_current_task`. The server applies a reply and a disconnect after the handler returns. |
| **Send** | `PacketDistributor.sendToPlayer`, `sendToAllPlayers`, `sendToPlayersInDimension` | `network::send_to_player`, `send_to_all_players`, `send_to_players_in_dimension` |
| **Not negotiated** | `checkPacket` throws for a client without the channel | `send_to_player` and `reply` return `Err(NetworkError::ChannelNotNegotiated)`. A broadcast skips such a client and returns how many players got the payload. |
| **Configuration tasks** | `RegisterConfigurationTasksEvent#register(ICustomConfigurationTask)` | The same event, once per connection: `register(Arc<dyn CustomConfigurationTask>)`. `run` sends with `TaskContext::send`. |

---

## Code Comparison: a server-bound channel with a reply

### 1. NeoForge implementation (Java)

```java
public record SetLevel(int level) implements CustomPacketPayload {
    public static final Type<SetLevel> TYPE = new Type<>(Identifier.fromNamespaceAndPath("examplemod", "set_level"));
    public static final StreamCodec<FriendlyByteBuf, SetLevel> CODEC =
            StreamCodec.composite(ByteBufCodecs.VAR_INT, SetLevel::level, SetLevel::new);

    @Override
    public Type<SetLevel> type() {
        return TYPE;
    }
}

@Mod(ExampleMod.MOD_ID)
public final class ExampleMod {
    public static final String MOD_ID = "examplemod";

    public ExampleMod(IEventBus modBus) {
        modBus.addListener(this::register);
    }

    private void register(RegisterPayloadHandlersEvent event) {
        PayloadRegistrar registrar = event.registrar("1");
        registrar.playToServer(SetLevel.TYPE, SetLevel.CODEC, (payload, context) -> {
            ServerPlayer player = (ServerPlayer) context.player();
            LOGGER.info("{} set the level to {}", player.getName().getString(), payload.level());
            context.reply(new LevelChanged(payload.level()));
        });
        registrar.playToClient(LevelChanged.TYPE, LevelChanged.CODEC);
    }
}
```

### 2. IronPumpkin implementation (Rust)

The snippet is a doc-test of `ironpumpkin-mods`. The body of `examplemod:set_level` is one byte here.

```rust
use std::sync::Arc;

use ironpumpkin_mods::{
    ModInit, NativeMod,
    event::{BoxFuture, EventHandler, EventPriority},
    network::{PayloadContext, RegisterPayloadHandlersEvent},
    register_mod,
    server::Server,
    tracing::{error, info},
};

const MOD_ID: &str = "examplemod";
const SET_LEVEL: &str = "examplemod:set_level";
const LEVEL_CHANGED: &str = "examplemod:level_changed";

fn on_set_level(data: &[u8], context: &mut PayloadContext<'_>) {
    let Some(&level) = data.first() else {
        return;
    };
    if let Some(player) = context.player() {
        info!("{} set the level to {level}", player.gameprofile.name);
    }
    if let Err(err) = context.reply(LEVEL_CHANGED, [level]) {
        error!("[{MOD_ID}] {err}");
    }
}

struct Payloads;

impl EventHandler<RegisterPayloadHandlersEvent> for Payloads {
    fn handle<'a>(
        &'a self,
        _server: &'a Arc<Server>,
        event: &'a RegisterPayloadHandlersEvent,
    ) -> BoxFuture<'a, ()> {
        let registrar = event.registrar("1");
        let registered = registrar
            .play_to_server(SET_LEVEL, on_set_level)
            .and_then(|registrar| registrar.play_to_client(LEVEL_CHANGED));
        if let Err(err) = registered {
            error!("[{MOD_ID}] {err}");
        }
        Box::pin(async {})
    }
}

struct ExampleMod;

impl NativeMod for ExampleMod {
    fn id(&self) -> &'static str {
        MOD_ID
    }

    fn display_name(&self) -> &'static str {
        "Example Mod"
    }

    fn version(&self) -> &'static str {
        "1.0.0"
    }

    fn init(&self, cx: &mut ModInit) {
        cx.register_event::<RegisterPayloadHandlersEvent, _>(
            Arc::new(Payloads),
            EventPriority::Normal,
            false,
            false,
        );
    }
}

register_mod!(ExampleMod);
```

The mapping from the Java code to the Rust code:

| Java | Rust |
| :--- | :--- |
| `modBus.addListener(this::register)` | `cx.register_event::<RegisterPayloadHandlersEvent, _>(..)` in `init` |
| `event.registrar("1")` | `event.registrar("1")` |
| `registrar.optional()`, `versioned(v)`, `executesOn(t)` | `registrar.optional()`, `versioned(v)`, `executes_on(t)` |
| `registrar.playToServer(TYPE, CODEC, handler)` | `registrar.play_to_server("ns:path", handler)?` |
| `registrar.playToClient(TYPE, CODEC)` | `registrar.play_to_client("ns:path")?` |
| `CODEC.decode(buf)` | The handler reads the `&[u8]` itself |
| `context.player()` | `context.player()`, an `Option<&Arc<Player>>` |
| `context.reply(payload)` | `context.reply("ns:path", bytes)?` |
| `context.disconnect(reason)` | `context.disconnect(reason)` |
| `PacketDistributor.sendToPlayer(player, payload)` | `network::send_to_player(&player, "ns:path", &bytes)?` |
| `PacketDistributor.sendToAllPlayers(payload)` | `network::send_to_all_players(&server, "ns:path", &bytes)` |
| `PacketDistributor.sendToPlayersInDimension(level, payload)` | `network::send_to_players_in_dimension(&world, "ns:path", &bytes)` |

---

## The negotiation

The server offers every registered channel in the NeoForge channel negotiation of the configuration phase, with its version, its direction and its optional flag.

- A NeoForge client without a required channel, or with another version or direction of a channel, gets `neoforge:modded_network_setup_failed` and is disconnected.
- A client without NeoForge cannot join while a required channel exists.
- An optional channel that the client did not negotiate is unavailable for that client: a send to it fails with `ChannelNotNegotiated`.

## Receiving a payload

A payload reaches the handler of its channel once, with its bytes:

- A payload on a channel that no mod registered goes to the plugin events (`ConfigCustomPayloadEvent`, `PlayerCustomPayloadEvent`), and to no mod handler.
- A payload on a required channel that the client did not negotiate disconnects the client with `multiplayer.disconnect.incompatible`, as NeoForge does. An optional channel can be read without the negotiation.
- A payload on a client-bound channel is dropped.

A handler on `HandlerThread::Main` runs in the world tick of the player, where the server handles the other play packets of the player. A handler on `HandlerThread::Network` runs on the connection task, at the same time as the world tick, and the plugin events for the packet do not fire. In the configuration phase, every handler runs on the connection task.

## Configuration tasks

A configuration task is a `CustomConfigurationTask`: a type, such as `"examplemod:licence"`, and a `run` method that sends payloads through `TaskContext::send`. A handler of `RegisterConfigurationTasksEvent` registers the tasks for one connection. The event tells the connection type and the channels of the client (`has_channel`), as `getListener()` does in NeoForge.

```rust
use std::sync::Arc;

use ironpumpkin_mods::{
    event::{BoxFuture, EventHandler},
    network::{CustomConfigurationTask, PayloadContext, RegisterConfigurationTasksEvent, TaskContext},
    server::Server,
};

const LICENCE: &str = "examplemod:licence";

struct LicenceTask;

impl CustomConfigurationTask for LicenceTask {
    fn task_type(&self) -> &'static str {
        LICENCE
    }

    fn run(&self, context: &mut TaskContext<'_>) {
        let _ = context.send(LICENCE, b"challenge".to_vec());
    }
}

/// The handler of the `examplemod:licence` channel, registered with
/// `configuration_bidirectional`.
fn on_licence(data: &[u8], context: &mut PayloadContext<'_>) {
    if data == b"challenge" {
        let _ = context.finish_current_task(LICENCE);
    }
}

struct Tasks;

impl EventHandler<RegisterConfigurationTasksEvent> for Tasks {
    fn handle<'a>(
        &'a self,
        _server: &'a Arc<Server>,
        event: &'a RegisterConfigurationTasksEvent,
    ) -> BoxFuture<'a, ()> {
        if event.has_channel(LICENCE) {
            event.register(Arc::new(LicenceTask));
        }
        Box::pin(async {})
    }
}
```

The tasks of the mods run after the tasks of NeoForge, in registration order, and before the end of the configuration. Each task waits until it finishes. It finishes in `run` through `TaskContext::finish`, or later through `PayloadContext::finish_current_task` in the handler of the client's answer. A finish request for a task that is not the current task disconnects the client with `Unexpected request for task finish, current task: <current>, requested: <type>`.

---

## Do and don't

**Do**

- Register the channels in a handler of `RegisterPayloadHandlersEvent`. The registration closes when the event returns.
- Keep the registrar in a local variable when you chain calls: each call returns a reference to it.
- Give each channel the namespace of the mod, and raise the registrar version when the body format changes.
- Make a channel `optional()` when a client without the mod can still play.
- Check the `Result` of a registrar call, of `send_to_player` and of `reply`.
- Register a configuration task only when the client has its channel (`event.has_channel`).
- Finish every configuration task, or the client waits until it times out.

**Don't**

- Do not use the `minecraft` namespace. It fails with `ReservedNamespace`.
- Do not keep a registrar for later. A call after the event fails with `RegistrationClosed`.
- Do not expect `context.player()` in the configuration phase. It is `None`: the player is not in a world yet.
- Do not block in a handler on `HandlerThread::Main`: it runs in the world tick.
- Do not call `finish_current_task` in play. It fails with `NotConfiguration`.
- Do not send to a client that did not negotiate the channel and ignore the error: the payload does not leave the server.
