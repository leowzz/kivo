use crate::hardware::DeviceId;
use std::{
    collections::{BTreeMap, VecDeque},
    sync::{Arc, Mutex, mpsc},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

#[cfg(target_os = "macos")]
use std::{
    io::Write,
    process::{Command, Stdio},
};

pub const ACTION_TIMEOUT: Duration = Duration::from_millis(1800);

pub trait Clock: Send + Sync + 'static {
    fn monotonic_now(&self) -> Instant;
    fn unix_time_ms(&self) -> u64;
    fn schedule_deadline(&self, deadline: Instant, wake: Box<dyn FnOnce() + Send>);
}

#[derive(Default)]
pub struct SystemClock {
    scheduler: Mutex<Option<SystemDeadlineScheduler>>,
}

impl Clock for SystemClock {
    fn monotonic_now(&self) -> Instant {
        Instant::now()
    }

    fn unix_time_ms(&self) -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    }

    fn schedule_deadline(&self, deadline: Instant, wake: Box<dyn FnOnce() + Send>) {
        self.scheduler
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get_or_insert_with(SystemDeadlineScheduler::start)
            .schedule(deadline, wake);
    }
}

type DeadlineWake = Box<dyn FnOnce() + Send>;

enum SchedulerMessage {
    Schedule {
        deadline: Instant,
        wake: DeadlineWake,
    },
    Shutdown,
}

struct ScheduledDeadline {
    deadline: Instant,
    wake: DeadlineWake,
}

struct SystemDeadlineScheduler {
    sender: mpsc::Sender<SchedulerMessage>,
    join: Option<JoinHandle<()>>,
}

impl SystemDeadlineScheduler {
    fn start() -> Self {
        let (sender, receiver) = mpsc::channel();
        let join = thread::Builder::new()
            .name("kivo-paste-deadlines".into())
            .spawn(move || run_deadline_scheduler(receiver))
            .expect("spawn paste deadline scheduler");
        Self {
            sender,
            join: Some(join),
        }
    }

    fn schedule(&self, deadline: Instant, wake: DeadlineWake) {
        let _ = self
            .sender
            .send(SchedulerMessage::Schedule { deadline, wake });
    }
}

