use std::error::Error;
use std::io;
use std::path::PathBuf;
use std::process::ExitCode;

use airs_audio::AudioOutput;
use airs_tts::{InputSource, OutputTarget, TextInput, TtsBackendKind, TtsEngine};
use futures::SinkExt;
use futures::StreamExt;

type AppResult<T> = Result<T, Box<dyn Error>>;

#[derive(Debug, Clone)]
struct PipeOptions {
    source: InputSource,
    targets: Vec<OutputTarget>,
    voice: String,
    speed: f32,
    backend: TtsBackendKind,
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
            "-i:t" => {
                i += 1;
                let text = args.get(i).ok_or_else(|| invalid("-i:t requires text"))?;
                if source.is_some() {
                    return Err(invalid("-i can only be used once"));
                }
                source = Some(InputSource::Text(text.clone()));
            }
            "-i:f" => {
                i += 1;
                let path = args
                    .get(i)
                    .ok_or_else(|| invalid("-i:f requires a file path"))?;
                if source.is_some() {
                    return Err(invalid("-i can only be used once"));
                }
                source = Some(InputSource::File(PathBuf::from(path)));
            }
            "-i:s" => {
                if source.is_some() {
                    return Err(invalid("-i can only be used once"));
                }
                source = Some(InputSource::Stdin);
            }
            "-o:d" => {
                let name = peek_value(args, &mut i);
                targets.push(OutputTarget::Device(name));
            }
            "-o:f" => {
                i += 1;
                let path = args
                    .get(i)
                    .ok_or_else(|| invalid("-o:f requires a file path"))?;
                targets.push(OutputTarget::File(PathBuf::from(path)));
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

    let source = source.ok_or_else(|| invalid("pipe requires -i:t, -i:f, or -i:s"))?;
    if targets.is_empty() {
        return Err(invalid("pipe requires at least one -o:d or -o:f"));
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

/// Peek at the next argument; if it looks like a value (not a flag), consume it.
fn peek_value(args: &[String], i: &mut usize) -> Option<String> {
    if let Some(next) = args.get(*i + 1) {
        if !next.starts_with('-') {
            *i += 1;
            return Some(next.clone());
        }
    }
    None
}

fn cmd_help() {
    let defaults = TtsDefaults::default();
    println!("Usage:");
    println!("  airs-tts --help");
    println!("  airs-tts --version");
    println!("  airs-tts list_voices [--backend <name>]");
    println!(
        "  airs-tts pipe -i:t <text> -i:f <file> -i:s -o:d [device] -o:f <file> [--voice <name>] [--speed <value>] [--backend <name>]"
    );
    println!();
    println!("Source:");
    println!("  -i:t <text>      Text");
    println!("  -i:f <path>      Text file");
    println!("  -i:s             Standard input");
    println!();
    println!("Target:");
    println!("  -o:d             Default audio output device");
    println!("  -o:d <name>      Named audio output device");
    println!("  -o:f <path>      Audio file");
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
    let mut tts = TtsEngine::new()
        .set_backend(options.backend)
        .set_voice(&options.voice)
        .set_speed(options.speed)
        .init().await?;

    // input
    let mut input: TextInput = match &options.source {
        InputSource::Stdin => {
            eprintln!("Reading from stdin...");
            TextInput::new(options.source.clone())
        }
        _ => TextInput::new(options.source.clone()),
    };

    // output
    let mut outputs: Vec<AudioOutput> = options
        .targets
        .iter()
        .map(|target| AudioOutput::new(target.clone()))
        .collect::<Vec<_>>();

    // process
    while let Some(sentence) = input.next().await {
        let audio_slice = tts.call(sentence?).await?;
        for output in outputs.iter_mut() {
            output.send(audio_slice.clone()).await?;
        }
    }

    for output in &mut outputs {
        output.close().await?;
    }

    Ok(())
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
            "-i:t".to_string(),
            "Hello world".to_string(),
            "-o:d".to_string(),
        ])
        .expect("parse command");

        match cmd {
            Command::Pipe { options } => {
                assert_eq!(options.source, InputSource::Text("Hello world".to_string()));
                assert_eq!(options.targets, vec![OutputTarget::Device(None)]);
            }
            _ => panic!("expected Pipe"),
        }
    }

