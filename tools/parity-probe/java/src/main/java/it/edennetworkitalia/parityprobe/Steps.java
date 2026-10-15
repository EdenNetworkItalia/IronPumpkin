package it.edennetworkitalia.parityprobe;

import java.util.ArrayList;
import java.util.List;

/**
 * The step of the client script (slice 2). The headless client ends each step with a
 * {@code ping_request} barrier and then waits {@code wait_ms} (1000 ms in {@code slice2.toml})
 * before the next action. The step of an event is the number of pings that arrived at least
 * {@link #GRACE_NANOS} before it: an event of step N fires a few ticks after its action, so before
 * the grace of ping N ends, and the action of step N + 1 arrives after it.
 *
 * <p>The ping is counted where it arrives, on the network thread ({@code mixin.PingMixin}). The
 * Rust probe counts the same packet in the same way.
 */
public final class Steps {
    static final long GRACE_NANOS = 500_000_000L;

    private static final List<Long> PINGS = new ArrayList<>();

    private Steps() {}

    public static synchronized void ping() {
        PINGS.add(System.nanoTime());
    }

    static synchronized int current() {
        long limit = System.nanoTime() - GRACE_NANOS;
        int step = 0;
        for (long ping : PINGS) {
            if (ping <= limit) {
                step++;
            }
        }
        return step;
    }
}
