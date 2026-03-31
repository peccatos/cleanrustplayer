# ReplayCore

чтобы запустить, ты должен написать cargo run, там будет список команд, потом ты пишешь cargo run --serve в другом терме, там будет ссылка на локальный хост, переходишь и слушаешь свою музыку



ReplayCore is now a pragmatic Rust music backend with two modes:

- old CLI / playback shell
- simple web server for a local public music library

The new `serve` mode is the one that matters for this MVP.

## What the MVP does

- scans local music folders from disk
- stores track metadata in SQLite through `sqlx`
- serves a simple HTML interface directly from Rust
- exposes `GET /api/tracks`
- exposes `POST /api/library/rescan`
- streams local audio files through `GET /api/tracks/:id/stream`
- supports `Range` requests for browser audio playback

No Google Drive is needed for this mode.

## Quick start

1. Point the library to your music folder.
2. Start the web server.
3. Open the local page in the browser.

Example PowerShell setup:

```powershell
$env:REPLAYCORE_LOCAL_MUSIC_ROOT = "D:\music-library"
cargo run -- serve
```

Default server address:

```text
http://127.0.0.1:3001
```

## Important env vars

Local music roots:

```text
REPLAYCORE_LOCAL_MUSIC_ROOT=D:\music-library
REPLAYCORE_LOCAL_MUSIC_ROOTS=D:\music-library;D:\archive\music
```

Web server:

```text
REPLAYCORE_HTTP_HOST=127.0.0.1
REPLAYCORE_HTTP_PORT=3001
REPLAYCORE_SQLITE_PATH=data\replaycore.sqlite3
```

## Commands

Web mode:

```text
cargo run -- serve
```

Old CLI mode still exists:

```text
cargo run
cargo run -- list
cargo run -- play 0
```

Explicit Postgres `db` mode also still exists for the older backend flow:

```text
cargo run -- db status
cargo run -- db migrate
cargo run -- db sync
```

## API

Useful routes in `serve` mode:

```text
GET  /api/health
GET  /api/tracks
GET  /api/tracks/:id
GET  /api/tracks/:id/stream
POST /api/library/rescan
```

## Storage choices

For this MVP:

- audio files live on local disk
- metadata lives in SQLite
- frontend is static and served by the Rust backend itself

That is the right level of complexity for now.

## What this deliberately does not do yet

- auth
- upload UI
- delete/edit metadata UI
- playlists
- multi-user logic
- remote cloud storage

If you add all that now, you will bury the project.
