use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use clap::{Args, Parser, Subcommand};
use pumpkin_neoforge_client::{
    Error,
    channels::ChannelMap,
    check,
    compare::compare_with_reference,
    parse_expected,
    record::{Entry, read_jsonl},
    registries::{RegistryExpectations, check_registries},
    session,
};
use tracing::{error, info};

#[derive(Parser)]
#[command(about = "Headless client that records or asserts the configuration phase of a server")]
struct Cli {
    #[command(subcommand)]
    mode: Mode,
}

#[derive(Subcommand)]
enum Mode {
    /// Connect, run until `finish_configuration` or a disconnect, and write the payload sequence.
    Record {
        #[command(flatten)]
        connection: Connection,
    },
    /// Like record, then fail when the clientbound payload channels differ from the expected list,
    /// a payload does not decode, or the configuration does not end as expected.
    Assert {
        #[command(flatten)]
        connection: Connection,
        /// Expected channels in order, comma separated.
        #[arg(
            long,
            value_delimiter = ',',
            conflicts_with = "expected_file",
            required_unless_present_any = ["expected_file", "expect_disconnect", "registries"]
        )]
        expect: Vec<String>,
        /// File with one expected channel per line; `#` starts a comment.
        #[arg(long)]
        expected_file: Option<PathBuf>,
        /// Translation key of the configuration disconnect that must end the run, instead of
        /// `finish_configuration`.
        #[arg(long, value_name = "TRANSLATION_KEY")]
        expect_disconnect: Option<String>,
        /// TOML file with the expected `neoforge:frozen_registry` payloads.
        #[arg(long)]
        registries: Option<PathBuf>,
    },
    /// Compare a recording with a capture of a real `NeoForge` server: the clientbound channel
    /// order from `neoforge:frozen_registry_sync_start` to `finish_configuration`, with the
    /// registries limited to block, item and entity type, and the bodies of the modded task
    /// payloads.
    Compare {
        /// The capture, JSONL or gzipped JSONL.
        #[arg(long)]
        reference: PathBuf,
        /// The recording, JSONL or gzipped JSONL.
        recording: PathBuf,
    },
}

#[derive(Args)]
struct Connection {
    #[arg(long, default_value = "127.0.0.1")]
    host: String,
    #[arg(long, default_value_t = 25565)]
    port: u16,
    /// Offline player name. Defaults to a random `Probe` name, because a server can reject a
    /// second login of a name whose last session it has not dropped yet.
    #[arg(long)]
    username: Option<String>,
    /// Channel map (TOML, or JSON by extension). With it the client acts as a `NeoForge` client.
    #[arg(long)]
    channels: Option<PathBuf>,
    /// Brand sent in minecraft:brand. Defaults to "neoforge" with a channel map, else "vanilla".
    #[arg(long)]
    brand: Option<String>,
    /// Seconds to wait for the configuration phase to end.
    #[arg(long, default_value_t = 60)]
    timeout: u64,
    /// JSONL file that receives one line per recorded packet.
    #[arg(long)]
    out: Option<PathBuf>,
}

impl Connection {
    fn options(&self) -> Result<session::Options, Error> {
        let channels = self.channels.as_deref().map(ChannelMap::load).transpose()?;
        let brand = self.brand.clone().unwrap_or_else(|| {
            if channels.is_some() {
                "neoforge"
            } else {
                "vanilla"
            }
            .to_owned()
        });
        Ok(session::Options {
            host: self.host.clone(),
            port: self.port,
            username: self
                .username
                .clone()
                .unwrap_or_else(session::random_username),
            brand,
            channels,
            timeout: Duration::from_secs(self.timeout),
        })
    }
}

fn write_jsonl(path: &Path, entries: &[Entry]) -> Result<(), Error> {
    let mut out = std::io::BufWriter::new(std::fs::File::create(path)?);
    for entry in entries {
        serde_json::to_writer(&mut out, entry)?;
        out.write_all(b"\n")?;
    }
    out.flush()?;
    Ok(())
}

async fn record(connection: &Connection) -> Result<(Vec<Entry>, session::Outcome), Error> {
    let options = connection.options()?;
    let mut entries = Vec::new();
    let mut stdout = std::io::stdout().lock();
    let result = session::run(&options, &mut |entry| {
        // A closed stdout must not stop the recording.
        let _ = writeln!(stdout, "{}", entry.line());
        entries.push(entry.clone());
    })
    .await;
    if let Some(path) = &connection.out {
        write_jsonl(path, &entries)?;
        info!("wrote {} entries to {}", entries.len(), path.display());
    }
    let outcome = result?;
    info!("configuration phase ended: {outcome}");
    Ok((entries, outcome))
}

async fn run(cli: Cli) -> Result<bool, Error> {
    match cli.mode {
        Mode::Record { connection } => {
            record(&connection).await?;
            Ok(true)
        }
        Mode::Assert {
            connection,
            expect,
            expected_file,
            expect_disconnect,
            registries,
        } => {
            let registries = registries
                .as_deref()
                .map(RegistryExpectations::load)
                .transpose()?;
            let expected = match expected_file {
                Some(path) => Some(parse_expected(&std::fs::read_to_string(path)?)),
                None if expect.is_empty() => None,
                None => Some(expect),
            };
            let (entries, outcome) = record(&connection).await?;
            let mut failures = check(
                &entries,
                &outcome,
                expected.as_deref(),
                expect_disconnect.as_deref(),
            );
            if let Some(registries) = &registries {
                failures.extend(check_registries(&entries, registries));
            }
            for failure in &failures {
                error!("{failure}");
            }
            if failures.is_empty() {
                info!(
                    "assert passed: {outcome}, {} channels and {} registries matched",
                    expected.as_ref().map_or(0, Vec::len),
                    registries.as_ref().map_or(0, |r| r.0.len())
                );
            }
            Ok(failures.is_empty())
        }
        Mode::Compare {
            reference,
            recording,
        } => {
            let failures =
                compare_with_reference(&read_jsonl(&recording)?, &read_jsonl(&reference)?);
            for failure in &failures {
                error!("{failure}");
            }
            if failures.is_empty() {
                info!("the recording matches {}", reference.display());
            }
            Ok(failures.is_empty())
        }
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();
    match run(Cli::parse()).await {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(e) => {
            error!("{e}");
            ExitCode::from(2)
        }
    }
}
