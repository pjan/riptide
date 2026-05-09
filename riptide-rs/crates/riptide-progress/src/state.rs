//! Progress state and pure event reducer.

use crate::event::{
    ArtifactKind, ArtifactOutcome, DisplayLabel, DownloadEvent, DownloadPhase, GroupOutcome,
    ItemKind, ItemOutcome, OperationConfig, OperationKind, QualityBadge, TransferProgress,
};
use crate::id::{GroupId, ItemId, OperationId};
use crate::summary::{ArtifactSummary, ProgressSummary};
use indexmap::IndexMap;
use std::collections::HashMap;
use std::path::PathBuf;
use thiserror::Error;

/// Key that uniquely identifies an in-flight artifact.
///
/// `item_id` disambiguates per-track artifacts of the same [`ArtifactKind`]
/// within a group (e.g. one `.lrc` per track). Group-level artifacts (album
/// cover, M3U) use `item_id: None`.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct ArtifactKey {
    /// Parent operation.
    pub operation_id: OperationId,
    /// Parent group, if any.
    pub group_id: Option<GroupId>,
    /// Parent item for per-track artifacts; `None` for group-level artifacts.
    pub item_id: Option<ItemId>,
    /// Artifact kind.
    pub kind: ArtifactKind,
}

/// Active state for an in-flight artifact.
///
/// The [`ArtifactKey`] is the `IndexMap` key; it is not duplicated here.
pub struct ArtifactState {
    /// Filename or display label.
    pub label: String,
    /// Current write phase.
    pub phase: DownloadPhase,
}

/// Active state for an in-flight item.
pub struct ItemState {
    /// Item ID.
    pub id: ItemId,
    /// Parent operation.
    pub operation_id: OperationId,
    /// Parent group, if any.
    pub group_id: Option<GroupId>,
    /// Item kind.
    pub kind: ItemKind,
    /// Display label.
    pub label: DisplayLabel,
    /// Resolved quality badge, once known.
    pub quality: Option<QualityBadge>,
    /// Current download phase.
    pub phase: DownloadPhase,
    /// Current transfer snapshot, if in a transfer phase.
    pub transfer: Option<TransferProgress>,
    /// Final output path, once known via `ItemOutputResolved`.
    pub output_path: Option<PathBuf>,
}

/// Active state for a group.
pub struct GroupState {
    /// Group ID.
    pub id: GroupId,
    /// Parent operation.
    pub operation_id: OperationId,
    /// Group kind.
    pub kind: OperationKind,
    /// Group label.
    pub label: DisplayLabel,
    /// Number of items finished so far.
    pub items_done: u64,
    /// Total item count, once known.
    pub total_items: Option<u64>,
}

/// Snapshot of the operation-level metadata.
pub struct OperationState {
    /// Operation ID.
    pub id: OperationId,
    /// Operation kind.
    pub kind: OperationKind,
    /// Operation label.
    pub label: DisplayLabel,
    /// Total group count, once known.
    pub total_groups: Option<u64>,
    /// Total item count, once known.
    pub total_items: Option<u64>,
}

/// A record for an item, artifact, or group that has finished.
pub enum CompletedRecord {
    /// A download item finished.
    Item {
        /// Item ID.
        item_id: ItemId,
        /// Parent group, if any.
        group_id: Option<GroupId>,
        /// Display label at finish time.
        label: DisplayLabel,
        /// Quality badge at finish time.
        quality: Option<QualityBadge>,
        /// Outcome.
        outcome: ItemOutcome,
    },
    /// An artifact write finished.
    Artifact {
        /// Parent operation.
        operation_id: OperationId,
        /// Parent group, if any.
        group_id: Option<GroupId>,
        /// Parent item, if per-track.
        item_id: Option<ItemId>,
        /// Artifact kind.
        kind: ArtifactKind,
        /// Filename or display label.
        label: String,
        /// Outcome.
        outcome: ArtifactOutcome,
    },
    /// A group finished.
    Group {
        /// Group ID.
        group_id: GroupId,
        /// Group kind.
        kind: OperationKind,
        /// Group label at finish time.
        label: DisplayLabel,
        /// Group outcome.
        outcome: GroupOutcome,
        /// Items done snapshot from `GroupState` at finish time.
        items_done: u64,
        /// Total items snapshot from `GroupState` at finish time.
        items_total: Option<u64>,
    },
}

