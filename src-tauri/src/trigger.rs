use crate::{
    coordinator::RuntimeEventContext,
    device::RuntimeProfileSnapshot,
    profile::ActionTrigger,
    protocol::{InputState, PhysicalInput},
};
use std::{collections::BTreeMap, sync::Arc};

#[derive(Clone, Debug)]
pub(crate) struct TriggerEdge {
    pub(crate) input: PhysicalInput,
    pub(crate) state: InputState,
    pub(crate) now_ms: u64,
    pub(crate) snapshot: Arc<RuntimeProfileSnapshot>,
    pub(crate) context: Option<RuntimeEventContext>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TriggerOccurrence {
    pub(crate) sequence: u64,
    pub(crate) input: PhysicalInput,
    pub(crate) trigger: ActionTrigger,
    pub(crate) origin_down_ms: u64,
    pub(crate) snapshot: Arc<RuntimeProfileSnapshot>,
    pub(crate) context: Option<RuntimeEventContext>,
}

#[derive(Clone, Debug)]
struct DownOrigin {
    down_ms: u64,
    deadline_ms: u64,
    snapshot: Arc<RuntimeProfileSnapshot>,
    context: Option<RuntimeEventContext>,
    long_fired: bool,
}

#[derive(Clone, Copy, Debug)]
struct CompletedPress {
    released_ms: u64,
}

#[derive(Default)]
struct InputTracker {
    down: Option<DownOrigin>,
    previous_short_press: Option<CompletedPress>,
}

#[derive(Default)]
pub(crate) struct TriggerTracker {
    inputs: BTreeMap<PhysicalInput, InputTracker>,
    next_sequence: u64,
    latest_now_ms: Option<u64>,
}

impl TriggerTracker {
    pub(crate) fn edge(&mut self, edge: TriggerEdge) -> Vec<TriggerOccurrence> {
        self.latest_now_ms = Some(edge.now_ms);
        match edge.state {
            InputState::Down => self.down(edge),
            InputState::Up => self.up(edge),
        }
    }

    pub(crate) fn poll(&mut self, now_ms: u64) -> Vec<TriggerOccurrence> {
        self.latest_now_ms = Some(now_ms);
        let due = self
            .inputs
            .iter_mut()
            .filter_map(|(input, state)| {
                let down = state.down.as_mut()?;
                if down.long_fired || !deadline_reached(now_ms, down.deadline_ms) {
                    return None;
                }
                down.long_fired = true;
                state.previous_short_press = None;
                Some((*input, down.clone()))
            })
            .collect::<Vec<_>>();

        due.into_iter()
            .map(|(input, down)| self.occurrence(input, ActionTrigger::LongPress, &down))
            .collect()
    }

    pub(crate) fn next_deadline_ms(&self) -> Option<u64> {
        let now_ms = self.latest_now_ms?;
        self.inputs
            .iter()
            .filter_map(|(input, state)| {
                state
                    .down
                    .as_ref()
                    .filter(|down| !down.long_fired)
                    .map(|down| (*input, down.deadline_ms))
            })
            .min_by_key(|(input, deadline_ms)| (deadline_delay(now_ms, *deadline_ms), *input))
            .map(|(_, deadline_ms)| deadline_ms)
    }

    pub(crate) fn reset(&mut self) {
        self.inputs.clear();
        self.latest_now_ms = None;
    }

    fn down(&mut self, edge: TriggerEdge) -> Vec<TriggerOccurrence> {
        let input = edge.input;
        let (down, is_double_press) = {
            let state = self.inputs.entry(input).or_default();
            if state.down.is_some() {
                return Vec::new();
            }

            let is_double_press = state.previous_short_press.is_some_and(|previous| {
                edge.now_ms.wrapping_sub(previous.released_ms)
                    <= u64::from(edge.snapshot.profile.trigger_settings.double_press_ms)
            });
            state.previous_short_press = None;

            let down = DownOrigin {
                down_ms: edge.now_ms,
                deadline_ms: edge.now_ms.wrapping_add(u64::from(
                    edge.snapshot.profile.trigger_settings.long_press_ms,
                )),
                snapshot: edge.snapshot,
                context: edge.context,
                long_fired: false,
            };
            state.down = Some(down.clone());
            (down, is_double_press)
        };

        let mut occurrences = vec![self.occurrence(input, ActionTrigger::Press, &down)];
        if is_double_press {
            occurrences.push(self.occurrence(input, ActionTrigger::DoublePress, &down));
        }
        occurrences
    }

