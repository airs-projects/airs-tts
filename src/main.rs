use std::error::Error;
use std::io::{self, Read};
use std::path::PathBuf;
use std::process::ExitCode;

use airs_audio::AudioSink;
use airs_tts::{Processor, TtsBackendKind};
use futures::SinkExt;

type AppResult<T> = Result<T, Box<dyn Error>>;

#[derive(Debug, Clone)]
struct PipeOptions {
    source: SourceSpec,
    targets: Vec<TargetSpec>,
    voice: String,
    speed: f32,
    backend: TtsBackendKind,
}

#[derive(Debug, Clone, Eq, PartialEq)]
enum SourceSpec {
    Text(String),
    File(PathBuf),
    Stdin,
}

#[derive(Debug, Clone, Eq, PartialEq)]
enum TargetSpec {
    File(PathBuf),
    Device(Option<String>),
}

#[derive(Debug, Clone)]
struct TtsDefaults {
    voice: String,
    speed: f32,
    backend: TtsBackendKind,
}

impl Default for TtsDefaults {
    fn default() -> Self {
        Self {
            voice: "af_heart".into(),
            speed: 1.0,
            backend: TtsBackendKind::Kokoro,
        }
    }
}

#[derive(Debug)]
enum Command {
    Help,
    Version,
    ListVoices { backend: TtsBackendKind },
    Pipe { options: PipeOptions },
}

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> AppResult<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();

    match parse_command(args)? {
        Command::Help => cmd_help(),
        Command::Version => cmd_version(),
        Command::ListVoices { backend } => cmd_list_voices(backend)?,
        Command::Pipe { options } => cmd_pipe(options).await?,
    }

    Ok(())
}

fn parse_command(args: Vec<String>) -> Result<Command, io::Error> {
    match args.as_slice() {
        [] => Ok(Command::Help),
        [arg] if arg == "--help" => Ok(Command::Help),
        [arg] if arg == "--version" => Ok(Command::Version),
        [command, ..] if command == "list_voices" => parse_list_voices(&args[1..]),
        [command, ..] if command == "pipe" => parse_pipe(&args[1..]),
        [command, ..] => Err(invalid(format!("unknown function: {command}"))),
    }
}

fn parse_list_voices(args: &[String]) -> Result<Command, io::Error> {
    let mut backend = TtsDefaults::default().backend;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--backend" => {
                i += 1;
                let value = args
                    .get(i)
                    .ok_or_else(|| invalid("--backend requires a name"))?;
                backend = parse_backend(value)?;
            }
            arg => return Err(invalid(format!("unexpected argument: {arg}"))),
        }
        i += 1;
    }

    Ok(Command::ListVoices { backend })
}

fn parse_pipe(args: &[String]) -> Result<Command, io::Error> {
    let defaults = TtsDefaults::default();
    let mut source = None;
    let mut targets = Vec::new();
    let mut voice = defaults.voice;
    let mut speed = defaults.speed;
    let mut backend = defaults.backend;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-i" => {
                if source.is_some() {
                    return Err(invalid("-i can only be used once"));
                }
                i += 1;
                let value = args
                    .get(i)
                    .ok_or_else(|| invalid("-i requires text:<text>, file:<path>, or stdin"))?;
                source = Some(parse_source(value)?);
            }
            "-o" => {
                i += 1;
                let value = args
                    .get(i)
                    .ok_or_else(|| invalid("-o requires file:<path> or device[:name]"))?;
                targets.push(parse_target(value)?);
            }
            "--backend" => {
                i += 1;
                let value = args
                    .get(i)
                    .ok_or_else(|| invalid("--backend requires a name"))?;
                backend = parse_backend(value)?;
            }
            "--voice" => {
                i += 1;
                voice = args
                    .get(i)
                    .ok_or_else(|| invalid("--voice requires a name"))?
                    .clone();
            }
            "--speed" => {
                i += 1;
                let val = args
                    .get(i)
                    .ok_or_else(|| invalid("--speed requires a value"))?;
                speed = val
                    .parse::<f32>()
                    .map_err(|_| invalid(format!("invalid speed: {val}")))?;
            }
            arg => return Err(invalid(format!("unexpected argument: {arg}"))),
        }
        i += 1;
    }

    let source = source
        .ok_or_else(|| invalid("pipe requires -i text:<text>, -i file:<path>, or -i stdin"))?;
    if targets.is_empty() {
        return Err(invalid(
            "pipe requires at least one -o file:<path> or -o device[:name]",
        ));
    }

    Ok(Command::Pipe {
        options: PipeOptions {
            source,
            targets,
            voice,
            speed,
            backend,
        },
    })
}

fn parse_source(value: &str) -> Result<SourceSpec, io::Error> {
    match split_typed_value(value) {
        ("text", Some(text)) if !text.is_empty() => Ok(SourceSpec::Text(text.to_string())),
        ("text", _) => Err(invalid("-i text requires text")),
        ("file", Some(path)) if !path.is_empty() => Ok(SourceSpec::File(PathBuf::from(path))),
        ("file", _) => Err(invalid("-i file requires a path")),
        ("stdin", None) => Ok(SourceSpec::Stdin),
        ("stdin", Some("")) => Ok(SourceSpec::Stdin),
        ("stdin", Some(_)) => Err(invalid("-i stdin does not accept a value")),
        (kind, _) => Err(invalid(format!("unsupported input type: {kind}"))),
    }
}

