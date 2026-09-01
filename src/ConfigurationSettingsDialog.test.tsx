import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
// Vitest runs this source assertion in Node, while the production tsconfig excludes Node globals.
// @ts-expect-error Test-only Node module.
import { readFileSync } from "node:fs";
import { expect, test, vi } from "vitest";
import { ConfigurationSettingsDialog } from "./ConfigurationSettingsDialog";
import type { DeviceProfile } from "./types";

const profile: DeviceProfile = {
  schema_version: 3,
  profile: { id: "phone", name: "Phone", groups: [] },
  trigger_settings: { long_press_ms: 500, double_press_ms: 300 },
  hardware_profiles: [],
  actions: {},
};
const viewCss = readFileSync("src/styles/views.css", "utf8");

test("keeps configuration settings actions in a stable footer", () => {
  expect(viewCss).toMatch(/\.configuration-settings-dialog\s*\{[^}]*overflow:\s*hidden/);
  expect(viewCss).toMatch(/\.configuration-settings-dialog \.device-setup-body\s*\{[^}]*overflow:\s*auto/);
  expect(viewCss).toMatch(/\.configuration-settings-dialog \.device-setup-footer\s*\{[^}]*justify-content:\s*flex-end/);
});

test("saves validated timing settings and can duplicate the complete draft", async () => {
  const user = userEvent.setup();
  const onSave = vi.fn();
  const onDuplicate = vi.fn().mockResolvedValue(undefined);
  render(
    <ConfigurationSettingsDialog
      open
      profile={profile}
      sharedDeviceCount={2}
      onSave={onSave}
      onDuplicate={onDuplicate}
      onCancel={vi.fn()}
    />,
  );

  expect(screen.getByRole("spinbutton", { name: "长按阈值（秒）" })).toHaveAttribute("step", "0.001");
  expect(screen.getByRole("spinbutton", { name: "双击阈值（秒）" })).toHaveAttribute("step", "0.001");

  expect(screen.getByRole("button", { name: "取消" })).toHaveClass("secondary-button");
  expect(screen.getByRole("button", { name: "保存共享配置" })).toHaveClass("primary-button");
  expect(screen.getByRole("button", { name: "复制并仅用于此设备" })).toHaveClass("secondary-button");

  await user.clear(screen.getByRole("spinbutton", { name: "长按阈值（秒）" }));
  await user.type(screen.getByRole("spinbutton", { name: "长按阈值（秒）" }), "0.7");
  await user.click(screen.getByRole("button", { name: "保存共享配置" }));
  expect(onSave).toHaveBeenCalledWith({ long_press_ms: 700, double_press_ms: 300 });

  await user.type(screen.getByRole("textbox", { name: "副本名称" }), " Phone copy");
  await user.click(screen.getByRole("button", { name: "复制并仅用于此设备" }));
  expect(onDuplicate).toHaveBeenCalledWith("Phone Phone copy");
});

test("rejects timing values outside the second-based ranges", async () => {
  const user = userEvent.setup();
  render(
    <ConfigurationSettingsDialog
      open
      profile={profile}
      sharedDeviceCount={1}
      onSave={vi.fn()}
      onDuplicate={vi.fn()}
      onCancel={vi.fn()}
    />,
  );
  await user.clear(screen.getByRole("spinbutton", { name: "双击阈值（秒）" }));
  await user.type(screen.getByRole("spinbutton", { name: "双击阈值（秒）" }), "0.05");
  expect(screen.getByRole("button", { name: "保存" })).toBeDisabled();
  expect(screen.getByRole("alert")).toHaveTextContent("秒数范围");
});

test("converts fractional seconds to exact integer milliseconds", async () => {
  const user = userEvent.setup();
  const onSave = vi.fn();
  render(
    <ConfigurationSettingsDialog
      open
      profile={profile}
      sharedDeviceCount={1}
      onSave={onSave}
      onDuplicate={vi.fn()}
      onCancel={vi.fn()}
    />,
  );

  await user.clear(screen.getByRole("spinbutton", { name: "双击阈值（秒）" }));
  await user.type(screen.getByRole("spinbutton", { name: "双击阈值（秒）" }), "0.125");
  await user.click(screen.getByRole("button", { name: "保存" }));

  expect(onSave).toHaveBeenCalledWith({ long_press_ms: 500, double_press_ms: 125 });
});

test("keeps decimal entry intact when a shared draft rerenders its parent", async () => {
  const user = userEvent.setup();

  function Harness() {
    const [currentProfile, setCurrentProfile] = useState(profile);
    return (
      <ConfigurationSettingsDialog
        open
        profile={currentProfile}
        sharedDeviceCount={2}
        onSave={vi.fn()}
        onDuplicate={vi.fn()}
        onCancel={vi.fn()}
        onDraftChange={(settings) => setCurrentProfile((current) => ({ ...current, trigger_settings: settings }))}
      />
    );
  }

  render(<Harness />);
  const input = screen.getByRole("spinbutton", { name: "长按阈值（秒）" });
  await user.clear(input);
  await user.type(input, "0.7");

  expect(input).toHaveValue(0.7);
});
