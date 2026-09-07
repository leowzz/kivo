import { useState } from "react";
import { fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, test, vi } from "vitest";
import { ActionEditor } from "./ActionEditor";
import type { Language, TriggerActions } from "./types";

const emptyGroups = (): TriggerActions => ({
  press: [],
  release: [],
  long_press: [],
  double_press: [],
});

function Harness({ initial = emptyGroups(), onChange, onRename, language = "en-US" }: {
  initial?: TriggerActions;
  onChange?: (actions: TriggerActions) => void;
  onRename?: (buttonId: string, label: string) => void;
  language?: Language;
}) {
  const [actions, setActions] = useState(initial);
  const [buttonLabel, setButtonLabel] = useState("A");
  const [, setRenderTick] = useState(0);
  const update = (next: TriggerActions) => {
    setActions(next);
    onChange?.(next);
  };
  return (
    <>
      <button type="button" onClick={() => setRenderTick((value) => value + 1)}>Rerender parent</button>
      <button type="button" onClick={() => setActions(emptyGroups())}>Clear actions</button>
      <ActionEditor
        language={language}
        button={{ id: "stable-id", label: buttonLabel }}
        actions={actions}
        onChange={update}
        onRename={(buttonId, label) => {
          onRename?.(buttonId, label);
          setButtonLabel(label);
        }}
      />
      <output data-testid="actions-json">{JSON.stringify(actions)}</output>
    </>
  );
}

function configuredActions() {
  return JSON.parse(screen.getByTestId("actions-json").textContent ?? "{}") as TriggerActions;
}

test("edits and trims the selected button note", async () => {
  const user = userEvent.setup();
  render(<Harness language="zh-CN" />);

  const note = screen.getByRole("textbox", { name: "按键备注" });
  await user.type(note, "  发给当前联系人  ");
  await user.tab();

  expect(configuredActions().note).toBe("发给当前联系人");
});

test("keeps press visible and collapses advanced trigger groups by default", async () => {
  const user = userEvent.setup();
  render(<Harness initial={{
    ...emptyGroups(),
    press: [{ type: "paste", text: "Hello from Kivo" }],
    release: [{ type: "media", command: "play_pause" }],
    long_press: [{ type: "paste", text: "Long press" }],
    double_press: [{ type: "delay", duration_ms: 300 }],
  }} />);

  expect(screen.getAllByRole("heading", { level: 3 }).map((node) => node.textContent))
    .toEqual(["Action library", "Press"]);
  expect(screen.getByText("Paste - Hello from Kivo")).toBeInTheDocument();
  expect(screen.getByText("More trigger options")).toBeInTheDocument();
  expect(screen.queryByRole("heading", { name: "Long press", level: 3 })).not.toBeInTheDocument();

  await user.click(screen.getByText("More trigger options"));
  expect(screen.getAllByRole("heading", { level: 3 }).map((node) => node.textContent))
    .toEqual(["Action library", "Press", "Release", "Long press", "Double press"]);
  expect(screen.getByText("Wait - 300 ms")).toBeInTheDocument();
});

test("searches the action library and filters by action type", async () => {
  const user = userEvent.setup();
  render(<Harness />);

  const library = screen.getByRole("region", { name: "Action library" });
  const libraryItems = library.querySelector(".action-library-items") as HTMLElement;
  const search = screen.getByRole("searchbox", { name: "Search actions" });
  expect(within(libraryItems).getByRole("button", { name: /Paste text/ })).toBeInTheDocument();
  await user.type(search, "media");

  expect(within(libraryItems).getByRole("button", { name: /Media control/ })).toBeInTheDocument();
  expect(within(libraryItems).queryByRole("button", { name: /Paste text/ })).not.toBeInTheDocument();

  const categories = screen.getByRole("group", { name: "Action categories" });
  await user.click(within(categories).getByRole("button", { name: "Media control" }));
  expect(within(libraryItems).getByRole("button", { name: /Media control/ })).toBeInTheDocument();
  expect(within(libraryItems).queryByRole("button", { name: /Paste text/ })).not.toBeInTheDocument();
});

