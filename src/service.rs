// Contract builders and validation for the public backend shape.
use std::sync::Arc;

use anyhow::{Context, Result};
use serde_json::Value;

use crate::app::RepeatMode;
use crate::context::AppContext;
use crate::contract::{
    PlaybackState, QueueEntry, QueueRepeatMode, QueueState, ReplayCoreContract, Settings,
    SourceRecord, UserLibrary,
};

#[derive(Debug, Clone)]
pub struct ContractRuntime {
    pub playback: PlaybackState,
    pub queue_order: Vec<usize>,
    pub current_queue_index: Option<usize>,
    pub repeat_mode: QueueRepeatMode,
    pub shuffle_enabled: bool,
}

#[derive(Clone)]
pub struct ReplayCoreService {
    validator: Arc<jsonschema::Validator>,
}

impl ReplayCoreService {
    pub fn new() -> Result<Self> {
        let schema: Value = serde_json::from_str(include_str!("schema/replaycore.schema.json"))
            .context("failed to parse ReplayCore schema")?;
        let validator =
            jsonschema::validator_for(&schema).context("failed to compile ReplayCore schema")?;

        Ok(Self {
            validator: Arc::new(validator),
        })
    }

    pub fn headless_runtime(context: &AppContext) -> ContractRuntime {
        ContractRuntime {
            playback: PlaybackState::stopped(1.0),
            queue_order: (0..context.catalog.tracks.len()).collect(),
            current_queue_index: None,
            repeat_mode: QueueRepeatMode::Off,
            shuffle_enabled: false,
        }
    }

    pub fn build_contract(
        &self,
        context: &AppContext,
        runtime: ContractRuntime,
    ) -> ReplayCoreContract {
        let enabled_source_ids = context
            .catalog
            .sources
            .iter()
            .filter(|source| source.enabled)
            .map(|source| source.id.clone())
            .collect();

        let local_music_roots = context
            .local_music_roots
            .iter()
            .map(|path| path.display().to_string())
            .collect();

        let settings = Settings::new(
            enabled_source_ids,
            local_music_roots,
            context.volume_step,
            context.cache_enabled,
        );

        let user_library = UserLibrary {
            user_id: context.user_id.clone(),
            saved_track_ids: context.saved_track_ids.clone(),
            hidden_track_ids: context.hidden_track_ids.clone(),
        };

        let entries = runtime
            .queue_order
            .iter()
            .enumerate()
            .filter_map(|(queue_position, &track_index)| {
                context
                    .catalog
                    .tracks
                    .get(track_index)
                    .map(|track| QueueEntry {
                        id: format!("queue-entry-{queue_position}"),
                        track_id: track.identity.track_id.clone(),
                    })
            })
            .collect();

        let queue = QueueState::new(
            entries,
            runtime.current_queue_index,
            runtime.shuffle_enabled,
            runtime.repeat_mode,
        );

        ReplayCoreContract {
            catalog: context.catalog.clone(),
            user_library,
            playback: runtime.playback,
            queue,
            settings,
        }
    }

    pub fn validate_contract(&self, contract: &ReplayCoreContract) -> Result<()> {
        let value = serde_json::to_value(contract).context("failed to serialize contract")?;
        self.validate_value(&value)
    }

    pub fn validate_value(&self, value: &Value) -> Result<()> {
        if self.validator.is_valid(value) {
            return Ok(());
        }

        let mut errors = Vec::new();
        for error in self.validator.iter_errors(value) {
            errors.push(format!("{} at {}", error, error.instance_path()));
        }

        Err(anyhow::anyhow!(
            "ReplayCore contract validation failed: {}",
            errors.join("; ")
        ))
    }
}

pub fn contract_repeat_mode(repeat_mode: RepeatMode) -> QueueRepeatMode {
    match repeat_mode {
        RepeatMode::Off => QueueRepeatMode::Off,
        RepeatMode::One => QueueRepeatMode::One,
        RepeatMode::All => QueueRepeatMode::All,
    }
}

pub fn source_records_for_context(
    bandcamp_enabled: bool,
) -> Vec<SourceRecord> {
    vec![SourceRecord::local_import(true), SourceRecord::bandcamp(bandcamp_enabled)]
}

#[cfg(test)]
mod tests {
    use anyhow::Result;

    use crate::contract::{
        AvailabilityState, CatalogIndex, CommandEnvelope, CommandError, OwnershipScope,
        PlaybackState, QueueEntry, QueueRepeatMode, QueueState, Settings, SourceRecord,
        StorageKind, TrackIdentity, TrackLocationRecord, TrackMetadata, TrackRecord, UserLibrary,
    };

    use super::ReplayCoreService;

    #[test]
    fn validates_minimal_contract() -> Result<()> {
        let track_id = "track_001".to_string();
        let location_id = "location_001".to_string();

        let catalog = CatalogIndex::new(
            vec![TrackRecord::new(
                TrackIdentity::new(
                    track_id.clone(),
                    "local_import",
                    "C:/Music/test.mp3",
                    Some("fingerprint-001".to_string()),
                ),
                TrackMetadata {
                    title: "Song A".to_string(),
                    artist: "Artist A".to_string(),
                    album: "Album A".to_string(),
                    album_artist: None,
                    genre: None,
                    track_number: None,
                    disc_number: None,
                    year: None,
                    duration_ms: None,
                },
                OwnershipScope::UserOwned,
                AvailabilityState::Available,
                Some(location_id.clone()),
            )],
            vec![SourceRecord::local_import(true)],
            vec![TrackLocationRecord::new(
                location_id,
                track_id.clone(),
                "local_import",
                StorageKind::LocalFile,
                Some("C:/Music/test.mp3".to_string()),
                None,
                AvailabilityState::Available,
            )],
        );

        let contract = crate::contract::ReplayCoreContract {
            catalog,
            user_library: UserLibrary {
                user_id: "user-1".to_string(),
                saved_track_ids: Vec::new(),
                hidden_track_ids: Vec::new(),
            },
            playback: PlaybackState::stopped(1.0),
            queue: QueueState::new(
                vec![QueueEntry {
                    id: "queue-entry-0".to_string(),
                    track_id,
                }],
                Some(0),
                false,
                QueueRepeatMode::Off,
            ),
            settings: Settings::new(
                vec!["local_import".to_string()],
                vec!["C:/Music".to_string()],
                0.05,
                false,
            ),
        };

        let service = ReplayCoreService::new()?;
        service.validate_contract(&contract)?;
        Ok(())
    }

    #[test]
    fn command_envelope_error_serializes() {
        let envelope: CommandEnvelope<()> = CommandEnvelope {
            ok: false,
            data: None,
            error: Some(CommandError {
                code: "bad_request".to_string(),
                message: "broken".to_string(),
            }),
        };
        let value = serde_json::to_value(envelope).expect("serialize envelope");

        assert_eq!(value["ok"], false);
        assert_eq!(value["error"]["code"], "bad_request");
        assert_eq!(value["error"]["message"], "broken");
    }
}
