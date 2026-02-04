use color_eyre::eyre::Result;
use pocket_tts::{ModelState, TTSModel};
use rodio::source::Source;
use std::collections::VecDeque;
use std::io::{Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use tts_processor::{deserialize_command, serialize_response, TtsCommand, TtsResponse};

// A streaming audio source that reads from a shared buffer
struct StreamingAudioSource {
    buffer: Arc<Mutex<VecDeque<f32>>>,
    sample_rate: u32,
    channels: u16,
    finished: Arc<Mutex<bool>>,
}

impl StreamingAudioSource {
    fn new(sample_rate: u32, channels: u16) -> (Self, StreamingAudioHandle) {
        let buffer = Arc::new(Mutex::new(VecDeque::new()));
        let finished = Arc::new(Mutex::new(false));

        let handle = StreamingAudioHandle {
            buffer: buffer.clone(),
            finished: finished.clone(),
        };

        let source = Self {
            buffer,
            sample_rate,
            channels,
            finished,
        };

        (source, handle)
    }
}

// Handle to feed audio chunks into the stream
#[derive(Clone)]
struct StreamingAudioHandle {
    buffer: Arc<Mutex<VecDeque<f32>>>,
    finished: Arc<Mutex<bool>>,
}

impl StreamingAudioHandle {
    fn push_chunk(&self, samples: Vec<f32>) {
        let mut buffer = self.buffer.lock().unwrap();
        buffer.extend(samples);
    }

    fn mark_finished(&self) {
        *self.finished.lock().unwrap() = true;
    }

    fn clear(&self) {
        let mut buffer = self.buffer.lock().unwrap();
        buffer.clear();
        *self.finished.lock().unwrap() = false;
    }
}

impl Source for StreamingAudioSource {
    fn current_span_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> u16 {
        self.channels
    }

    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

impl Iterator for StreamingAudioSource {
    type Item = f32;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let sample = {
                let mut buffer = self.buffer.lock().unwrap();
                buffer.pop_front()
            };

            if let Some(s) = sample {
                return Some(s);
            }

            let finished = *self.finished.lock().unwrap();
            if finished {
                return None;
            }

            std::thread::sleep(Duration::from_micros(100));
        }
    }
}

fn read_length_prefixed_message(stream: &mut UnixStream) -> Result<Vec<u8>> {
    let mut len_bytes = [0u8; 4];
    stream.read_exact(&mut len_bytes)?;
    let len = u32::from_le_bytes(len_bytes) as usize;

    let mut buffer = vec![0u8; len];
    stream.read_exact(&mut buffer)?;
    Ok(buffer)
}

fn write_length_prefixed_message(stream: &mut UnixStream, data: &[u8]) -> Result<()> {
    let len = data.len() as u32;
    stream.write_all(&len.to_le_bytes())?;
    stream.write_all(data)?;
    stream.flush()?;
    Ok(())
}

struct AudioState {
    current_sink: Arc<Mutex<Option<rodio::Sink>>>,
    streaming_handle: Arc<Mutex<Option<StreamingAudioHandle>>>,
}

impl AudioState {
    fn new() -> Self {
        Self {
            current_sink: Arc::new(Mutex::new(None)),
            streaming_handle: Arc::new(Mutex::new(None)),
        }
    }

    fn stop(&self) {
        if let Some(ref handle) = *self.streaming_handle.lock().unwrap() {
            handle.mark_finished();
        }
        *self.streaming_handle.lock().unwrap() = None;

        if let Some(sink) = self.current_sink.lock().unwrap().take() {
            sink.stop();
        }
    }

    fn is_finished(&self) -> bool {
        let streaming_done = {
            let handle_guard = self.streaming_handle.lock().unwrap();
            if let Some(ref handle) = *handle_guard {
                *handle.finished.lock().unwrap()
            } else {
                true
            }
        };

        let sink_empty = self
            .current_sink
            .lock()
            .unwrap()
            .as_ref()
            .map(|sink| sink.empty())
            .unwrap_or(true);

        streaming_done && sink_empty
    }
}

