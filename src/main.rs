use std::str::FromStr;

use color_eyre::eyre::{OptionExt, Result};
use cpal::traits::StreamTrait;
use cpal::{
    Device, StreamConfig,
    traits::{DeviceTrait, HostTrait},
};
use whisper_rs::{SamplingStrategy, WhisperContext, WhisperContextParameters};

use crate::speech::{SpeechSegment, SpeechToTextClient};
use crate::speech_listener::SpeechEvent;
use crate::{speech_listener::create_stream, tts_client::TtsClient};

mod audio_resampler;
mod command_executor;
mod speech;
mod speech_listener;
mod tts_client;
use clap::Parser;
use url::Url;

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    #[arg(short, long, env = "HOME_ASSISTANT_BASE_URL")]
    home_assistant_base_url: Url,

    #[arg(short, long, env = "HOME_ASSISTANT_TOKEN")]
    home_assistant_token: String,
}

fn get_device_and_config() -> Result<(Device, StreamConfig)> {
    let device_id = cpal::DeviceId::from_str("coreaudio:BuiltInMicrophoneDevice")?;

    let host = cpal::default_host();
    let device = {
        let input_devices = host.input_devices()?.collect::<Vec<_>>();
        let input_devices_names = input_devices
            .iter()
            .map(|d| -> Result<(String, String)> {
                Ok((d.id()?.to_string(), d.description()?.to_string()))
            })
            .collect::<Result<Vec<_>>>()?;

        println!("Input devices: {:#?}", input_devices_names);
        input_devices
            .into_iter()
            .find(|d| d.id().ok().map(|id| id == device_id).unwrap_or(false))
            .ok_or_eyre("No input device found")?
    };

    println!("Using input device: {}", device.id()?.1);

    let supported_configs = device.supported_input_configs()?;
    for config in supported_configs {
        println!(
            "config channels: {}, min sample rate: {}, max sample rate: {}, buffer size: {:?}",
            config.channels(),
            config.min_sample_rate(),
            config.max_sample_rate(),
            config.buffer_size(),
        );
    }

    // Configure for 16kHz mono f32
    let config = device.default_input_config()?;

    println!("Device config:");
    println!("  Sample rate: {}", config.sample_rate());
    println!("  Channels: {}", config.channels());
    println!("  Sample format: {:?}", config.sample_format());

    Ok((device, config.config()))
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

fn main() -> Result<()> {
    let args = Cli::parse();

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

    let (device, config) = get_device_and_config()?;
    let (stream, channel_rx) = create_stream(device, config, 0.2, 0.75, 1.0, 2.0)?;
    stream.play()?;

    let command_executor_config = command_executor::CommandExecutorConfig::new(
        args.home_assistant_base_url,
        args.home_assistant_token,
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
