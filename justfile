#  wget https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.bin
# Docker-based development commands
# ===================================

# Build the Docker image
docker-build:
    docker compose build

# Run the app in Docker
docker-run:
    docker compose run --rm dev

# Open a shell in the Docker container for development
docker-shell:
    docker compose run --rm shell

# Build inside Docker (useful for CI or clean builds)
docker-cargo-build:
    docker compose run --rm dev cargo build --release

# Run cargo check inside Docker
docker-check:
    docker compose run --rm dev cargo check

# Clean Docker volumes (cargo cache)
docker-clean:
    docker compose down -v

# Watch for changes and rebuild (inside Docker shell)
docker-watch:
    docker compose run --rm dev cargo watch -x 'build --release'

# Native build commands (uses vendored protobuf - no protoc required)
# ===================================================================
# Set environment for native build with vendored protobuf

export PROTOBUF_NO_VENDOR := "1"
export PROTOC_NO_VENDOR := "1"
export SENTENCEPIECE_SYS_USE_PKG_CONFIG := "0"
export SENTENCEPIECE_SYS_BUILD := "1"
export PKG_CONFIG_PATH := ""
export CMAKE_PREFIX_PATH := ""

# Native build (uses vendored protobuf - no protoc required)
build:
    cargo build --release

# Native run
run:
    cargo run --release

# Clean native build artifacts
clean:
    cargo clean

# Download models
download-models:
    wget -P whisper_model/ https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.bin
    wget https://github.com/ggerganov/whisper.cpp/raw/master/samples/jfk.wav