/// Error returned when an event cannot be reduced into the current state.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ProgressError {
    /// `OperationTotalsUpdated` or `OperationFinished` referenced a different
    /// operation ID than the one this state was constructed for.
    #[error("unknown operation `{0:?}`")]
    UnknownOperation(OperationId),

    /// `GroupStarted` used a `GroupId` that is already active.
    #[error("group `{0:?}` was already started")]
    DuplicateGroup(GroupId),

    /// An event referenced a `GroupId` that has not been started.
    #[error("unknown group `{0:?}`")]
    UnknownGroup(GroupId),

    /// `ItemQueued` used an `ItemId` that is already active.
    #[error("item `{0:?}` was already queued")]
    DuplicateItem(ItemId),

    /// An event referenced an `ItemId` that is not active.
    #[error("unknown item `{0:?}`")]
    UnknownItem(ItemId),

    /// `ArtifactFinished` referenced an artifact that is not in the active map.
    #[error("artifact `{0:?}` is not active")]
    UnknownArtifact(ArtifactKey),

    /// An event arrived in an invalid order for the referenced item.
    #[error("event order violation for item {item_id:?}: {reason}")]
    InvalidTransition {
        /// Item that triggered the violation.
        item_id: ItemId,
        /// Static description of the violation.
        reason: &'static str,
    },
}

/// Complete progress state for one operation.
///
/// Construct via [`ProgressState::new`]; reduce events via
/// [`ProgressState::reduce`]; derive a summary via [`ProgressState::summary`].
///
/// Scoped to a **single** operation. The CLI creates one state per input URL.
/// If a future command needs a multi-operation dashboard, add an outer
/// coordinator rather than expanding this struct.
pub struct ProgressState {
    /// Operation-level metadata.
    pub operation: OperationState,
    /// Active groups, ordered by insertion (announcement order).
    pub groups: IndexMap<GroupId, GroupState>,
    /// Active items, ordered by insertion.
    pub items: IndexMap<ItemId, ItemState>,
    /// Active artifacts keyed by their composite `ArtifactKey`.
    pub artifacts: IndexMap<ArtifactKey, ArtifactState>,
    /// Finished items, artifacts, and groups in completion order.
    pub completed: Vec<CompletedRecord>,
    finished: bool,
}

impl ProgressState {
    /// Creates a new state from an [`OperationConfig`].
    ///
    /// [`DownloadEvent::OperationStarted`] does not exist in the event enum;
    /// this constructor is the only way to start a state. Calling
    /// [`reduce`](Self::reduce) with any event before this is not possible by
    /// construction.
    pub fn new(config: OperationConfig) -> Self {
        Self {
            operation: OperationState {
                id: config.id,
                kind: config.kind,
                label: config.label,
                total_groups: config.total_groups,
                total_items: config.total_items,
            },
            groups: IndexMap::new(),
            items: IndexMap::new(),
            artifacts: IndexMap::new(),
            completed: Vec::new(),
            finished: false,
        }
    }

    /// Whether [`DownloadEvent::OperationFinished`] has been reduced.
    pub fn is_finished(&self) -> bool {
        self.finished
    }