    fn up(&mut self, edge: TriggerEdge) -> Vec<TriggerOccurrence> {
        let input = edge.input;
        let down = {
            let state = self.inputs.entry(input).or_default();
            let Some(down) = state.down.take() else {
                return Vec::new();
            };
            state.previous_short_press = (!down.long_fired).then_some(CompletedPress {
                released_ms: edge.now_ms,
            });
            down
        };

        vec![self.occurrence(input, ActionTrigger::Release, &down)]
    }

    fn occurrence(
        &mut self,
        input: PhysicalInput,
        trigger: ActionTrigger,
        down: &DownOrigin,
    ) -> TriggerOccurrence {
        self.next_sequence = self
            .next_sequence
            .checked_add(1)
            .expect("trigger occurrence sequence exhausted");
        TriggerOccurrence {
            sequence: self.next_sequence,
            input,
            trigger,
            origin_down_ms: down.down_ms,
            snapshot: Arc::clone(&down.snapshot),
            context: down.context.clone(),
        }
    }
}

fn deadline_reached(now_ms: u64, deadline_ms: u64) -> bool {
    now_ms.wrapping_sub(deadline_ms) < (1_u64 << 63)
}

fn deadline_delay(now_ms: u64, deadline_ms: u64) -> u64 {
    if deadline_reached(now_ms, deadline_ms) {
        0
    } else {
        deadline_ms.wrapping_sub(now_ms)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        coordinator::RuntimeEventContext,
        device::RuntimeProfileSnapshot,
        hardware::{DeviceId, LUATOS_ESP32S3_AIO_BOARD_ID},
        metrics::MetricAttribution,
        profile::{ActionTrigger, TriggerSettings, blank_device_profile},
        protocol::{InputState, PhysicalInput},
    };
    use std::sync::Arc;

    fn direct(gpio: u8) -> PhysicalInput {
        PhysicalInput::Direct { gpio }
    }

    fn snapshot(long_press_ms: u32, double_press_ms: u32) -> Arc<RuntimeProfileSnapshot> {
        let mut profile = blank_device_profile(
            "profile".into(),
            "Profile".into(),
            LUATOS_ESP32S3_AIO_BOARD_ID.into(),
        );
        profile.trigger_settings = TriggerSettings {
            long_press_ms,
            double_press_ms,
        };
        Arc::new(RuntimeProfileSnapshot {
            profile,
            hardware_profile_id: "hardware".into(),
            metric_attribution: MetricAttribution {
                device_id: DeviceId::new(LUATOS_ESP32S3_AIO_BOARD_ID, "ABCDEF123456").unwrap(),
                device_name: "Desk".into(),
                device_profile_id: "profile".into(),
                hardware_profile_id: "hardware".into(),
            },
        })
    }

    fn edge(
        input: PhysicalInput,
        state: InputState,
        now_ms: u64,
        snapshot: Arc<RuntimeProfileSnapshot>,
    ) -> TriggerEdge {
        TriggerEdge {
            input,
            state,
            now_ms,
            snapshot,
            context: None,
        }
    }

    fn edge_with_context(
        input: PhysicalInput,
        state: InputState,
        now_ms: u64,
        snapshot: Arc<RuntimeProfileSnapshot>,
        context: RuntimeEventContext,
    ) -> TriggerEdge {
        TriggerEdge {
            context: Some(context),
            ..edge(input, state, now_ms, snapshot)
        }
    }

    fn occurrence(trigger: ActionTrigger, origin_down_ms: u64) -> (ActionTrigger, u64) {
        (trigger, origin_down_ms)
    }

    fn occurrences(items: &[TriggerOccurrence]) -> Vec<(ActionTrigger, u64)> {
        items
            .iter()
            .map(|item| occurrence(item.trigger, item.origin_down_ms))
            .collect()
    }

    #[test]
    fn long_hold_emits_press_long_press_then_release() {
        let input = PhysicalInput::Direct { gpio: 6 };
        let snapshot = snapshot(500, 300);
        let mut tracker = TriggerTracker::default();
        assert_eq!(
            occurrences(&tracker.edge(edge(input, InputState::Down, 10, snapshot.clone()))),
            vec![occurrence(ActionTrigger::Press, 10)]
        );
        assert_eq!(tracker.poll(509), vec![]);
        assert_eq!(
            occurrences(&tracker.poll(510)),
            vec![occurrence(ActionTrigger::LongPress, 10)]
        );
        assert_eq!(tracker.poll(900), vec![]);
        assert_eq!(
            occurrences(&tracker.edge(edge(input, InputState::Up, 901, snapshot))),
            vec![occurrence(ActionTrigger::Release, 10)]
        );
    }

    #[test]
    fn complete_second_down_emits_press_before_double_press() {
        let mut tracker = TriggerTracker::default();
        let snapshot = snapshot(500, 300);
        tracker.edge(edge(direct(6), InputState::Down, 0, snapshot.clone()));
        tracker.edge(edge(direct(6), InputState::Up, 40, snapshot.clone()));
        let result = tracker.edge(edge(direct(6), InputState::Down, 200, snapshot));
        assert_eq!(
            result.iter().map(|item| item.trigger).collect::<Vec<_>>(),
            vec![ActionTrigger::Press, ActionTrigger::DoublePress]
        );
    }

    #[test]
    fn second_press_long_hold_emits_double_press_then_long_press() {
        let mut tracker = TriggerTracker::default();
        let snapshot = snapshot(500, 300);
        tracker.edge(edge(direct(6), InputState::Down, 0, snapshot.clone()));
        tracker.edge(edge(direct(6), InputState::Up, 40, snapshot.clone()));
        assert_eq!(
            occurrences(&tracker.edge(edge(direct(6), InputState::Down, 200, snapshot.clone()))),
            vec![
                occurrence(ActionTrigger::Press, 200),
                occurrence(ActionTrigger::DoublePress, 200),
            ]
        );
        assert_eq!(
            occurrences(&tracker.poll(700)),
            vec![occurrence(ActionTrigger::LongPress, 200)]
        );
        assert_eq!(
            occurrences(&tracker.edge(edge(direct(6), InputState::Up, 701, snapshot))),
            vec![occurrence(ActionTrigger::Release, 200)]
        );
    }

    #[test]
    fn long_press_invalidates_a_double_press_candidate() {
        let mut tracker = TriggerTracker::default();
        let snapshot = snapshot(500, 300);
        tracker.edge(edge(direct(6), InputState::Down, 0, snapshot.clone()));
        tracker.poll(500);
        tracker.edge(edge(direct(6), InputState::Up, 510, snapshot.clone()));
        assert_eq!(
            occurrences(&tracker.edge(edge(direct(6), InputState::Down, 600, snapshot))),
            vec![occurrence(ActionTrigger::Press, 600)]
        );
    }

    #[test]
    fn release_cancels_an_unfired_long_press() {
        let mut tracker = TriggerTracker::default();
        let snapshot = snapshot(500, 300);
        tracker.edge(edge(direct(6), InputState::Down, 10, snapshot.clone()));
        assert_eq!(tracker.next_deadline_ms(), Some(510));
        tracker.edge(edge(direct(6), InputState::Up, 100, snapshot));
        assert_eq!(tracker.next_deadline_ms(), None);
        assert_eq!(tracker.poll(510), vec![]);
    }

    #[test]
    fn duplicate_edges_do_not_create_occurrences() {
        let mut tracker = TriggerTracker::default();
        let snapshot = snapshot(500, 300);
        assert_eq!(
            occurrences(&tracker.edge(edge(direct(6), InputState::Down, 10, snapshot.clone()))),
            vec![occurrence(ActionTrigger::Press, 10)]
        );
        assert_eq!(
            tracker.edge(edge(direct(6), InputState::Down, 11, snapshot.clone())),
            vec![]
        );
        assert_eq!(
            occurrences(&tracker.edge(edge(direct(6), InputState::Up, 20, snapshot.clone()))),
            vec![occurrence(ActionTrigger::Release, 10)]
        );
        assert_eq!(
            tracker.edge(edge(direct(6), InputState::Up, 21, snapshot)),
            vec![]
        );
    }

    #[test]
    fn simultaneous_inputs_have_independent_state_and_deadlines() {
        let mut tracker = TriggerTracker::default();
        let snapshot = snapshot(500, 300);
        tracker.edge(edge(direct(6), InputState::Down, 10, snapshot.clone()));
        tracker.edge(edge(direct(7), InputState::Down, 12, snapshot.clone()));
        tracker.edge(edge(direct(6), InputState::Up, 20, snapshot));
        assert_eq!(tracker.next_deadline_ms(), Some(512));
        assert_eq!(
            occurrences(&tracker.poll(512)),
            vec![occurrence(ActionTrigger::LongPress, 12)]
        );
    }

    #[test]
    fn reset_clears_held_inputs_history_and_deadlines() {
        let mut tracker = TriggerTracker::default();
        let snapshot = snapshot(500, 300);
        tracker.edge(edge(direct(6), InputState::Down, 0, snapshot.clone()));
        tracker.edge(edge(direct(6), InputState::Up, 40, snapshot.clone()));
        tracker.reset();
        assert_eq!(tracker.next_deadline_ms(), None);
        assert_eq!(tracker.poll(500), vec![]);
        assert_eq!(
            occurrences(&tracker.edge(edge(direct(6), InputState::Down, 200, snapshot.clone()))),
            vec![occurrence(ActionTrigger::Press, 200)]
        );
        tracker.reset();
        assert_eq!(
            tracker.edge(edge(direct(6), InputState::Up, 201, snapshot)),
            vec![]
        );
    }

    #[test]
    fn gesture_occurrences_keep_the_originating_down_snapshot_and_context() {
        let mut tracker = TriggerTracker::default();
        let first_snapshot = snapshot(500, 300);
        let second_snapshot = snapshot(700, 300);
        let first_context = RuntimeEventContext::unassigned(10);
        let second_context = RuntimeEventContext::unassigned(200);

        tracker.edge(edge_with_context(
            direct(6),
            InputState::Down,
            10,
            first_snapshot.clone(),
            first_context.clone(),
        ));
        let long_press = tracker.poll(510).pop().unwrap();
        assert!(Arc::ptr_eq(&long_press.snapshot, &first_snapshot));
        assert_eq!(long_press.context, Some(first_context));
        tracker.edge(edge(
            direct(6),
            InputState::Up,
            520,
            second_snapshot.clone(),
        ));

        tracker.edge(edge(direct(7), InputState::Down, 0, first_snapshot.clone()));
        tracker.edge(edge(direct(7), InputState::Up, 40, first_snapshot));
        let second_down = tracker.edge(edge_with_context(
            direct(7),
            InputState::Down,
            200,
            second_snapshot.clone(),
            second_context.clone(),
        ));
        assert!(Arc::ptr_eq(&second_down[1].snapshot, &second_snapshot));
        assert_eq!(second_down[1].context, Some(second_context));
    }

    #[test]
    fn equal_deadlines_use_physical_input_order() {
        let mut tracker = TriggerTracker::default();
        let snapshot = snapshot(500, 300);
        tracker.edge(edge(direct(9), InputState::Down, 10, snapshot.clone()));
        tracker.edge(edge(direct(3), InputState::Down, 10, snapshot));
        let result = tracker.poll(510);
        assert_eq!(
            result.iter().map(|item| item.input).collect::<Vec<_>>(),
            vec![direct(3), direct(9)]
        );
        assert_eq!(
            result.iter().map(|item| item.sequence).collect::<Vec<_>>(),
            vec![3, 4]
        );
    }

    #[test]
    fn long_press_deadlines_are_rollover_safe() {
        let mut tracker = TriggerTracker::default();
        let snapshot = snapshot(10, 300);
        let down_ms = u64::MAX - 4;
        tracker.edge(edge(direct(6), InputState::Down, down_ms, snapshot));
        assert_eq!(tracker.next_deadline_ms(), Some(5));
        assert_eq!(tracker.poll(4), vec![]);
        assert_eq!(
            occurrences(&tracker.poll(5)),
            vec![occurrence(ActionTrigger::LongPress, down_ms)]
        );
    }

    #[test]
    fn occurrence_sequences_are_nonzero_monotonic_and_deterministic() {
        let snapshot = snapshot(500, 300);
        let mut tracker = TriggerTracker::default();
        let mut occurrences = tracker.edge(edge(direct(6), InputState::Down, 0, snapshot.clone()));
        occurrences.extend(tracker.edge(edge(direct(6), InputState::Up, 40, snapshot.clone())));
        occurrences.extend(tracker.edge(edge(direct(6), InputState::Down, 200, snapshot.clone())));
        assert_eq!(
            occurrences
                .iter()
                .map(|item| item.sequence)
                .collect::<Vec<_>>(),
            vec![1, 2, 3, 4]
        );
        tracker.reset();
        assert_eq!(
            tracker.edge(edge(direct(7), InputState::Down, 300, snapshot))[0].sequence,
            5
        );
    }
}
