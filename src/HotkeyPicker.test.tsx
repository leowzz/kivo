import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, test, vi } from "vitest";
import { HotkeyPicker } from "./HotkeyPicker";

test("renders searchable categories and supports a six-key multi-select", async () => {
  const user = userEvent.setup();
  const onChange = vi.fn();
  render(<HotkeyPicker value={[]} onChange={onChange} language="en-US" />);

  expect(screen.getByRole("tab", { name: "Common" })).toHaveAttribute("aria-selected", "true");
  expect(screen.getByRole("tab", { name: "Function Keys F1-F24" })).toBeInTheDocument();
  await user.click(screen.getByRole("tab", { name: "Letters" }));
  await user.click(screen.getByRole("checkbox", { name: "A" }));
  expect(onChange).toHaveBeenLastCalledWith(["a"]);

  await user.click(screen.getByRole("tab", { name: "Common" }));
  const search = screen.getByRole("searchbox", { name: "Search keys" });
  await user.type(search, "escape");
  expect(screen.getByRole("checkbox", { name: "Escape" })).toBeInTheDocument();
  expect(screen.queryByRole("checkbox", { name: "A" })).not.toBeInTheDocument();
});

test("locks unselected ordinary keys at six while selected keys remain removable", async () => {
  const user = userEvent.setup();
  const onChange = vi.fn();
  render(<HotkeyPicker value={["a", "b", "c", "d", "e", "f"]} onChange={onChange} language="en-US" />);

  await user.click(screen.getByRole("tab", { name: "Letters" }));
  expect(screen.getByRole("checkbox", { name: "A" })).toBeChecked();
  expect(screen.getByRole("checkbox", { name: "G" })).toBeDisabled();
  await user.click(screen.getByRole("button", { name: /Remove A/i }));
  expect(onChange).toHaveBeenLastCalledWith(["b", "c", "d", "e", "f"]);
});

test("supports modifier-only and distinct physical modifier sides", async () => {
  const user = userEvent.setup();
  const onChange = vi.fn();
  const { rerender } = render(<HotkeyPicker value={[]} onChange={(keys) => { onChange(keys); rerender(<HotkeyPicker value={keys} onChange={onChange} language="en-US" />); }} language="en-US" />);
  await user.click(screen.getByRole("checkbox", { name: "Command" }));
  expect(onChange).toHaveBeenLastCalledWith(["cmd"]);
  await user.click(screen.getByRole("button", { name: "More modifier keys" }));
  await user.click(screen.getByRole("checkbox", { name: "Right Command" }));
  expect(onChange).toHaveBeenLastCalledWith(["cmd", "right_cmd"]);
});

test("records sided keys only after every captured key is released, including Escape", async () => {
  const user = userEvent.setup();
  const onChange = vi.fn();
  render(<HotkeyPicker value={[]} onChange={onChange} language="en-US" />);
  await user.click(screen.getByRole("button", { name: "Record shortcut" }));
  fireEvent.keyDown(window, { code: "MetaRight", key: "Meta" });
  fireEvent.keyDown(window, { code: "Escape", key: "Escape" });
  fireEvent.keyUp(window, { code: "MetaRight", key: "Meta" });
  expect(onChange).not.toHaveBeenCalled();
  fireEvent.keyUp(window, { code: "Escape", key: "Escape" });
  expect(onChange).toHaveBeenCalledWith(["right_cmd", "escape"]);
});

test("keeps a recording chord across controlled parent rerenders", async () => {
  const user = userEvent.setup();
  const firstChange = vi.fn();
  const secondChange = vi.fn();
  const { rerender } = render(<HotkeyPicker value={[]} onChange={firstChange} language="en-US" />);
  await user.click(screen.getByRole("button", { name: "Record shortcut" }));
  fireEvent.keyDown(window, { code: "KeyK", key: "k" });
  rerender(<HotkeyPicker value={[]} onChange={secondChange} language="en-US" />);
  fireEvent.keyUp(window, { code: "KeyK", key: "k" });
  expect(firstChange).not.toHaveBeenCalled();
  expect(secondChange).toHaveBeenCalledWith(["k"]);
});

