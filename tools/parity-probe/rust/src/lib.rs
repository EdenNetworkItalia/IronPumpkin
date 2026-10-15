//! Rust probe of the differential parity harness. It is a native mod that listens to the Pumpkin
//! events that carry the semantics of the slice 1 `NeoForge` events and writes `probe-trace.jsonl`
//! in the server directory, in the format of the Java probe (`tools/parity-probe/README.md`).
//!
//! It traces a `NeoForge` event only where `IronPumpkin` fires it today. `diff.py` lists the other
//! events of the Java trace as differences.

use std::{
    fs::File,
    io::{self, BufWriter, Write},
    sync::{
        Arc, Mutex, PoisonError,
        atomic::{AtomicU64, Ordering},
    },
};

use ironpumpkin_mods::{
    ModInit, NativeMod,
    event::{
        BoxFuture, EventHandler, EventPriority,
        server::{LoadType, ServerLoadEvent, ServerTickEndEvent, ServerTickStartEvent},
    },
    register_mod,
    server::Server,
    tracing::{error, info},
};

pub const ID: &str = "parityprobe";
pub const TRACE_FILE: &str = "probe-trace.jsonl";

/// A field value of a trace line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Bool(bool),
    Text(String),
}

/// One trace line: `{"ordinal":N,"name":"...","fields":{...}}`, with the keys in this order and
/// the fields sorted by name, as the Java probe writes it.
#[must_use]
pub fn line(ordinal: u64, name: &str, fields: &[(&str, Value)]) -> String {
    let mut fields = fields.to_vec();
    fields.sort_by(|a, b| a.0.cmp(b.0));
    let fields: Vec<String> = fields
        .iter()
        .map(|(key, value)| {
            let value = match value {
                Value::Bool(value) => value.to_string(),
                Value::Text(value) => quote(value),
            };
            format!("{}:{value}", quote(key))
        })
        .collect();
    format!(
        "{{\"ordinal\":{ordinal},\"name\":{},\"fields\":{{{}}}}}",
        quote(name),
        fields.join(",")
    )
}

fn quote(text: &str) -> String {
    serde_json::Value::String(text.to_owned()).to_string()
}

struct Trace {
    ordinal: u64,
    out: Option<BufWriter<File>>,
}

static TRACE: Mutex<Trace> = Mutex::new(Trace {
    ordinal: 0,
    out: None,
});

fn trace(name: &str, fields: &[(&str, Value)]) {
    let mut trace = TRACE.lock().unwrap_or_else(PoisonError::into_inner);
    let line = line(trace.ordinal, name, fields);
    trace.ordinal += 1;
    if let Err(err) = write_line(&mut trace.out, &line) {
        error!("[{ID}] cannot write {TRACE_FILE}: {err}");
    }
    info!("[{ID}] {line}");
}

fn write_line(out: &mut Option<BufWriter<File>>, line: &str) -> io::Result<()> {
    let out = match out {
        Some(out) => out,
        None => out.insert(BufWriter::new(File::create(TRACE_FILE)?)),
    };
    writeln!(out, "{line}")?;
    out.flush()
}

struct ParityProbe;

impl NativeMod for ParityProbe {
    fn id(&self) -> &'static str {
        ID
    }

    fn display_name(&self) -> &'static str {
        "IronPumpkin parity probe"
    }

    fn version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    // The Java probe adds no content and only an optional channel, so a NeoForge client without it
    // joins. The same holds here.
    fn client_required(&self) -> bool {
        false
    }

    fn init(&self, cx: &mut ModInit) {
        cx.register_event::<ServerLoadEvent, _>(Arc::new(Started), EventPriority::Normal, false);
        cx.register_event::<ServerTickStartEvent, _>(
            Arc::new(Tick {
                name: "ServerTickEvent.Pre",
                count: AtomicU64::new(0),
            }),
            EventPriority::Normal,
            false,
        );
        cx.register_event::<ServerTickEndEvent, _>(
            Arc::new(Tick {
                name: "ServerTickEvent.Post",
                count: AtomicU64::new(0),
            }),
            EventPriority::Normal,
            false,
        );
    }
}

register_mod!(ParityProbe);

/// `ServerStartedEvent`: `ServerLoadEvent` with [`LoadType::Startup`].
struct Started;

impl EventHandler<ServerLoadEvent> for Started {
    fn handle<'a>(
        &'a self,
        _server: &'a Arc<Server>,
        event: &'a ServerLoadEvent,
    ) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            if matches!(event.load_type, LoadType::Startup) {
                trace("ServerStartedEvent", &[]);
            }
        })
    }
}

/// `ServerTickEvent.Pre` and `.Post`. The ticks are counted and only the first one is traced, as
/// in the Java probe.
struct Tick {
    name: &'static str,
    count: AtomicU64,
}

impl<E: ironpumpkin_mods::event::Payload> EventHandler<E> for Tick {
    fn handle<'a>(&'a self, _server: &'a Arc<Server>, _event: &'a E) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            if self.count.fetch_add(1, Ordering::Relaxed) == 0 {
                trace(self.name, &[]);
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{Value, line};

    #[test]
    fn writes_the_line_format_of_the_java_probe() {
        assert_eq!(
            line(0, "ServerStartedEvent", &[]),
            r#"{"ordinal":0,"name":"ServerStartedEvent","fields":{}}"#
        );
        assert_eq!(
            line(
                7,
                "ModConfigEvent.Loading",
                &[
                    ("type", Value::Text("SYNCED".to_owned())),
                    ("enabled", Value::Bool(true)),
                    ("file_name", Value::Text("a \"b\".toml".to_owned())),
                ],
            ),
            r#"{"ordinal":7,"name":"ModConfigEvent.Loading","fields":{"enabled":true,"file_name":"a \"b\".toml","type":"SYNCED"}}"#
        );
    }
}
