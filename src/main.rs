use std::str::FromStr;
use std::thread;
use std::time::Duration;

use color_eyre::eyre::{OptionExt, Result};
use cpal::traits::StreamTrait;
use cpal::{
    BufferSize, Device, SampleFormat, StreamConfig, SupportedStreamConfigRange,
    traits::{DeviceTrait, HostTrait},
};
use whisper_rs::{SamplingStrategy, WhisperContext, WhisperContextParameters};

use crate::speech::{SpeechSegment, SpeechToTextClient};
use crate::speech_listener::SpeechEvent;
use crate::{speech_listener::create_stream, tts_client::TtsClient};
use std::sync::mpsc;

mod audio_resampler;
mod command_executor;
mod speech;
mod speech_listener;
mod tts_client;
use clap::{Parser, Subcommand};
use url::Url;

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    RunVoiceAssistant {
        #[arg(short, long, env = "HOME_ASSISTANT_BASE_URL")]
        home_assistant_base_url: Url,

        #[arg(short, long, env = "HOME_ASSISTANT_TOKEN")]
        home_assistant_token: String,

        #[arg(short, long, env = "INPUT_DEVICE_ID")]
        input_device_id: String,

        #[arg(short, long, env = "SILENCE_SECONDS", default_value = "1.0")]
        silence_seconds: f64,

        #[arg(
            short,
            long,
            env = "ROLLING_BUFFER_DURATION_SECONDS",
            default_value = "2.0"
        )]
        rolling_buffer_duration_seconds: f64,
    },
    GetInputDevices,
}

fn get_device(device_id: &str) -> Result<Device> {
    let device_id = cpal::DeviceId::from_str(device_id)?;

    // Get a completely fresh host each time to ensure clean state
    // This is critical for ALSA devices that may get into bad states
    let host = cpal::default_host();
    let input_devices: Vec<Device> = host.input_devices()?.collect();

    let device = input_devices
        .into_iter()
        .find(|d| d.id().ok().map(|id| id == device_id).unwrap_or(false))
        .ok_or_eyre("No input device found")?;

    // Verify device is accessible before returning
    device.id()?;

    Ok(device)
}

fn generate_candidate_configs(device_id: &str) -> Result<Vec<(StreamConfig, SampleFormat)>> {
    // Get device fresh just to query supported configs, then drop it immediately
    let device = get_device(device_id)?;
    let supported_configs: Vec<SupportedStreamConfigRange> =
        device.supported_input_configs()?.collect();
    // Drop device immediately after querying configs to avoid holding onto it
    drop(device);

    for config in &supported_configs {
        println!(
            "config channels: {}, min sample rate: {}, max sample rate: {}, buffer size: {:?}, sample format: {:?}",
            config.channels(),
            config.min_sample_rate(),
            config.max_sample_rate(),
            config.buffer_size(),
            config.sample_format(),
        );
    }

    let mut candidates = Vec::new();

    // Phase 1: Device-reported supported configs with reasonable sample rates
    let preferred_rates = [48000, 44100, 32000, 16000, 8000];

    // Try F32 format first (preferred since all downstream systems use it)
    if let Some(f32_config) = supported_configs
        .iter()
        .find(|config| config.sample_format() == SampleFormat::F32)
    {
        let min_rate = f32_config.min_sample_rate();
        let max_rate = f32_config.max_sample_rate();
        let channels = f32_config.channels();

        println!("F32 config found - min: {}, max: {}", min_rate, max_rate);

        // Try each preferred rate that's within range and valid
        for &rate in &preferred_rates {
            // Skip invalid rates
            if rate == u32::MAX || rate == 0 {
                continue;
            }

            // Only include rates within device's reported range (if max_rate is valid)
            if max_rate != u32::MAX && max_rate != 0 {
                if rate < min_rate || rate > max_rate {
                    continue;
                }
            }

            // Validate rate is in reasonable range for resampling (4000-96000 Hz)
            if rate < 4000 || rate > 96000 {
                continue;
            }

            candidates.push((
                StreamConfig {
                    channels,
                    sample_rate: rate,
                    buffer_size: BufferSize::Default,
                },
                SampleFormat::F32,
            ));
        }
    }

    // Try I16 format as fallback
    if let Some(i16_config) = supported_configs
        .iter()
        .find(|config| config.sample_format() == SampleFormat::I16)
    {
        let min_rate = i16_config.min_sample_rate();
        let max_rate = i16_config.max_sample_rate();
        let channels = i16_config.channels();

        println!("I16 config found - min: {}, max: {}", min_rate, max_rate);

        // Try each preferred rate that's within range and valid
        for &rate in &preferred_rates {
            // Skip invalid rates
            if rate == u32::MAX || rate == 0 {
                continue;
            }

            // Only include rates within device's reported range (if max_rate is valid)
            if max_rate != u32::MAX && max_rate != 0 {
                if rate < min_rate || rate > max_rate {
                    continue;
                }
            }

            // Validate rate is in reasonable range for resampling (4000-96000 Hz)
            if rate < 4000 || rate > 96000 {
                continue;
            }

            candidates.push((
                StreamConfig {
                    channels,
                    sample_rate: rate,
                    buffer_size: BufferSize::Default,
                },
                SampleFormat::I16,
            ));
        }
    }

    // Phase 2: Hardcoded fallbacks (if we have no candidates yet, or as additional fallbacks)
    let hardcoded_fallbacks = vec![
        (16000, SampleFormat::F32),
        (16000, SampleFormat::I16),
        (8000, SampleFormat::F32),
        (8000, SampleFormat::I16),
        (48000, SampleFormat::F32),
    ];

    for (rate, format) in hardcoded_fallbacks {
        // Only add if not already in candidates
        if !candidates
            .iter()
            .any(|(cfg, fmt)| cfg.sample_rate == rate && *fmt == format)
        {
            candidates.push((
                StreamConfig {
                    channels: 1, // Mono
                    sample_rate: rate,
                    buffer_size: BufferSize::Default,
                },
                format,
            ));
        }
    }

    println!("Generated {} candidate configurations", candidates.len());

    Ok(candidates)
}