test("clicking a library entry adds a press action and records its recent type", async () => {
  const user = userEvent.setup();
  const onChange = vi.fn();
  render(<Harness onChange={onChange} />);

  const library = screen.getByRole("region", { name: "Action library" });
  const libraryItems = library.querySelector(".action-library-items") as HTMLElement;
  await user.click(within(libraryItems).getByRole("button", { name: /Paste text/ }));
  expect(configuredActions().press).toEqual([{ type: "paste", text: "Paste text" }]);
  expect(screen.getByText("Paste - Paste text")).toBeInTheDocument();
  expect(onChange).toHaveBeenCalledTimes(1);

  const categories = screen.getByRole("group", { name: "Action categories" });
  await user.click(within(categories).getByRole("button", { name: "Recent" }));
  expect(within(libraryItems).getByRole("button", { name: /Paste text/ })).toBeInTheDocument();
  expect(within(libraryItems).queryByRole("button", { name: /Press key/ })).not.toBeInTheDocument();
});

test("dragging a library entry onto the press group adds the same action as clicking", () => {
  const dataTransfer = {
    dropEffect: "none",
    effectAllowed: "none",
    types: ["application/x-kivo-action"],
    getData: vi.fn(() => "delay"),
    setData: vi.fn(),
  };
  render(<Harness />);

  const library = screen.getByRole("region", { name: "Action library" });
  const libraryItems = library.querySelector(".action-library-items") as HTMLElement;
  const entry = within(libraryItems).getByRole("button", { name: /Wait/ });
  const pressGroup = screen.getByRole("region", { name: "Press" });

  expect(entry).toHaveAttribute("draggable", "true");
  fireEvent.dragStart(entry, { dataTransfer });
  expect(dataTransfer.setData).toHaveBeenCalledWith("application/x-kivo-action", "delay");
  fireEvent.dragOver(pressGroup, { dataTransfer });
  fireEvent.drop(pressGroup, { dataTransfer });

  expect(configuredActions().press).toEqual([{ type: "delay", duration_ms: 500 }]);
  expect(screen.getByText("Wait - 500 ms")).toBeInTheDocument();
});

test("dragging a library entry onto an advanced trigger adds it to that sequence", async () => {
  const user = userEvent.setup();
  const dataTransfer = {
    dropEffect: "none",
    effectAllowed: "none",
    types: ["application/x-kivo-action"],
    getData: vi.fn(() => "media"),
    setData: vi.fn(),
  };
  render(<Harness />);

  await user.click(screen.getByText("More trigger options"));
  const library = screen.getByRole("region", { name: "Action library" });
  const releaseGroup = screen.getByRole("region", { name: "Release" });
  const entry = within(library.querySelector(".action-library-items") as HTMLElement)
    .getByRole("button", { name: /Media control/ });

  fireEvent.dragStart(entry, { dataTransfer });
  fireEvent.dragOver(releaseGroup, { dataTransfer });
  fireEvent.drop(releaseGroup, { dataTransfer });

  expect(configuredActions().press).toEqual([]);
  expect(configuredActions().release).toEqual([{ type: "media", command: "play_pause" }]);
  expect(screen.getByText("Media - Play / pause")).toBeInTheDocument();
});

test("renames the selected button while preserving its stable id", async () => {
  const user = userEvent.setup();
  const onRename = vi.fn();
  render(<Harness onRename={onRename} />);

  await user.click(screen.getByRole("button", { name: "Rename button A" }));
  const label = screen.getByRole("textbox", { name: "Button name" });
  await user.clear(label);
  await user.type(label, "  Launch  ");
  await user.click(screen.getByRole("button", { name: "Confirm rename" }));

  expect(onRename).toHaveBeenCalledWith("stable-id", "Launch");
  expect(screen.getByRole("heading", { name: "Launch" })).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Rename button Launch" })).toBeInTheDocument();
});

test("cancels a button rename without changing the label", async () => {
  const user = userEvent.setup();
  const onRename = vi.fn();
  render(<Harness onRename={onRename} />);

  await user.click(screen.getByRole("button", { name: "Rename button A" }));
  const label = screen.getByRole("textbox", { name: "Button name" });
  await user.clear(label);
  await user.type(label, "Changed");
  await user.click(screen.getByRole("button", { name: "Cancel" }));

  expect(onRename).not.toHaveBeenCalled();
  expect(screen.getByRole("heading", { name: "A" })).toBeInTheDocument();
});

