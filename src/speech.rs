use std::{sync::mpsc, thread};

use color_eyre::eyre::{Context, Result};
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperState};

pub struct SpeechSegment {
    pub start_timestamp: i64,
    pub end_timestamp: i64,
    pub text: String,
}

struct SpeechDetector {
    whisper: WhisperState,
    whisper_params: FullParams<'static, 'static>,
    audio_data: Vec<f32>,
}

impl SpeechDetector {
    pub fn new(ctx: WhisperContext, sampling_strategy: SamplingStrategy) -> Result<Self> {
        let params = FullParams::new(sampling_strategy);
        Ok(Self {
            whisper: ctx.create_state()?,
            whisper_params: params,
            audio_data: Vec::new(),
        })
    }

    pub fn add_audio_data(&mut self, audio_data: Vec<f32>) {
        self.audio_data.extend(audio_data);
    }

    pub fn clear_audio_data(&mut self) {
        self.audio_data.clear();
    }

    pub fn process(&mut self) -> Result<Vec<SpeechSegment>> {
        if self.audio_data.is_empty() {
            return Ok(Vec::new());
        }

        self.whisper
            .full(self.whisper_params.clone(), &self.audio_data[..])
            .wrap_err("failed to run model")?;

        let mut segments = Vec::new();
        for segment in self.whisper.as_iter() {
            segments.push(SpeechSegment {
                start_timestamp: segment.start_timestamp(),
                end_timestamp: segment.end_timestamp(),
                text: segment.to_string(),
            });
        }

        Ok(segments)
    }
}

pub enum TextToSpeechEvent {
    ConvertSpeechToText {
        audio_data: Vec<f32>,
        response_tx: oneshot::Sender<Vec<SpeechSegment>>,
    },
    Stop,
}

pub struct SpeechToTextClient {
    channel_tx: mpsc::Sender<TextToSpeechEvent>,
    thread_handle: thread::JoinHandle<Result<()>>,
}

impl SpeechToTextClient {
    pub fn new(ctx: WhisperContext, sampling_strategy: SamplingStrategy) -> Result<Self> {
        let mut speech_detector = SpeechDetector::new(ctx, sampling_strategy)?;
        let (channel_tx, channel_rx) = mpsc::channel();
        let thread_handle = thread::spawn(move || {
            loop {
                while let Ok(event) = channel_rx.recv() {
                    match event {
                        TextToSpeechEvent::ConvertSpeechToText {
                            audio_data,
                            response_tx,
                        } => {
                            speech_detector.add_audio_data(audio_data);
                            let segments = speech_detector.process()?;
                            response_tx.send(segments).unwrap();
                            speech_detector.clear_audio_data();
                        }
                        TextToSpeechEvent::Stop => {
                            break;
                        }
                    }
                }
            }
        });
        Ok(Self {
            channel_tx,
            thread_handle,
        })
    }

    pub fn process(&self, audio_data: Vec<f32>) -> Result<Vec<SpeechSegment>> {
        let (response_tx, response_rx) = oneshot::channel();
        self.channel_tx
            .send(TextToSpeechEvent::ConvertSpeechToText {
                audio_data,
                response_tx,
            });
        response_rx
            .recv()
            .wrap_err("Failed to receive response from speech detector")
    }
}