    /// Reduces a single [`DownloadEvent`] into the state.
    ///
    /// Returns [`ProgressError`] for invalid transitions (duplicate IDs, unknown
    /// references, order violations). Errors are non-fatal — the caller may log
    /// and continue.
    pub fn reduce(&mut self, event: DownloadEvent) -> Result<(), ProgressError> {
        match event {
            DownloadEvent::OperationTotalsUpdated {
                id,
                total_groups,
                total_items,
            } => {
                if id != self.operation.id {
                    return Err(ProgressError::UnknownOperation(id));
                }
                if let Some(v) = total_groups {
                    self.operation.total_groups = Some(v);
                }
                if let Some(v) = total_items {
                    self.operation.total_items = Some(v);
                }
            }

            DownloadEvent::GroupStarted {
                operation_id: _,
                group_id,
                kind,
                label,
                total_items,
            } => {
                if self.groups.contains_key(&group_id) {
                    return Err(ProgressError::DuplicateGroup(group_id));
                }
                self.groups.insert(
                    group_id,
                    GroupState {
                        id: group_id,
                        operation_id: self.operation.id,
                        kind,
                        label,
                        items_done: 0,
                        total_items,
                    },
                );
            }

            DownloadEvent::GroupTotalsUpdated {
                group_id,
                total_items,
            } => {
                let group = self
                    .groups
                    .get_mut(&group_id)
                    .ok_or(ProgressError::UnknownGroup(group_id))?;
                if let Some(v) = total_items {
                    group.total_items = Some(v);
                }
            }

            DownloadEvent::ItemQueued {
                operation_id: _,
                group_id,
                item_id,
                kind,
                label,
                quality,
            } => {
                if self.items.contains_key(&item_id) {
                    return Err(ProgressError::DuplicateItem(item_id));
                }
                self.items.insert(
                    item_id,
                    ItemState {
                        id: item_id,
                        operation_id: self.operation.id,
                        group_id,
                        kind,
                        label,
                        quality,
                        phase: DownloadPhase::Queued,
                        transfer: None,
                        output_path: None,
                    },
                );
            }

            DownloadEvent::ItemQualityResolved { item_id, quality } => {
                let item = self
                    .items
                    .get_mut(&item_id)
                    .ok_or(ProgressError::UnknownItem(item_id))?;
                item.quality = Some(quality);
            }

            DownloadEvent::ItemPhaseChanged { item_id, phase } => {
                let item = self
                    .items
                    .get_mut(&item_id)
                    .ok_or(ProgressError::UnknownItem(item_id))?;
                item.phase = phase;
                // Clear stale transfer data when leaving a transfer phase.
                if !matches!(item.phase, DownloadPhase::Downloading | DownloadPhase::Copying) {
                    item.transfer = None;
                }
            }

            DownloadEvent::TransferUpdate { item_id, progress } => {
                let item = self
                    .items
                    .get_mut(&item_id)
                    .ok_or(ProgressError::UnknownItem(item_id))?;
                item.transfer = Some(progress);
            }

            DownloadEvent::ArtifactPhaseChanged {
                operation_id,
                group_id,
                item_id,
                kind,
                label,
                phase,
            } => {
                let key = ArtifactKey {
                    operation_id,
                    group_id,
                    item_id,
                    kind,
                };
                self.artifacts
                    .entry(key)
                    .and_modify(|s| {
                        s.phase = phase.clone();
                        s.label.clone_from(&label);
                    })
                    .or_insert_with(|| ArtifactState { label, phase });
            }

            DownloadEvent::ArtifactFinished {
                operation_id,
                group_id,
                item_id,
                kind,
                label,
                outcome,
            } => {
                let key = ArtifactKey {
                    operation_id,
                    group_id,
                    item_id: item_id.clone(),
                    kind: kind.clone(),
                };
                // Remove from active artifacts. It is acceptable for
                // ArtifactFinished to arrive without a prior ArtifactPhaseChanged
                // (e.g. an immediately-skipped artifact).
                self.artifacts.remove(&key);
                self.completed.push(CompletedRecord::Artifact {
                    operation_id,
                    group_id,
                    item_id,
                    kind,
                    label,
                    outcome,
                });
            }

            DownloadEvent::ItemOutputResolved { item_id, path } => {
                let item = self
                    .items
                    .get_mut(&item_id)
                    .ok_or(ProgressError::UnknownItem(item_id))?;
                item.output_path = Some(path);
            }

            DownloadEvent::ItemFinished { item_id, outcome } => {
                let item = self
                    .items
                    .remove(&item_id)
                    .ok_or(ProgressError::UnknownItem(item_id))?;
                // Increment parent group's done counter.
                if let Some(gid) = item.group_id {
                    if let Some(group) = self.groups.get_mut(&gid) {
                        group.items_done += 1;
                    }
                }
                self.completed.push(CompletedRecord::Item {
                    item_id,
                    group_id: item.group_id,
                    label: item.label,
                    quality: item.quality,
                    outcome,
                });
            }

            DownloadEvent::GroupFinished { group_id, outcome } => {
                let group = self
                    .groups
                    .remove(&group_id)
                    .ok_or(ProgressError::UnknownGroup(group_id))?;
                self.completed.push(CompletedRecord::Group {
                    group_id,
                    kind: group.kind,
                    label: group.label,
                    outcome,
                    items_done: group.items_done,
                    items_total: group.total_items,
                });
            }

            DownloadEvent::OperationFinished { id } => {
                if id != self.operation.id {
                    return Err(ProgressError::UnknownOperation(id));
                }
                self.finished = true;
            }
        }
        Ok(())
    }

