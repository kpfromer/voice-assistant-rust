default:
    just --list

format-check:
    cargo fmt --check

format:
    cargo fmt

alias fmt := format

lint:
    cargo clippy

lint-fix:
    cargo clippy --fix

test:
    cargo test --release
