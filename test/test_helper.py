import io
import os
import tempfile
import unittest
from contextlib import redirect_stdout
from pathlib import Path
from unittest.mock import patch

import yaml

from host.text_helper import (
    MappingConfig,
    handle_press,
    main,
    parse_press_line,
    save_mappings,
)


class MappingConfigTests(unittest.TestCase):
    def setUp(self):
        self.temporary_directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary_directory.cleanup)
        self.path = Path(self.temporary_directory.name) / "config.yaml"

    def write_config(self, content: str) -> None:
        previous_mtime = self.path.stat().st_mtime_ns if self.path.exists() else 0
        self.path.write_text(content, encoding="utf-8")
        next_mtime = max(self.path.stat().st_mtime_ns, previous_mtime + 1_000_000)
        os.utime(self.path, ns=(next_mtime, next_mtime))

    def test_loads_unicode_and_multiline_button_mappings(self):
        self.write_config(
            "buttons:\n"
            "  6: |\n"
            "    第一行\n"
            "    第二行\n"
            "  7: 简短文本\n"
        )

        config = MappingConfig(self.path)

        self.assertTrue(config.reload_if_changed())
        self.assertEqual("第一行\n第二行\n", config.buttons[6])
        self.assertEqual("简短文本", config.buttons[7])

    def test_hot_reload_replaces_mapping(self):
        self.write_config("buttons:\n  6: 旧文本\n")
        config = MappingConfig(self.path)
        config.reload_if_changed()

        self.write_config("buttons:\n  6: 新文本\n  7: 新按键\n")

        self.assertTrue(config.reload_if_changed())
        self.assertEqual({6: "新文本", 7: "新按键"}, config.buttons)

    def test_invalid_reload_keeps_last_valid_mapping(self):
        self.write_config("buttons:\n  6: 可用文本\n")
        config = MappingConfig(self.path)
        config.reload_if_changed()

        self.write_config("buttons: [not-a-mapping]\n")

        self.assertFalse(config.reload_if_changed())
        self.assertEqual({6: "可用文本"}, config.buttons)

    def test_rejects_unsafe_gpio_keys(self):
        self.write_config("buttons:\n  10: 不允许\n")
        config = MappingConfig(self.path)

        self.assertFalse(config.reload_if_changed())
        self.assertEqual({}, config.buttons)


class SaveMappingsTests(unittest.TestCase):
    def setUp(self):
        self.temporary_directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary_directory.cleanup)
        self.path = Path(self.temporary_directory.name) / "config.yaml"

    def test_replaces_config_with_unicode_multiline_mapping(self):
        self.path.write_text("buttons:\n  1: old\n", encoding="utf-8")

        save_mappings(self.path, {6: "你好\n第二行", 7: ""})

        self.assertEqual(
            {"buttons": {6: "你好\n第二行"}},
            yaml.safe_load(self.path.read_text(encoding="utf-8")),
        )

    def test_replace_failure_keeps_original_config(self):
        self.path.write_text("buttons:\n  1: old\n", encoding="utf-8")

        with patch("host.text_helper.os.replace", side_effect=OSError("full")):
            with self.assertRaisesRegex(OSError, "full"):
                save_mappings(self.path, {6: "new"})

        self.assertEqual(
            "buttons:\n  1: old\n", self.path.read_text(encoding="utf-8")
        )
        self.assertEqual([self.path], list(self.path.parent.iterdir()))


class ProtocolTests(unittest.TestCase):
    def test_parses_press_line(self):
        self.assertEqual((12, 6), parse_press_line("PRESS 12 6\n"))
        self.assertIsNone(parse_press_line("PRESS nope 6\n"))
        self.assertIsNone(parse_press_line("OTHER 12 6\n"))

    def test_mapped_press_copies_text_and_requests_paste(self):
        copied = []

        response = handle_press(12, 6, {6: "中文\n第二行"}, copied.append)

        self.assertEqual(["中文\n第二行"], copied)
        self.assertEqual("PASTE 12\n", response)

    def test_unmapped_or_empty_press_is_skipped(self):
        copied = []

        self.assertEqual("SKIP 1\n", handle_press(1, 7, {}, copied.append))
        self.assertEqual(
            "SKIP 2\n", handle_press(2, 6, {6: ""}, copied.append)
        )
        self.assertEqual([], copied)


class CliTests(unittest.TestCase):
    def test_keyboard_interrupt_exits_cleanly(self):
        output = io.StringIO()

        with patch("host.text_helper.serve", side_effect=KeyboardInterrupt), patch(
            "sys.argv", ["text-helper"]
        ), redirect_stdout(output):
            try:
                main()
            except KeyboardInterrupt:
                self.fail("main() propagated KeyboardInterrupt")

        self.assertEqual("helper stopped\n", output.getvalue())


if __name__ == "__main__":
    unittest.main()