    /// Derives a [`ProgressSummary`] from the accumulated completed records.
    ///
    /// Call after reducing [`DownloadEvent::OperationFinished`]. The `elapsed`
    /// parameter is measured by the caller (e.g. `Instant::now() - start`).
    pub fn summary(&self, elapsed: Option<time::Duration>) -> ProgressSummary {
        let mut downloaded = 0u64;
        let mut copied = 0u64;
        let mut enriched = 0u64;
        let mut skipped_existing = 0u64;
        let mut failed = 0u64;
        let mut artifacts_failed = 0u64;
        let mut groups_done = 0u64;
        // Map from ArtifactKind -> (label_of_last_seen, count)
        let mut artifact_counts: HashMap<ArtifactKind, (String, u64)> = HashMap::new();

        for record in &self.completed {
            match record {
                CompletedRecord::Item { outcome, .. } => match outcome {
                    ItemOutcome::Downloaded => downloaded += 1,
                    ItemOutcome::Copied => copied += 1,
                    ItemOutcome::Enriched => enriched += 1,
                    ItemOutcome::Exists | ItemOutcome::Skipped { .. } => skipped_existing += 1,
                    ItemOutcome::Failed { .. } => failed += 1,
                },
                CompletedRecord::Artifact {
                    outcome,
                    kind,
                    label,
                    ..
                } => match outcome {
                    ArtifactOutcome::Written => {
                        let entry = artifact_counts
                            .entry(kind.clone())
                            .or_insert_with(|| (label.clone(), 0));
                        entry.0.clone_from(label);
                        entry.1 += 1;
                    }
                    ArtifactOutcome::Failed { .. } => artifacts_failed += 1,
                    ArtifactOutcome::Skipped { .. } => {}
                },
                CompletedRecord::Group { outcome, .. } => {
                    groups_done += 1;
                    if let GroupOutcome::Partial { failed: f } = outcome {
                        failed += f;
                    }
                }
            }
        }

        let artifacts = artifact_counts
            .into_iter()
            .map(|(kind, (label, count))| ArtifactSummary { kind, label, count })
            .collect();

        ProgressSummary {
            downloaded,
            copied,
            enriched,
            skipped_existing,
            failed,
            artifacts_failed,
            groups_done,
            groups_total: self.operation.total_groups,
            items_total: self.operation.total_items,
            elapsed,
            artifacts,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{ItemKind, OperationKind};
    use crate::id::ProgressIdAllocator;

    fn test_config(alloc: &ProgressIdAllocator) -> OperationConfig {
        OperationConfig {
            id: alloc.next_operation_id(),
            kind: OperationKind::Album,
            label: DisplayLabel {
                title: "Images and Words".to_owned(),
                artist: Some("Dream Theater".to_owned()),
                group: None,
                track_number: None,
            },
            total_groups: None,
            total_items: Some(8),
        }
    }

    fn track_label(
        title: &str,
        n: u32,
    ) -> Result<DisplayLabel, Box<dyn std::error::Error>> {
        Ok(DisplayLabel {
            title: title.to_owned(),
            artist: Some("Dream Theater".to_owned()),
            group: Some("Images and Words".to_owned()),
            track_number: Some(riptide_core::TrackNumber::new(n)?),
        })
    }

    #[test]
    fn reducer_counts_downloaded_and_skipped() -> Result<(), Box<dyn std::error::Error>> {
        let alloc = ProgressIdAllocator::new();
        let config = test_config(&alloc);
        let op_id = config.id;
        let mut state = ProgressState::new(config);

        let i1 = alloc.next_item_id();
        let i2 = alloc.next_item_id();

        state.reduce(DownloadEvent::ItemQueued {
            operation_id: op_id,
            group_id: None,
            item_id: i1,
            kind: ItemKind::Track,
            label: track_label("Pull Me Under", 1)?,
            quality: None,
        })?;
        state.reduce(DownloadEvent::ItemQueued {
            operation_id: op_id,
            group_id: None,
            item_id: i2,
            kind: ItemKind::Track,
            label: track_label("Another Day", 2)?,
            quality: None,
        })?;

        state.reduce(DownloadEvent::ItemFinished {
            item_id: i1,
            outcome: ItemOutcome::Downloaded,
        })?;
        state.reduce(DownloadEvent::ItemFinished {
            item_id: i2,
            outcome: ItemOutcome::Exists,
        })?;
        state.reduce(DownloadEvent::OperationFinished { id: op_id })?;

        let summary = state.summary(None);
        assert_eq!(summary.downloaded, 1);
        assert_eq!(summary.skipped_existing, 1);
        assert_eq!(summary.failed, 0);
        Ok(())
    }

    #[test]
    fn reducer_rejects_duplicate_item_id() -> Result<(), Box<dyn std::error::Error>> {
        let alloc = ProgressIdAllocator::new();
        let config = test_config(&alloc);
        let op_id = config.id;
        let mut state = ProgressState::new(config);
        let item_id = alloc.next_item_id();

        state.reduce(DownloadEvent::ItemQueued {
            operation_id: op_id,
            group_id: None,
            item_id,
            kind: ItemKind::Track,
            label: track_label("Pull Me Under", 1)?,
            quality: None,
        })?;

        let err = state
            .reduce(DownloadEvent::ItemQueued {
                operation_id: op_id,
                group_id: None,
                item_id, // same ID
                kind: ItemKind::Track,
                label: track_label("Another Day", 2)?,
                quality: None,
            })
            .err()
            .ok_or("expected error but got Ok")?;
        assert!(matches!(err, ProgressError::DuplicateItem(_)));
        Ok(())
    }

    #[test]
    fn reducer_rejects_unknown_item_phase() -> Result<(), Box<dyn std::error::Error>> {
        let alloc = ProgressIdAllocator::new();
        let config = test_config(&alloc);
        let mut state = ProgressState::new(config);
        let unknown_id = alloc.next_item_id();

        let err = state
            .reduce(DownloadEvent::ItemPhaseChanged {
                item_id: unknown_id,
                phase: DownloadPhase::Downloading,
            })
            .err()
            .ok_or("expected error but got Ok")?;
        assert!(matches!(err, ProgressError::UnknownItem(_)));
        Ok(())
    }

    #[test]
    fn late_totals_update_state_for_none_means_no_change() -> Result<(), Box<dyn std::error::Error>>
    {
        let alloc = ProgressIdAllocator::new();
        let config = OperationConfig {
            id: alloc.next_operation_id(),
            kind: OperationKind::Album,
            label: DisplayLabel {
                title: "Test".to_owned(),
                artist: None,
                group: None,
                track_number: None,
            },
            total_groups: None,
            total_items: Some(3),
        };
        let op_id = config.id;
        let mut state = ProgressState::new(config);

        // None → no change, Some(5) → update
        state.reduce(DownloadEvent::OperationTotalsUpdated {
            id: op_id,
            total_groups: None,
            total_items: Some(5),
        })?;

        assert_eq!(state.operation.total_items, Some(5));
        assert_eq!(state.operation.total_groups, None); // unchanged
        Ok(())
    }

    #[test]
    fn quality_resolved_updates_item() -> Result<(), Box<dyn std::error::Error>> {
        let alloc = ProgressIdAllocator::new();
        let config = test_config(&alloc);
        let op_id = config.id;
        let mut state = ProgressState::new(config);
        let item_id = alloc.next_item_id();

        state.reduce(DownloadEvent::ItemQueued {
            operation_id: op_id,
            group_id: None,
            item_id,
            kind: ItemKind::Track,
            label: track_label("Pull Me Under", 1)?,
            quality: None,
        })?;

        assert!(state.items[&item_id].quality.is_none());

        state.reduce(DownloadEvent::ItemQualityResolved {
            item_id,
            quality: QualityBadge {
                symbol_kind: crate::event::QualitySymbol::Lossless,
                text: "Lossless".to_owned(),
                detail: None,
            },
        })?;

        assert!(state.items[&item_id].quality.is_some());
        Ok(())
    }

    #[test]
    fn artifact_finished_moves_to_completed() -> Result<(), Box<dyn std::error::Error>> {
        let alloc = ProgressIdAllocator::new();
        let config = test_config(&alloc);
        let op_id = config.id;
        let mut state = ProgressState::new(config);

        // ArtifactFinished without prior ArtifactPhaseChanged is valid (skipped artifact).
        state.reduce(DownloadEvent::ArtifactFinished {
            operation_id: op_id,
            group_id: None,
            item_id: None,
            kind: ArtifactKind::Cover,
            label: "cover.jpg".to_owned(),
            outcome: ArtifactOutcome::Written,
        })?;

        assert!(state.artifacts.is_empty());
        assert_eq!(state.completed.len(), 1);

        let summary = state.summary(None);
        assert_eq!(summary.artifacts.len(), 1);
        assert_eq!(summary.artifacts[0].count, 1);
        Ok(())
    }

    #[test]
    fn completion_rows_do_not_include_output_path_by_default(
    ) -> Result<(), Box<dyn std::error::Error>> {
        // The output_path in ItemState is populated by ItemOutputResolved but
        // row builders only surface it under --progress-detail path|both.
        // This test verifies the field exists and is separately managed.
        let alloc = ProgressIdAllocator::new();
        let config = test_config(&alloc);
        let op_id = config.id;
        let mut state = ProgressState::new(config);
        let item_id = alloc.next_item_id();

        state.reduce(DownloadEvent::ItemQueued {
            operation_id: op_id,
            group_id: None,
            item_id,
            kind: ItemKind::Track,
            label: track_label("Pull Me Under", 1)?,
            quality: None,
        })?;

        state.reduce(DownloadEvent::ItemOutputResolved {
            item_id,
            path: std::path::PathBuf::from("/music/track.flac"),
        })?;

        // Path is in ItemState but should not appear in the label field.
        let item = &state.items[&item_id];
        assert_eq!(
            item.output_path.as_deref(),
            Some(std::path::Path::new("/music/track.flac"))
        );
        // The label title is unaffected.
        assert_eq!(item.label.title, "Pull Me Under");
        Ok(())
    }
}
