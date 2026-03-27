# ReplayCore

ReplayCore is a local-first Rust music backend and CLI.

This repository keeps the backend only:
- local library import and playback
- an explicit Postgres-backed `db` mode
- Bandcamp provider support
- JSON contract generation and validation

The abandoned Tauri frontend layer is ignored by the root project and kept out of git status.

## Project layout

- `src/` - backend, CLI, API, playback, search, and contract code
- `migrations/` - SQLx migrations
- `docker-compose.yml` - local Postgres for explicit database mode
- `.env.example` - sample environment variables

## Quick start

Run the local backend shell:

```text
cargo run -- status
cargo run -- serve
```

Work with the database only when you need persistence or provider account storage:

```text
docker compose up -d
cargo run -- db status
cargo run -- db migrate
cargo run -- db sync
cargo run -- db serve
```

## Local music

Point ReplayCore at your music folder with one of these variables:

```text
REPLAYCORE_LOCAL_MUSIC_ROOT=C:\Music
REPLAYCORE_LOCAL_MUSIC_ROOTS=C:\Music;D:\Archive\Music
```

If no folder is configured, the app falls back to the default user Music directory.

## Database mode

Database mode is explicit on purpose. The normal shell does not wait on Postgres.

Required or useful variables for `db` mode:

```text
DATABASE_URL=postgres://replaycore:replaycore@127.0.0.1:5432/replaycore
REPLAYCORE_DATABASE_CONNECT_TIMEOUT_MS=1000
REPLAYCORE_TOKEN_ENCRYPTION_KEY=base64-encoded-32-byte-key
```

## Commands

Useful shell commands:

```text
contract
status
list
queue
find <query>
search <query>
resolve <url>
providers
provider set <id> '<json>'
provider clear <id>
open <path>
play <index|query>
next
prev
pause
resume
stop
volume <0..1>
seek <seconds>
reload
```

## Notes

- Local disk is the source for import, not the long-term source of truth.
- Postgres is optional and only used in explicit `db` mode.
- YouTube and the old UI experiments were removed from the active root project.
