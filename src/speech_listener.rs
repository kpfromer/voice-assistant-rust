use std::{
    collections::VecDeque,
    sync::{Arc, Mutex, mpsc},
    time::Duration,
};

use color_eyre::eyre::Result;
use cpal::traits::DeviceTrait;
use cpal::{Device, Stream, StreamConfig};
use oww_rs::{
    mic::{process_audio::resample_into_chunks, resampler::make_resampler},
    oww::{OWW_MODEL_CHUNK_SIZE, OwwModel},
};
use voice_activity_detector::VoiceActivityDetector;

use crate::audio_resampler::AudioResampler;

const SAMPLE_RATE: u32 = 16000;
const CHUNK_SIZE: usize = 512;

/// Rolling buffer that stores audio chunks with a configurable maximum duration.
/// When a wake word is detected, this buffer can be drained to include preceding audio
/// that occurred before the wake word detection (which has inherent latency).
struct RollingBuffer {
    chunks: VecDeque<Vec<f32>>,
    max_chunks: usize,
}

impl RollingBuffer {
    /// Create a new rolling buffer that will store up to `duration_seconds` of audio.
    /// `chunk_duration` is the duration of each chunk (e.g., 512 samples at 16kHz = 0.032s).
    fn new(duration_seconds: f64, chunk_duration: Duration) -> Self {
        let max_chunks = (duration_seconds / chunk_duration.as_secs_f64()).ceil() as usize;
        Self {
            chunks: VecDeque::with_capacity(max_chunks),
            max_chunks,
        }
    }

    /// Push a chunk into the buffer, evicting the oldest chunk if at capacity.
    fn push(&mut self, chunk: Vec<f32>) {
        if self.chunks.len() >= self.max_chunks {
            self.chunks.pop_front();
        }
        self.chunks.push_back(chunk);
    }

    /// Drain all chunks from the buffer and flatten them into a single Vec<f32>.
    fn drain_flat(&mut self) -> Vec<f32> {
        let mut result = Vec::new();
        for chunk in self.chunks.drain(..) {
            result.extend(chunk);
        }
        result
    }
}

#[derive(Debug)]
struct InProgressSpeechState {
    /// This is the duration of the speech so far
    speech_duration: Duration,
    /// This is the audio data that has been collected so far
    audio_data: Vec<f32>,
    /// This is a rolling window of past has been speech detections
    /// True means non-speech, false means speech
    /// This is used to ensure we wait for N seconds of no speech before declaring speech end
    past_has_been_speech: VecDeque<bool>,
}

#[derive(Debug)]
enum SpeechListenerState {
    WaitingForWakeWord,
    ListeningForEndOfSpeech(InProgressSpeechState),
}

impl Default for SpeechListenerState {
    fn default() -> Self {
        Self::WaitingForWakeWord
    }
}

struct WakeWordDetector {
    buffer: Arc<Mutex<Vec<f32>>>,
    channels: usize,
    audio_resampler: oww_rs::mic::resampler::Resamplers,
    model: OwwModel,
}

impl WakeWordDetector {
    /// Create a new wake word detector.
    /// `threshold` is the detection threshold passed to OwwModel (typically 0.3).
    /// `channels` is the number of audio channels.
    /// `input_rate` is the input sample rate.
    fn new(threshold: f32, channels: usize, input_rate: u32) -> Result<Self> {
        let audio_resampler =
            make_resampler(input_rate as _, OWW_MODEL_CHUNK_SIZE as _, channels as _)
                .map_err(|e| color_eyre::eyre::eyre!("failed to create OWW resampler: {}", e))?;

        let model = OwwModel::new(
            oww_rs::config::SpeechUnlockType::OpenWakeWordAlexa,
            threshold,
        )
        .map_err(|e| color_eyre::eyre::eyre!("failed to load OWW model: {}", e))?;

        Ok(Self {
            buffer: Arc::new(Mutex::new(Vec::new())),
            channels,
            audio_resampler,
            model,
        })
    }

    /// Detect wake word in raw audio data.
    /// Returns true if wake word is detected.
    fn detect(&mut self, data: &[f32]) -> bool {
        let chunks = resample_into_chunks(
            data,
            &self.buffer.clone(),
            self.channels as _,
            &mut self.audio_resampler,
        );
        for chunk in chunks {
            let d = self.model.detection(chunk.data_f32.first().clone());
            if d.detected {
                return true;
            }
        }
        false
    }
}

