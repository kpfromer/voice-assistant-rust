# Voice Assistant

A Rust-based voice assistant with speech recognition and text-to-speech capabilities.

## Prerequisites

- [Docker](https://docs.docker.com/get-docker/) and Docker Compose
- [Just](https://github.com/casey/just) command runner (`brew install just` on macOS)

## Quick Start

```bash
# Build the Docker image (first time only, or after Dockerfile changes)
just docker-build

# Open a development shell inside the container
just docker-shell
```

Inside the container:

```bash
cargo build --release   # First build caches dependencies
cargo run --release     # Run the assistant
```

## Development Workflow

This project uses Docker to provide a consistent build environment, avoiding protobuf version conflicts that occur on macOS.

### Available Commands

| Command | Description |
|---------|-------------|
| `just docker-build` | Build the Docker image |
| `just docker-shell` | Open interactive shell for development |
| `just docker-run` | Build and run the app |
| `just docker-check` | Run `cargo check` |
| `just docker-clean` | Remove Docker volumes (cargo cache) |

### How It Works

- **Source code** is mounted at `/app` — edits on your host are immediately visible in the container
- **Cargo cache** (registry, git, target) is stored in Docker volumes for fast rebuilds
- **Model files** are mounted read-only from `model/`, `whisper_model/`, and `parakeet_model/`

### First Build

The first `cargo build` inside the container will take several minutes as it:
1. Downloads all crate dependencies
2. Compiles native libraries (whisper, ONNX runtime, etc.)

Subsequent builds are fast because everything is cached in Docker volumes.

## Models

Download the required models before running:

```bash
# Whisper model for speech recognition
wget -P whisper_model/ https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.bin
```

## Project Structure

```
├── src/
│   ├── main.rs              # Entry point
│   ├── speech.rs            # Text-to-speech
│   ├── speech_listener.rs   # Voice activity detection
│   └── audio.rs             # Audio utilities
├── model/                   # TTS model files
├── whisper_model/           # Whisper STT model
├── parakeet_model/          # Parakeet model
├── Dockerfile               # Dev container definition
├── docker-compose.yml       # Container orchestration
└── justfile                 # Task runner commands
```

## Troubleshooting

### Protobuf Version Mismatch

If you see errors like:
```
This program was compiled against version 3.14.0 of the Protocol Buffer runtime library,
which is not compatible with the installed version (3.21.12)
```

Use the Docker environment instead of native builds. The container has the correct protobuf version.

### Audio Not Working

Docker on macOS doesn't have access to audio devices. For testing with real microphone input:
1. Record audio on your host machine and save as WAV
2. Copy the WAV file into the container or mounted directory
3. Process the file inside the container

### Slow Rebuilds

If rebuilds are slow, make sure you're using the Docker volumes for caching:
```bash
# Check volumes exist
docker volume ls | grep voice-assistant

# If missing, rebuild
just docker-clean
just docker-build
```

## Native Build (Not Recommended on macOS)

If you must build natively:

```bash
just build   # Uses environment variables to force vendored protobuf
just run
```

This may still fail due to system library conflicts.


https://www.home-assistant.io/voice_control/create_wake_word/
https://colab.research.google.com/drive/1q1oe2zOyZp7UsB3jJiQ1IFn8z5YfjwEb?usp=sharing#scrollTo=qgaKWIY6WlJ1