impl Drop for SystemDeadlineScheduler {
    fn drop(&mut self) {
        let _ = self.sender.send(SchedulerMessage::Shutdown);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

fn run_deadline_scheduler(receiver: mpsc::Receiver<SchedulerMessage>) {
    let mut deadlines = Vec::<ScheduledDeadline>::new();
    loop {
        let message = deadlines
            .iter()
            .map(|scheduled| scheduled.deadline)
            .min()
            .map_or_else(
                || {
                    receiver
                        .recv()
                        .map_err(|_| mpsc::RecvTimeoutError::Disconnected)
                },
                |deadline| {
                    receiver.recv_timeout(deadline.saturating_duration_since(Instant::now()))
                },
            );
        match message {
            Ok(SchedulerMessage::Schedule { deadline, wake }) => {
                deadlines.push(ScheduledDeadline { deadline, wake });
            }
            Ok(SchedulerMessage::Shutdown) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
        while let Some(index) = deadlines
            .iter()
            .enumerate()
            .filter(|(_, scheduled)| scheduled.deadline <= Instant::now())
            .min_by_key(|(_, scheduled)| scheduled.deadline)
            .map(|(index, _)| index)
        {
            let scheduled = deadlines.remove(index);
            (scheduled.wake)();
        }
    }
}

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
        Self::with_clock(
            SystemClipboard,
            ACTION_TIMEOUT,
            Arc::new(SystemClock::default()),
        )
    }

    pub fn with_timeout(clipboard: impl ClipboardWriter, timeout: Duration) -> Self {
        Self::with_clock(clipboard, timeout, Arc::new(SystemClock::default()))
    }

    pub fn with_clock(
        clipboard: impl ClipboardWriter,
        timeout: Duration,
        clock: Arc<dyn Clock>,
    ) -> Self {
        let (sender, receiver) = mpsc::channel();
        let loop_sender = sender.clone();
        let join =
            thread::spawn(move || run_paste_loop(receiver, loop_sender, clipboard, timeout, clock));
        Self {
            handle: PasteHandle { sender },
            join: Mutex::new(Some(join)),
        }
    }

    pub fn handle(&self) -> PasteHandle {
        self.handle.clone()
    }

    pub(crate) fn wait_for_request(
        &self,
        device_id: &DeviceId,
        event_id: u64,
        step: u16,
        text: &str,
        timeout: Duration,
    ) -> Result<(), String> {
        let (reply, observed) = mpsc::channel();
        self.handle
            .sender
            .send(PasteMessage::ObserveRequest {
                expected: PasteRequestObservation {
                    device_id: device_id.clone(),
                    event_id,
                    step,
                    text: text.to_owned(),
                },
                reply,
            })
            .map_err(|_| "paste_coordinator_stopped".to_owned())?;
        observed
            .recv_timeout(timeout)
            .map_err(|_| "paste_request_observation_timeout".to_owned())
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
    DeadlineElapsed {
        token: u64,
    },
    ObserveRequest {
        expected: PasteRequestObservation,
        reply: mpsc::Sender<()>,
    },
    Shutdown {
        reply: mpsc::Sender<()>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PasteRequestObservation {
    device_id: DeviceId,
    event_id: u64,
    step: u16,
    text: String,
}

impl PasteRequestObservation {
    fn matches(&self, request: &PasteRequest) -> bool {
        self.device_id == request.device_id
            && self.event_id == request.event_id
            && self.step == request.step
            && self.text == request.text
    }
}

struct RequestObserver {
    expected: PasteRequestObservation,
    reply: mpsc::Sender<()>,
}

#[derive(Default)]
struct SequenceQueue {
    requests: VecDeque<PasteRequest>,
    finished: bool,
    device_id: Option<DeviceId>,
}

struct ActivePaste {
    request: PasteRequest,
    deadline_token: u64,
}

fn run_paste_loop(
    receiver: mpsc::Receiver<PasteMessage>,
    sender: mpsc::Sender<PasteMessage>,
    clipboard: impl ClipboardWriter,
    timeout: Duration,
    clock: Arc<dyn Clock>,
) {
    let mut sequences = BTreeMap::<u64, SequenceQueue>::new();
    let mut active: Option<ActivePaste> = None;
    let mut request_observers = Vec::<RequestObserver>::new();
    let mut next_deadline_token = 0u64;
    loop {
        let Ok(message) = receiver.recv() else {
            break;
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
            PasteMessage::DeadlineElapsed { token } => {
                if active
                    .as_ref()
                    .is_some_and(|active| active.deadline_token == token)
                {
                    let timed_out = active.take().expect("active paste exists");
                    let sequence = timed_out.request.receive_sequence;
                    let _ = timed_out.request.reply.send(PasteReply::TimedOut);
                    cancel_sequence(&mut sequences, sequence);
                }
            }
            PasteMessage::ObserveRequest { expected, reply } => {
                request_observers.push(RequestObserver { expected, reply });
            }
            PasteMessage::Shutdown { reply } => {
                cancel_all(&mut sequences, active.take());
                let _ = reply.send(());
                break;
            }
        }
        notify_request_observers(&sequences, active.as_ref(), &mut request_observers);
        start_next(
            &mut sequences,
            &mut active,
            &clipboard,
            timeout,
            clock.as_ref(),
            &sender,
            &mut next_deadline_token,
        );
    }
    cancel_all(&mut sequences, active);
}

fn notify_request_observers(
    sequences: &BTreeMap<u64, SequenceQueue>,
    active: Option<&ActivePaste>,
    observers: &mut Vec<RequestObserver>,
) {
    observers.retain(|observer| {
        let observed = active.is_some_and(|active| observer.expected.matches(&active.request))
            || sequences.values().any(|sequence| {
                sequence
                    .requests
                    .iter()
                    .any(|request| observer.expected.matches(request))
            });
        if observed {
            let _ = observer.reply.send(());
        }
        !observed
    });
}

fn start_next(
    sequences: &mut BTreeMap<u64, SequenceQueue>,
    active: &mut Option<ActivePaste>,
    clipboard: &impl ClipboardWriter,
    timeout: Duration,
    clock: &dyn Clock,
    sender: &mpsc::Sender<PasteMessage>,
    next_deadline_token: &mut u64,
) {
    while active.is_none() {
        sequences.retain(|_, sequence| !(sequence.finished && sequence.requests.is_empty()));
        let Some(sequence_id) = sequences.iter().find_map(|(sequence_id, sequence)| {
            (!sequence.requests.is_empty()).then_some(*sequence_id)
        }) else {
            return;
        };
        let request = sequences
            .get_mut(&sequence_id)
            .expect("sequence key exists")
            .requests
            .pop_front()
            .expect("selected sequence has a paste request");
        match clipboard.write(&request.text) {
            Ok(()) => {
                let _ = request.reply.send(PasteReply::Granted);
                *next_deadline_token = next_deadline_token
                    .checked_add(1)
                    .expect("paste deadline token exhausted");
                let deadline_token = *next_deadline_token;
                *active = Some(ActivePaste {
                    request,
                    deadline_token,
                });
                let deadline = clock.monotonic_now() + timeout;
                let sender = sender.clone();
                clock.schedule_deadline(
                    deadline,
                    Box::new(move || {
                        let _ = sender.send(PasteMessage::DeadlineElapsed {
                            token: deadline_token,
                        });
                    }),
                );
                return;
            }
            Err(error) => {
                let _ = request.reply.send(PasteReply::ClipboardError(error));
                cancel_sequence(sequences, sequence_id);
            }
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
        #[cfg(target_os = "macos")]
        return write_macos_clipboard(text);

        #[cfg(target_os = "windows")]
        return write_windows_clipboard(text);

        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        Err("clipboard is unsupported on this platform".to_owned())
    }
}

#[cfg(target_os = "macos")]
fn write_macos_clipboard(text: &str) -> Result<(), String> {
    let mut child = Command::new("/usr/bin/pbcopy")
        .env("LC_CTYPE", "UTF-8")
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

#[cfg(target_os = "windows")]
fn write_windows_clipboard(text: &str) -> Result<(), String> {
    use std::{ptr, thread};
    use windows_sys::Win32::{
        Foundation::GlobalFree,
        System::{
            DataExchange::{CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData},
            Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock},
            Ole::CF_UNICODETEXT,
        },
    };

    struct ClipboardGuard;
    impl Drop for ClipboardGuard {
        fn drop(&mut self) {
            unsafe {
                CloseClipboard();
            }
        }
    }

    struct GlobalMemory(windows_sys::Win32::Foundation::HGLOBAL);
    impl Drop for GlobalMemory {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe {
                    GlobalFree(self.0);
                }
            }
        }
    }

    let mut open_error = None;
    for _ in 0..10 {
        if unsafe { OpenClipboard(ptr::null_mut()) } != 0 {
            open_error = None;
            break;
        }
        open_error = Some(std::io::Error::last_os_error());
        thread::sleep(Duration::from_millis(10));
    }
    if let Some(error) = open_error {
        return Err(format!("open clipboard: {error}"));
    }
    let _clipboard = ClipboardGuard;

    if unsafe { EmptyClipboard() } == 0 {
        return Err(format!(
            "empty clipboard: {}",
            std::io::Error::last_os_error()
        ));
    }

    let wide = text
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let memory = unsafe { GlobalAlloc(GMEM_MOVEABLE, wide.len() * size_of::<u16>()) };
    if memory.is_null() {
        return Err(format!(
            "allocate clipboard memory: {}",
            std::io::Error::last_os_error()
        ));
    }
    let mut memory = GlobalMemory(memory);
    let destination = unsafe { GlobalLock(memory.0) }.cast::<u16>();
    if destination.is_null() {
        return Err(format!(
            "lock clipboard memory: {}",
            std::io::Error::last_os_error()
        ));
    }
    unsafe {
        ptr::copy_nonoverlapping(wide.as_ptr(), destination, wide.len());
        GlobalUnlock(memory.0);
    }

    if unsafe { SetClipboardData(CF_UNICODETEXT as u32, memory.0) }.is_null() {
        return Err(format!(
            "set clipboard data: {}",
            std::io::Error::last_os_error()
        ));
    }
    memory.0 = ptr::null_mut();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hardware::DeviceId;
    use std::{
        sync::{
            Arc, Mutex,
            atomic::{AtomicUsize, Ordering},
            mpsc,
        },
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

    struct DropProbe(Arc<AtomicUsize>);

    impl Drop for DropProbe {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[test]
    fn system_clock_runs_deadlines_on_one_owned_scheduler() {
        let clock = SystemClock::default();
        let deadline = Instant::now() + Duration::from_millis(20);
        let (thread_ids, observed) = mpsc::channel();
        for _ in 0..2 {
            let thread_ids = thread_ids.clone();
            clock.schedule_deadline(
                deadline,
                Box::new(move || {
                    thread_ids.send(thread::current().id()).unwrap();
                }),
            );
        }
        let first = observed.recv_timeout(Duration::from_secs(1)).unwrap();
        let second = observed.recv_timeout(Duration::from_secs(1)).unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn system_clock_preempts_a_later_deadline() {
        let clock = SystemClock::default();
        let (events, observed) = mpsc::channel();

        let later_events = events.clone();
        clock.schedule_deadline(
            Instant::now() + Duration::from_millis(500),
            Box::new(move || later_events.send("later").unwrap()),
        );

        clock.schedule_deadline(
            Instant::now() + Duration::from_millis(20),
            Box::new(move || events.send("earlier").unwrap()),
        );

        assert_eq!(
            observed.recv_timeout(Duration::from_millis(250)).unwrap(),
            "earlier"
        );
        assert_eq!(
            observed.recv_timeout(Duration::from_secs(1)).unwrap(),
            "later"
        );
    }

    #[test]
    fn dropping_system_clock_joins_scheduler_and_discards_pending_deadlines() {
        let dropped = Arc::new(AtomicUsize::new(0));
        {
            let clock = SystemClock::default();
            for _ in 0..2 {
                let probe = DropProbe(Arc::clone(&dropped));
                clock.schedule_deadline(
                    Instant::now() + Duration::from_secs(60),
                    Box::new(move || drop(probe)),
                );
            }
        }
        assert_eq!(dropped.load(Ordering::SeqCst), 2);
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
                device_id: DeviceId::new("yd-esp32-s3", serial).unwrap(),
                event_id,
                step,
                text: format!("{serial}-{step}"),
                reply,
            },
            replies,
        )
    }

    #[test]
    fn processes_ready_requests_strictly_by_receive_sequence_without_coalescing() {
        let clipboard = FakeClipboard::default();
        let writes = Arc::clone(&clipboard.0);
        let coordinator = PasteCoordinator::with_timeout(clipboard, Duration::from_secs(1));
        let handle = coordinator.handle();
        handle.register_sequence(1).unwrap();
        handle.register_sequence(2).unwrap();
        handle.register_sequence(3).unwrap();
        let (active, active_reply) = request(1, "A", 10, 1);
        let (first, first_reply) = request(2, "B", 20, 1);
        let (second, second_reply) = request(3, "C", 30, 1);
        handle.submit(active).unwrap();
        assert_eq!(active_reply.recv().unwrap(), PasteReply::Granted);
        handle.submit(second).unwrap();
        assert!(second_reply.try_recv().is_err());
        handle.submit(first).unwrap();
        assert_eq!(writes.lock().unwrap().as_slice(), ["A-1"]);
        assert!(first_reply.try_recv().is_err());
        assert!(second_reply.try_recv().is_err());
        handle
            .complete(&DeviceId::new("yd-esp32-s3", "A").unwrap(), 10, 1)
            .unwrap();
        handle.finish_sequence(1).unwrap();
        assert_eq!(first_reply.recv().unwrap(), PasteReply::Granted);
        assert_eq!(writes.lock().unwrap().as_slice(), ["A-1", "B-1"]);
        handle
            .complete(&DeviceId::new("yd-esp32-s3", "B").unwrap(), 20, 1)
            .unwrap();
        handle.finish_sequence(2).unwrap();
        assert_eq!(second_reply.recv().unwrap(), PasteReply::Granted);
        assert_eq!(writes.lock().unwrap().as_slice(), ["A-1", "B-1", "C-1"]);
        handle
            .complete(&DeviceId::new("yd-esp32-s3", "C").unwrap(), 30, 1)
            .unwrap();
        handle.finish_sequence(3).unwrap();
        coordinator.shutdown();
    }

    #[test]
    fn unfinished_empty_sequence_does_not_block_a_later_paste() {
        let clipboard = FakeClipboard::default();
        let writes = Arc::clone(&clipboard.0);
        let coordinator = PasteCoordinator::with_timeout(clipboard, Duration::from_secs(1));
        let handle = coordinator.handle();
        handle.register_sequence(1).unwrap();
        handle.register_sequence(2).unwrap();

        let (later, later_reply) = request(2, "B", 20, 1);
        handle.submit(later).unwrap();

        assert_eq!(
            later_reply
                .recv_timeout(Duration::from_millis(100))
                .unwrap(),
            PasteReply::Granted
        );
        assert_eq!(writes.lock().unwrap().as_slice(), ["B-1"]);
        handle
            .complete(&DeviceId::new("yd-esp32-s3", "B").unwrap(), 20, 1)
            .unwrap();
        handle.finish_sequence(2).unwrap();

        let (earlier, earlier_reply) = request(1, "A", 10, 1);
        handle.submit(earlier).unwrap();
        assert_eq!(
            earlier_reply
                .recv_timeout(Duration::from_millis(100))
                .unwrap(),
            PasteReply::Granted
        );
        assert_eq!(writes.lock().unwrap().as_slice(), ["B-1", "A-1"]);
        handle
            .complete(&DeviceId::new("yd-esp32-s3", "A").unwrap(), 10, 1)
            .unwrap();
        handle.finish_sequence(1).unwrap();
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
            .complete(&DeviceId::new("yd-esp32-s3", "B").unwrap(), 20, 1)
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
            .complete(&DeviceId::new("yd-esp32-s3", "A").unwrap(), 10, 1)
            .unwrap();
        assert_eq!(second_reply.recv().unwrap(), PasteReply::Granted);
        assert_eq!(writes.lock().unwrap().as_slice(), ["A-1", "A-2"]);
        handle
            .complete(&DeviceId::new("yd-esp32-s3", "A").unwrap(), 10, 2)
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
            .cancel_device(&DeviceId::new("yd-esp32-s3", "A").unwrap())
            .unwrap();
        assert_eq!(first_reply.recv().unwrap(), PasteReply::Cancelled);
        assert_eq!(second_reply.recv().unwrap(), PasteReply::Granted);
        coordinator.shutdown();
    }
}
