-- Remove rows that point at provider kinds no longer supported by the backend.
UPDATE playback_sessions
SET current_track_id = NULL,
    current_identifier_id = NULL,
    queue_state = '{"entries":[]}'::jsonb,
    updated_at = NOW()
WHERE current_track_id IN (
    SELECT ct.id
    FROM catalog_tracks ct
    JOIN providers p ON p.id = ct.primary_provider_id
    WHERE p.kind NOT IN ('local_disk', 'bandcamp')
)
OR current_identifier_id IN (
    SELECT ti.id
    FROM track_identifiers ti
    JOIN providers p ON p.id = ti.provider_id
    WHERE p.kind NOT IN ('local_disk', 'bandcamp')
);

DELETE FROM user_library_items
WHERE track_id IN (
    SELECT ct.id
    FROM catalog_tracks ct
    JOIN providers p ON p.id = ct.primary_provider_id
    WHERE p.kind NOT IN ('local_disk', 'bandcamp')
);

DELETE FROM track_identifiers
WHERE provider_id IN (
    SELECT id
    FROM providers
    WHERE kind NOT IN ('local_disk', 'bandcamp')
);

DELETE FROM catalog_tracks
WHERE primary_provider_id IN (
    SELECT id
    FROM providers
    WHERE kind NOT IN ('local_disk', 'bandcamp')
);

DELETE FROM provider_accounts
WHERE provider_id IN (
    SELECT id
    FROM providers
    WHERE kind NOT IN ('local_disk', 'bandcamp')
);

DELETE FROM providers
WHERE kind NOT IN ('local_disk', 'bandcamp');
