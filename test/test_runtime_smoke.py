from collections import deque

from scripts.smoke_runtime_protocol import run_smoke


class FakeSerial:
    def __init__(self, responses: list[bytes]) -> None:
        self.responses = deque(responses)
        self.writes: list[bytes] = []

    def write(self, data: bytes) -> int:
        self.writes.append(data)
        return len(data)

    def readline(self) -> bytes:
        return self.responses.popleft() if self.responses else b""


def test_smoke_requires_expected_protocol_responses() -> None:
    device = FakeSerial(
        [
            b"HELLO 3 esp32s3 luatos-esp32s3-aio test-build\n",
            b"CONFIG_OK 1\n",
            b"CONFIG_ERROR 2 invalid_direct\n",
            b"LEARN_OK 3\n",
            b"LEARN_OK 3\n",
        ]
    )

    run_smoke(
        device,
        family="esp32s3",
        board="luatos-esp32s3-aio",
        build="test-build",
        valid_pins=[1, 2],
        rejected_pins=[99],
    )

    assert device.writes == [
        b"HELLO\n",
        b"CONFIG_BEGIN 1 30\n",
        b"CONFIG_DIRECT 1 0 2 1 2\n",
        b"CONFIG_COMMIT 1\n",
        b"CONFIG_BEGIN 2 30\n",
        b"CONFIG_DIRECT 2 0 1 99\n",
        b"LEARN_BEGIN 3 2 1 2\n",
        b"LEARN_END 3\n",
    ]


def test_smoke_rejects_wrong_hello() -> None:
    device = FakeSerial([b"HELLO 2 esp32s3 luatos-esp32s3-aio test-build\n"])

    try:
        run_smoke(
            device,
            family="esp32s3",
            board="luatos-esp32s3-aio",
            build="test-build",
            valid_pins=[1, 2],
            rejected_pins=[],
        )
    except RuntimeError as error:
        assert "expected HELLO 3" in str(error)
    else:
        raise AssertionError("wrong HELLO must fail")


def test_smoke_actions_use_matching_event_and_sequential_done_steps() -> None:
    device = FakeSerial(
        [
            b"HELLO 3 esp32s3 luatos-esp32s3-aio test-build\n",
            b"CONFIG_OK 1\n",
            b"LEARN_OK 2\n",
            b"LEARN_OK 2\n",
            b"\n",
            b"STATE 7 DIRECT 1 DOWN\n",
            b"DONE 7 1\n",
            b"DONE 7 2\n",
        ]
    )

    run_smoke(
        device,
        family="esp32s3",
        board="luatos-esp32s3-aio",
        build="test-build",
        valid_pins=[1, 2],
        rejected_pins=[],
        exercise_actions=True,
    )

    assert device.writes[-2:] == [b"PASTE 7 1 2\n", b"HOTKEY 7 2 2 1 25\n"]