    #[test]
    fn parse_pipe_file_to_file() {
        let cmd = parse_command(vec![
            "pipe".to_string(),
            "-i:f".to_string(),
            "input.txt".to_string(),
            "-o:f".to_string(),
            "out.wav".to_string(),
        ])
        .expect("parse command");

        match cmd {
            Command::Pipe { options } => {
                assert_eq!(
                    options.source,
                    InputSource::File(PathBuf::from("input.txt"))
                );
                assert_eq!(
                    options.targets,
                    vec![OutputTarget::File(PathBuf::from("out.wav"))]
                );
            }
            _ => panic!("expected Pipe"),
        }
    }

    #[test]
    fn parse_pipe_stdin_to_named_device() {
        let cmd = parse_command(vec![
            "pipe".to_string(),
            "-i:s".to_string(),
            "-o:d".to_string(),
            "Speakers".to_string(),
        ])
        .expect("parse command");

        match cmd {
            Command::Pipe { options } => {
                assert_eq!(options.source, InputSource::Stdin);
                assert_eq!(
                    options.targets,
                    vec![OutputTarget::Device(Some("Speakers".to_string()))]
                );
            }
            _ => panic!("expected Pipe"),
        }
    }

    #[test]
    fn parse_pipe_multiple_targets() {
        let cmd = parse_command(vec![
            "pipe".to_string(),
            "-i:t".to_string(),
            "Hello".to_string(),
            "-o:d".to_string(),
            "Speakers".to_string(),
            "-o:f".to_string(),
            "out.wav".to_string(),
        ])
        .expect("parse command");

        match cmd {
            Command::Pipe { options } => {
                assert_eq!(
                    options.targets,
                    vec![
                        OutputTarget::Device(Some("Speakers".to_string())),
                        OutputTarget::File(PathBuf::from("out.wav")),
                    ]
                );
            }
            _ => panic!("expected Pipe"),
        }
    }

    #[test]
    fn parse_pipe_missing_source_fails() {
        let err = parse_command(vec!["pipe".to_string(), "-o:d".to_string()])
            .expect_err("missing source should fail");

        assert_eq!(err.to_string(), "pipe requires -i:t, -i:f, or -i:s");
    }

    #[test]
    fn parse_pipe_missing_target_fails() {
        let err = parse_command(vec![
            "pipe".to_string(),
            "-i:t".to_string(),
            "Hello".to_string(),
        ])
        .expect_err("missing target should fail");

        assert_eq!(err.to_string(), "pipe requires at least one -o:d or -o:f");
    }

    #[test]
    fn parse_pipe_unknown_input_flag_fails() {
        let err = parse_command(vec![
            "pipe".to_string(),
            "-i:x".to_string(),
            "Hello".to_string(),
            "-o:d".to_string(),
        ])
        .expect_err("unknown input flag should fail");

        assert_eq!(err.to_string(), "unexpected argument: -i:x");
    }

    #[test]
    fn parse_pipe_unknown_output_flag_fails() {
        let err = parse_command(vec![
            "pipe".to_string(),
            "-i:t".to_string(),
            "Hello".to_string(),
            "-o:x".to_string(),
        ])
        .expect_err("unknown output flag should fail");

        assert_eq!(err.to_string(), "unexpected argument: -o:x");
    }

    #[test]
    fn parse_pipe_backend_voice_and_speed() {
        let cmd = parse_command(vec![
            "pipe".to_string(),
            "-i:t".to_string(),
            "Hello".to_string(),
            "-o:d".to_string(),
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
