package it.edennetworkitalia.parityprobe.mixin;

import it.edennetworkitalia.parityprobe.Steps;
import net.minecraft.network.protocol.ping.ServerboundPingRequestPacket;
import net.minecraft.server.network.ServerGamePacketListenerImpl;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** Counts the play {@code ping_request} of the client script barrier. NeoForge has no event for it. */
@Mixin(ServerGamePacketListenerImpl.class)
public abstract class PingMixin {
    @Inject(method = "handlePingRequest", at = @At("HEAD"))
    private void parityprobe$onPingRequest(ServerboundPingRequestPacket packet, CallbackInfo ci) {
        Steps.ping();
    }
}