fn parse_target(value: &str) -> Result<TargetSpec, io::Error> {
    match split_typed_value(value) {
        ("file", Some(path)) if !path.is_empty() => Ok(TargetSpec::File(PathBuf::from(path))),
        ("file", _) => Err(invalid("-o file requires a path")),
        ("device", name) => Ok(TargetSpec::Device(
            name.filter(|name| !name.is_empty()).map(str::to_owned),
        )),
        (kind, _) => Err(invalid(format!("unsupported output type: {kind}"))),
    }
}

fn split_typed_value(value: &str) -> (&str, Option<&str>) {
    match value.split_once(':') {
        Some((kind, value)) => (kind, Some(value)),
        None => (value, None),
    }
}

fn cmd_help() {
    let defaults = TtsDefaults::default();
    println!("Usage:");
    println!("  airs-tts --help");
    println!("  airs-tts --version");
    println!("  airs-tts list_voices [--backend <name>]");
    println!(
        "  airs-tts pipe -i <source> -o <target> [-o <target>...] [--voice <name>] [--speed <value>] [--backend <name>]"
    );
    println!();
    println!("Source:");
    println!("  -i text:<text>   Text");
    println!("  -i file:<path>   Text file");
    println!("  -i stdin         Standard input");
    println!();
    println!("Target:");
    println!("  -o file:<path>   Audio file");
    println!("  -o device        Default audio output device");
    println!("  -o device:<name> Named audio output device");
    println!();
    println!(
        "Defaults: voice={}, speed={}, backend={}.",
        defaults.voice,
        defaults.speed,
        backend_name(defaults.backend)
    );
}

fn cmd_version() {
    println!("{}", airs_tts::version());
}

fn cmd_list_voices(_backend: TtsBackendKind) -> AppResult<()> {
    let voices = airs_tts::tts_list_voices()?;

    if voices.is_empty() {
        println!("No voices found.");
        return Ok(());
    }

    for voice in &voices {
        println!("{voice}");
    }

    println!();
    println!("{} voices loaded", voices.len());
    Ok(())
}

async fn cmd_pipe(options: PipeOptions) -> AppResult<()> {
    let mut tts = Processor::new()
        .set_backend(options.backend)
        .set_voice(&options.voice)
        .set_speed(options.speed)
        .init()
        .await?;

    let input = options.source.read()?;
    let mut outputs: Vec<AudioSink> = options
        .targets
        .into_iter()
        .map(TargetSpec::into_sink)
        .collect::<Vec<_>>();

    let audio_frame = tts.process(input).await?;
    for output in outputs.iter_mut() {
        output.send(audio_frame.clone()).await?;
    }

    for output in &mut outputs {
        output.close().await?;
    }

    Ok(())
}

impl SourceSpec {
    fn read(&self) -> io::Result<String> {
        match self {
            Self::Text(text) => Ok(text.clone()),
            Self::File(path) => std::fs::read_to_string(path),
            Self::Stdin => {
                eprintln!("Reading from stdin...");
                let mut text = String::new();
                io::stdin().read_to_string(&mut text)?;
                Ok(text)
            }
        }
    }
}

impl TargetSpec {
    fn into_sink(self) -> AudioSink {
        match self {
            Self::File(path) => AudioSink::to_file(path),
            Self::Device(name) => AudioSink::to_device(name),
        }
    }
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message.into())
}

fn parse_backend(name: &str) -> Result<TtsBackendKind, io::Error> {
    match name {
        "kokoro" => Ok(TtsBackendKind::Kokoro),
        _ => Err(invalid(format!("unsupported backend: {name}"))),
    }
}

