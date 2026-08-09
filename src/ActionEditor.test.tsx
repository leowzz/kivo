import { useState } from "react";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, test, vi } from "vitest";
import { ActionEditor } from "./ActionEditor";
import type { TriggerActions } from "./types";

const emptyGroups = (): TriggerActions => ({
  press: [],
  release: [],
  long_press: [],
  double_press: [],
});

function Harness({ initial = emptyGroups(), onChange }: {
  initial?: TriggerActions;
  onChange?: (actions: TriggerActions) => void;
}) {
  const [actions, setActions] = useState(initial);
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
        language="en-US"
        button={{ id: "A", label: "A" }}
        actions={actions}
        onChange={update}
      />
      <output data-testid="actions-json">{JSON.stringify(actions)}</output>
    </>
  );
}

function configuredActions() {
  return JSON.parse(screen.getByTestId("actions-json").textContent ?? "{}") as TriggerActions;
}

test("shows only populated groups in trigger order with compact summaries", () => {
  render(<Harness initial={{
    ...emptyGroups(),
    press: [{ type: "paste", text: "Hello from Kivo" }],
    release: [{ type: "media", command: "play_pause" }],
    double_press: [{ type: "delay", duration_ms: 300 }],
  }} />);

  expect(screen.getAllByRole("heading", { level: 3 }).map((node) => node.textContent))
    .toEqual(["Press", "Release", "Double press"]);
  expect(screen.getByText("Paste - Hello from Kivo")).toBeInTheDocument();
  expect(screen.getByText("Wait - 300 ms")).toBeInTheDocument();
  expect(screen.queryByRole("heading", { name: "Long press", level: 3 })).not.toBeInTheDocument();
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
