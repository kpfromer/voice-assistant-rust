use color_eyre::eyre::Result;
use rodio::Sink;
use rodio::source::Source;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::Duration;

// A streaming audio source that reads from a shared buffer
pub struct StreamingAudioSource {
    buffer: Arc<Mutex<VecDeque<f32>>>,
    sample_rate: u32,
    channels: u16,
    finished: Arc<Mutex<bool>>,
}

impl StreamingAudioSource {
    pub fn new(sample_rate: u32, channels: u16) -> (Self, StreamingAudioHandle) {
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
pub struct StreamingAudioHandle {
    buffer: Arc<Mutex<VecDeque<f32>>>,
    finished: Arc<Mutex<bool>>,
}

impl StreamingAudioHandle {
    pub fn push_chunk(&self, samples: Vec<f32>) {
        let mut buffer = self.buffer.lock().unwrap();
        buffer.extend(samples);
    }

    pub fn mark_finished(&self) {
        *self.finished.lock().unwrap() = true;
    }

    pub fn clear(&self) {
        let mut buffer = self.buffer.lock().unwrap();
        buffer.clear();
        *self.finished.lock().unwrap() = false;
    }
}

impl Source for StreamingAudioSource {
    fn current_span_len(&self) -> Option<usize> {
        // Unknown length since we're streaming
        None
    }

    fn channels(&self) -> u16 {
        self.channels
    }

    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    fn total_duration(&self) -> Option<Duration> {
        // Unknown duration since we're streaming
        None
    }
}

impl Iterator for StreamingAudioSource {
    type Item = f32;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            // Try to get a sample from the buffer
            let sample = {
                let mut buffer = self.buffer.lock().unwrap();
                buffer.pop_front()
            };

            if let Some(s) = sample {
                return Some(s);
            }

            // Buffer is empty, check if we're finished
            let finished = *self.finished.lock().unwrap();
            if finished {
                return None; // End of stream
            }

            // Not finished but no data yet - yield silence or wait
            // For low latency, we could yield silence, but that might cause clicks
            // Better to wait a tiny bit and retry
            std::thread::sleep(Duration::from_micros(100));
        }
    }
}

// Updated audio controller with streaming support
#[derive(Debug, Clone)]
enum AudioCommand {
    /// Play a regular audio file
    Play(Vec<f32>, u32),
    /// Stop the current audio playback
    Stop,
    /// Stop the current audio playback and play a new audio file
    StopAndPlay(Vec<f32>, u32),
    /// Start streaming audio
    StartStreaming(u32), // sample_rate
    /// Push a chunk of audio to the current stream
    StreamChunk(Vec<f32>),
    /// Mark the stream as finished
    FinishStreaming,
    /// Wait until the current audio playback (streaming or regular) is finished.
    WaitUntilFinished(mpsc::Sender<()>), // Signal when done
}

pub struct AudioController {
    command_tx: mpsc::Sender<AudioCommand>,
    streaming_handle: Arc<Mutex<Option<StreamingAudioHandle>>>,
}

impl AudioController {
    pub fn play(&self, audio: Vec<f32>, sample_rate: u32) -> Result<()> {
        self.command_tx
            .send(AudioCommand::Play(audio, sample_rate))
            .map_err(|e| color_eyre::eyre::eyre!("Failed to send play command: {}", e))?;
        Ok(())
    }

    pub fn stop(&self) -> Result<()> {
        self.command_tx
            .send(AudioCommand::Stop)
            .map_err(|e| color_eyre::eyre::eyre!("Failed to send stop command: {}", e))?;
        Ok(())
    }

    pub fn stop_and_play(&self, audio: Vec<f32>, sample_rate: u32) -> Result<()> {
        self.command_tx
            .send(AudioCommand::StopAndPlay(audio, sample_rate))
            .map_err(|e| color_eyre::eyre::eyre!("Failed to send stop_and_play command: {}", e))?;
        Ok(())
    }

    // Start a streaming session
    pub fn start_streaming(&self, sample_rate: u32) -> Result<()> {
        self.command_tx
            .send(AudioCommand::StartStreaming(sample_rate))
            .map_err(|e| color_eyre::eyre::eyre!("Failed to start streaming: {}", e))?;
        Ok(())
    }

    pub fn stream_chunk(&self, samples: Vec<f32>) -> Result<()> {
        if let Some(ref handle) = *self.streaming_handle.lock().unwrap() {
            handle.push_chunk(samples);
        }
        Ok(())
    }

    pub fn finish_streaming(&self) -> Result<()> {
        if let Some(ref handle) = *self.streaming_handle.lock().unwrap() {
            handle.mark_finished();
        }
        self.command_tx
            .send(AudioCommand::FinishStreaming)
            .map_err(|e| color_eyre::eyre::eyre!("Failed to finish streaming: {}", e))?;
        Ok(())
    }

    /// Wait until the current audio playback (streaming or regular) is finished.
    /// This will block until the sink is empty and any active stream is finished.
    pub fn wait_until_finished(&self) -> Result<()> {
        let (tx, rx) = mpsc::channel();
        self.command_tx
            .send(AudioCommand::WaitUntilFinished(tx))
            .map_err(|e| color_eyre::eyre::eyre!("Failed to send wait command: {}", e))?;

        // Block until we receive the signal
        rx.recv()
            .map_err(|e| color_eyre::eyre::eyre!("Failed to receive completion signal: {}", e))?;
        Ok(())
    }
}

