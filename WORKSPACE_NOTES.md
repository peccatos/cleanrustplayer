# Workspace Notes

## Current Direction

- Keep the repository CLI-only.
- The `winui/` and `front-rustplayer/` frontend layers are dead weight.
- Prefer Rust CLI/backend changes over UI or HTTP work.

## What Is Implemented

- Local music scanning from disk already exists.
- `cargo build` passes on the backend.
- The main entrypoint is CLI-only.

## Current Environment

- Repo root: `C:\Users\burav\Desktop\rustplayer`
- Main backend entrypoint: `src/main.rs`
- Application state: `src/context.rs`

## Important Files

- `src/context.rs` - library sync and catalog building
- `src/main.rs` - CLI entrypoint
- `.env` - local runtime settings
- `.env.example` - sample settings

## Env Variables That Matter

- `REPLAYCORE_DATABASE_URL`
- `REPLAYCORE_TOKEN_ENCRYPTION_KEY`

## Notes

- Do not reintroduce the WinUI frontend unless explicitly requested.
- If something breaks, check whether a runtime is being dropped inside async code.
