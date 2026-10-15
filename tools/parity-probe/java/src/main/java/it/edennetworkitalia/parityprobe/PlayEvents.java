package it.edennetworkitalia.parityprobe;

import java.util.HashMap;
import java.util.HashSet;
import java.util.Locale;
import java.util.Map;
import java.util.Set;
import net.minecraft.core.BlockPos;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.level.block.state.BlockState;
import net.neoforged.bus.api.IEventBus;
import net.neoforged.neoforge.event.ServerChatEvent;
import net.neoforged.neoforge.event.entity.player.ClientInformationUpdatedEvent;
import net.neoforged.neoforge.event.entity.player.PlayerEvent;
import net.neoforged.neoforge.event.entity.player.PlayerInteractEvent;
import net.neoforged.neoforge.event.entity.player.PlayerSwitchHotbarSlotEvent;
import net.neoforged.neoforge.event.entity.player.UseItemOnBlockEvent;
import net.neoforged.neoforge.event.level.BlockEvent;
import net.neoforged.neoforge.event.level.block.BreakBlockEvent;
import net.neoforged.neoforge.event.server.ServerStoppingEvent;
import net.neoforged.neoforge.event.tick.PlayerTickEvent;
import net.neoforged.neoforge.event.tick.ServerTickEvent;

/**
 * The play events of slice 2: the events of the player, the chat, the block break and place and the
 * ticks that the client script {@code scripts/slice2.toml} fires. Each line has the field
 * {@code step} ({@link Steps}). The window opens at {@code PlayerLoggedInEvent} and closes at
 * {@code ServerStoppingEvent}: the events of the configuration phase belong to slice 1.
 *
 * <p>Positions are relative to the block of the player's feet at the login, the origin of the
 * {@code ~} coordinates of the script, because the two servers spawn the player at different blocks,
 * and NeoForge at a random block around the world spawn. For the same reason no field names a
 * block of the terrain: only the placed block, which comes from the script.
 * The tick events are traced once per step.
 */
final class PlayEvents {
    private static boolean open;
    private static BlockPos origin = BlockPos.ZERO;
    // The step of the running server tick, taken at ServerTickEvent.Pre, so the four tick events of
    // one tick share it. -1 before the first tick of the window.
    private static int tickStep = -1;
    private static final Set<String> TRACED_IN_STEP = new HashSet<>();

    private PlayEvents() {}

    static void register(IEventBus bus) {
        bus.addListener(PlayEvents::onLoggedIn);
        bus.addListener(PlayEvents::onLoggedOut);
        bus.addListener(PlayEvents::onServerStopping);
        bus.addListener(PlayEvents::onLoadFromFile);
        bus.addListener(PlayEvents::onSaveToFile);
        bus.addListener(PlayEvents::onNameFormat);
        bus.addListener(PlayEvents::onTabListNameFormat);
        bus.addListener(PlayEvents::onChangeGameMode);
        bus.addListener(PlayEvents::onClientInformationUpdated);
        bus.addListener(PlayEvents::onServerChat);
        bus.addListener(PlayEvents::onLeftClickBlock);
        bus.addListener(PlayEvents::onRightClickBlock);
        bus.addListener(PlayEvents::onRightClickItem);
        bus.addListener(PlayEvents::onUseItemOnBlock);
        bus.addListener(PlayEvents::onSwitchHotbarSlotPre);
        bus.addListener(PlayEvents::onSwitchHotbarSlotPost);
        bus.addListener(PlayEvents::onBreakSpeed);
        bus.addListener(PlayEvents::onHarvestCheck);
        bus.addListener(PlayEvents::onBreakBlock);
        bus.addListener(PlayEvents::onEntityPlace);
        bus.addListener(PlayEvents::onServerTickPre);
        bus.addListener(PlayEvents::onServerTickPost);
        bus.addListener(PlayEvents::onPlayerTickPre);
        bus.addListener(PlayEvents::onPlayerTickPost);
    }

    private static void trace(String name, Map<String, Object> fields, int step) {
        Map<String, Object> withStep = new HashMap<>(fields);
        withStep.put("step", step);
        Trace.event(name, withStep);
    }

    private static void trace(String name, Map<String, Object> fields) {
        if (open) {
            trace(name, fields, Steps.current());
        }
    }

    /** Traces {@code name} only the first time it fires in {@code step}. */
    private static void traceOncePerStep(String name, int step) {
        if (open && step >= 0 && TRACED_IN_STEP.add(name + " " + step)) {
            trace(name, Map.of(), step);
        }
    }

    private static String relative(BlockPos pos) {
        BlockPos offset = pos.subtract(origin);
        return offset.getX() + " " + offset.getY() + " " + offset.getZ();
    }

    private static String block(BlockState state) {
        return BuiltInRegistries.BLOCK.getKey(state.getBlock()).toString();
    }

    private static String item(ItemStack stack) {
        return BuiltInRegistries.ITEM.getKey(stack.getItem()).toString();
    }

    private static String lower(Enum<?> value) {
        return value.name().toLowerCase(Locale.ROOT);
    }