test("does not save a blank button name", async () => {
  const user = userEvent.setup();
  const onRename = vi.fn();
  render(<Harness onRename={onRename} />);

  await user.click(screen.getByRole("button", { name: "Rename button A" }));
  const label = screen.getByRole("textbox", { name: "Button name" });
  await user.clear(label);
  await user.type(label, "   ");

  expect(screen.getByRole("button", { name: "Confirm rename" })).toBeDisabled();
  expect(onRename).not.toHaveBeenCalled();
});

test("uses the picker technical key names in Action summaries", () => {
  render(<Harness initial={{
    ...emptyGroups(),
    press: [{ type: "hotkey", keys: ["primary", "alt", "left_alt", "right_alt"] }],
  }} />);

  expect(screen.getByText(
    "Hotkey - Primary (Command / Control) + Option / Alt + Left Option / Alt + Right Option / Alt",
  )).toBeInTheDocument();
});

test("localizes hotkey summaries without rewriting action tokens", () => {
  render(<Harness language="zh-CN" initial={{
    ...emptyGroups(),
    press: [{ type: "hotkey", keys: ["right_cmd", "left"] }],
  }} />);

  expect(screen.getByText("快捷键 - 右cmd + 方向左")).toBeInTheDocument();
  expect(configuredActions().press).toEqual([
    { type: "hotkey", keys: ["right_cmd", "left"] },
  ]);
});

test("changing trigger appends the Action to the destination group", async () => {
  const user = userEvent.setup();
  const onChange = vi.fn();
  render(<Harness
    initial={{ ...emptyGroups(), press: [{ type: "paste", text: "A" }] }}
    onChange={onChange}
  />);

  await user.click(screen.getByRole("button", { name: "Edit Paste - A" }));
  await user.selectOptions(screen.getByLabelText("Trigger"), "release");
  await user.click(screen.getByRole("button", { name: "Save" }));

  expect(onChange).toHaveBeenCalledTimes(1);
  expect(onChange.mock.calls[0][0].press).toEqual([]);
  expect(onChange.mock.calls[0][0].release).toEqual([{ type: "paste", text: "A" }]);
});

test("keeps dialog edits local until save and supports cancel", async () => {
  const user = userEvent.setup();
  const onChange = vi.fn();
  render(<Harness
    initial={{ ...emptyGroups(), press: [{ type: "paste", text: "A" }] }}
    onChange={onChange}
  />);

  await user.click(screen.getByRole("button", { name: "Edit Paste - A" }));
  const text = screen.getByRole("textbox", { name: "Text" });
  await user.clear(text);
  await user.type(text, "changed");
  expect(onChange).not.toHaveBeenCalled();
  await user.click(screen.getByRole("button", { name: "Cancel" }));
  expect(configuredActions().press).toEqual([{ type: "paste", text: "A" }]);
});

test("preserves an in-progress dialog draft when its parent rerenders", async () => {
  const user = userEvent.setup();
  render(<Harness initial={{ ...emptyGroups(), press: [{ type: "paste", text: "A" }] }} />);

  await user.click(screen.getByRole("button", { name: "Edit Paste - A" }));
  const text = screen.getByRole("textbox", { name: "Text" });
  await user.clear(text);
  await user.type(text, "draft survives");
  await user.click(screen.getByRole("button", { name: "Rerender parent" }));

  expect(screen.getByRole("textbox", { name: "Text" })).toHaveValue("draft survives");
});

test("does not apply an edit after its source action disappears", async () => {
  const user = userEvent.setup();
  const onChange = vi.fn();
  render(<Harness
    initial={{ ...emptyGroups(), press: [{ type: "paste", text: "A" }] }}
    onChange={onChange}
  />);

  await user.click(screen.getByRole("button", { name: "Edit Paste - A" }));
  await user.click(screen.getByRole("button", { name: "Clear actions" }));
  await user.click(screen.getByRole("button", { name: "Save" }));

  expect(onChange).not.toHaveBeenCalled();
  expect(configuredActions().press).toEqual([]);
});

