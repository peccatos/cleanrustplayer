# ReplayCore Service Plan

## Goal
Build ReplayCore as a service-first, DB-backed federated music platform.

## Decisions
- Postgres is the source of truth.
- Local disk is only for import, cache, and temp artifacts.
- The CLI stays as a thin shell and admin harness.
- API/CLI projections must preserve the contract shape.

## Next Steps
1. Add Postgres schema and migrations for `users`, `sources`, `tracks`, `locations`, `library_items`, `queues`, `playback_sessions`, and `settings`.
2. Introduce a repository layer so reads and writes go through SQL instead of in-memory state.
3. Convert local library scanning into an ingest/cache job only.
4. Build projection builders for `catalog`, `user_library`, `playback`, `queue`, and `settings`.
5. Add provider account and token storage.
6. Integrate providers one by one: Spotify, Apple Music, YouTube, SoundCloud.
7. Finish with schema validation, migration tests, and contract tests.

## Start Point
Begin with steps 1 and 2.
