use std::collections::BTreeMap;

use super::{DisplayRegion, RenderedScene};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SceneMode {
    Full,
    Delta,
}

#[derive(Debug)]
pub(crate) struct SceneUpdate {
    pub new_revision: u32,
    pub base_revision: u32,
    pub mode: SceneMode,
    pub regions: Vec<DisplayRegion>,
}

#[derive(Default)]
pub(crate) struct SceneTracker {
    acked: Option<AckedScene>,
    pending: Option<PendingScene>,
    desired: Option<RenderedScene>,
    next_revision: u32,
}

struct PendingScene {
    revision: u32,
    scene: RenderedScene,
}

struct AckedScene {
    revision: u32,
    scene: RenderedScene,
}

impl SceneTracker {
    pub(crate) fn prepare(&mut self, scene: RenderedScene) -> Option<SceneUpdate> {
        self.desired = Some(scene);
        if self.pending.is_some() {
            return None;
        }
        self.emit_desired()
    }

    pub(crate) fn ack(&mut self, revision: u32) -> Result<Option<SceneUpdate>, &'static str> {
        let Some(pending) = self.pending.take() else {
            self.acked = None;
            return Err("display_scene_ack_mismatch");
        };
        if pending.revision != revision {
            self.acked = None;
            return Err("display_scene_ack_mismatch");
        }
        self.acked = Some(AckedScene {
            revision,
            scene: pending.scene,
        });
        Ok(self.emit_desired())
    }

    pub(crate) fn resync(&mut self) -> Option<SceneUpdate> {
        self.acked = None;
        self.pending = None;
        self.emit_desired()
    }

    fn emit_desired(&mut self) -> Option<SceneUpdate> {
        let desired = self.desired.clone()?;
        let force_full = self.next_revision == u32::MAX;
        let (mode, base_revision, regions) = match &self.acked {
            _ if force_full => (SceneMode::Full, 0, desired.regions.clone()),
            None => (SceneMode::Full, 0, desired.regions.clone()),
            Some(acked) if acked.scene == desired => return None,
            Some(acked) => (
                SceneMode::Delta,
                acked.revision,
                changed_regions(&acked.scene, &desired),
            ),
        };
        let new_revision = self.next_revision();
        self.pending = Some(PendingScene {
            revision: new_revision,
            scene: desired,
        });
        Some(SceneUpdate {
            new_revision,
            base_revision,
            mode,
            regions,
        })
    }

    fn next_revision(&mut self) -> u32 {
        if self.next_revision == 0 {
            self.next_revision = 1;
        }
        if self.next_revision == u32::MAX {
            self.next_revision = 2;
            return 1;
        }
        let revision = self.next_revision;
        self.next_revision += 1;
        revision
    }
}

