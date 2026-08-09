import { useState } from "react";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, test } from "vitest";
import { ActionEditor } from "./ActionEditor";
import type { ButtonAction } from "./types";

function Harness({ initial = [] }: { initial?: ButtonAction[] }) {
  const [actions, setActions] = useState(initial);
  return (
    <>
      <ActionEditor
        language="en-US"
        button={{ id: "A", label: "A" }}
        actions={actions}
        onChange={setActions}
      />
      <output data-testid="actions-json">{JSON.stringify(actions)}</output>
    </>
  );
}

function configuredActions() {
  return JSON.parse(screen.getByTestId("actions-json").textContent ?? "[]") as ButtonAction[];
}

test("configures delay, media, and structured open actions", async () => {
  const user = userEvent.setup();
  render(<Harness />);

  await user.click(screen.getByRole("button", { name: "Wait" }));
  const duration = screen.getByRole("spinbutton", { name: "Wait time (milliseconds)" });
  await user.clear(duration);
  await user.type(duration, "450");

  await user.click(screen.getByRole("button", { name: "Media control" }));
  await user.selectOptions(
    screen.getByRole("combobox", { name: "Media command" }),
    "volume_down",
  );

  await user.click(screen.getByRole("button", { name: "Open target" }));
  await user.type(
    screen.getByRole("textbox", { name: "Application, URL, file, or folder" }),
    "https://example.com",
  );

  expect(configuredActions()).toEqual([
    { type: "delay", duration_ms: 450 },
    { type: "media", command: "volume_down" },
    { type: "open", target: "https://example.com" },
  ]);
});

test("selects extended HID keys and the portable primary modifier", async () => {
  const user = userEvent.setup();
  render(<Harness initial={[{ type: "hotkey", keys: ["enter"] }]} />);

  await user.click(screen.getByRole("checkbox", { name: "Cmd/Ctrl" }));
  const key = screen.getByRole("combobox", { name: "Key" });
  await user.selectOptions(key, "f24");
  expect(screen.getByText("Cmd/Ctrl + F24", { selector: "output" })).toBeInTheDocument();

  await user.selectOptions(key, "numpad_add");
  expect(configuredActions()).toEqual([
    { type: "hotkey", keys: ["primary", "numpad_add"] },
  ]);

  await user.click(screen.getByRole("checkbox", { name: "Ctrl" }));
  expect(screen.getByRole("checkbox", { name: "Cmd/Ctrl" })).not.toBeChecked();
  expect(configuredActions()).toEqual([
    { type: "hotkey", keys: ["ctrl", "numpad_add"] },
  ]);
});