fn try_create_stream(
    device_id: &str,
    candidates: Vec<(StreamConfig, SampleFormat)>,
    wake_word_threshold: f32,
    vad_threshold: f32,
    silence_seconds: f64,
    rolling_buffer_duration_seconds: f64,
) -> Result<(
    cpal::Stream,
    mpsc::Receiver<SpeechEvent>,
    StreamConfig,
    SampleFormat,
)> {
    let mut errors = Vec::new();

    for (config, sample_format) in candidates {
        println!(
            "Trying {:?} @ {}Hz ({} channel{})...",
            sample_format,
            config.sample_rate,
            config.channels,
            if config.channels == 1 { "" } else { "s" }
        );

        // Get device fresh for each attempt since create_stream takes ownership
        // Verify device is still available before attempting to use it
        let device = match get_device(device_id) {
            Ok(d) => {
                // Verify device is still accessible by checking its name
                if let Err(e) = d.id() {
                    let error_msg = format!("  Error: Device no longer accessible - {}", e);
                    println!("{}", error_msg);
                    errors.push(error_msg);
                    thread::sleep(Duration::from_millis(200));
                    continue;
                }
                d
            }
            Err(e) => {
                let error_msg = format!("  Error: Failed to get device - {}", e);
                println!("{}", error_msg);
                errors.push(error_msg);
                thread::sleep(Duration::from_millis(200));
                continue;
            }
        };

        // Immediately attempt to create stream - don't query the device further
        // as additional queries might invalidate it or put it in a bad state
        // Attempt to create stream in a scope to ensure proper cleanup on failure
        let result = {
            // Create stream - if this fails, device will be dropped automatically
            create_stream(
                device,
                config.clone(),
                sample_format,
                wake_word_threshold,
                vad_threshold,
                silence_seconds,
                rolling_buffer_duration_seconds,
            )
        };

        match result {
            Ok((stream, channel_rx)) => {
                println!(
                    "  Success! Using {:?} @ {}Hz ({} channel{})",
                    sample_format,
                    config.sample_rate,
                    config.channels,
                    if config.channels == 1 { "" } else { "s" }
                );
                return Ok((stream, channel_rx, config, sample_format));
            }
            Err(e) => {
                let error_msg = format!(
                    "  Error: {:?} @ {}Hz ({} channel{}) - {}",
                    sample_format,
                    config.sample_rate,
                    config.channels,
                    if config.channels == 1 { "" } else { "s" },
                    e
                );
                println!("{}", error_msg);
                errors.push(error_msg);

                // Give the device/host time to recover before trying the next configuration
                // ALSA devices may need time to release resources after a failed attempt
                // The device and any partial streams are automatically dropped when we exit this scope
                thread::sleep(Duration::from_millis(200));
            }
        }
    }

    // All configurations failed
    Err(color_eyre::eyre::eyre!(
        "Failed to create audio stream with any configuration. Attempted {} configurations:\n{}",
        errors.len(),
        errors.join("\n")
    ))
}

