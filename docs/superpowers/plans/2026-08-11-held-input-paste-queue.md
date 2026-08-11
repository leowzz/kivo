# Held Input Paste Queue Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Allow Paste and following shortcuts on one device to execute while another device keeps an input pressed indefinitely.

**Architecture:** Keep the existing host receive-sequence registry and the single active Paste transaction. Change `PasteCoordinator::start_next` so unfinished sequences without a submitted Paste request remain registered but do not block the first sequence that does contain a request.

**Tech Stack:** Rust, `std::sync::mpsc`, existing Kivo device workers and fake serial integration harness, Cargo tests.

## Global Constraints

- Only sequences containing a submitted Paste request participate in Paste selection.
- Among ready Paste requests, the lowest host receive sequence runs first.
- Never preempt an active Paste or allow concurrent clipboard writes.
- Preserve per-device action order, including `Paste -> Hotkey`.
- Do not change firmware, serial protocol, profile schemas, held-input timeouts, or runtime-log severity classification.
- Preserve unrelated work and stage only files named by this plan.

---

### Task 1: Let Ready Paste Requests Bypass Empty Held Sequences

**Files:**
- Modify: `src-tauri/src/paste.rs:805-910`
- Modify: `src-tauri/src/paste.rs:525-570`
- Modify: `src-tauri/tests/parallel_devices.rs:445-645`

**Interfaces:**
- Consumes: `PasteHandle::register_sequence`, `PasteHandle::submit`, `PasteHandle::complete`, `SequenceQueue { requests, finished }`, and the existing fake serial runtime harness.
- Produces: unchanged public interfaces; `start_next` selects the lowest registered sequence that currently contains a Paste request.

- [ ] **Step 1: Add the focused Paste coordinator regression test**

Add this test after `processes_registered_inputs_strictly_by_receive_sequence_without_coalescing` in `src-tauri/src/paste.rs`:

```rust
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
        later_reply.recv_timeout(Duration::from_millis(100)).unwrap(),
        PasteReply::Granted
    );
    assert_eq!(writes.lock().unwrap().as_slice(), ["B-1"]);
    handle
        .complete(&DeviceId::new("luatos-esp32s3-aio", "B").unwrap(), 20, 1)
        .unwrap();
    handle.finish_sequence(2).unwrap();

    let (earlier, earlier_reply) = request(1, "A", 10, 1);
    handle.submit(earlier).unwrap();
    assert_eq!(
        earlier_reply.recv_timeout(Duration::from_millis(100)).unwrap(),
        PasteReply::Granted
    );
    assert_eq!(writes.lock().unwrap().as_slice(), ["B-1", "A-1"]);
    handle
        .complete(&DeviceId::new("luatos-esp32s3-aio", "A").unwrap(), 10, 1)
        .unwrap();
    handle.finish_sequence(1).unwrap();
    coordinator.shutdown();
}
```

- [ ] **Step 2: Run the focused unit test and verify RED**

Run:

```bash
rtk cargo test --manifest-path src-tauri/Cargo.toml paste::tests::unfinished_empty_sequence_does_not_block_a_later_paste -- --exact
```

Expected: FAIL because `later_reply.recv_timeout(...)` times out while unfinished sequence 1 is empty.

- [ ] **Step 3: Extend the fake HELLO helper for protocol v6**

Replace the existing `hello` helper in `src-tauri/tests/parallel_devices.rs` with:

```rust
fn hello_with_protocol(board: &BoardProfile, build: &str, protocol: u16) -> String {
    format!(
        "HELLO {protocol} {} {} {} {} {}\n",
        board.family_id,
        board.id,
        build,
        board.safe_pins.len(),
        board
            .safe_pins
            .iter()
            .map(u8::to_string)
            .collect::<Vec<_>>()
            .join(" ")
    )
}

fn hello(board: &BoardProfile, build: &str) -> String {
    hello_with_protocol(board, build, 4)
}
```

- [ ] **Step 4: Add the two-device held-input integration regression**

Add this test before `four_concurrent_devices_keep_runtime_and_global_paste_isolated` in `src-tauri/tests/parallel_devices.rs`:

