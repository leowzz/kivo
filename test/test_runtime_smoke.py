from collections import deque
from types import SimpleNamespace

import pytest

from scripts.smoke_runtime_protocol import build_parser, run_from_args, run_smoke


class FakeSerial:
    def __init__(self, responses: list[bytes]) -> None:
        self.responses = deque(responses)
        self.writes: list[bytes] = []

    def write(self, data: bytes) -> int:
        self.writes.append(data)
        return len(data)

    def readline(self) -> bytes:
        return self.responses.popleft() if self.responses else b""

    def __enter__(self) -> "FakeSerial":
        return self

    def __exit__(self, *_: object) -> None:
        return None


def test_smoke_requires_expected_protocol_responses() -> None:
    device = FakeSerial(
        [
            b"HELLO 6 esp32s3 yd-esp32-s3 test-build 2 1 2\n",
            b"CONFIG_OK 1\n",
            b"CONFIG_ERROR 2 invalid_direct\n",
            b"LEARN_OK 3\n",
            b"LEARN_OK 3\n",
        ]
    )

    run_smoke(
        device,
        family="esp32s3",
        board="yd-esp32-s3",
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


def test_smoke_accepts_generic_protocol_v9_hello() -> None:
    device = FakeSerial(
        [
            b"HELLO 9 rp2040 yd-rp2040 test-build - 2 1 2\n",
            b"CONFIG_OK 1\n",
            b"LEARN_OK 2\n",
            b"LEARN_OK 2\n",
        ]
    )

    run_smoke(
        device,
        family="rp2040",
        board="yd-rp2040",
        build="test-build",
        valid_pins=[1, 2],
        rejected_pins=[],
        protocol_version=9,
    )


def test_smoke_ignores_duplicate_hello_before_command_ack() -> None:
    hello = b"HELLO 6 esp32s3 yd-esp32-s3 test-build 2 1 2\n"
    device = FakeSerial(
        [
            hello,
            hello,
            b"CONFIG_OK 1\n",
            b"CONFIG_ERROR 2 invalid_direct\n",
            b"LEARN_OK 3\n",
            b"LEARN_OK 3\n",
        ]
    )

    run_smoke(
        device,
        family="esp32s3",
        board="yd-esp32-s3",
        build="test-build",
        valid_pins=[1, 2],
        rejected_pins=[99],
    )


def test_smoke_rejects_wrong_hello() -> None:
    device = FakeSerial([b"HELLO 2 esp32s3 yd-esp32-s3 test-build\n"])

    try:
        run_smoke(
            device,
            family="esp32s3",
            board="yd-esp32-s3",
            build="test-build",
            valid_pins=[1, 2],
            rejected_pins=[],
        )
    except RuntimeError as error:
        assert "invalid HELLO" in str(error)
    else:
        raise AssertionError("wrong HELLO must fail")


@pytest.mark.parametrize(
    "hello",
    [
        "HELLO 6 rp2040 yd-esp32-s3 test-build 2 1 2",
        "HELLO 6 esp32s3 other-board test-build 2 1 2",
        "HELLO 6 esp32s3 yd-esp32-s3 other-build 2 1 2",
        "HELLO 6 esp32s3 yd-esp32-s3 test-build",
        "HELLO 6 esp32s3 yd-esp32-s3 test-build nope 1",
        "HELLO 6 esp32s3 yd-esp32-s3 test-build 2 1",
        "HELLO 6 esp32s3 yd-esp32-s3 test-build 2 1 1",
        "HELLO 6 esp32s3 yd-esp32-s3 test-build 0",
        "HELLO 6 esp32s3 yd-esp32-s3 test-build 1 -1",
        "HELLO 6 esp32s3 yd-esp32-s3 test-build 1 256",
    ],
)
def test_smoke_rejects_invalid_hello_v6(hello: str) -> None:
    with pytest.raises(RuntimeError, match="invalid HELLO"):
        run_smoke(
            FakeSerial([hello.encode() + b"\n"]),
            family="esp32s3",
            board="yd-esp32-s3",
            build="test-build",
            valid_pins=[1, 2],
            rejected_pins=[],
        )


def test_smoke_cli_requires_build_and_passes_it_to_run_arguments() -> None:
    parser = build_parser()
    arguments = [
        "--serial", "TARGET", "--vid", "0x303a", "--pid", "0x4002",
        "--family", "esp32s3", "--board", "yd-esp32-s3",
        "--valid-pins", "1,2", "--rejected-pins", "99",
    ]

    with pytest.raises(SystemExit):
        parser.parse_args(arguments)
    args = parser.parse_args(arguments + ["--build", "test-build"])
    assert args.build == "test-build"
    device = FakeSerial(
        [
            b"HELLO 9 esp32s3 yd-esp32-s3 test-build - 2 1 2\n",
            b"CONFIG_OK 1\n",
            b"CONFIG_ERROR 2 invalid_direct\n",
            b"LEARN_OK 3\n",
            b"LEARN_OK 3\n",
        ]
    )
    run_from_args(
        args,
        port_waiter=lambda *_: SimpleNamespace(device="/dev/fake"),
        serial_factory=lambda *_args, **_kwargs: device,
    )


@pytest.mark.parametrize(
    ("response", "message"),
    [
        (b"CONFIG_OK 2\n", "CONFIG_OK 1"),
        (b"CONFIG_ERROR 3 invalid_direct\n", "CONFIG_ERROR 2 invalid_direct"),
        (b"CONFIG_ERROR 2 invalid_matrix\n", "CONFIG_ERROR 2 invalid_direct"),
    ],
)
def test_smoke_rejects_wrong_configuration_ack(response: bytes, message: str) -> None:
    responses = [b"HELLO 6 esp32s3 yd-esp32-s3 test-build 2 1 2\n"]
    if response.startswith(b"CONFIG_ERROR"):
        responses.append(b"CONFIG_OK 1\n")
    responses.append(response)
    with pytest.raises(RuntimeError, match=message):
        run_smoke(
            FakeSerial(responses),
            family="esp32s3",
            board="yd-esp32-s3",
            build="test-build",
            valid_pins=[1, 2],
            rejected_pins=[99] if response.startswith(b"CONFIG_ERROR") else [],
        )


def test_smoke_rejects_wrong_learning_ack() -> None:
    device = FakeSerial(
        [
            b"HELLO 6 esp32s3 yd-esp32-s3 test-build 2 1 2\n",
            b"CONFIG_OK 1\n",
            b"LEARN_OK 3\n",
        ]
    )
    with pytest.raises(RuntimeError, match="LEARN_OK 2"):
        run_smoke(
            device,
            family="esp32s3",
            board="yd-esp32-s3",
            build="test-build",
            valid_pins=[1, 2],
            rejected_pins=[],
        )


def test_smoke_actions_use_one_host_created_run_and_sequential_done_steps() -> None:
    device = FakeSerial(
        [
            b"HELLO 6 esp32s3 yd-esp32-s3 test-build 2 1 2\n",
            b"CONFIG_OK 1\n",
            b"LEARN_OK 2\n",
            b"LEARN_OK 2\n",
            b"\n",
            b"STATE 7 DIRECT 1 DOWN\n",
            b"DONE 1 1\n",
            b"DONE 1 2\n",
            b"DONE 1 3\n",
        ]
    )

    run_smoke(
        device,
        family="esp32s3",
        board="yd-esp32-s3",
        build="test-build",
        valid_pins=[1, 2],
        rejected_pins=[],
        exercise_actions=True,
    )

    action_writes = device.writes[6:]
    assert action_writes == [
        b"PASTE 1 1 3\n",
        b"DELAY 1 2 3 500\n",
        b"MEDIA 1 3 3 205\n",
    ]


@pytest.mark.parametrize(
    ("done_responses", "expected_done", "expected_writes"),
    [
        ([b"DONE 8 1\n"], "DONE 1 1", [b"PASTE 1 1 3\n"]),
        ([b"DONE 1 2\n"], "DONE 1 1", [b"PASTE 1 1 3\n"]),
        (
            [b"DONE 1 1\n", b"DONE 1 3\n"],
            "DONE 1 2",
            [b"PASTE 1 1 3\n", b"DELAY 1 2 3 500\n"],
        ),
        (
            [b"DONE 1 1\n", b"DONE 1 2\n", b"DONE 2 3\n"],
            "DONE 1 3",
            [
                b"PASTE 1 1 3\n",
                b"DELAY 1 2 3 500\n",
                b"MEDIA 1 3 3 205\n",
            ],
        ),
    ],
)
def test_smoke_rejects_wrong_action_completion_before_advancing(
    done_responses: list[bytes],
    expected_done: str,
    expected_writes: list[bytes],
) -> None:
    device = FakeSerial(
        [
            b"HELLO 6 esp32s3 yd-esp32-s3 test-build 2 1 2\n",
            b"CONFIG_OK 1\n",
            b"LEARN_OK 2\n",
            b"LEARN_OK 2\n",
            b"STATE 7 DIRECT 1 DOWN\n",
            *done_responses,
        ]
    )
    with pytest.raises(RuntimeError, match=expected_done):
        run_smoke(
            device,
            family="esp32s3",
            board="yd-esp32-s3",
            build="test-build",
            valid_pins=[1, 2],
            rejected_pins=[],
            exercise_actions=True,
        )
    action_writes = device.writes[6:]
    assert action_writes == expected_writes


@pytest.mark.parametrize("protocol_version", [3, 4, 5])
def test_smoke_preserves_legacy_event_id_action_exchange(protocol_version: int) -> None:
    device = FakeSerial(
        [
            (
                f"HELLO {protocol_version} esp32s3 yd-esp32-s3 "
                "test-build 2 1 2\n"
            ).encode(),
            b"CONFIG_OK 1\n",
            b"LEARN_OK 2\n",
            b"LEARN_OK 2\n",
            b"STATE 7 DIRECT 1 DOWN\n",
            b"DONE 7 1\n",
            b"DONE 7 2\n",
        ]
    )

    run_smoke(
        device,
        family="esp32s3",
        board="yd-esp32-s3",
        build="test-build",
        valid_pins=[1, 2],
        rejected_pins=[],
        protocol_version=protocol_version,
        exercise_actions=True,
    )

    assert device.writes[-2:] == [b"PASTE 7 1 2\n", b"HOTKEY 7 2 2 1 25\n"]
