use anyhow::{bail, Result};

use crate::context::AppContext;
use crate::domain::playback::{PlaybackKind, PlaybackSource};
use crate::domain::track::{TrackItem, TrackRef, TrackSource};

pub struct PlaybackResolver;

impl PlaybackResolver {
    pub fn list_local(context: &AppContext) -> Vec<TrackItem> {
        context.local_track_items()
    }

    pub fn list_cloud(context: &AppContext) -> Result<Vec<TrackItem>> {
        if context.cloud_tracks.is_empty() {
            bail!("cloud library is empty");
        }

        Ok(context.cloud_track_items())
    }

    pub fn resolve(context: &AppContext, track_ref: TrackRef) -> Result<(TrackItem, PlaybackSource)> {
        match track_ref.source {
            TrackSource::Local => {
                let track = context
                    .local_tracks
                    .iter()
                    .find(|track| track.path.display().to_string() == track_ref.track_id)
                    .ok_or_else(|| anyhow::anyhow!("local track not found"))?;

                let item = context
                    .local_track_items()
                    .into_iter()
                    .find(|item| item.backend_track_id == track.path.display().to_string())
                    .ok_or_else(|| anyhow::anyhow!("local track item not found"))?;

                Ok((
                    item.clone(),
                    PlaybackSource {
                        kind: PlaybackKind::Url,
                        source: TrackSource::Local,
                        track_id: track_ref.track_id,
                        url: Some(track.path.display().to_string()),
                        stream_endpoint: None,
                        mime_type: None,
                        expires_at: None,
                    },
                ))
            }
            TrackSource::Cloud => {
                let track = context
                    .cloud_tracks
                    .iter()
                    .find(|track| track.path.display().to_string() == track_ref.track_id)
                    .ok_or_else(|| anyhow::anyhow!("cloud track not found"))?;

                let item = context
                    .cloud_track_items()
                    .into_iter()
                    .find(|item| item.backend_track_id == track.path.display().to_string())
                    .ok_or_else(|| anyhow::anyhow!("cloud track item not found"))?;

                Ok((
                    item.clone(),
                    PlaybackSource {
                        kind: PlaybackKind::StreamEndpoint,
                        source: TrackSource::Cloud,
                        track_id: track_ref.track_id,
                        url: None,
                        stream_endpoint: Some(format!("/api/stream/cloud/{}", item.id)),
                        mime_type: None,
                        expires_at: None,
                    },
                ))
            }
        }
    }
}
