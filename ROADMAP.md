# ReplayCore Service Plan

## Goal
Build ReplayCore as a clean service-first, DB-backed local music platform.

## Decisions
- Postgres is the source of truth.
- Local disk is only for import, cache, and temp artifacts.
- The CLI stays as a thin shell and admin harness.
- API/CLI projections must preserve the contract shape.

## Next Steps
1. Keep the backend/front split clean and delete the dead UI experiments.
2. Remove unsupported provider branches from the backend and docs.
3. Make startup persisted-first and move scanning into explicit import/refresh only.
4. Trim dead helpers until `cargo check` is clean.
5. Finish with schema validation, migration tests, and contract tests.

## Start Point
Begin with steps 1 and 2.
