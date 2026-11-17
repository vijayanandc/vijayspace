# Every developer and agent command. `just check` runs what CI runs.
# Recipes for later backlog tasks (db-reset, gen, xtask checks) are added by those tasks.

set dotenv-load := true

default:
    @just --list

# Install the cargo tools used by the recipes below
setup:
    cargo install --locked just cargo-nextest cargo-insta

# Start MySQL 8.4 and Mailpit
db-up:
    docker compose -f deploy/docker-compose.yml up -d --wait

# Stop local dependencies
db-down:
    docker compose -f deploy/docker-compose.yml down

# Migrate, then run the API and the worker on :8080 (local networks allowed for testing)
serve:
    WEFT_ALLOW_PRIVATE_NETWORKS=${WEFT_ALLOW_PRIVATE_NETWORKS:-true} cargo run -p weft-server -- serve

# Apply migrations only
migrate:
    cargo run -p weft-server -- migrate

fmt:
    cargo fmt --all

lint:
    cargo fmt --all -- --check
    cargo clippy --workspace --all-targets -- -D warnings

# Fast tests, no database needed
test-unit:
    cargo nextest run --workspace --no-tests=pass -E 'not (binary(db) | binary(http) | binary(scenarios))'

# All tests (DB suites need `just db-up`)
test:
    cargo nextest run --workspace --no-tests=pass

# Everything CI runs. Must pass before a task is done.
check: lint test-unit
