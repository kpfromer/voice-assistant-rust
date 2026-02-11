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
    cargo clippy

# Runs the clippy linter and fixes the issues
lint-fix:
    cargo clippy --fix

# Runs the tests
test:
    cargo test --release

# Checks for unused dependencies or files
shear:
    cargo shear
