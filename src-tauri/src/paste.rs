use crate::hardware::DeviceId;
use std::{
    collections::{BTreeMap, VecDeque},
    io::Write,
    process::{Command, Stdio},
    sync::{Mutex, mpsc},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

pub const ACTION_TIMEOUT: Duration = Duration::from_millis(1800);

pub trait ClipboardWriter: Send + 'static {
    fn write(&self, text: &str) -> Result<(), String>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PasteReply {
    Granted,
    TimedOut,
    Cancelled,
    ClipboardError(String),
}

#[derive(Debug)]
pub struct PasteRequest {
    pub receive_sequence: u64,
    pub device_id: DeviceId,
    pub event_id: u64,
    pub step: u16,
    pub text: String,
    pub reply: mpsc::Sender<PasteReply>,
}

#[derive(Clone)]
pub struct PasteHandle {
    sender: mpsc::Sender<PasteMessage>,
}

pub struct PasteCoordinator {
    handle: PasteHandle,
    join: Mutex<Option<JoinHandle<()>>>,
}

impl PasteCoordinator {
    pub fn system() -> Self {
        Self::with_timeout(SystemClipboard, ACTION_TIMEOUT)
    }

    pub fn with_timeout(clipboard: impl ClipboardWriter, timeout: Duration) -> Self {
        let (sender, receiver) = mpsc::channel();
        let join = thread::spawn(move || run_paste_loop(receiver, clipboard, timeout));
        Self {
            handle: PasteHandle { sender },
            join: Mutex::new(Some(join)),
        }
    }

    pub fn handle(&self) -> PasteHandle {
        self.handle.clone()
    }

    pub fn shutdown(&self) {
        let Some(join) = self
            .join
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take()
        else {
            return;
        };
        let (reply, result) = mpsc::channel();
        let _ = self.handle.sender.send(PasteMessage::Shutdown { reply });
        let _ = result.recv();
        let _ = join.join();
    }
}

impl Drop for PasteCoordinator {
    fn drop(&mut self) {
        self.shutdown();
    }
}

impl PasteHandle {
    pub fn register_sequence(&self, receive_sequence: u64) -> Result<(), String> {
        self.request(|reply| PasteMessage::Register {
            receive_sequence,
            reply,
        })
    }

    pub fn submit(&self, request: PasteRequest) -> Result<(), String> {
        self.request(|reply| PasteMessage::Submit { request, reply })
    }

    pub fn complete(&self, device_id: &DeviceId, event_id: u64, step: u16) -> Result<(), String> {
        self.request(|reply| PasteMessage::Complete {
            device_id: device_id.clone(),
            event_id,
            step,
            reply,
        })
    }

    pub fn finish_sequence(&self, receive_sequence: u64) -> Result<(), String> {
        self.request(|reply| PasteMessage::FinishSequence {
            receive_sequence,
            reply,
        })
    }

    pub fn cancel_device(&self, device_id: &DeviceId) -> Result<(), String> {
        self.request(|reply| PasteMessage::CancelDevice {
            device_id: device_id.clone(),
            reply,
        })
    }

    fn request(
        &self,
        message: impl FnOnce(mpsc::Sender<Result<(), String>>) -> PasteMessage,
    ) -> Result<(), String> {
        let (reply, result) = mpsc::channel();
        self.sender
            .send(message(reply))
            .map_err(|_| "paste_coordinator_stopped".to_owned())?;
        result
            .recv()
            .map_err(|_| "paste_coordinator_stopped".to_owned())?
    }
}

enum PasteMessage {
    Register {
        receive_sequence: u64,
        reply: mpsc::Sender<Result<(), String>>,
    },
    Submit {
        request: PasteRequest,
        reply: mpsc::Sender<Result<(), String>>,
    },
    Complete {
        device_id: DeviceId,
        event_id: u64,
        step: u16,
        reply: mpsc::Sender<Result<(), String>>,
    },
    FinishSequence {
        receive_sequence: u64,
        reply: mpsc::Sender<Result<(), String>>,
    },
    CancelDevice {
        device_id: DeviceId,
        reply: mpsc::Sender<Result<(), String>>,
    },
    Shutdown {
        reply: mpsc::Sender<()>,
    },
}

#[derive(Default)]
struct SequenceQueue {
    requests: VecDeque<PasteRequest>,
    finished: bool,
    device_id: Option<DeviceId>,
}

struct ActivePaste {
    request: PasteRequest,
    deadline: Instant,
}

fn run_paste_loop(
    receiver: mpsc::Receiver<PasteMessage>,
    clipboard: impl ClipboardWriter,
    timeout: Duration,
) {
    let mut sequences = BTreeMap::<u64, SequenceQueue>::new();
    let mut active: Option<ActivePaste> = None;
    loop {
        let message = match active.as_ref() {
            Some(current) => match receiver
                .recv_timeout(current.deadline.saturating_duration_since(Instant::now()))
            {
                Ok(message) => Some(message),
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    let timed_out = active.take().expect("active paste exists");
                    let sequence = timed_out.request.receive_sequence;
                    let _ = timed_out.request.reply.send(PasteReply::TimedOut);
                    cancel_sequence(&mut sequences, sequence);
                    start_next(&mut sequences, &mut active, &clipboard, timeout);
                    None
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            },
            None => match receiver.recv() {
                Ok(message) => Some(message),
                Err(_) => break,
            },
        };
        let Some(message) = message else {
            continue;
        };
        match message {
            PasteMessage::Register {
                receive_sequence,
                reply,
            } => {
                let result = if receive_sequence == 0 || sequences.contains_key(&receive_sequence) {
                    Err("invalid_receive_sequence".into())
                } else {
                    sequences.insert(receive_sequence, SequenceQueue::default());
                    Ok(())
                };
                let _ = reply.send(result);
            }
            PasteMessage::Submit { request, reply } => {
                let result = sequences
                    .get_mut(&request.receive_sequence)
                    .ok_or_else(|| "unregistered_receive_sequence".to_owned())
                    .and_then(|sequence| {
                        if sequence.finished {
                            Err("finished_receive_sequence".into())
                        } else if sequence
                            .device_id
                            .as_ref()
                            .is_some_and(|device_id| device_id != &request.device_id)
                        {
                            Err("receive_sequence_device_mismatch".into())
                        } else {
                            sequence.device_id = Some(request.device_id.clone());
                            sequence.requests.push_back(request);
                            Ok(())
                        }
                    });
                let _ = reply.send(result);
            }
            PasteMessage::Complete {
                device_id,
                event_id,
                step,
                reply,
            } => {
                let matches = active.as_ref().is_some_and(|active| {
                    active.request.device_id == device_id
                        && active.request.event_id == event_id
                        && active.request.step == step
                });
                let result = if matches {
                    active = None;
                    Ok(())
                } else {
                    Err("paste_completion_mismatch".into())
                };
                let _ = reply.send(result);
            }
            PasteMessage::FinishSequence {
                receive_sequence,
                reply,
            } => {
                let result = sequences
                    .get_mut(&receive_sequence)
                    .ok_or_else(|| "unknown_receive_sequence".to_owned())
                    .map(|sequence| sequence.finished = true);
                let _ = reply.send(result);
            }
            PasteMessage::CancelDevice { device_id, reply } => {
                if active
                    .as_ref()
                    .is_some_and(|active| active.request.device_id == device_id)
                {
                    let cancelled = active.take().expect("active paste exists");
                    let _ = cancelled.request.reply.send(PasteReply::Cancelled);
                    cancel_sequence(&mut sequences, cancelled.request.receive_sequence);
                }
                sequences.retain(|_, sequence| {
                    let mut matched = false;
                    sequence.requests.retain(|request| {
                        if request.device_id == device_id {
                            matched = true;
                            let _ = request.reply.send(PasteReply::Cancelled);
                            false
                        } else {
                            true
                        }
                    });
                    !(matched || sequence.requests.is_empty() && sequence.finished)
                });
                let _ = reply.send(Ok(()));
            }
            PasteMessage::Shutdown { reply } => {
                cancel_all(&mut sequences, active.take());
                let _ = reply.send(());
                break;
            }
        }
        start_next(&mut sequences, &mut active, &clipboard, timeout);
    }
    cancel_all(&mut sequences, active);
}

fn start_next(
    sequences: &mut BTreeMap<u64, SequenceQueue>,
    active: &mut Option<ActivePaste>,
    clipboard: &impl ClipboardWriter,
    timeout: Duration,
) {
    while active.is_none() {
        let Some(sequence_id) = sequences.keys().next().copied() else {
            return;
        };
        let sequence = sequences
            .get_mut(&sequence_id)
            .expect("sequence key exists");
        if let Some(request) = sequence.requests.pop_front() {
            match clipboard.write(&request.text) {
                Ok(()) => {
                    let _ = request.reply.send(PasteReply::Granted);
                    *active = Some(ActivePaste {
                        request,
                        deadline: Instant::now() + timeout,
                    });
                    return;
                }
                Err(error) => {
                    let _ = request.reply.send(PasteReply::ClipboardError(error));
                    cancel_sequence(sequences, sequence_id);
                }
            }
        } else if sequence.finished {
            sequences.remove(&sequence_id);
        } else {
            return;
        }
    }
}

fn cancel_all(sequences: &mut BTreeMap<u64, SequenceQueue>, active: Option<ActivePaste>) {
    if let Some(active) = active {
        let _ = active.request.reply.send(PasteReply::Cancelled);
    }
    for (_, sequence) in std::mem::take(sequences) {
        for request in sequence.requests {
            let _ = request.reply.send(PasteReply::Cancelled);
        }
    }
}

fn cancel_sequence(sequences: &mut BTreeMap<u64, SequenceQueue>, receive_sequence: u64) {
    if let Some(sequence) = sequences.remove(&receive_sequence) {
        for request in sequence.requests {
            let _ = request.reply.send(PasteReply::Cancelled);
        }
    }
}

struct SystemClipboard;

impl ClipboardWriter for SystemClipboard {
    fn write(&self, text: &str) -> Result<(), String> {
        let executable = if cfg!(target_os = "windows") {
            "clip.exe"
        } else {
            "/usr/bin/pbcopy"
        };
        let mut command = Command::new(executable);
        #[cfg(target_os = "macos")]
        command.env("LC_CTYPE", "UTF-8");
        let mut child = command
            .stdin(Stdio::piped())
            .spawn()
            .map_err(|error| format!("start clipboard command: {error}"))?;
        child
            .stdin
            .take()
            .ok_or_else(|| "open clipboard command stdin".to_owned())?
            .write_all(text.as_bytes())
            .map_err(|error| format!("write clipboard command: {error}"))?;
        let status = child
            .wait()
            .map_err(|error| format!("wait for clipboard command: {error}"))?;
        status
            .success()
            .then_some(())
            .ok_or_else(|| format!("clipboard command exited {status}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hardware::DeviceId;
    use std::{
        sync::{Arc, Mutex, mpsc},
        time::Duration,
    };

    #[derive(Default)]
    struct FakeClipboard(Arc<Mutex<Vec<String>>>);

    impl ClipboardWriter for FakeClipboard {
        fn write(&self, text: &str) -> Result<(), String> {
            self.0.lock().unwrap().push(text.into());
            Ok(())
        }
    }

    fn request(
        receive_sequence: u64,
        serial: &str,
        event_id: u64,
        step: u16,
    ) -> (PasteRequest, mpsc::Receiver<PasteReply>) {
        let (reply, replies) = mpsc::channel();
        (
            PasteRequest {
                receive_sequence,
                device_id: DeviceId::new("luatos-esp32s3-aio", serial).unwrap(),
                event_id,
                step,
                text: format!("{serial}-{step}"),
                reply,
            },
            replies,
        )
    }

    #[test]
    fn processes_registered_inputs_strictly_by_receive_sequence_without_coalescing() {
        let clipboard = FakeClipboard::default();
        let writes = Arc::clone(&clipboard.0);
        let coordinator = PasteCoordinator::with_timeout(clipboard, Duration::from_secs(1));
        let handle = coordinator.handle();
        handle.register_sequence(1).unwrap();
        handle.register_sequence(2).unwrap();
        let (second, second_reply) = request(2, "B", 20, 1);
        handle.submit(second).unwrap();
        assert!(second_reply.try_recv().is_err());
        let (first, first_reply) = request(1, "A", 10, 1);
        handle.submit(first).unwrap();
        assert_eq!(first_reply.recv().unwrap(), PasteReply::Granted);
        assert_eq!(writes.lock().unwrap().as_slice(), ["A-1"]);

        assert!(
            handle
                .complete(&DeviceId::new("luatos-esp32s3-aio", "B").unwrap(), 20, 1)
                .is_err()
        );
        assert!(second_reply.try_recv().is_err());
        handle
            .complete(&DeviceId::new("luatos-esp32s3-aio", "A").unwrap(), 10, 1)
            .unwrap();
        handle.finish_sequence(1).unwrap();
        assert_eq!(second_reply.recv().unwrap(), PasteReply::Granted);
        assert_eq!(writes.lock().unwrap().as_slice(), ["A-1", "B-1"]);
        handle
            .complete(&DeviceId::new("luatos-esp32s3-aio", "B").unwrap(), 20, 1)
            .unwrap();
        handle.finish_sequence(2).unwrap();
        coordinator.shutdown();
    }

    #[test]
    fn timeout_releases_only_the_source_slot_and_starts_next_request() {
        let coordinator =
            PasteCoordinator::with_timeout(FakeClipboard::default(), Duration::from_millis(30));
        let handle = coordinator.handle();
        handle.register_sequence(1).unwrap();
        handle.register_sequence(2).unwrap();
        let (first, first_reply) = request(1, "A", 10, 1);
        let (second, second_reply) = request(2, "B", 20, 1);
        handle.submit(first).unwrap();
        handle.submit(second).unwrap();
        assert_eq!(first_reply.recv().unwrap(), PasteReply::Granted);
        assert_eq!(first_reply.recv().unwrap(), PasteReply::TimedOut);
        assert_eq!(second_reply.recv().unwrap(), PasteReply::Granted);
        handle
            .complete(&DeviceId::new("luatos-esp32s3-aio", "B").unwrap(), 20, 1)
            .unwrap();
        handle.finish_sequence(2).unwrap();
        coordinator.shutdown();
    }

    #[test]
    fn multiple_pastes_for_one_input_are_not_coalesced() {
        let clipboard = FakeClipboard::default();
        let writes = Arc::clone(&clipboard.0);
        let coordinator = PasteCoordinator::with_timeout(clipboard, Duration::from_secs(1));
        let handle = coordinator.handle();
        handle.register_sequence(1).unwrap();
        let (first, first_reply) = request(1, "A", 10, 1);
        let (second, second_reply) = request(1, "A", 10, 2);
        handle.submit(first).unwrap();
        handle.submit(second).unwrap();

        assert_eq!(first_reply.recv().unwrap(), PasteReply::Granted);
        handle
            .complete(&DeviceId::new("luatos-esp32s3-aio", "A").unwrap(), 10, 1)
            .unwrap();
        assert_eq!(second_reply.recv().unwrap(), PasteReply::Granted);
        assert_eq!(writes.lock().unwrap().as_slice(), ["A-1", "A-2"]);
        handle
            .complete(&DeviceId::new("luatos-esp32s3-aio", "A").unwrap(), 10, 2)
            .unwrap();
        handle.finish_sequence(1).unwrap();
        coordinator.shutdown();
    }

    #[test]
    fn sequence_rejects_another_device_and_cancels_queued_steps_after_timeout() {
        let coordinator =
            PasteCoordinator::with_timeout(FakeClipboard::default(), Duration::from_millis(30));
        let handle = coordinator.handle();
        handle.register_sequence(1).unwrap();
        let (first, first_reply) = request(1, "A", 10, 1);
        let (queued, queued_reply) = request(1, "A", 10, 2);
        let (wrong_device, _wrong_reply) = request(1, "B", 20, 1);
        handle.submit(first).unwrap();
        handle.submit(queued).unwrap();
        assert_eq!(
            handle.submit(wrong_device).unwrap_err(),
            "receive_sequence_device_mismatch"
        );

        assert_eq!(first_reply.recv().unwrap(), PasteReply::Granted);
        assert_eq!(first_reply.recv().unwrap(), PasteReply::TimedOut);
        assert_eq!(queued_reply.recv().unwrap(), PasteReply::Cancelled);
        coordinator.shutdown();
    }

    #[test]
    fn disconnect_cancels_only_that_device_and_advances_fifo() {
        let coordinator =
            PasteCoordinator::with_timeout(FakeClipboard::default(), Duration::from_secs(1));
        let handle = coordinator.handle();
        handle.register_sequence(1).unwrap();
        handle.register_sequence(2).unwrap();
        let (first, first_reply) = request(1, "A", 10, 1);
        let (second, second_reply) = request(2, "B", 20, 1);
        handle.submit(first).unwrap();
        handle.submit(second).unwrap();
        assert_eq!(first_reply.recv().unwrap(), PasteReply::Granted);
        handle
            .cancel_device(&DeviceId::new("luatos-esp32s3-aio", "A").unwrap())
            .unwrap();
        assert_eq!(first_reply.recv().unwrap(), PasteReply::Cancelled);
        assert_eq!(second_reply.recv().unwrap(), PasteReply::Granted);
        coordinator.shutdown();
    }
}
