import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, test, vi } from "vitest";
import { CreateDeviceProfileForm } from "./CreateDeviceProfileForm";
import type { BoardProfileSummary, DeviceProfile } from "./types";

const boards: BoardProfileSummary[] = [
  {
    id: "rp",
    controllerFamilyId: "rp2040",
    displayName: "RP2040 Pad",
    runtimeUsb: "2e8a:102e",
    bootloaderUsb: "2e8a:0003",
    safePins: [0, 1],
  },
  {
    id: "esp",
    controllerFamilyId: "esp32s3",
    displayName: "ESP32 Pad",
    runtimeUsb: "303a:4002",
    bootloaderUsb: null,
    safePins: [1, 2],
  },
];

const profiles: DeviceProfile[] = [
  {
    schema_version: 2,
    profile: { id: "rp-source", name: "RP Source", groups: [] },
    hardware_profiles: [
      {
        id: "rp-hardware",
        name: "RP Hardware",
        board_profile_id: "rp",
        debounce_ms: 30,
        inputs: [],
      },
    ],
    actions: {},
  },
  {
    schema_version: 2,
    profile: { id: "esp-source", name: "ESP Source", groups: [] },
    hardware_profiles: [
      {
        id: "esp-hardware",
        name: "ESP Hardware",
        board_profile_id: "esp",
        debounce_ms: 30,
        inputs: [],
      },
    ],
    actions: {},
  },
];

test("submits a blank profile for the fixed device board", async () => {
  const user = userEvent.setup();
  const onCreate = vi.fn().mockResolvedValue(undefined);
  render(
    <CreateDeviceProfileForm
      language="zh-CN"
      boardProfiles={boards}
      deviceProfiles={profiles}
      fixedBoardProfileId="rp"
      onCreate={onCreate}
      onCancel={vi.fn()}
    />,
  );

  await user.click(screen.getByRole("radio", { name: "空白配置" }));
  await user.type(screen.getByRole("textbox", { name: "配置名称" }), "桌面键盘");
  expect(screen.queryByRole("combobox", { name: "板型" })).toBeNull();
  await user.click(screen.getByRole("button", { name: "创建配置" }));

  expect(onCreate).toHaveBeenCalledWith({
    kind: "blank",
    name: "桌面键盘",
    board_profile_id: "rp",
  });
});

test("filters clone sources by a fixed board and submits once while pending", async () => {
  const user = userEvent.setup();
  let resolveCreate!: () => void;
  const onCreate = vi.fn(
    () =>
      new Promise<void>((resolve) => {
        resolveCreate = resolve;
      }),
  );
  render(
    <CreateDeviceProfileForm
      language="zh-CN"
      boardProfiles={boards}
      deviceProfiles={profiles}
      fixedBoardProfileId="rp"
      onCreate={onCreate}
      onCancel={vi.fn()}
    />,
  );

  expect(screen.getByRole("option", { name: "RP Source" })).toBeInTheDocument();
  expect(screen.queryByRole("option", { name: "ESP Source" })).toBeNull();
  await user.type(screen.getByRole("textbox", { name: "配置名称" }), "RP 副本");
  const create = screen.getByRole("button", { name: "创建配置" });
  await user.click(create);
  expect(onCreate).toHaveBeenCalledWith({
    kind: "clone",
    name: "RP 副本",
    source_profile_id: "rp-source",
  });
  expect(create).toBeDisabled();
  await user.click(create);
  expect(onCreate).toHaveBeenCalledTimes(1);
  resolveCreate();
});

test("requires a board for an independent blank profile", async () => {
  const user = userEvent.setup();
  const onCreate = vi.fn();
  render(
    <CreateDeviceProfileForm
      language="zh-CN"
      boardProfiles={boards}
      deviceProfiles={[]}
      onCreate={onCreate}
      onCancel={vi.fn()}
    />,
  );

  await user.type(screen.getByRole("textbox", { name: "配置名称" }), "离线配置");
  await user.selectOptions(screen.getByRole("combobox", { name: "板型" }), "esp");
  await user.click(screen.getByRole("button", { name: "创建配置" }));

  expect(onCreate).toHaveBeenCalledWith({
    kind: "blank",
    name: "离线配置",
    board_profile_id: "esp",
  });
});