test("clears recording state when the picker unmounts", async () => {
  const user = userEvent.setup();
  const onRecordingChange = vi.fn();
  const { unmount } = render(<HotkeyPicker value={[]} onChange={vi.fn()} onRecordingChange={onRecordingChange} language="en-US" />);
  await user.click(screen.getByRole("button", { name: "Record shortcut" }));
  expect(onRecordingChange).toHaveBeenLastCalledWith(true);
  unmount();
  expect(onRecordingChange).toHaveBeenLastCalledWith(false);
});

test("aborts an unsupported recording without replacing the previous chord", async () => {
  const user = userEvent.setup();
  const onChange = vi.fn();
  render(<HotkeyPicker value={["a"]} onChange={onChange} language="en-US" />);
  await user.click(screen.getByRole("button", { name: "Record shortcut" }));
  fireEvent.keyDown(window, { code: "IntlRo", key: "ろ" });
  expect(onChange).not.toHaveBeenCalled();
  expect(screen.getByText("This key cannot be recorded")).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Record shortcut" })).toBeInTheDocument();
});

test("localizes categories but keeps protocol key names stable", async () => {
  render(<HotkeyPicker value={[]} onChange={vi.fn()} language="zh-CN" />);
  expect(screen.getByRole("tablist", { name: "按键分类" })).toBeInTheDocument();
  expect(screen.getByRole("tab", { name: "常用" })).toHaveAttribute("aria-selected", "true");
  expect(screen.getByRole("checkbox", { name: "Command" })).toBeInTheDocument();
  expect(screen.getByRole("checkbox", { name: "Control" })).toBeInTheDocument();
  expect(screen.getByRole("checkbox", { name: "Option / Alt" })).toBeInTheDocument();
  expect(screen.getByRole("checkbox", { name: "Shift" })).toBeInTheDocument();
  expect(screen.getByRole("checkbox", { name: "Enter" })).toBeInTheDocument();
  expect(screen.getByRole("checkbox", { name: "Backspace" })).toBeInTheDocument();
  expect(screen.getByRole("checkbox", { name: "Space" })).toBeInTheDocument();
  expect(screen.getByRole("checkbox", { name: "Caps Lock" })).toBeInTheDocument();
  expect(screen.queryByRole("checkbox", { name: "命令" })).not.toBeInTheDocument();
  expect(screen.queryByRole("checkbox", { name: "回车" })).not.toBeInTheDocument();
});

test("shows one category panel and keeps recording above search", async () => {
  const user = userEvent.setup();
  render(<HotkeyPicker value={["enter"]} onChange={vi.fn()} language="zh-CN" />);

  const record = screen.getByRole("button", { name: "录入快捷键" });
  const search = screen.getByRole("searchbox", { name: "搜索按键" });
  expect(record.compareDocumentPosition(search) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  expect(screen.getAllByRole("tabpanel")).toHaveLength(1);
  expect(screen.getByRole("checkbox", { name: "Enter" })).toBeChecked();

  await user.click(screen.getByRole("tab", { name: "功能键 F1-F24" }));
  expect(screen.getAllByRole("tabpanel")).toHaveLength(1);
  expect(screen.getByRole("checkbox", { name: "F1" })).toBeInTheDocument();
  expect(screen.getByRole("checkbox", { name: "F24" })).toBeInTheDocument();
  expect(screen.queryByRole("checkbox", { name: "Enter" })).not.toBeInTheDocument();
  expect(screen.getByRole("button", { name: "移除 Enter" })).toBeInTheDocument();
});

test("only the active category tab controls the rendered panel", async () => {
  const user = userEvent.setup();
  render(<HotkeyPicker value={[]} onChange={vi.fn()} language="en-US" />);

  const expectTabPanelRelationship = () => {
    const panel = screen.getByRole("tabpanel");
    const activeTab = screen.getByRole("tab", { selected: true });

    for (const tab of screen.getAllByRole("tab")) {
      if (tab === activeTab) {
        expect(tab).toHaveAttribute("aria-controls", panel.id);
        expect(document.getElementById(tab.getAttribute("aria-controls")!)).toBe(panel);
      } else {
        expect(tab).not.toHaveAttribute("aria-controls");
      }
    }
    expect(panel).toHaveAttribute("aria-labelledby", activeTab.id);
  };

  expectTabPanelRelationship();
  await user.click(screen.getByRole("tab", { name: "Letters" }));
  expectTabPanelRelationship();
});