fn clean_text_segments(segments: Vec<SpeechSegment>, voice_activation_text: &str) -> String {
    let full_text = {
        let mut text = String::new();
        for segment in segments {
            text.push_str(&segment.text);
        }
        text
    };
    let full_text = full_text.trim().to_lowercase();
    // Only keep alphanumeric characters and spaces; collapse multiple spaces
    let cleaned: String = full_text
        .chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .collect::<String>();
    // Collapse extra spaces
    let cleaned = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");

    // If the voice activation text is found, return the text after the voice activation text
    let voice_activation_text_position = cleaned.find(voice_activation_text);
    if let Some(voice_activation_text_position) = voice_activation_text_position {
        cleaned[(voice_activation_text_position + voice_activation_text.len())..]
            .trim()
            .to_string()
    } else {
        cleaned
    }
}

struct VoiceAssistantConfig {
    pub home_assistant_base_url: Url,
    pub home_assistant_token: String,
    pub input_device_id: String,
    pub silence_seconds: f64,
    pub rolling_buffer_duration_seconds: f64,
}

fn run_voice_assistant(voice_assistant_config: VoiceAssistantConfig) -> Result<()> {
    let mut tts_client = TtsClient::new()?;
    let ctx = WhisperContext::new_with_params(
        "./whisper_model/ggml-tiny.bin",
        WhisperContextParameters::default(),
    )
    .map_err(|e| color_eyre::eyre::eyre!("failed to load model: {}", e))?;
    let sampling_strategy = SamplingStrategy::BeamSearch {
        beam_size: 5,
        patience: -1.0,
    };
    let speech_to_text_client = SpeechToTextClient::new(ctx, sampling_strategy)?;

    // Print device list once
    let device_id = cpal::DeviceId::from_str(&voice_assistant_config.input_device_id)?;
    let host = cpal::default_host();
    let input_devices = host.input_devices()?.collect::<Vec<_>>();
    let input_devices_names = input_devices
        .iter()
        .map(|d| -> Result<(String, String)> {
            Ok((d.id()?.to_string(), d.description()?.to_string()))
        })
        .collect::<Result<Vec<_>>>()?;
    println!("Input devices: {:#?}", input_devices_names);

    // Verify device exists
    let device_name = input_devices
        .iter()
        .find(|d| d.id().ok().map(|id| id == device_id).unwrap_or(false))
        .and_then(|d| d.id().ok().map(|id| id.1))
        .ok_or_eyre("No input device found")?;
    println!("Using input device: {}", device_name);

    // Generate candidate configs (gets device internally and drops it immediately)
    let candidates = generate_candidate_configs(&voice_assistant_config.input_device_id)?;

    // Give the system time to fully release the device after querying configs
    // This is especially important for ALSA devices
    thread::sleep(Duration::from_millis(300));

    // Try each candidate until one works
    let (stream, channel_rx, _config, _sample_format) = try_create_stream(
        &voice_assistant_config.input_device_id,
        candidates,
        0.2,
        0.75,
        voice_assistant_config.silence_seconds,
        voice_assistant_config.rolling_buffer_duration_seconds,
    )?;

    stream.play()?;

    let command_executor_config = command_executor::CommandExecutorConfig::new(
        voice_assistant_config.home_assistant_base_url,
        voice_assistant_config.home_assistant_token,
    );

    println!("Listening for speech... say alexa to start");
    tts_client.generate_audio("Listening for speech...".to_string())?;
    let voice_activation_text = "alexa";
    for event in channel_rx {
        match event {
            SpeechEvent::SpeechDetected(audio) => {
                let segments = speech_to_text_client.process(audio)?;
                let cleaned_text = clean_text_segments(segments, voice_activation_text);
                match command_executor::execute_command(&command_executor_config, &cleaned_text) {
                    Ok(response_text) => {
                        tts_client.generate_audio(response_text)?;
                    }
                    Err(e) => {
                        println!("Error executing command: {}", e);
                        tts_client.generate_audio(
                            "Something went wrong. Please try again.".to_string(),
                        )?;
                    }
                }
            }
        }
    }

    tts_client.wait_until_finished()?;

    Ok(())
}

fn get_input_devices() -> Result<()> {
    let host = cpal::default_host();
    let input_devices = host.input_devices()?.collect::<Vec<_>>();
    for device in input_devices {
        let id = device.id()?;
        println!("Device: {}:{}", id.0, id.1);
    }
    Ok(())
}

fn main() -> Result<()> {
    let args = Cli::parse();
    match args.command {
        Commands::RunVoiceAssistant {
            home_assistant_base_url,
            home_assistant_token,
            input_device_id,
            silence_seconds,
            rolling_buffer_duration_seconds,
        } => run_voice_assistant(VoiceAssistantConfig {
            home_assistant_base_url,
            home_assistant_token,
            input_device_id,
            silence_seconds,
            rolling_buffer_duration_seconds,
        }),
        Commands::GetInputDevices => get_input_devices(),
    }
}