enum EndOfSpeechResult {
    StillListening(InProgressSpeechState),
    SpeechEnded {
        audio_data: Vec<f32>,
        duration: Duration,
    },
}

struct EndOfSpeechDetector {
    vad: VoiceActivityDetector,
    probability_threshold: f32,
    chunk_duration: Duration,
    /// Number of consecutive non-speech chunks needed to declare end of speech
    silence_chunks_needed: usize,
}

impl EndOfSpeechDetector {
    /// Create a new end-of-speech detector.
    /// `sample_rate` is the sample rate of the audio chunks (should be 16kHz).
    /// `chunk_size` is the size of each chunk (should be 512).
    /// `probability_threshold` is the VAD threshold (typically 0.75).
    /// `silence_seconds` is how many seconds of consecutive silence triggers end-of-speech.
    fn new(
        sample_rate: u32,
        chunk_size: usize,
        probability_threshold: f32,
        silence_seconds: f64,
    ) -> Result<Self> {
        let vad = VoiceActivityDetector::builder()
            .sample_rate(sample_rate)
            .chunk_size(chunk_size)
            .build()?;

        let chunk_duration = Duration::from_secs_f64(chunk_size as f64 / sample_rate as f64);
        let silence_chunks_needed =
            (silence_seconds / chunk_duration.as_secs_f64()).ceil() as usize;

        Ok(Self {
            vad,
            probability_threshold,
            chunk_duration,
            silence_chunks_needed,
        })
    }

    /// Process chunks and determine if speech has ended.
    /// `chunks` are pre-resampled 16kHz chunks from the pipeline.
    /// Returns `StillListening` if speech continues, or `SpeechEnded` once the configured
    /// silence duration of consecutive non-speech is detected.
    fn process_chunks(
        &mut self,
        chunks: Vec<Vec<f32>>,
        mut speech: InProgressSpeechState,
    ) -> EndOfSpeechResult {
        for chunk in chunks {
            let probability = self.vad.predict(chunk.clone());

            if probability > self.probability_threshold {
                // Speech detected
                if speech.past_has_been_speech.len() >= self.silence_chunks_needed {
                    speech.past_has_been_speech.pop_front();
                }
                speech.past_has_been_speech.push_back(false); // false = speech
                speech.audio_data.extend(chunk);
                speech.speech_duration += self.chunk_duration;
            } else {
                // Non-speech detected
                if speech.past_has_been_speech.len() >= self.silence_chunks_needed {
                    // Check if all recent chunks were non-speech
                    let all_non_speech = speech.past_has_been_speech.iter().all(|&b| b);
                    if all_non_speech {
                        // Configured silence duration reached - speech has ended
                        return EndOfSpeechResult::SpeechEnded {
                            audio_data: speech.audio_data,
                            duration: speech.speech_duration,
                        };
                    } else {
                        // Some speech in the window, continue listening
                        speech.past_has_been_speech.pop_front();
                        speech.past_has_been_speech.push_back(true); // true = non-speech
                        speech.audio_data.extend(chunk);
                        speech.speech_duration += self.chunk_duration;
                    }
                } else {
                    // Not enough chunks yet, add to front and continue
                    speech.past_has_been_speech.push_front(true); // true = non-speech
                    speech.audio_data.extend(chunk);
                    speech.speech_duration += self.chunk_duration;
                }
            }
        }

        EndOfSpeechResult::StillListening(speech)
    }
}

pub enum SpeechEvent {
    /// Speech detected, send the audio data, this needs to be f32 bit, 16KHz, mono
    SpeechDetected(Vec<f32>),
}

struct SpeechPipeline {
    state: SpeechListenerState,
    audio_resampler: AudioResampler,
    wake_word_detector: WakeWordDetector,
    end_of_speech_detector: EndOfSpeechDetector,
    rolling_buffer: RollingBuffer,
    chunk_duration: Duration,
}