test("deletes an action from its trigger group", async () => {
  const user = userEvent.setup();
  render(<Harness initial={{ ...emptyGroups(), release: [{ type: "delay", duration_ms: 100 }] }} />);

  await user.click(screen.getByText("More trigger options"));
  await user.click(screen.getByRole("button", { name: "Edit Wait - 100 ms" }));
  await user.click(screen.getByRole("button", { name: "Delete action" }));

  expect(configuredActions().release).toEqual([]);
  expect(configuredActions().press).toEqual([]);
});

test("moves actions only within the same trigger group", async () => {
  const user = userEvent.setup();
  render(<Harness initial={{
    ...emptyGroups(),
    press: [
      { type: "paste", text: "first" },
      { type: "paste", text: "second" },
    ],
    release: [{ type: "delay", duration_ms: 10 }],
  }} />);

  const moveUp = screen.getAllByRole("button", { name: "Move up" })[1];
  await user.click(moveUp);
  expect(configuredActions().press.map((action) => (action.type === "paste" ? action.text : "")))
    .toEqual(["second", "first"]);
  expect(configuredActions().release).toEqual([{ type: "delay", duration_ms: 10 }]);
});

test("drags actions to reorder within one trigger group", () => {
  const values = new Map<string, string>();
  const dataTransfer = {
    dropEffect: "none",
    effectAllowed: "none",
    types: [] as string[],
    getData: vi.fn((type: string) => values.get(type) ?? ""),
    setData: vi.fn((type: string, value: string) => {
      values.set(type, value);
      if (!dataTransfer.types.includes(type)) dataTransfer.types.push(type);
    }),
  };
  render(<Harness initial={{
    ...emptyGroups(),
    press: [
      { type: "paste", text: "first" },
      { type: "paste", text: "second" },
      { type: "paste", text: "third" },
    ],
  }} />);

  const rows = document.querySelectorAll<HTMLElement>(".action-row");
  fireEvent.dragStart(rows[0], { dataTransfer });
  fireEvent.dragOver(rows[2], { dataTransfer });
  fireEvent.drop(rows[2], { dataTransfer });

  expect(dataTransfer.setData).toHaveBeenCalledWith(
    "application/x-kivo-action-index",
    JSON.stringify({ trigger: "press", index: 0 }),
  );
  expect(configuredActions().press.map((action) =>
    action.type === "paste" ? action.text : ""
  )).toEqual(["second", "third", "first"]);
});

test("ignores an out-of-range action reorder payload", () => {
  const onChange = vi.fn();
  const dataTransfer = {
    dropEffect: "none",
    effectAllowed: "none",
    types: ["application/x-kivo-action-index"],
    getData: vi.fn(() => JSON.stringify({ trigger: "press", index: -1 })),
    setData: vi.fn(),
  };
  render(<Harness
    onChange={onChange}
    initial={{ ...emptyGroups(), press: [{ type: "paste", text: "only" }] }}
  />);

  const row = document.querySelector<HTMLElement>(".action-row");
  expect(row).not.toBeNull();
  fireEvent.drop(row as HTMLElement, { dataTransfer });

  expect(onChange).not.toHaveBeenCalled();
  expect(configuredActions().press).toEqual([{ type: "paste", text: "only" }]);
});

test("creates a new action with the default press trigger", async () => {
  const user = userEvent.setup();
  render(<Harness />);

  await user.click(screen.getByRole("button", { name: "Add action" }));
  expect(screen.getByLabelText("Trigger")).toHaveValue("press");
  expect(screen.getByLabelText("Action type")).toHaveValue("hotkey");
  await user.click(screen.getByLabelText("Action type"));
  await user.selectOptions(screen.getByLabelText("Action type"), "paste");
  await user.type(screen.getByRole("textbox", { name: "Text" }), "new");
  await user.click(screen.getByRole("button", { name: "Save" }));
  expect(configuredActions().press).toEqual([{ type: "paste", text: "new" }]);
});