fn handle_connection(
    mut stream: UnixStream,
    model: &TTSModel,
    voice_state: &ModelState,
    mixer: &rodio::mixer::Mixer,
    audio_state: &AudioState,
) -> Result<()> {
    loop {
        let cmd_bytes = match read_length_prefixed_message(&mut stream) {
            Ok(bytes) => bytes,
            Err(e) => {
                eprintln!("Error reading command: {}", e);
                break;
            }
        };

        let cmd = match deserialize_command(&cmd_bytes) {
            Ok(cmd) => cmd,
            Err(e) => {
                eprintln!("Error deserializing command: {}", e);
                let resp = serialize_response(&TtsResponse::Error(format!(
                    "Failed to deserialize command: {}",
                    e
                )))?;
                write_length_prefixed_message(&mut stream, &resp)?;
                continue;
            }
        };

        match cmd {
            TtsCommand::GenerateAudio(text) => {
                // Stop any current playback
                audio_state.stop();

                // Send started response
                let resp = serialize_response(&TtsResponse::Started)?;
                write_length_prefixed_message(&mut stream, &resp)?;

                // Create streaming source
                let (source, handle) = StreamingAudioSource::new(model.sample_rate as u32, 1);
                *audio_state.streaming_handle.lock().unwrap() = Some(handle.clone());

                // Start playing the stream
                let sink = rodio::Sink::connect_new(mixer);
                sink.append(source);
                *audio_state.current_sink.lock().unwrap() = Some(sink);

                // Generate and stream audio chunks
                for chunk in model.generate_stream(&text, voice_state) {
                    let audio_chunk = chunk
                        .map_err(|e| color_eyre::eyre::eyre!("Failed to get audio chunk: {}", e))?;

                    let audio_chunk_2d = audio_chunk
                        .squeeze(0)
                        .map_err(|e| color_eyre::eyre::eyre!("Failed to squeeze tensor: {}", e))?;

                    let audio_data_2d = audio_chunk_2d.to_vec2::<f32>().map_err(|e| {
                        color_eyre::eyre::eyre!("Failed to convert tensor to vec: {}", e)
                    })?;

                    let first_channel = audio_data_2d.into_iter().next();
                    if let Some(channel) = first_channel {
                        if let Some(ref handle) = *audio_state.streaming_handle.lock().unwrap() {
                            handle.push_chunk(channel);
                        }
                    }
                }

                // Mark streaming as finished
                if let Some(ref handle) = *audio_state.streaming_handle.lock().unwrap() {
                    handle.mark_finished();
                }

                // Wait until playback is finished
                while !audio_state.is_finished() {
                    thread::sleep(Duration::from_millis(10));
                }

                // Clean up
                *audio_state.streaming_handle.lock().unwrap() = None;

                // Send finished response
                let resp = serialize_response(&TtsResponse::Finished)?;
                write_length_prefixed_message(&mut stream, &resp)?;
            }
            TtsCommand::Stop => {
                audio_state.stop();
                let resp = serialize_response(&TtsResponse::Stopped)?;
                write_length_prefixed_message(&mut stream, &resp)?;
            }
            TtsCommand::WaitUntilFinished => {
                // Wait until finished
                while !audio_state.is_finished() {
                    thread::sleep(Duration::from_millis(10));
                }
                let resp = serialize_response(&TtsResponse::Finished)?;
                write_length_prefixed_message(&mut stream, &resp)?;
            }
        }
    }

    Ok(())
}

fn main() -> Result<()> {
    color_eyre::install()?;

    // Get socket path from environment variable
    let socket_path = std::env::var("TTS_SOCKET_PATH")
        .map(PathBuf::from)
        .map_err(|_| color_eyre::eyre::eyre!("TTS_SOCKET_PATH environment variable not set"))?;

    // Load TTS model
    let cwd = std::env::current_dir()?;
    let model_path = cwd.join("model/tts_b6369a24.safetensors");
    let model_path_str = model_path.to_str().ok_or(color_eyre::eyre::eyre!(
        "Failed to convert model path to string"
    ))?;
    println!("Model path: {}", model_path_str);

    let model = TTSModel::load(model_path_str)
        .map_err(|e| color_eyre::eyre::eyre!("Failed to load model: {}", e))?;

    let voice_path = cwd.join("model/p303_023.wav");
    let voice_path_str = voice_path.to_str().ok_or(color_eyre::eyre::eyre!(
        "Failed to convert voice path to string"
    ))?;
    println!("Voice path: {}", voice_path_str);

    let voice_state = model
        .get_voice_state(voice_path_str)
        .map_err(|e| color_eyre::eyre::eyre!("Failed to get voice state: {}", e))?;

    // Create Unix socket listener
    if socket_path.exists() {
        std::fs::remove_file(&socket_path)?;
    }
    let listener = UnixListener::bind(&socket_path)?;
    println!("Listening on socket: {:?}", socket_path);

    // Initialize audio output
    let stream_handle = rodio::OutputStreamBuilder::open_default_stream()
        .map_err(|e| color_eyre::eyre::eyre!("Failed to open audio stream: {}", e))?;
    let mixer = stream_handle.mixer();

    // Create shared audio state
    let audio_state = AudioState::new();

    // Accept connections
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                println!("New connection accepted");
                // Handle connection in current thread
                if let Err(e) =
                    handle_connection(stream, &model, &voice_state, &mixer, &audio_state)
                {
                    eprintln!("Error handling connection: {}", e);
                }
            }
            Err(e) => {
                eprintln!("Error accepting connection: {}", e);
            }
        }
    }

    Ok(())
}