impl SpeechPipeline {
    fn new(
        config: &StreamConfig,
        wake_word_threshold: f32,
        vad_threshold: f32,
        silence_seconds: f64,
        rolling_buffer_duration_seconds: f64,
    ) -> Result<Self> {
        let input_rate = config.sample_rate;
        let channels = config.channels;

        let audio_resampler = AudioResampler::new(input_rate, SAMPLE_RATE, channels, CHUNK_SIZE);

        let wake_word_detector =
            WakeWordDetector::new(wake_word_threshold, channels as usize, input_rate)?;

        let end_of_speech_detector =
            EndOfSpeechDetector::new(SAMPLE_RATE, CHUNK_SIZE, vad_threshold, silence_seconds)?;

        let chunk_duration = Duration::from_secs_f64(CHUNK_SIZE as f64 / SAMPLE_RATE as f64);
        let rolling_buffer = RollingBuffer::new(rolling_buffer_duration_seconds, chunk_duration);

        Ok(Self {
            state: SpeechListenerState::WaitingForWakeWord,
            audio_resampler,
            wake_word_detector,
            end_of_speech_detector,
            rolling_buffer,
            chunk_duration,
        })
    }

    /// Process raw audio data and return a SpeechEvent if speech has been detected and completed.
    fn process(&mut self, raw_data: &[f32]) -> Option<SpeechEvent> {
        // Always resample to 16kHz chunks
        let chunks = self.audio_resampler.resample(raw_data);

        // Use mem::take to avoid borrow checker issues
        let state = std::mem::take(&mut self.state);

        match state {
            SpeechListenerState::WaitingForWakeWord => {
                // Store chunks in rolling buffer
                for chunk in &chunks {
                    self.rolling_buffer.push(chunk.clone());
                }

                // Check for wake word on raw data
                if self.wake_word_detector.detect(raw_data) {
                    println!("Wake word detected!");

                    // Drain the rolling buffer to get preceding audio
                    let preceding_audio = self.rolling_buffer.drain_flat();

                    // Transition to listening for end of speech
                    self.state =
                        SpeechListenerState::ListeningForEndOfSpeech(InProgressSpeechState {
                            speech_duration: Duration::from_secs(0),
                            audio_data: preceding_audio,
                            past_has_been_speech: VecDeque::new(),
                        });
                } else {
                    self.state = SpeechListenerState::WaitingForWakeWord;
                }
                None
            }
            SpeechListenerState::ListeningForEndOfSpeech(in_progress_speech_state) => {
                // Keep the wake word detector's internal state current by feeding it audio,
                // even though we don't care about the detection result right now.
                // Without this, the model's internal activation from the previous wake word
                // detection remains frozen and immediately re-triggers when we return to
                // WaitingForWakeWord.
                let _ = self.wake_word_detector.detect(raw_data);

                // Process chunks through end-of-speech detector
                match self
                    .end_of_speech_detector
                    .process_chunks(chunks, in_progress_speech_state)
                {
                    EndOfSpeechResult::StillListening(updated_state) => {
                        self.state = SpeechListenerState::ListeningForEndOfSpeech(updated_state);
                        None
                    }
                    EndOfSpeechResult::SpeechEnded {
                        audio_data,
                        duration,
                    } => {
                        println!("Speech detected for {:.2} seconds", duration.as_secs_f64());
                        self.state = SpeechListenerState::WaitingForWakeWord;
                        Some(SpeechEvent::SpeechDetected(audio_data))
                    }
                }
            }
        }
    }
}

pub fn create_stream(
    device: Device,
    config: StreamConfig,
    wake_word_threshold: f32,
    vad_threshold: f32,
    silence_seconds: f64,
    rolling_buffer_duration_seconds: f64,
) -> Result<(Stream, mpsc::Receiver<SpeechEvent>)> {
    let pipeline = Arc::new(Mutex::new(SpeechPipeline::new(
        &config,
        wake_word_threshold,
        vad_threshold,
        silence_seconds,
        rolling_buffer_duration_seconds,
    )?));

    // Channel to send audio data assumes f32 bit, 16KHz, mono
    let (channel_tx, channel_rx) = mpsc::channel::<SpeechEvent>();

    let pipeline_clone = pipeline.clone();
    let stream = device.build_input_stream(
        &config,
        move |data: &[f32], _: &cpal::InputCallbackInfo| {
            let mut pipeline_guard = match pipeline_clone.lock() {
                Ok(guard) => guard,
                Err(e) => {
                    eprintln!("Failed to acquire pipeline lock: {e}");
                    return;
                }
            };

            if let Some(event) = pipeline_guard.process(data) {
                let _ = channel_tx.send(event);
            }
        },
        |err| eprintln!("Stream error: {err}"),
        None,
    )?;

    Ok((stream, channel_rx))
}
