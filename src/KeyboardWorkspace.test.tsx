import { render, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
// Vitest runs this source assertion in Node, while the production tsconfig excludes Node globals.
// @ts-expect-error Test-only Node module.
import { readFileSync } from "node:fs";
import { expect, test, vi } from "vitest";
import { KeyboardWorkspace } from "./KeyboardWorkspace";
import type { DeviceProfile, DeviceStatus, TriggerActions } from "./types";

const viewCss = readFileSync("src/styles/views.css", "utf8");

const profile: DeviceProfile = {
  schema_version: 3,
  profile: {
    id: "desk-profile",
    name: "Desk profile",
    groups: [{
      id: "keys",
      columns: 2,
      buttons: [{ id: "COPY", label: "复制" }, { id: "PASTE", label: "粘贴" }],
    }],
  },
  trigger_settings: { long_press_ms: 500, double_press_ms: 300 },
  hardware_profiles: [],
  actions: {
    COPY: { press: [{ type: "paste", text: "copy" }], release: [], long_press: [], double_press: [] },
  },
};

function device(overrides: Partial<DeviceStatus> = {}): DeviceStatus {
  return {
    deviceId: "desk-device",
    name: "Desk keyboard",
    connection: "online",
    mode: "runtime",
    identity: "valid",
    assignment: "valid",
    runtime: "ready",
    hardwareSerial: "DESK-001",
    port: "/dev/cu.desk",
    controllerFamilyId: "rp2040",
    boardProfileId: "rp",
    firmwareBuildId: "test",
    capabilities: [],
    runtimeAssignment: { device_profile_id: profile.profile.id, hardware_profile_id: "desk" },
    latestError: null,
    learning: null,
    ...overrides,
  };
}

function renderWorkspace({
  device: selectedDevice = device(),
  selectedButtonId = "COPY",
  pressedButtonIds = new Set<string>(),
  selectedProfile = profile,
}: {
  device?: DeviceStatus | null;
  selectedButtonId?: string | null;
  pressedButtonIds?: Set<string>;
  selectedProfile?: DeviceProfile | undefined;
} = {}) {
  const onChangeActions = vi.fn<(buttonId: string, actions: TriggerActions) => void>();
  return render(
    <KeyboardWorkspace
      language="zh-CN"
      device={selectedDevice}
      profile={selectedProfile}
      hasCandidates={false}
      selectedButtonId={selectedButtonId}
      pressedButtonIds={pressedButtonIds}
      onSelectButton={vi.fn()}
      onChangeActions={onChangeActions}
      onRenameButton={vi.fn()}
      onOpenSetup={vi.fn()}
    />,
  );
}

test("shows the connection empty state when no keyboard is selected", () => {
  expect(renderWorkspace({ device: null }).getByText("连接你的键盘")).toBeInTheDocument();
});

test("keeps the keyboard editor geometry stable across desktop and narrow layouts", () => {
  expect(viewCss).toMatch(
    /\.keyboard-workspace\s*\{[^}]*grid-template-columns:\s*minmax\(0,\s*1fr\)\s+minmax\(320px,\s*380px\)/,
  );
  expect(viewCss).toMatch(
    /@media \(max-width: 980px\)[\s\S]*?\.keyboard-workspace\s*\{[^}]*grid-template-columns:\s*1fr[^}]*\}/,
  );
  expect(viewCss).toMatch(
    /@media \(max-width: 980px\)[\s\S]*?\.keyboard-workspace \.action-panel\s*\{[^}]*grid-row:\s*3[^}]*\}/,
  );
  expect(viewCss).toMatch(
    /@media \(max-width: 680px\)[\s\S]*?\.action-dialog[^\{]*\{[^}]*max-height:\s*calc\(100dvh - 20px\)[^}]*\}/,
  );
});

test("continues setup for an unassigned keyboard", () => {
  expect(renderWorkspace({
    device: device({ assignment: "unassigned", runtime: "inactive", runtimeAssignment: null }),
    selectedProfile: undefined,
  }).getByRole("button", { name: "继续设置" })).toBeInTheDocument();
});

test("repairs an invalid keyboard assignment", () => {
  expect(renderWorkspace({
    device: device({ assignment: "invalid_assignment", runtime: "inactive", runtimeAssignment: null }),
    selectedProfile: undefined,
  }).getByRole("button", { name: "修复设置" })).toBeInTheDocument();
});

test("renders the assigned keypad for a ready keyboard", () => {
  const result = renderWorkspace();
  expect(within(result.getByLabelText("Desk profile")).getByRole("button", { name: /复制/ })).toBeInTheDocument();
});

test("keeps an assigned offline keyboard editable and marks it disconnected", () => {
  expect(renderWorkspace({
    device: device({ connection: "offline", mode: null, runtime: "inactive", port: null }),
  }).getByText("未连接，仍可编辑；重新连接后更改将生效")).toBeInTheDocument();
});

test("selecting a key updates the action panel without remounting the keypad", async () => {
  const user = userEvent.setup();
  const onSelectButton = vi.fn();
  const result = render(
    <KeyboardWorkspace
      language="zh-CN"
      device={device()}
      profile={profile}
      hasCandidates={false}
      selectedButtonId="COPY"
      pressedButtonIds={new Set()}
      onSelectButton={onSelectButton}
      onChangeActions={vi.fn()}
      onRenameButton={vi.fn()}
      onOpenSetup={vi.fn()}
    />,
  );
  const keypad = result.getByLabelText("Desk profile");

  await user.click(within(keypad).getByRole("button", { name: /粘贴/ }));
  expect(onSelectButton).toHaveBeenCalledWith("PASTE");
  expect(keypad).toBe(result.getByLabelText("Desk profile"));

  result.rerender(
    <KeyboardWorkspace
      language="zh-CN"
      device={device()}
      profile={profile}
      hasCandidates={false}
      selectedButtonId="PASTE"
      pressedButtonIds={new Set()}
      onSelectButton={onSelectButton}
      onChangeActions={vi.fn()}
      onRenameButton={vi.fn()}
      onOpenSetup={vi.fn()}
    />,
  );
  expect(result.getByRole("heading", { name: "粘贴" })).toBeInTheDocument();
  expect(keypad).toBe(result.getByLabelText("Desk profile"));
});

test("only renders supplied selected-device runtime presses", () => {
  const { getByLabelText, rerender } = render(
    <KeyboardWorkspace
      language="zh-CN"
      device={device()}
      profile={profile}
      hasCandidates={false}
      selectedButtonId="COPY"
      pressedButtonIds={new Set()}
      onSelectButton={vi.fn()}
      onChangeActions={vi.fn()}
      onRenameButton={vi.fn()}
      onOpenSetup={vi.fn()}
    />,
  );
  expect(within(getByLabelText("Desk profile")).getByRole("button", { name: /复制/ })).not.toHaveClass("is-pressed");

  rerender(
    <KeyboardWorkspace
      language="zh-CN"
      device={device()}
      profile={profile}
      hasCandidates={false}
      selectedButtonId="COPY"
      pressedButtonIds={new Set(["COPY"])}
      onSelectButton={vi.fn()}
      onChangeActions={vi.fn()}
      onRenameButton={vi.fn()}
      onOpenSetup={vi.fn()}
    />,
  );
  expect(within(getByLabelText("Desk profile")).getByRole("button", { name: /复制/ })).toHaveClass("is-pressed");
});
