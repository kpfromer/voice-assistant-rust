default:
    just --list

# Checks if the code is formatted correctly
format-check:
    cargo fmt --check

# Formats the code
format:
    cargo fmt

alias fmt := format

# Runs the clippy linter
lint:
    cargo clippy --no-deps

# Runs the clippy linter and fixes the issues
lint-fix:
    cargo clippy --fix --no-deps

# Runs the tests
test:
    cargo test --release

# Checks for unused dependencies or files
shear:
    cargo shear

# Runs the checks
check: lint format-check shear

alias c := check

sync-to-pi:
    #!/bin/bash

    REMOTE=kpfromer@10.1.0.33
    REMOTE_PATH=/home/kpfromer/voice-assistant

    rsync -av --delete \
      --exclude 'target' \
      --exclude '.git' \
      --exclude '.jj' \
      ./ $REMOTE:$REMOTE_PATH