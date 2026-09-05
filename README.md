# codex2api

`codex2api` is a small, independent Rust service that signs in to one separate
ChatGPT account and exposes its Codex backend on a local, OpenAI
Responses-compatible HTTP API. It is not an official OpenAI API gateway.

## Account separation

Personal Codex (Account A) continues to use its own `~/.codex` files and macOS
Keychain entries. `codex2api` (Account B) uses the distinct Keychain service
`dev.codex2api.credentials` and never invokes `codex login`, imports Codex CLI/app
credentials, or reads/writes `~/.codex`. Local clients authenticate to
`codex2api` with a separate randomly generated API key.

## Build and initialize

```bash
cargo build --release
./target/release/codex2api init
```

Configuration is stored at
`~/Library/Application Support/codex2api/config.toml` (`0600`) inside a `0700`
directory. OAuth credentials and the local key are stored in macOS Keychain by
default. Setting `credential_store = "file"` uses `0600` files in that same
directory; this is intended for systems where Keychain is unavailable and is
less secure than Keychain storage.

`CODEX2API_DATA_DIR`, `CODEX2API_AUTH_CREDENTIAL_STORE`,
`CODEX2API_SERVER_HOST`, and `CODEX2API_SERVER_PORT` can override their
corresponding settings for isolated testing or service launch configuration.

## Login and run

Use a separate browser profile or private window that is signed in only to
Account B:

```bash
codex2api login --no-open
# paste the displayed URL into that Account B browser profile
codex2api serve
```

Without `--no-open`, the default browser opens automatically. OAuth uses PKCE
S256, validates a random state, and listens only on `localhost:1455`. If that
port is occupied, stop the process using it and retry; `codex2api` does not
silently select another callback port. Run `codex2api login` again to re-login.

Client settings:

```text
Base URL: http://127.0.0.1:8318/v1
API key: value returned by codex2api key show
API mode: Responses
```

Every `/v1/*` request requires the local key. `key show` warns and asks before
printing it; do not put the key in source control or logs. `key rotate`
immediately invalidates the old key. `status` prints only masked identity,
expiry, listener state, and a key fingerprint.

Endpoints are `POST /v1/responses`, `GET /v1/models`, `GET /healthz`, and
`GET /readyz`. Model listing uses authenticated upstream discovery and falls
back to the configurable static list if discovery is temporarily unavailable;
fallback responses include `x-codex2api-model-source: static-fallback`.

## Security and compatibility limits

The server binds to `127.0.0.1` by default and refuses another address unless
`--allow-non-loopback` is passed; local API-key authentication remains
mandatory either way. Do not publish it to the Internet or through Tailscale.
Request bodies and secrets are never intentionally logged, and body sizes,
connection time, response-header time, and non-stream aggregation are bounded.
Established streams have no total timeout and are cancelled upstream when the
client disconnects.

[Official OpenAI documentation](https://learn.chatgpt.com/docs/auth?surface=app)
supports ChatGPT sign-in for Codex clients and explains that official Codex
clients may share cached credentials. The ChatGPT Codex backend used by this
project is not documented as a stable, general-purpose public API. Its fields,
headers, models, and authentication behavior may change without notice.

## Commands

```text
codex2api init
codex2api login [--no-open]
codex2api serve [--allow-non-loopback]
codex2api status
codex2api logout
codex2api key rotate
codex2api key show
codex2api config path
```

`logout` removes only Account B OAuth credentials; it retains the config and
local key. To uninstall completely, run `logout`, delete the
`dev.codex2api.credentials` / `local-api-key` Keychain entry, remove
`~/Library/Application Support/codex2api`, and delete the binary. No Codex,
AIO, OpenClaw, Hermes, CLIProxyAPI, or browser credentials are changed.

## Development

Tests use local mock servers and never call OpenAI:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all
cargo build --release
```

See `NOTICE` for the narrowly adapted AIO Coding Hub behavior and attribution.
