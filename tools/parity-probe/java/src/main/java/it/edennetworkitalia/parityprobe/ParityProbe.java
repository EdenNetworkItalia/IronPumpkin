package it.edennetworkitalia.parityprobe;

import java.util.Map;
import net.minecraft.core.registries.Registries;
import net.minecraft.network.FriendlyByteBuf;
import net.minecraft.network.codec.StreamCodec;
import net.minecraft.network.protocol.common.custom.CustomPacketPayload;
import net.minecraft.resources.Identifier;
import net.neoforged.bus.api.IEventBus;
import net.neoforged.fml.ModContainer;
import net.neoforged.fml.common.Mod;
import net.neoforged.fml.config.ModConfig;
import net.neoforged.fml.event.config.ModConfigEvent;
import net.neoforged.fml.event.lifecycle.FMLCommonSetupEvent;
import net.neoforged.fml.event.lifecycle.FMLConstructModEvent;
import net.neoforged.fml.event.lifecycle.FMLLoadCompleteEvent;
import net.neoforged.neoforge.common.ModConfigSpec;
import net.neoforged.neoforge.common.NeoForge;
import net.neoforged.neoforge.event.server.ServerAboutToStartEvent;
import net.neoforged.neoforge.event.server.ServerStartedEvent;
import net.neoforged.neoforge.event.server.ServerStartingEvent;
import net.neoforged.neoforge.event.server.ServerStoppedEvent;
import net.neoforged.neoforge.event.server.ServerStoppingEvent;
import net.neoforged.neoforge.event.tick.ServerTickEvent;
import net.neoforged.neoforge.network.event.RegisterConfigurationTasksEvent;
import net.neoforged.neoforge.network.event.RegisterPayloadHandlersEvent;
import net.neoforged.neoforge.registries.RegisterEvent;

/**
 * Probe of the differential parity harness. It listens to the lifecycle, registration, config,
 * payload and server events of slice 1 and writes one trace line per event ({@link Trace}). The
 * Rust probe in {@code tools/parity-probe/rust} writes the same trace on IronPumpkin.
 */
@Mod(ParityProbe.MOD_ID)
public final class ParityProbe {
    public static final String MOD_ID = "parityprobe";

    private static final ModConfigSpec.Builder CONFIG = new ModConfigSpec.Builder();
    private static final ModConfigSpec.BooleanValue ENABLED = CONFIG.define("enabled", true);
    private static final ModConfigSpec SPEC = CONFIG.build();

    // The tick events are counted. Only the first of each kind is traced: the tick count depends
    // on how long the server runs.
    private static long preTicks;
    private static long postTicks;

    public ParityProbe(IEventBus modBus, ModContainer container) {
        container.registerConfig(ModConfig.Type.SYNCED, SPEC);

        modBus.addListener(ParityProbe::onConstruct);
        modBus.addListener(ParityProbe::onRegister);
        modBus.addListener(ParityProbe::onCommonSetup);
        modBus.addListener(ParityProbe::onLoadComplete);
        modBus.addListener(ParityProbe::onConfigLoading);
        modBus.addListener(ParityProbe::onRegisterPayloadHandlers);
        modBus.addListener(ParityProbe::onRegisterConfigurationTasks);

        NeoForge.EVENT_BUS.addListener(ParityProbe::onServerAboutToStart);
        NeoForge.EVENT_BUS.addListener(ParityProbe::onServerStarting);
        NeoForge.EVENT_BUS.addListener(ParityProbe::onServerStarted);
        NeoForge.EVENT_BUS.addListener(ParityProbe::onServerStopping);
        NeoForge.EVENT_BUS.addListener(ParityProbe::onServerStopped);
        NeoForge.EVENT_BUS.addListener(ParityProbe::onServerTickPre);
        NeoForge.EVENT_BUS.addListener(ParityProbe::onServerTickPost);

        PlayEvents.register(NeoForge.EVENT_BUS);
    }

    private static void onConstruct(FMLConstructModEvent event) {
        Trace.event("FMLConstructModEvent", Map.of());
    }

    private static void onRegister(RegisterEvent event) {
        if (event.getRegistryKey().equals(Registries.BLOCK)
                || event.getRegistryKey().equals(Registries.ITEM)
                || event.getRegistryKey().equals(Registries.ENTITY_TYPE)) {
            Trace.event("RegisterEvent", Map.of("registry", event.getRegistryKey().identifier().toString()));
        }
    }

    private static void onCommonSetup(FMLCommonSetupEvent event) {
        Trace.event("FMLCommonSetupEvent", Map.of());
    }

    private static void onLoadComplete(FMLLoadCompleteEvent event) {
        Trace.event("FMLLoadCompleteEvent", Map.of());
    }

    private static void onConfigLoading(ModConfigEvent.Loading event) {
        ModConfig config = event.getConfig();
        Trace.event("ModConfigEvent.Loading", Map.of(
                "file_name", config.getFileName(),
                "type", config.getType().name(),
                "enabled", ENABLED.get()));
    }

    private static void onRegisterPayloadHandlers(RegisterPayloadHandlersEvent event) {
        Trace.event("RegisterPayloadHandlersEvent", Map.of());
        event.registrar("1").optional().configurationBidirectional(Ping.TYPE, Ping.STREAM_CODEC, (payload, context) -> {});
    }

    private static void onRegisterConfigurationTasks(RegisterConfigurationTasksEvent event) {
        Trace.event("RegisterConfigurationTasksEvent", Map.of(
                "connection_type", event.getListener().getConnectionType().name(),
                "ping_channel", event.getListener().hasChannel(Ping.TYPE)));
    }

    private static void onServerAboutToStart(ServerAboutToStartEvent event) {
        Trace.event("ServerAboutToStartEvent", Map.of());
    }

    private static void onServerStarting(ServerStartingEvent event) {
        Trace.event("ServerStartingEvent", Map.of());
    }

    private static void onServerStarted(ServerStartedEvent event) {
        Trace.event("ServerStartedEvent", Map.of());
    }

    private static void onServerStopping(ServerStoppingEvent event) {
        Trace.log("ServerTickEvent.Pre fired " + preTicks + " times, ServerTickEvent.Post " + postTicks + " times");
        Trace.event("ServerStoppingEvent", Map.of());
    }

    private static void onServerStopped(ServerStoppedEvent event) {
        Trace.event("ServerStoppedEvent", Map.of());
    }

    private static void onServerTickPre(ServerTickEvent.Pre event) {
        if (preTicks++ == 0) {
            Trace.event("ServerTickEvent.Pre", Map.of());
        }
    }

    private static void onServerTickPost(ServerTickEvent.Post event) {
        if (postTicks++ == 0) {
            Trace.event("ServerTickEvent.Post", Map.of());
        }
    }

    /** The payload on {@code parityprobe:ping}. It carries no data: only its negotiation matters. */
    record Ping() implements CustomPacketPayload {
        static final Ping INSTANCE = new Ping();
        static final Type<Ping> TYPE = new Type<>(Identifier.fromNamespaceAndPath(MOD_ID, "ping"));
        static final StreamCodec<FriendlyByteBuf, Ping> STREAM_CODEC = StreamCodec.unit(INSTANCE);

        @Override
        public Type<Ping> type() {
            return TYPE;
        }
    }
}