    private static void onLoggedIn(PlayerEvent.PlayerLoggedInEvent event) {
        open = true;
        origin = event.getEntity().blockPosition();
        trace("PlayerEvent.PlayerLoggedInEvent", Map.of());
    }

    private static void onLoggedOut(PlayerEvent.PlayerLoggedOutEvent event) {
        trace("PlayerEvent.PlayerLoggedOutEvent", Map.of());
    }

    private static void onServerStopping(ServerStoppingEvent event) {
        open = false;
    }

    private static void onLoadFromFile(PlayerEvent.LoadFromFile event) {
        trace("PlayerEvent.LoadFromFile", Map.of());
    }

    private static void onSaveToFile(PlayerEvent.SaveToFile event) {
        trace("PlayerEvent.SaveToFile", Map.of());
    }

    // NeoForge caches the names until a refresh; a server that does not fires these on every read.
    private static void onNameFormat(PlayerEvent.NameFormat event) {
        trace("PlayerEvent.NameFormat", Map.of());
    }

    private static void onTabListNameFormat(PlayerEvent.TabListNameFormat event) {
        trace("PlayerEvent.TabListNameFormat", Map.of());
    }

    private static void onChangeGameMode(PlayerEvent.PlayerChangeGameModeEvent event) {
        trace("PlayerEvent.PlayerChangeGameModeEvent", Map.of(
                "current", event.getCurrentGameMode().getName(),
                "new", event.getNewGameMode().getName()));
    }

    private static void onClientInformationUpdated(ClientInformationUpdatedEvent event) {
        trace("ClientInformationUpdatedEvent", Map.of());
    }

    private static void onServerChat(ServerChatEvent event) {
        trace("ServerChatEvent", Map.of("raw_text", event.getRawText()));
    }

    private static void onLeftClickBlock(PlayerInteractEvent.LeftClickBlock event) {
        trace("PlayerInteractEvent.LeftClickBlock", Map.of(
                "action", lower(event.getAction()),
                "face", event.getFace() == null ? "none" : event.getFace().getSerializedName(),
                "hand", lower(event.getHand()),
                "pos", relative(event.getPos())));
    }

    private static void onRightClickBlock(PlayerInteractEvent.RightClickBlock event) {
        trace("PlayerInteractEvent.RightClickBlock", Map.of(
                "face", event.getFace() == null ? "none" : event.getFace().getSerializedName(),
                "hand", lower(event.getHand()),
                "item", item(event.getItemStack()),
                "pos", relative(event.getPos())));
    }

    private static void onRightClickItem(PlayerInteractEvent.RightClickItem event) {
        trace("PlayerInteractEvent.RightClickItem", Map.of(
                "hand", lower(event.getHand()),
                "item", item(event.getItemStack())));
    }

    private static void onUseItemOnBlock(UseItemOnBlockEvent event) {
        trace("UseItemOnBlockEvent", Map.of(
                "face", event.getFace() == null ? "none" : event.getFace().getSerializedName(),
                "hand", lower(event.getHand()),
                "item", item(event.getItemStack()),
                "phase", lower(event.getUsePhase()),
                "pos", relative(event.getPos())));
    }

    private static void onSwitchHotbarSlotPre(PlayerSwitchHotbarSlotEvent.Pre event) {
        trace("PlayerSwitchHotbarSlotEvent.Pre", Map.of(
                "new_slot", event.getNewSlotIndex(),
                "old_slot", event.getOldSlotIndex()));
    }

    private static void onSwitchHotbarSlotPost(PlayerSwitchHotbarSlotEvent.Post event) {
        trace("PlayerSwitchHotbarSlotEvent.Post", Map.of(
                "new_slot", event.getNewSlotIndex(),
                "old_slot", event.getOldSlotIndex()));
    }

    private static void onBreakSpeed(PlayerEvent.BreakSpeed event) {
        trace("PlayerEvent.BreakSpeed", Map.of());
    }

    private static void onHarvestCheck(PlayerEvent.HarvestCheck event) {
        trace("PlayerEvent.HarvestCheck", Map.of());
    }

    private static void onBreakBlock(BreakBlockEvent event) {
        trace("BreakBlockEvent", Map.of("pos", relative(event.getPos())));
    }

    private static void onEntityPlace(BlockEvent.EntityPlaceEvent event) {
        trace("BlockEvent.EntityPlaceEvent", Map.of(
                "block", block(event.getPlacedBlock()),
                "pos", relative(event.getPos())));
    }

    private static void onServerTickPre(ServerTickEvent.Pre event) {
        if (open) {
            tickStep = Steps.current();
            traceOncePerStep("ServerTickEvent.Pre", tickStep);
        }
    }

    private static void onServerTickPost(ServerTickEvent.Post event) {
        traceOncePerStep("ServerTickEvent.Post", tickStep);
    }

    private static void onPlayerTickPre(PlayerTickEvent.Pre event) {
        traceOncePerStep("PlayerTickEvent.Pre", tickStep);
    }

    private static void onPlayerTickPost(PlayerTickEvent.Post event) {
        traceOncePerStep("PlayerTickEvent.Post", tickStep);
    }
}