fn backend_name(backend: TtsBackendKind) -> &'static str {
    match backend {
        TtsBackendKind::Kokoro => "kokoro",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_pipe_text_to_device() {
        let cmd = parse_command(vec![
            "pipe".to_string(),
            "-i".to_string(),
            "text:Hello world".to_string(),
            "-o".to_string(),
            "device".to_string(),
        ])
        .expect("parse command");

        match cmd {
            Command::Pipe { options } => {
                assert_eq!(options.source, SourceSpec::Text("Hello world".to_string()));
                assert_eq!(options.targets, vec![TargetSpec::Device(None)]);
            }
            _ => panic!("expected Pipe"),
        }
    }

    #[test]
    fn parse_pipe_file_to_file() {
        let cmd = parse_command(vec![
            "pipe".to_string(),
            "-i".to_string(),
            "file:input.txt".to_string(),
            "-o".to_string(),
            "file:out.wav".to_string(),
        ])
        .expect("parse command");

        match cmd {
            Command::Pipe { options } => {
                assert_eq!(options.source, SourceSpec::File(PathBuf::from("input.txt")));
                assert_eq!(
                    options.targets,
                    vec![TargetSpec::File(PathBuf::from("out.wav"))]
                );
            }
            _ => panic!("expected Pipe"),
        }
    }

    #[test]
    fn parse_pipe_stdin_to_named_device() {
        let cmd = parse_command(vec![
            "pipe".to_string(),
            "-i".to_string(),
            "stdin".to_string(),
            "-o".to_string(),
            "device:Speakers".to_string(),
        ])
        .expect("parse command");

        match cmd {
            Command::Pipe { options } => {
                assert_eq!(options.source, SourceSpec::Stdin);
                assert_eq!(
                    options.targets,
                    vec![TargetSpec::Device(Some("Speakers".to_string()))]
                );
            }
            _ => panic!("expected Pipe"),
        }
    }

    #[test]
    fn parse_pipe_multiple_targets() {
        let cmd = parse_command(vec![
            "pipe".to_string(),
            "-i".to_string(),
            "text:Hello".to_string(),
            "-o".to_string(),
            "device:Speakers".to_string(),
            "-o".to_string(),
            "file:out.wav".to_string(),
        ])
        .expect("parse command");

        match cmd {
            Command::Pipe { options } => {
                assert_eq!(
                    options.targets,
                    vec![
                        TargetSpec::Device(Some("Speakers".to_string())),
                        TargetSpec::File(PathBuf::from("out.wav")),
                    ]
                );
            }
            _ => panic!("expected Pipe"),
        }
    }

    #[test]
    fn parse_pipe_missing_source_fails() {
        let err = parse_command(vec![
            "pipe".to_string(),
            "-o".to_string(),
            "device".to_string(),
        ])
        .expect_err("missing source should fail");

        assert_eq!(
            err.to_string(),
            "pipe requires -i text:<text>, -i file:<path>, or -i stdin"
        );
    }

    #[test]
    fn parse_pipe_missing_target_fails() {
        let err = parse_command(vec![
            "pipe".to_string(),
            "-i".to_string(),
            "text:Hello".to_string(),
        ])
        .expect_err("missing target should fail");

        assert_eq!(
            err.to_string(),
            "pipe requires at least one -o file:<path> or -o device[:name]"
        );
    }

    #[test]
    fn parse_pipe_unknown_input_flag_fails() {
        let err = parse_command(vec![
            "pipe".to_string(),
            "-i".to_string(),
            "url:Hello".to_string(),
            "-o".to_string(),
            "device".to_string(),
        ])
        .expect_err("unknown input flag should fail");

        assert_eq!(err.to_string(), "unsupported input type: url");
    }

    #[test]
    fn parse_pipe_unknown_output_flag_fails() {
        let err = parse_command(vec![
            "pipe".to_string(),
            "-i".to_string(),
            "text:Hello".to_string(),
            "-o".to_string(),
            "url:speaker".to_string(),
        ])
        .expect_err("unknown output flag should fail");

        assert_eq!(err.to_string(), "unsupported output type: url");
    }

    #[test]
    fn parse_pipe_backend_voice_and_speed() {
        let cmd = parse_command(vec![
            "pipe".to_string(),
            "-i".to_string(),
            "text:Hello".to_string(),
            "-o".to_string(),
            "device".to_string(),
            "--backend".to_string(),
            "kokoro".to_string(),
            "--voice".to_string(),
            "bf_emma".to_string(),
            "--speed".to_string(),
            "1.25".to_string(),
        ])
        .expect("parse command");

        match cmd {
            Command::Pipe { options } => {
                assert_eq!(options.backend, TtsBackendKind::Kokoro);
                assert_eq!(options.voice, "bf_emma");
                assert_eq!(options.speed, 1.25);
            }
            _ => panic!("expected Pipe"),
        }
    }

    #[test]
    fn parse_pipe_windows_file_paths() {
        let cmd = parse_command(vec![
            "pipe".to_string(),
            "-i".to_string(),
            "file:E:\\input.txt".to_string(),
            "-o".to_string(),
            "file:E:\\speech.wav".to_string(),
        ])
        .expect("parse command");

        match cmd {
            Command::Pipe { options } => {
                assert_eq!(
                    options.source,
                    SourceSpec::File(PathBuf::from("E:\\input.txt"))
                );
                assert_eq!(
                    options.targets,
                    vec![TargetSpec::File(PathBuf::from("E:\\speech.wav"))]
                );
            }
            _ => panic!("expected Pipe"),
        }
    }

    #[test]
    fn parse_list_voices() {
        let cmd = parse_command(vec!["list_voices".to_string()]).expect("parse command");
        match cmd {
            Command::ListVoices { backend } => {
                assert_eq!(backend, TtsBackendKind::Kokoro);
            }
            _ => panic!("expected ListVoices"),
        }
    }

    #[test]
    fn parse_list_voices_backend() {
        let cmd = parse_command(vec![
            "list_voices".to_string(),
            "--backend".to_string(),
            "kokoro".to_string(),
        ])
        .expect("parse command");
        match cmd {
            Command::ListVoices { backend } => {
                assert_eq!(backend, TtsBackendKind::Kokoro);
            }
            _ => panic!("expected ListVoices"),
        }
    }
}
