import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
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

  expect(screen.getByRole("button", { name: "取消" })).toHaveClass("secondary-button");
  expect(screen.getByRole("button", { name: "保存共享配置" })).toHaveClass("primary-button");
  expect(screen.getByRole("button", { name: "复制并仅用于此设备" })).toHaveClass("secondary-button");

  await user.clear(screen.getByRole("spinbutton", { name: "长按阈值" }));
  await user.type(screen.getByRole("spinbutton", { name: "长按阈值" }), "700");
  await user.click(screen.getByRole("button", { name: "保存共享配置" }));
  expect(onSave).toHaveBeenCalledWith({ long_press_ms: 700, double_press_ms: 300 });

  await user.type(screen.getByRole("textbox", { name: "副本名称" }), " Phone copy");
  await user.click(screen.getByRole("button", { name: "复制并仅用于此设备" }));
  expect(onDuplicate).toHaveBeenCalledWith("Phone Phone copy");
});

test("rejects non-integer timing values", async () => {
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
  await user.clear(screen.getByRole("spinbutton", { name: "双击阈值" }));
  await user.type(screen.getByRole("spinbutton", { name: "双击阈值" }), "12.5");
  expect(screen.getByRole("button", { name: "保存" })).toBeDisabled();
  expect(screen.getByRole("alert")).toHaveTextContent("整数");
});
