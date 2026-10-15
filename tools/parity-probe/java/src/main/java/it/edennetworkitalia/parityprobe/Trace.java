package it.edennetworkitalia.parityprobe;

import com.mojang.logging.LogUtils;
import java.io.BufferedWriter;
import java.io.IOException;
import java.io.UncheckedIOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.Map;
import java.util.TreeMap;
import org.slf4j.Logger;

/**
 * Writes {@code probe-trace.jsonl} in the server directory: one JSON object per event with the
 * keys {@code ordinal}, {@code name} and {@code fields}, in this order, and the fields sorted by
 * name. The format is in {@code tools/parity-probe/README.md}.
 */
final class Trace {
    static final Path FILE = Path.of("probe-trace.jsonl");
    private static final Logger LOGGER = LogUtils.getLogger();
    private static BufferedWriter writer;
    private static int ordinal;

    private Trace() {}

    // Mod bus events of the parallel dispatch run on worker threads.
    static synchronized void event(String name, Map<String, Object> fields) {
        StringBuilder line = new StringBuilder();
        line.append("{\"ordinal\":").append(ordinal++);
        line.append(",\"name\":");
        quote(line, name);
        line.append(",\"fields\":{");
        boolean first = true;
        for (Map.Entry<String, Object> field : new TreeMap<>(fields).entrySet()) {
            if (!first) {
                line.append(',');
            }
            first = false;
            quote(line, field.getKey());
            line.append(':');
            Object value = field.getValue();
            if (value instanceof Boolean || value instanceof Integer || value instanceof Long) {
                line.append(value);
            } else {
                quote(line, String.valueOf(value));
            }
        }
        line.append("}}");
        try {
            if (writer == null) {
                writer = Files.newBufferedWriter(FILE, StandardCharsets.UTF_8);
            }
            writer.write(line.toString());
            writer.newLine();
            writer.flush();
        } catch (IOException e) {
            throw new UncheckedIOException(e);
        }
        LOGGER.info("[parityprobe] {}", line);
    }

    static void log(String message) {
        LOGGER.info("[parityprobe] {}", message);
    }

    private static void quote(StringBuilder out, String value) {
        out.append('"');
        for (int i = 0; i < value.length(); i++) {
            char c = value.charAt(i);
            switch (c) {
                case '"' -> out.append("\\\"");
                case '\\' -> out.append("\\\\");
                case '\n' -> out.append("\\n");
                case '\r' -> out.append("\\r");
                case '\t' -> out.append("\\t");
                default -> {
                    if (c < 0x20) {
                        out.append(String.format("\\u%04x", (int) c));
                    } else {
                        out.append(c);
                    }
                }
            }
        }
        out.append('"');
    }
}
