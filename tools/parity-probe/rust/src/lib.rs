//! Rust probe of the differential parity harness. It is a native mod that traces the slice 1
//! `NeoForge` events from the mod lifecycle methods, the config loading hook and the Pumpkin
//! events that carry their semantics, and writes `probe-trace.jsonl` in the server directory, in
//! the format of the Java probe (`tools/parity-probe/README.md`). Like the Java probe, it
//! registers the synced config `parityprobe-synced.toml` with one value, `enabled = true`.
//!
//! It traces a `NeoForge` event only where `IronPumpkin` fires it today. `diff.py` lists the other
//! events of the Java trace as differences.

use std::{
    fs::File,
    io::{self, BufWriter, Write},
    marker::PhantomData,
    sync::{
        Arc, Mutex, PoisonError,
        atomic::{AtomicBool, Ordering},
    },
};

use ironpumpkin_mods::{
    ModInit, NativeMod,
    config::{ConfigError, ConfigSpec, ConfigType, ModConfig},
    event::{
        BoxFuture, EventHandler, EventPriority, Payload,
        registries::RegisterEvent,
        server::{
            LoadType, ServerAboutToStartEvent, ServerLoadEvent, ServerStartingEvent,
            ServerStoppedEvent, ServerStoppingEvent, ServerTickEndEvent, ServerTickStartEvent,
        },
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
        trace("FMLConstructModEvent", &[]);
        if let Err(err) = register_config(cx) {
            error!("[{ID}] cannot register the synced config: {err}");
        }
        cx.register_event::<RegisterEvent, _>(
            Arc::new(Register),
            EventPriority::Normal,
            false,
            false,
        );
        cx.register_event::<ServerLoadEvent, _>(
            Arc::new(Started),
            EventPriority::Normal,
            false,
            false,
        );
        trace_first::<ServerAboutToStartEvent>(cx, "ServerAboutToStartEvent");
        trace_first::<ServerStartingEvent>(cx, "ServerStartingEvent");
        trace_first::<ServerTickStartEvent>(cx, "ServerTickEvent.Pre");
        trace_first::<ServerTickEndEvent>(cx, "ServerTickEvent.Post");
        trace_first::<ServerStoppingEvent>(cx, "ServerStoppingEvent");
        trace_first::<ServerStoppedEvent>(cx, "ServerStoppedEvent");
    }

    fn common_setup(&self) {
        trace("FMLCommonSetupEvent", &[]);
    }

    fn load_complete(&self) {
        trace("FMLLoadCompleteEvent", &[]);
    }

    fn config_loading(&self, config: &ModConfig) {
        let mut fields = vec![
            ("file_name", Value::Text(config.file_name().to_owned())),
            (
                "type",
                Value::Text(config.config_type().extension().to_ascii_uppercase()),
            ),
        ];
        match config.get::<bool>("enabled") {
            Ok(enabled) => fields.push(("enabled", Value::Bool(enabled))),
            Err(err) => error!("[{ID}] cannot read enabled: {err}"),
        }
        trace("ModConfigEvent.Loading", &fields);
    }
}

/// The config of the Java probe: `SYNCED`, with only `enabled = true`. Its file is
/// `parityprobe-synced.toml`.
fn register_config(cx: &mut ModInit) -> Result<(), ConfigError> {
    let mut spec = ConfigSpec::builder();
    spec.define("enabled", true);
    cx.register_config(ConfigType::Synced, spec.build()?)?;
    Ok(())
}

register_mod!(ParityProbe);

/// `RegisterEvent` with the key of the registry. `IronPumpkin` fires it for the three registries
/// that the Java probe traces, `minecraft:block`, `minecraft:entity_type` and `minecraft:item`.
struct Register;

impl EventHandler<RegisterEvent> for Register {
    fn handle<'a>(
        &'a self,
        _server: &'a Arc<Server>,
        event: &'a RegisterEvent,
    ) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            trace(
                "RegisterEvent",
                &[("registry", Value::Text(event.registry_key().to_owned()))],
            );
        })
    }
}

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

/// Traces `name` the first time its event fires. The ticks fire many times, so only the first
/// one is traced, as in the Java probe.
struct First<E> {
    name: &'static str,
    traced: AtomicBool,
    _event: PhantomData<fn() -> E>,
}

impl<E: Payload + Send + Sync + 'static> EventHandler<E> for First<E> {
    fn handle<'a>(&'a self, _server: &'a Arc<Server>, _event: &'a E) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            if !self.traced.swap(true, Ordering::Relaxed) {
                trace(self.name, &[]);
            }
        })
    }
}

fn trace_first<E: Payload + Send + Sync + 'static>(cx: &mut ModInit, name: &'static str) {
    let handler = Arc::new(First::<E> {
        name,
        traced: AtomicBool::new(false),
        _event: PhantomData,
    });
    cx.register_event::<E, _>(handler, EventPriority::Normal, false, false);
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