```rust
#[test]
fn held_input_on_one_device_does_not_block_another_devices_paste_sequence() {
    let rp = board("vccgnd-yd-rp2040");
    let specs = [
        (
            "HOLD",
            "/dev/fake-held",
            "profile-held",
            "hardware-held",
            10,
            11,
        ),
        (
            "PASTE",
            "/dev/fake-paste",
            "profile-paste",
            "hardware-paste",
            12,
            13,
        ),
    ];
    let held_profile = profile("profile-held", "hardware-held", rp.id, 10, 11);
    let mut paste_profile = profile("profile-paste", "hardware-paste", rp.id, 12, 13);
    paste_profile.actions.insert(
        "PASTE".into(),
        TriggerActions::press(vec![
            ButtonAction::Paste {
                text: "paste-profile-paste".into(),
            },
            ButtonAction::Hotkey {
                keys: vec!["enter".into()],
            },
        ]),
    );

    let directory = TestDirectory::new();
    let workspace = Arc::new(RwLock::new(
        Workspace::create(&directory.0, vec![held_profile, paste_profile]).unwrap(),
    ));
    let enumerator = Arc::new(FakeUsbEnumerator::default());
    let transports = Arc::new(FakeTransportFactory::default());
    let clock = Arc::new(FakeClock::new());
    let clipboard = RecordingClipboard::default();
    let paste =
        PasteCoordinator::with_clock(clipboard.clone(), Duration::from_secs(60), clock.clone());
    let endpoints = specs
        .iter()
        .map(|(serial_id, port, ..)| {
            let endpoint = FakeEndpoint::new(
                port,
                hello_with_protocol(rp, &format!("build-{serial_id}"), 6),
            );
            transports.add(endpoint.clone());
            ((*serial_id).to_owned(), endpoint)
        })
        .collect::<BTreeMap<_, _>>();
    enumerator.set(
        specs
            .iter()
            .map(|(serial_id, port, ..)| serial(port, rp, serial_id))
            .collect(),
        Vec::new(),
    );
    let launcher = Arc::new(SystemWorkerLauncher::with_runtime(
        paste.handle(),
        None,
        Arc::new(RwLock::new(())),
        transports,
        clock,
    ));
    let coordinator = RuntimeCoordinator::with_paste(
        enumerator,
        launcher,
        workspace.clone(),
        Some(paste.handle()),
    );
    let mut coordinator = RuntimeFixture { coordinator, paste };

    coordinator.scan_once().unwrap();
    wait_until(&mut coordinator, |coordinator| {
        coordinator.devices().len() == 2
    });
    {
        let mut workspace = workspace.write().unwrap();
        for (serial_id, _, profile_id, hardware_id, ..) in specs {
            workspace
                .set_assignment(
                    &DeviceId::new(rp.id, serial_id).unwrap(),
                    RuntimeAssignment {
                        device_profile_id: profile_id.into(),
                        hardware_profile_id: hardware_id.into(),
                    },
                )
                .unwrap();
        }
        coordinator.apply_workspace_revision(WorkspaceRevision::capture(&workspace));
    }
    for (serial_id, _, _, _, hot_pin, paste_pin) in specs {
        let endpoint = &endpoints[serial_id];
        let suffix = format!(" 0 2 {hot_pin} {paste_pin}\n");
        let direct = endpoint
            .wait_for_line(|line| line.starts_with("CONFIG_DIRECT ") && line.ends_with(&suffix));
        let revision = direct
            .split_whitespace()
            .nth(1)
            .unwrap()
            .parse::<u32>()
            .unwrap();
        endpoint.emit(&format!("CONFIG_OK {revision}\n"));
    }
    wait_until(&mut coordinator, |coordinator| {
        ready_count(coordinator) == 2
    });

    let held = &endpoints["HOLD"];
    held.emit("STATE 100 DIRECT 10 DOWN\n");
    wait_for_input_event(&mut coordinator, "HOLD");
    held.wait_for_line(|line| line == "HOTKEY 1 1 2 0 40\n");
    held.emit("DONE 1 1\n");
    held.wait_for_line(|line| line == "HOTKEY 1 2 2 0 43\n");
    held.emit("DONE 1 2\n");

    let paste_device = &endpoints["PASTE"];
    paste_device.emit("STATE 200 DIRECT 13 DOWN\n");
    wait_for_input_event(&mut coordinator, "PASTE");
    paste_device.wait_for_line(|line| line == paste_action_line(1, 1, 2));
    assert_eq!(clipboard.writes(), ["paste-profile-paste"]);
    paste_device.emit("DONE 1 1\n");
    paste_device.wait_for_line(|line| line == "HOTKEY 1 2 2 0 40\n");
    paste_device.emit("DONE 1 2\n");

    wait_until(&mut coordinator, |coordinator| {
        ready_count(coordinator) == 2
    });
}
```

- [ ] **Step 5: Run the integration test and verify RED**

Run:

```bash
rtk cargo test --manifest-path src-tauri/Cargo.toml --test parallel_devices held_input_on_one_device_does_not_block_another_devices_paste_sequence -- --exact
```

Expected: FAIL with `/dev/fake-paste did not emit expected line` because Device A's unfinished Down sequence blocks Device B's Paste grant.

- [ ] **Step 6: Implement request-aware Paste selection**

Replace the selection portion of `start_next` in `src-tauri/src/paste.rs` with:

```rust
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
```

- [ ] **Step 7: Format and verify both regression tests GREEN**

Run:

```bash
rtk cargo fmt --manifest-path src-tauri/Cargo.toml
rtk cargo test --manifest-path src-tauri/Cargo.toml paste::tests::unfinished_empty_sequence_does_not_block_a_later_paste -- --exact
rtk cargo test --manifest-path src-tauri/Cargo.toml --test parallel_devices held_input_on_one_device_does_not_block_another_devices_paste_sequence -- --exact
```

Expected: both tests PASS.

- [ ] **Step 8: Run focused regression suites**

Run:

```bash
rtk cargo test --manifest-path src-tauri/Cargo.toml paste::tests
rtk cargo test --manifest-path src-tauri/Cargo.toml --test parallel_devices
```

Expected: all Paste unit tests and the complete parallel-device integration suite PASS.

- [ ] **Step 9: Run complete verification**

Run:

```bash
rtk cargo test --manifest-path src-tauri/Cargo.toml
rtk cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
rtk git diff --check
```

Expected: the complete Rust suite passes, formatting is clean, and `git diff --check` reports no errors. Physical two-device acceptance remains explicitly Not Run until one RP2040 is held Down while another executes Paste followed by Hotkey.

- [ ] **Step 10: Review and commit the implementation**

Run:

```bash
rtk git diff -- src-tauri/src/paste.rs src-tauri/tests/parallel_devices.rs
rtk git status --short
rtk git add src-tauri/src/paste.rs src-tauri/tests/parallel_devices.rs
rtk git commit -m "fix: let paste bypass held inputs"
```

Expected: the commit contains only the Paste coordinator change and its unit/integration regressions.
