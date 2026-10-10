package it.edennetworkitalia.hellomod;

import net.minecraft.client.renderer.entity.PigRenderer;
import net.neoforged.api.distmarker.Dist;
import net.neoforged.bus.api.IEventBus;
import net.neoforged.fml.common.Mod;
import net.neoforged.neoforge.client.event.EntityRenderersEvent;

@Mod(value = HelloMod.MOD_ID, dist = Dist.CLIENT)
public final class HelloModClient {
    public HelloModClient(IEventBus modBus) {
        modBus.addListener(HelloModClient::registerRenderers);
    }

    private static void registerRenderers(EntityRenderersEvent.RegisterRenderers event) {
        event.registerEntityRenderer(HelloMod.GREETER.get(), PigRenderer::new);
    }
}
