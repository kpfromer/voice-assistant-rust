use std::sync::mpsc;
use std::thread;

use color_eyre::eyre::Result;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

use crate::speech;
use crate::speech_listener::SpeechEvent;

pub fn speech_to_text(channel_rx: mpsc::Receiver<SpeechEvent>) -> thread::JoinHandle<Result<()>> {
    let speech_detector_thread = thread::spawn(move || -> Result<()> {
        let ctx = WhisperContext::new_with_params(
            "./whisper_model/ggml-tiny.bin",
            WhisperContextParameters::default(),
        )
        .map_err(|e| color_eyre::eyre::eyre!("failed to load model: {}", e))?;

        // // create a params object
        let params = FullParams::new(SamplingStrategy::BeamSearch {
            beam_size: 5,
            patience: -1.0,
        });
        let mut speech_detector = speech::SpeechDetector::new(ctx, params);

        loop {
            while let Ok(audio_data) = channel_rx.recv() {
                match audio_data {
                    SpeechEvent::SpeechDetected(audio_data) => {
                        speech_detector.add_audio_data(audio_data);
                        match speech_detector.process() {
                            Ok(segments) => {
                                let mut all_text = String::new();
                                for segment in segments {
                                    println!("segment: {}", segment.text);
                                    all_text.push_str(&segment.text);
                                }
                                let cleaned_text =
                                    all_text
                                        .trim()
                                        .to_lowercase()
                                        .chars()
                                        .filter_map(|c| {
                                            if c.is_alphanumeric() { Some(c) } else { None }
                                        })
                                        .collect::<String>();
                                if cleaned_text == "stop" {
                                    println!("stopping...");
                                }
                            }
                            Err(e) => {
                                eprintln!("error processing speech: {}", e);
                            }
                        }
                        speech_detector.clear_audio_data();
                    }
                }
            }
        }
    });
    speech_detector_thread
}