fn changed_regions(acked: &RenderedScene, desired: &RenderedScene) -> Vec<DisplayRegion> {
    let acked_by_slot: BTreeMap<u8, &DisplayRegion> = acked
        .regions
        .iter()
        .map(|region| (region.slot, region))
        .collect();
    let desired_by_slot: BTreeMap<u8, &DisplayRegion> = desired
        .regions
        .iter()
        .map(|region| (region.slot, region))
        .collect();
    let mut changed = Vec::new();

    for (slot, region) in &desired_by_slot {
        if acked_by_slot
            .get(slot)
            .is_none_or(|previous| previous.content_hash != region.content_hash)
        {
            changed.push((*region).clone());
        }
    }
    for (slot, region) in &acked_by_slot {
        if !desired_by_slot.contains_key(slot) {
            changed.push(DisplayRegion::clear(region.slot, region.id, region.bounds));
        }
    }
    changed.sort_by_key(|region| region.slot);
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::display::{DisplayRegion, DrawOperation, Rect};

    fn scene(text: &str) -> RenderedScene {
        RenderedScene {
            regions: vec![DisplayRegion::new(
                0,
                "row0_left",
                Rect::new(0, 0, 64, 16),
                vec![
                    DrawOperation::ClearRegion,
                    DrawOperation::Text {
                        x: 0,
                        baseline_y: 12,
                        font_id: 0,
                        text: text.into(),
                    },
                ],
            )],
        }
    }

    #[test]
    fn unchanged_scene_emits_nothing_after_ack() {
        let mut tracker = SceneTracker::default();
        let update = tracker.prepare(scene("CODEX")).unwrap();
        tracker.ack(update.new_revision).unwrap();

        assert!(tracker.prepare(scene("CODEX")).is_none());
    }

    #[test]
    fn removed_slot_emits_a_clear_with_old_bounds() {
        let mut tracker = SceneTracker::default();
        let first = RenderedScene {
            regions: vec![
                scene("CODEX").regions.pop().unwrap(),
                DisplayRegion::new(
                    1,
                    "row0_right",
                    Rect::new(64, 0, 64, 16),
                    vec![DrawOperation::ClearRegion],
                ),
            ],
        };
        let update = tracker.prepare(first).unwrap();
        tracker.ack(update.new_revision).unwrap();

        let delta = tracker.prepare(scene("CODEX")).unwrap();

        assert_eq!(delta.mode, SceneMode::Delta);
        assert_eq!(delta.regions.len(), 1);
        assert_eq!(delta.regions[0].slot, 1);
        assert_eq!(delta.regions[0].bounds, Rect::new(64, 0, 64, 16));
        assert_eq!(
            delta.regions[0].operations,
            vec![DrawOperation::ClearRegion]
        );
    }

    #[test]
    fn pending_updates_coalesce_to_the_latest_desired_scene() {
        let mut tracker = SceneTracker::default();
        let first = tracker.prepare(scene("ONE")).unwrap();
        assert!(tracker.prepare(scene("TWO")).is_none());
        assert!(tracker.prepare(scene("THREE")).is_none());

        let next = tracker.ack(first.new_revision).unwrap().unwrap();

        assert_eq!(next.mode, SceneMode::Delta);
        assert_eq!(
            next.regions[0].operations.last(),
            Some(&DrawOperation::Text {
                x: 0,
                baseline_y: 12,
                font_id: 0,
                text: "THREE".into(),
            })
        );
    }

    #[test]
    fn delta_uses_the_last_acknowledged_revision_as_its_base() {
        let mut tracker = SceneTracker::default();
        let first = tracker.prepare(scene("ONE")).unwrap();
        tracker.ack(first.new_revision).unwrap();
        let second = tracker.prepare(scene("TWO")).unwrap();
        tracker.ack(second.new_revision).unwrap();

        let third = tracker.prepare(scene("THREE")).unwrap();

        assert_eq!(third.base_revision, second.new_revision);
    }

    #[test]
    fn mismatched_ack_discards_protocol_state_and_recovers_with_full_scene() {
        let mut tracker = SceneTracker::default();
        let first = tracker.prepare(scene("ONE")).unwrap();
        tracker.ack(first.new_revision).unwrap();
        let pending = tracker.prepare(scene("TWO")).unwrap();
        assert!(tracker.prepare(scene("THREE")).is_none());

        assert_eq!(
            tracker.ack(pending.new_revision + 1).unwrap_err(),
            "display_scene_ack_mismatch"
        );
        let recovered = tracker.prepare(scene("THREE")).unwrap();
        assert_eq!(recovered.mode, SceneMode::Full);
        assert_eq!(recovered.base_revision, 0);
        assert_eq!(recovered.regions, scene("THREE").regions);
    }

    #[test]
    fn duplicate_ack_without_pending_discards_the_acknowledged_base() {
        let mut tracker = SceneTracker::default();
        let first = tracker.prepare(scene("ONE")).unwrap();
        tracker.ack(first.new_revision).unwrap();

        assert_eq!(
            tracker.ack(first.new_revision).unwrap_err(),
            "display_scene_ack_mismatch"
        );
        let recovered = tracker.prepare(scene("TWO")).unwrap();

        assert_eq!(recovered.mode, SceneMode::Full);
        assert_eq!(recovered.base_revision, 0);
    }

    #[test]
    fn resync_forces_the_latest_desired_scene_to_full() {
        let mut tracker = SceneTracker::default();
        let first = tracker.prepare(scene("CODEX")).unwrap();
        tracker.ack(first.new_revision).unwrap();

        let resync = tracker.resync().unwrap();

        assert_eq!(resync.mode, SceneMode::Full);
        assert_eq!(resync.base_revision, 0);
        assert_eq!(resync.regions, scene("CODEX").regions);
    }

    #[test]
    fn revision_wrap_forces_full_revision_one() {
        let mut tracker = SceneTracker::default();
        let first = tracker.prepare(scene("CODEX")).unwrap();
        tracker.ack(first.new_revision).unwrap();
        tracker.next_revision = u32::MAX;

        let wrapped = tracker.prepare(scene("KIVO")).unwrap();

        assert_eq!(wrapped.mode, SceneMode::Full);
        assert_eq!(wrapped.base_revision, 0);
        assert_eq!(wrapped.new_revision, 1);
    }
}