pub fn start_audio_thread() -> Result<AudioController> {
    let (command_tx, command_rx) = mpsc::channel::<AudioCommand>();
    let streaming_handle = Arc::new(Mutex::new(None::<StreamingAudioHandle>));
    let streaming_handle_clone = streaming_handle.clone();

    thread::spawn(move || {
        let stream_handle =
            rodio::OutputStreamBuilder::open_default_stream().expect("Failed to open audio stream");
        let mixer = stream_handle.mixer();
        // Mutex to protect the current sink
        // We have two threads using the current sink:
        // - The main thread (this thread)
        // - The wait_until_finished thread
        let current_sink_arc: Arc<Mutex<Option<rodio::Sink>>> = Arc::new(Mutex::new(None));
        let current_sink = current_sink_arc.clone();

        loop {
            match command_rx.recv() {
                Ok(cmd) => {
                    match cmd {
                        AudioCommand::StartStreaming(sample_rate) => {
                            let mut current_sink_guard = current_sink.lock().unwrap();
                            // Stop any current playback
                            if let Some(sink) = current_sink_guard.take() {
                                sink.stop();
                            }

                            // Create streaming source
                            let (source, handle) = StreamingAudioSource::new(sample_rate, 1);

                            // Store handle in shared Arc
                            *streaming_handle_clone.lock().unwrap() = Some(handle.clone());

                            // Start playing the stream
                            let sink = rodio::Sink::connect_new(mixer);
                            sink.append(source);
                            *current_sink_guard = Some(sink);
                        }
                        AudioCommand::StreamChunk(samples) => {
                            if let Some(ref handle) = *streaming_handle_clone.lock().unwrap() {
                                handle.push_chunk(samples);
                            }
                        }
                        AudioCommand::FinishStreaming => {
                            if let Some(ref handle) = *streaming_handle_clone.lock().unwrap() {
                                handle.mark_finished();
                            }
                            *streaming_handle_clone.lock().unwrap() = None;
                        }
                        AudioCommand::Play(audio, sample_rate) => {
                            // Stop any active streaming
                            if let Some(ref handle) = *streaming_handle_clone.lock().unwrap() {
                                handle.mark_finished();
                            }
                            *streaming_handle_clone.lock().unwrap() = None;

                            let mut current_sink_guard = current_sink.lock().unwrap();
                            // Stop current sink
                            if let Some(sink) = current_sink_guard.take() {
                                sink.stop();
                            }

                            // Create new sink and play
                            let sink = Sink::connect_new(mixer);
                            let source = rodio::buffer::SamplesBuffer::new(1, sample_rate, audio);
                            sink.append(source);
                            *current_sink_guard = Some(sink);
                        }
                        AudioCommand::Stop => {
                            // Stop any active streaming
                            if let Some(ref handle) = *streaming_handle_clone.lock().unwrap() {
                                handle.mark_finished();
                            }
                            *streaming_handle_clone.lock().unwrap() = None;

                            let mut current_sink_guard = current_sink.lock().unwrap();
                            // Stop and clear current sink
                            if let Some(sink) = current_sink_guard.take() {
                                sink.stop();
                            }
                        }
                        AudioCommand::StopAndPlay(audio, sample_rate) => {
                            // Stop any active streaming
                            if let Some(ref handle) = *streaming_handle_clone.lock().unwrap() {
                                handle.mark_finished();
                            }
                            *streaming_handle_clone.lock().unwrap() = None;

                            let mut current_sink_guard = current_sink.lock().unwrap();
                            // Stop current audio immediately
                            if let Some(sink) = current_sink_guard.take() {
                                sink.stop();
                            }

                            // Create new sink and play immediately
                            let sink = Sink::connect_new(mixer);
                            let source = rodio::buffer::SamplesBuffer::new(1, sample_rate, audio);
                            sink.append(source);
                            *current_sink_guard = Some(sink);
                        }
                        AudioCommand::WaitUntilFinished(tx) => {
                            // Check if we're done
                            let is_done = {
                                // Check if streaming is finished
                                let streaming_done = {
                                    let handle_guard = streaming_handle_clone.lock().unwrap();
                                    if let Some(ref handle) = *handle_guard {
                                        // If handle exists, check if it's marked finished
                                        *handle.finished.lock().unwrap()
                                    } else {
                                        // No streaming active
                                        true
                                    }
                                };

                                // Check if sink is empty
                                let sink_empty = current_sink
                                    .lock()
                                    .unwrap()
                                    .as_ref()
                                    .map(|sink| sink.empty())
                                    .unwrap_or(true);

                                streaming_done && sink_empty
                            };

                            if is_done {
                                // Already done, signal immediately
                                let _ = tx.send(());
                            } else {
                                // Not done yet, spawn a thread to poll and signal when done
                                let tx_clone = tx.clone();
                                let streaming_handle_clone_2 = streaming_handle_clone.clone();
                                let current_sink_clone = current_sink.clone();

                                thread::spawn(move || {
                                    loop {
                                        // Check if streaming is finished
                                        let streaming_done = {
                                            let handle_guard =
                                                streaming_handle_clone_2.lock().unwrap();
                                            if let Some(ref handle) = *handle_guard {
                                                *handle.finished.lock().unwrap()
                                            } else {
                                                true
                                            }
                                        };

                                        // Check if sink is empty
                                        let sink_empty = current_sink_clone
                                            .lock()
                                            .unwrap()
                                            .as_ref()
                                            .map(|sink| sink.empty())
                                            .unwrap_or(true);

                                        if streaming_done && sink_empty {
                                            let _ = tx_clone.send(());
                                            break;
                                        }

                                        // Poll every 10ms
                                        thread::sleep(Duration::from_millis(10));
                                    }
                                });
                            }
                        }
                    }
                }
                Err(_) => break,
            }
        }
    });

    Ok(AudioController {
        command_tx,
        streaming_handle,
    })
}
