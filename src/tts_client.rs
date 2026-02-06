use color_eyre::eyre::Result;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::Duration;
use tts_processor::{TtsCommand, TtsResponse, deserialize_response, serialize_command};

pub struct TtsClient {
    stream: UnixStream,
    _process: Child,
    socket_path: PathBuf,
}

impl TtsClient {
    /// Create a new TTS client, spawning the TTS processor process
    pub fn new() -> Result<Self> {
        // Generate unique socket path
        let socket_path =
            std::env::temp_dir().join(format!("voice-assistant-tts-{}.sock", std::process::id()));

        // Try to find the built binary
        let cwd = std::env::current_dir()?;
        let release_bin = cwd.join("tts-processor/target/release/tts-processor");
        let debug_bin = cwd.join("tts-processor/target/debug/tts-processor");
        let binary_path = if release_bin.exists() {
            Some(release_bin)
        } else if debug_bin.exists() {
            Some(debug_bin)
        } else {
            None
        };

        let mut process = if let Some(bin_path) = binary_path {
            // Use built binary if available
            Command::new(bin_path)
                .env("TTS_SOCKET_PATH", &socket_path)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
        } else {
            // Fall back to cargo run
            Command::new("cargo")
                .args([
                    "run",
                    "--bin",
                    "tts-processor",
                    "--manifest-path",
                    "tts-processor/Cargo.toml",
                ])
                .env("TTS_SOCKET_PATH", &socket_path)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
        }
        .map_err(|e| {
            color_eyre::eyre::eyre!(
                "Failed to spawn TTS processor: {}. Make sure you're running from the workspace root and the TTS processor is built.",
                e
            )
        })?;

        // Wait for socket to be created (with timeout)
        let mut attempts = 0;
        let max_attempts = 500; // 50 seconds total
        while !socket_path.exists() && attempts < max_attempts {
            std::thread::sleep(Duration::from_millis(100));
            attempts += 1;

            // Check if process died
            if let Ok(Some(status)) = process.try_wait() {
                let stderr = process.stderr.take().and_then(|mut s| {
                    let mut buf = String::new();
                    s.read_to_string(&mut buf).ok().map(|_| buf)
                });
                return Err(color_eyre::eyre::eyre!(
                    "TTS processor exited early with status: {:?}. Stderr: {:?}",
                    status,
                    stderr
                ));
            }
        }

        if !socket_path.exists() {
            let _ = process.kill();
            return Err(color_eyre::eyre::eyre!(
                "TTS processor did not create socket in time"
            ));
        }

        // Connect to socket
        let stream = UnixStream::connect(&socket_path)
            .map_err(|e| color_eyre::eyre::eyre!("Failed to connect to TTS socket: {}", e))?;

        Ok(Self {
            stream,
            _process: process,
            socket_path,
        })
    }

    fn read_length_prefixed_message(&mut self) -> Result<Vec<u8>> {
        let mut len_bytes = [0u8; 4];
        self.stream.read_exact(&mut len_bytes)?;
        let len = u32::from_le_bytes(len_bytes) as usize;

        let mut buffer = vec![0u8; len];
        self.stream.read_exact(&mut buffer)?;
        Ok(buffer)
    }

    fn write_length_prefixed_message(&mut self, data: &[u8]) -> Result<()> {
        let len = data.len() as u32;
        self.stream.write_all(&len.to_le_bytes())?;
        self.stream.write_all(data)?;
        self.stream.flush()?;
        Ok(())
    }

    /// Generate audio from text and play it
    pub fn generate_audio(&mut self, text: String) -> Result<()> {
        let cmd = TtsCommand::GenerateAudio(text);
        let cmd_bytes = serialize_command(&cmd)?;
        self.write_length_prefixed_message(&cmd_bytes)?;

        // Read response
        let resp_bytes = self.read_length_prefixed_message()?;
        let resp = deserialize_response(&resp_bytes)?;

        match resp {
            TtsResponse::Started => {
                // Wait for finished response
                let resp_bytes = self.read_length_prefixed_message()?;
                let resp = deserialize_response(&resp_bytes)?;
                match resp {
                    TtsResponse::Finished => Ok(()),
                    TtsResponse::Error(e) => Err(color_eyre::eyre::eyre!("TTS error: {}", e)),
                    _ => Err(color_eyre::eyre::eyre!("Unexpected response: {:?}", resp)),
                }
            }
            TtsResponse::Error(e) => Err(color_eyre::eyre::eyre!("TTS error: {}", e)),
            _ => Err(color_eyre::eyre::eyre!("Unexpected response: {:?}", resp)),
        }
    }

    /// Stop current playback
    #[allow(dead_code)]
    pub fn stop(&mut self) -> Result<()> {
        let cmd = TtsCommand::Stop;
        let cmd_bytes = serialize_command(&cmd)?;
        self.write_length_prefixed_message(&cmd_bytes)?;

        let resp_bytes = self.read_length_prefixed_message()?;
        let resp = deserialize_response(&resp_bytes)?;

        match resp {
            TtsResponse::Stopped => Ok(()),
            TtsResponse::Error(e) => Err(color_eyre::eyre::eyre!("TTS error: {}", e)),
            _ => Err(color_eyre::eyre::eyre!("Unexpected response: {:?}", resp)),
        }
    }

    /// Wait until current audio playback is finished
    pub fn wait_until_finished(&mut self) -> Result<()> {
        let cmd = TtsCommand::WaitUntilFinished;
        let cmd_bytes = serialize_command(&cmd)?;
        self.write_length_prefixed_message(&cmd_bytes)?;

        let resp_bytes = self.read_length_prefixed_message()?;
        let resp = deserialize_response(&resp_bytes)?;

        match resp {
            TtsResponse::Finished => Ok(()),
            TtsResponse::Error(e) => Err(color_eyre::eyre::eyre!("TTS error: {}", e)),
            _ => Err(color_eyre::eyre::eyre!("Unexpected response: {:?}", resp)),
        }
    }
}

impl Drop for TtsClient {
    fn drop(&mut self) {
        // Try to stop the process gracefully
        let _ = self._process.kill();
        let _ = self._process.wait();

        // Clean up socket file
        if self.socket_path.exists() {
            let _ = std::fs::remove_file(&self.socket_path);
        }
    }
}
