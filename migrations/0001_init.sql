CREATE TABLE IF NOT EXISTS users (
    id TEXT PRIMARY KEY,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS user_settings (
    user_id TEXT PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    local_music_roots JSONB NOT NULL DEFAULT '[]'::jsonb,
    volume_step DOUBLE PRECISION NOT NULL DEFAULT 0.05,
    cache_enabled BOOLEAN NOT NULL DEFAULT FALSE,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS providers (
    id TEXT PRIMARY KEY,
    kind TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    scan BOOLEAN NOT NULL DEFAULT FALSE,
    stream BOOLEAN NOT NULL DEFAULT FALSE,
    download BOOLEAN NOT NULL DEFAULT FALSE,
    sync BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS provider_accounts (
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    provider_id TEXT NOT NULL REFERENCES providers(id) ON DELETE CASCADE,
    enabled BOOLEAN NOT NULL DEFAULT FALSE,
    priority INTEGER NULL,
    external_account_id TEXT NULL,
    scopes JSONB NOT NULL DEFAULT '[]'::jsonb,
    access_token_encrypted TEXT NULL,
    refresh_token_encrypted TEXT NULL,
    token_expires_at TIMESTAMPTZ NULL,
    status TEXT NOT NULL DEFAULT 'disabled',
    settings JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (user_id, provider_id)
);

CREATE TABLE IF NOT EXISTS catalog_tracks (
    id TEXT PRIMARY KEY,
    primary_provider_id TEXT NOT NULL REFERENCES providers(id) ON DELETE RESTRICT,
    primary_source_track_id TEXT NOT NULL,
    fingerprint TEXT NULL,
    file_name TEXT NOT NULL,
    title TEXT NULL,
    artist TEXT NULL,
    album TEXT NULL,
    album_artist TEXT NULL,
    genre TEXT NULL,
    track_number INTEGER NULL,
    disc_number INTEGER NULL,
    year INTEGER NULL,
    duration_ms BIGINT NULL,
    ownership_scope TEXT NOT NULL,
    availability TEXT NOT NULL,
    preferred_identifier_id TEXT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (primary_provider_id, primary_source_track_id)
);

CREATE TABLE IF NOT EXISTS track_identifiers (
    id TEXT PRIMARY KEY,
    track_id TEXT NOT NULL REFERENCES catalog_tracks(id) ON DELETE CASCADE,
    provider_id TEXT NOT NULL REFERENCES providers(id) ON DELETE CASCADE,
    identifier_kind TEXT NOT NULL,
    value TEXT NOT NULL,
    path TEXT NULL,
    url TEXT NULL,
    storage_kind TEXT NOT NULL,
    availability TEXT NOT NULL,
    is_preferred BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (track_id, provider_id, identifier_kind, value)
);

CREATE TABLE IF NOT EXISTS user_library_items (
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    track_id TEXT NOT NULL REFERENCES catalog_tracks(id) ON DELETE CASCADE,
    item_kind TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (user_id, track_id, item_kind)
);

CREATE TABLE IF NOT EXISTS playback_sessions (
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    id TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'stopped',
    current_track_id TEXT NULL REFERENCES catalog_tracks(id) ON DELETE SET NULL,
    current_identifier_id TEXT NULL REFERENCES track_identifiers(id) ON DELETE SET NULL,
    position_ms BIGINT NOT NULL DEFAULT 0,
    volume DOUBLE PRECISION NOT NULL DEFAULT 1.0,
    muted BOOLEAN NOT NULL DEFAULT FALSE,
    repeat_mode TEXT NOT NULL DEFAULT 'off',
    shuffle BOOLEAN NOT NULL DEFAULT FALSE,
    queue_state JSONB NOT NULL DEFAULT '{"entries":[]}'::jsonb,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (user_id, id)
);

CREATE INDEX IF NOT EXISTS idx_catalog_tracks_order
    ON catalog_tracks (artist, title, file_name, id);

CREATE INDEX IF NOT EXISTS idx_track_identifiers_track
    ON track_identifiers (track_id, is_preferred, provider_id, id);

CREATE INDEX IF NOT EXISTS idx_user_library_items_user_kind
    ON user_library_items (user_id, item_kind, track_id);

CREATE INDEX IF NOT EXISTS idx_provider_accounts_user_enabled
    ON provider_accounts (user_id, enabled, priority, provider_id);
