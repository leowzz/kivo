import { fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
// Vitest runs this source assertion in Node, while the production tsconfig excludes Node globals.
// @ts-expect-error Test-only Node module.
import { readFileSync } from "node:fs";
import { expect, test, vi } from "vitest";
import { HardwareMapping, hardwareProfilesAreValid } from "./HardwareMapping";
import type {
  BoardProfileSummary,
  DeviceStatus,
  HardwareProfile,
  ModelLayout,
} from "./types";

const viewCss = readFileSync("src/styles/views.css", "utf8");

const layout: ModelLayout = {
  id: "desk-phone",
  name: "Desk phone",
  groups: [{ id: "keys", columns: 2, buttons: [
    { id: "ONE", label: "1" },
    { id: "TWO", label: "2" },
  ] }],
};

const boardProfiles: BoardProfileSummary[] = [
  {
    id: "esp-board",
    controllerFamilyId: "esp32s3",
    displayName: "ESP Board",
    runtimeUsb: "303a:4002",
    bootloaderUsb: null,
    safePins: [1, 2, 6, 12, 13],
    supportsOled: false,
  },
  {
    id: "yd-rp2040",
    controllerFamilyId: "rp2040",
    displayName: "YD-RP2040",
    runtimeUsb: "2e8a:000a",
    bootloaderUsb: "2e8a:0003",
    safePins: Array.from({ length: 24 }, (_, pin) => pin),
    supportsOled: true,
  },
];

const hardwareProfiles: HardwareProfile[] = [
  {
    id: "front-desk",
    name: "Front desk",
    board_profile_id: "esp-board",
    debounce_ms: 30,
    inputs: [
      { type: "direct", id: "direct", keys: { ONE: 6 } },
      {
        type: "contact_matrix",
        id: "matrix",
        pins: [1, 2, 12, 13],
        keys: { TWO: [1, 12] },
      },
    ],
  },
  {
    id: "back-desk",
    name: "Back desk",
    board_profile_id: "esp-board",
    debounce_ms: 45,
    inputs: [{ type: "direct", id: "direct-back", keys: { ONE: 2 } }],
  },
];

function device(overrides: Partial<DeviceStatus> = {}): DeviceStatus {
  return {
    deviceId: "device-front",
    name: "Front device",
    connection: "online",
    mode: "runtime",
    identity: "valid",
    assignment: "valid",
    runtime: "ready",
    hardwareSerial: "FRONT",
    port: "/dev/front",
    controllerFamilyId: "esp32s3",
    boardProfileId: "esp-board",
    firmwareBuildId: "test",
    capabilities: [1, 6, 13],
    runtimeAssignment: {
      device_profile_id: "desk-phone",
      hardware_profile_id: "front-desk",
    },
    latestError: null,
    learning: null,
    ...overrides,
  };
}

function renderMapping(
  options: {
    profiles?: HardwareProfile[];
    boards?: BoardProfileSummary[];
    devices?: DeviceStatus[];
  } = {},
) {
  const onChange = vi.fn<(profiles: HardwareProfile[]) => void>();
  render(
    <HardwareMapping
      language="zh-CN"
      layout={layout}
      hardwareProfiles={options.profiles ?? structuredClone(hardwareProfiles)}
      boardProfiles={options.boards ?? boardProfiles}
      devices={options.devices ?? []}
      selectedButtonId={null}
      onSelectButton={vi.fn()}
      onChange={onChange}
      onSelectionChange={vi.fn()}
      learning={null}
      onBeginLearning={vi.fn()}
      onEndLearning={vi.fn()}
    />,
  );
  return onChange;
}

test("adds a selected Hardware Profile with a stable ID, name, and compiled Board Profile", async () => {
  const user = userEvent.setup();
  const onChange = vi.fn();
  const props = {
    language: "zh-CN" as const,
    layout,
    boardProfiles,
    devices: [] as DeviceStatus[],
    selectedButtonId: null,
    onSelectButton: vi.fn(),
    onChange,
    onSelectionChange: vi.fn(),
    learning: null,
    onBeginLearning: vi.fn(),
    onEndLearning: vi.fn(),
  };
  const { rerender } = render(<HardwareMapping {...props} hardwareProfiles={structuredClone(hardwareProfiles)} />);

  await user.click(screen.getByRole("button", { name: "添加硬件配置" }));

  const updated = [
    ...hardwareProfiles,
    {
      id: "esp-board-hardware",
      name: "ESP Board 硬件配置",
      board_profile_id: "esp-board",
      debounce_ms: 30,
      inputs: [],
    },
  ];
  expect(onChange).toHaveBeenLastCalledWith(updated);
  rerender(<HardwareMapping {...props} hardwareProfiles={updated} />);
  expect(screen.getByRole("combobox", { name: "硬件配置" })).toHaveValue("esp-board-hardware");
});

test("duplicates topology under a stable new ID and name, then renames only the selected profile", async () => {
  const user = userEvent.setup();
  const onChange = vi.fn();
  const { rerender } = render(
    <HardwareMapping
      language="zh-CN"
      layout={layout}
      hardwareProfiles={structuredClone(hardwareProfiles)}
      boardProfiles={boardProfiles}
      devices={[]}
      selectedButtonId={null}
      onSelectButton={vi.fn()}
      onChange={onChange}
      onSelectionChange={vi.fn()}
      learning={null}
      onBeginLearning={vi.fn()}
      onEndLearning={vi.fn()}
    />,
  );

  await user.click(screen.getByRole("button", { name: "复制硬件配置" }));
  const duplicated = onChange.mock.calls.at(-1)?.[0] as HardwareProfile[];
  expect(duplicated[2]).toEqual({
    ...hardwareProfiles[0],
    id: "front-desk-copy",
    name: "Front desk 副本",
  });
  expect(duplicated[2].inputs).not.toBe(hardwareProfiles[0].inputs);

  rerender(
    <HardwareMapping
      language="zh-CN"
      layout={layout}
      hardwareProfiles={duplicated}
      boardProfiles={boardProfiles}
      devices={[]}
      selectedButtonId={null}
      onSelectButton={vi.fn()}
      onChange={onChange}
      onSelectionChange={vi.fn()}
      learning={null}
      onBeginLearning={vi.fn()}
      onEndLearning={vi.fn()}
    />,
  );
  await user.click(screen.getByRole("button", { name: "重命名硬件配置" }));
  const name = screen.getByRole("textbox", { name: "硬件配置名称" });
  await user.clear(name);
  await user.type(name, "Reception copy{Enter}");

  expect(onChange).toHaveBeenLastCalledWith([
    hardwareProfiles[0],
    hardwareProfiles[1],
    { ...duplicated[2], name: "Reception copy" },
  ]);
});

test("requires confirmation to delete and leaves Device assignments outside the edit unchanged", async () => {
  const user = userEvent.setup();
  const assignedDevice = device();
  const onChange = renderMapping({ devices: [assignedDevice] });

  await user.click(screen.getByRole("button", { name: "删除硬件配置" }));
  expect(onChange).not.toHaveBeenCalled();
  const dialog = screen.getByRole("dialog", { name: "删除硬件配置" });
  expect(within(dialog).getByText(/Front desk/)).toBeInTheDocument();
  await user.click(within(dialog).getByRole("button", { name: "确认" }));

  expect(onChange).toHaveBeenCalledWith([hardwareProfiles[1]]);
  expect(assignedDevice.runtimeAssignment).toEqual({
    device_profile_id: "desk-phone",
    hardware_profile_id: "front-desk",
  });
});

test("keeps same-board profiles distinct and changes only the selected one", async () => {
  const user = userEvent.setup();
  const onChange = renderMapping();

  await user.selectOptions(screen.getByRole("combobox", { name: "硬件配置" }), "back-desk");
  fireEvent.change(screen.getByLabelText("消抖"), { target: { value: "55" } });

  const updated = onChange.mock.calls.at(-1)?.[0] as HardwareProfile[];
  expect(updated[0]).toEqual(hardwareProfiles[0]);
  expect(updated[1]).toEqual({ ...hardwareProfiles[1], debounce_ms: 55 });
});

test("applies a new navigation target once without overriding later manual selection", async () => {
  const user = userEvent.setup();
  const props = {
    language: "zh-CN" as const,
    layout,
    hardwareProfiles: structuredClone(hardwareProfiles),
    boardProfiles,
    devices: [device()],
    selectedButtonId: null,
    onSelectButton: vi.fn(),
    onChange: vi.fn(),
    onSelectionChange: vi.fn(),
    learning: null,
    onBeginLearning: vi.fn(),
    onEndLearning: vi.fn(),
  };
  const { rerender } = render(<HardwareMapping {...props} />);

  await user.selectOptions(
    screen.getByRole("combobox", { name: "硬件配置" }),
    "back-desk",
  );
  rerender(
    <HardwareMapping
      {...props}
      initialHardwareProfileId="front-desk"
      initialDeviceId="device-front"
    />,
  );

  expect(screen.getByRole("combobox", { name: "硬件配置" })).toHaveValue(
    "front-desk",
  );
  await user.click(screen.getByText("适配新设备"));
  expect(screen.getByRole("combobox", { name: "在线设备" })).toHaveValue(
    "device-front",
  );

  await user.selectOptions(
    screen.getByRole("combobox", { name: "硬件配置" }),
    "back-desk",
  );
  rerender(
    <HardwareMapping
      {...props}
      hardwareProfiles={structuredClone(hardwareProfiles)}
      initialHardwareProfileId="front-desk"
      initialDeviceId="device-front"
    />,
  );
  expect(screen.getByRole("combobox", { name: "硬件配置" })).toHaveValue(
    "back-desk",
  );
});

test("retains and visibly marks every invalid pin after changing Board Profile", async () => {
  const user = userEvent.setup();
  const onChange = renderMapping();

  await user.selectOptions(screen.getByLabelText("板型"), "yd-rp2040");

  const updated = onChange.mock.calls.at(-1)?.[0] as HardwareProfile[];
  expect(updated[0].inputs).toEqual(hardwareProfiles[0].inputs);
  expect(screen.getByLabelText("1 GPIO")).toHaveValue("6");

  await user.selectOptions(screen.getByLabelText("板型"), "esp-board");
  const invalidProfiles = structuredClone(hardwareProfiles);
  invalidProfiles[0].inputs[0] = { type: "direct", id: "direct", keys: { ONE: 23 } };
  invalidProfiles[0].inputs[1] = {
    type: "contact_matrix",
    id: "matrix",
    pins: [1, 24, 25],
    keys: { TWO: [24, 25] },
  };
  onChange.mockClear();
  renderMapping({ profiles: invalidProfiles });

  expect(screen.getAllByText(/无效 GPIO/).map((node) => node.textContent)).toEqual(
    expect.arrayContaining(["无效 GPIO 23", "无效 GPIO 24、25"]),
  );
  for (const input of screen.getAllByRole("combobox", { name: /GPIO|A|B/ })) {
    if (["23", "24", "25"].includes(String((input as HTMLSelectElement).value))) {
      expect(input).toHaveAttribute("aria-invalid", "true");
    }
  }
});

test("uses the exact offline safe set and explicitly narrows it with one compatible online Device", async () => {
  const user = userEvent.setup();
  renderMapping({
    devices: [
      device(),
      device({
        deviceId: "wrong-board",
        name: "Wrong board",
        boardProfileId: "yd-rp2040",
        controllerFamilyId: "rp2040",
        capabilities: [0, 1, 2],
      }),
    ],
  });
  await user.click(screen.getByText("适配新设备"));

  expect(screen.getByLabelText("在线设备")).toHaveDisplayValue("离线编辑");
  expect(screen.queryByRole("option", { name: "Wrong board" })).toBeNull();
  expect(screen.getAllByRole("checkbox", { name: /^GPIO/ }).map((input) => input.getAttribute("value")))
    .toEqual(["1", "2", "6", "12", "13"]);

  await user.selectOptions(screen.getByLabelText("在线设备"), "device-front");
  expect(screen.getAllByRole("checkbox", { name: /^GPIO/ }).map((input) => input.getAttribute("value")))
    .toEqual(["1", "6", "13"]);
});

test("offers learning only for online identity-valid runtime Devices on the exact Board Profile", async () => {
  const user = userEvent.setup();
  renderMapping({
    devices: [
      device({ deviceId: "eligible", name: "Eligible runtime" }),
      device({ deviceId: "bootloader", name: "Bootloader", mode: "bootloader" }),
      device({ deviceId: "invalid", name: "Invalid identity", identity: "invalid_identity" }),
      device({ deviceId: "offline", name: "Offline", connection: "offline", mode: null }),
      device({
        deviceId: "wrong-board",
        name: "Wrong board",
        boardProfileId: "yd-rp2040",
        controllerFamilyId: "rp2040",
      }),
    ],
  });

  await user.click(screen.getByText("适配新设备"));

  const picker = screen.getByLabelText("在线设备");
  expect(within(picker).getAllByRole("option").map((option) => option.textContent))
    .toEqual(["离线编辑", "Eligible runtime"]);
  expect(screen.getByRole("button", { name: "开始学习" })).toBeDisabled();
  expect(screen.getByLabelText("消抖")).toBeEnabled();
});

test("exposes GPIO0 through GPIO23 and GPIO26 through GPIO29 for yd-rp2040", async () => {
  const user = userEvent.setup();
  const rpProfile: HardwareProfile = {
    id: "rp",
    name: "RP",
    board_profile_id: "yd-rp2040",
    debounce_ms: 30,
    inputs: [],
  };
  const unsafeRegistry = boardProfiles.map((board) => board.id === "yd-rp2040"
    ? { ...board, safePins: Array.from({ length: 30 }, (_, pin) => pin) }
    : board);
  renderMapping({ profiles: [rpProfile], boards: unsafeRegistry });
  await user.click(screen.getByText("适配新设备"));

  expect(screen.getAllByRole("checkbox", { name: /^GPIO/ }).map((input) => Number(input.getAttribute("value"))))
    .toEqual([
      ...Array.from({ length: 24 }, (_, pin) => pin),
      26,
      27,
      28,
      29,
    ]);
  for (let pin = 24; pin <= 25; pin += 1) {
    expect(screen.queryByRole("checkbox", { name: `GPIO ${pin}` })).toBeNull();
  }
});

test("accepts GPIO26 through GPIO29 across direct, matrix, and OLED ownership", () => {
  const rpBoards = boardProfiles.map((board) => board.id === "yd-rp2040"
    ? { ...board, safePins: Array.from({ length: 30 }, (_, pin) => pin) }
    : board);
  const profile: HardwareProfile = {
    id: "high-gpio",
    name: "High GPIO",
    board_profile_id: "yd-rp2040",
    debounce_ms: 30,
    ssd1306: { sda: 28, scl: 29 },
    inputs: [
      { type: "direct", id: "direct", keys: { ONE: 26 } },
      {
        type: "contact_matrix",
        id: "matrix",
        pins: [0, 27],
        keys: { TWO: [0, 27] },
      },
    ],
  };

  expect(hardwareProfilesAreValid([profile], rpBoards)).toBe(true);
  renderMapping({ profiles: [profile], boards: rpBoards });

  const direct = screen.getByRole("combobox", { name: "1 GPIO" });
  const sda = screen.getByRole("combobox", { name: "OLED SDA" });
  const scl = screen.getByRole("combobox", { name: "OLED SCL" });
  expect(direct).toHaveValue("26");
  expect(direct).not.toHaveAttribute("aria-invalid", "true");
  expect(screen.getByLabelText("矩阵引脚")).not.toHaveAttribute("aria-invalid", "true");
  expect(sda).toHaveValue("28");
  expect(scl).toHaveValue("29");
  expect(sda).not.toHaveAttribute("aria-invalid", "true");
  expect(scl).not.toHaveAttribute("aria-invalid", "true");
  expect(within(direct).queryByRole("option", { name: "28" })).toBeNull();
  expect(within(direct).queryByRole("option", { name: "29" })).toBeNull();
  expect(within(sda).queryByRole("option", { name: "26" })).toBeNull();
  expect(within(sda).queryByRole("option", { name: "27" })).toBeNull();
});

test("configures affected buttons without exposing switch polarity", async () => {
  const user = userEvent.setup();
  const onChange = renderMapping({
    profiles: [{
      ...structuredClone(hardwareProfiles[0]),
      inputs: [{
        type: "feature_switch",
        id: "mode",
        name: "Mode",
        gpio: 12,
        buttons: [],
      }],
    }],
  });

  expect(screen.queryByLabelText("常态")).toBeNull();
  expect(screen.queryByLabelText("以下状态启用按钮")).toBeNull();
  await user.click(screen.getByRole("checkbox", { name: "1" }));

  const updates = onChange.mock.calls.map(([profiles]) => (profiles as HardwareProfile[])[0].inputs[0]);
  expect(updates).toEqual(expect.arrayContaining([
    expect.objectContaining({ buttons: ["ONE"] }),
  ]));
});

test("keeps repeated add names collision-free when custom names occupy generated suffixes", async () => {
  const user = userEvent.setup();
  const onChange = vi.fn<(profiles: HardwareProfile[]) => void>();
  const occupied = [
    ...structuredClone(hardwareProfiles),
    { ...structuredClone(hardwareProfiles[1]), id: "custom-name", name: "ESP Board 硬件配置" },
    { ...structuredClone(hardwareProfiles[1]), id: "custom-name-2", name: "ESP Board 硬件配置 2" },
  ];
  const props = {
    language: "zh-CN" as const,
    layout,
    boardProfiles,
    devices: [] as DeviceStatus[],
    selectedButtonId: null,
    onSelectButton: vi.fn(),
    onChange,
    onSelectionChange: vi.fn(),
    learning: null,
    onBeginLearning: vi.fn(),
    onEndLearning: vi.fn(),
  };
  const { rerender } = render(<HardwareMapping {...props} hardwareProfiles={occupied} />);

  await user.click(screen.getByRole("button", { name: "添加硬件配置" }));
  const firstAdd = onChange.mock.calls.at(-1)?.[0] as HardwareProfile[];
  expect(firstAdd.at(-1)).toMatchObject({
    id: "esp-board-hardware",
    name: "ESP Board 硬件配置 3",
  });
  rerender(<HardwareMapping {...props} hardwareProfiles={firstAdd} />);

  await user.click(screen.getByRole("button", { name: "添加硬件配置" }));
  expect((onChange.mock.calls.at(-1)?.[0] as HardwareProfile[]).at(-1)).toMatchObject({
    id: "esp-board-hardware-2",
    name: "ESP Board 硬件配置 4",
  });
});

test("keeps duplicate names collision-free when custom names occupy generated suffixes", async () => {
  const user = userEvent.setup();
  const onChange = vi.fn<(profiles: HardwareProfile[]) => void>();
  const occupied = [
    ...structuredClone(hardwareProfiles),
    { ...structuredClone(hardwareProfiles[1]), id: "custom-copy", name: "Front desk 副本" },
    { ...structuredClone(hardwareProfiles[1]), id: "front-desk-copy", name: "Front desk 副本 2" },
  ];
  render(
    <HardwareMapping
      language="zh-CN"
      layout={layout}
      hardwareProfiles={occupied}
      boardProfiles={boardProfiles}
      devices={[]}
      selectedButtonId={null}
      onSelectButton={vi.fn()}
      onChange={onChange}
      onSelectionChange={vi.fn()}
      learning={null}
      onBeginLearning={vi.fn()}
      onEndLearning={vi.fn()}
    />,
  );

  await user.click(screen.getByRole("button", { name: "复制硬件配置" }));

  expect((onChange.mock.calls.at(-1)?.[0] as HardwareProfile[]).at(-1)).toMatchObject({
    id: "front-desk-copy-2",
    name: "Front desk 副本 3",
  });
});

test("settles a removed selection on the fallback and does not let a reappearing ID steal it", async () => {
  const user = userEvent.setup();
  const onChange = vi.fn<(profiles: HardwareProfile[]) => void>();
  const props = {
    language: "zh-CN" as const,
    layout,
    boardProfiles,
    devices: [] as DeviceStatus[],
    selectedButtonId: null,
    onSelectButton: vi.fn(),
    onChange,
    onSelectionChange: vi.fn(),
    learning: null,
    onBeginLearning: vi.fn(),
    onEndLearning: vi.fn(),
  };
  const { rerender } = render(<HardwareMapping {...props} hardwareProfiles={structuredClone(hardwareProfiles)} />);
  await user.selectOptions(screen.getByRole("combobox", { name: "硬件配置" }), "back-desk");
  await user.click(screen.getByRole("button", { name: "重命名硬件配置" }));
  await user.clear(screen.getByRole("textbox", { name: "硬件配置名称" }));
  await user.type(screen.getByRole("textbox", { name: "硬件配置名称" }), "Stale draft");

  rerender(<HardwareMapping {...props} hardwareProfiles={[structuredClone(hardwareProfiles[0])]} />);
  expect(screen.getByRole("combobox", { name: "硬件配置" })).toHaveValue("front-desk");
  expect(screen.queryByRole("textbox", { name: "硬件配置名称" })).toBeNull();

  rerender(<HardwareMapping {...props} hardwareProfiles={structuredClone(hardwareProfiles)} />);
  expect(screen.getByRole("combobox", { name: "硬件配置" })).toHaveValue("front-desk");
  expect(onChange).not.toHaveBeenCalled();
});

test("cancels a rename only when authoritative props replace the selected profile under the same ID", async () => {
  const user = userEvent.setup();
  const onChange = vi.fn<(profiles: HardwareProfile[]) => void>();
  const props = {
    language: "zh-CN" as const,
    layout,
    boardProfiles,
    devices: [] as DeviceStatus[],
    selectedButtonId: null,
    onSelectButton: vi.fn(),
    onChange,
    onSelectionChange: vi.fn(),
    learning: null,
    onBeginLearning: vi.fn(),
    onEndLearning: vi.fn(),
  };
  const initial = structuredClone(hardwareProfiles);
  const { rerender } = render(<HardwareMapping {...props} hardwareProfiles={initial} />);
  await user.selectOptions(screen.getByRole("combobox", { name: "硬件配置" }), "back-desk");
  await user.click(screen.getByRole("button", { name: "重命名硬件配置" }));
  const rename = screen.getByRole("textbox", { name: "硬件配置名称" });
  await user.clear(rename);
  await user.type(rename, "Local draft");

  rerender(<HardwareMapping {...props} hardwareProfiles={structuredClone(initial)} />);
  expect(screen.getByRole("textbox", { name: "硬件配置名称" })).toHaveValue("Local draft");

  const refreshed = structuredClone(initial);
  refreshed[1].name = "Authoritative back desk";
  rerender(<HardwareMapping {...props} hardwareProfiles={refreshed} />);
  expect(screen.queryByRole("textbox", { name: "硬件配置名称" })).toBeNull();
  expect(screen.getByRole("combobox", { name: "硬件配置" })).toHaveDisplayValue("Authoritative back desk");
  expect(onChange).not.toHaveBeenCalled();
});

test("keeps a board-valid direct value visible when online capabilities narrow new choices", async () => {
  const user = userEvent.setup();
  renderMapping({ devices: [device({ capabilities: [1, 13] })] });
  await user.click(screen.getByText("适配新设备"));
  await user.selectOptions(screen.getByLabelText("在线设备"), "device-front");

  const direct = screen.getByRole("combobox", { name: "1 GPIO" });
  expect(direct).toHaveValue("6");
  expect(within(direct).getAllByRole("option").map((option) => option.getAttribute("value")))
    .toEqual(["", "6", "1", "13"]);
  expect(direct).not.toHaveAttribute("aria-invalid", "true");
});

test("renders and rejects matrix endpoints missing from source pins until repaired", async () => {
  const user = userEvent.setup();
  const inconsistent = structuredClone(hardwareProfiles);
  inconsistent[0].inputs[1] = {
    type: "contact_matrix",
    id: "matrix",
    pins: [1, 2],
    keys: { TWO: [1, 13] },
  };
  const onChange = renderMapping({ profiles: inconsistent });

  const endpoint = screen.getByRole("combobox", { name: "2 B" });
  expect(endpoint).toHaveValue("13");
  expect(endpoint).toHaveAttribute("aria-invalid", "true");
  expect(within(endpoint).getByRole("option", { name: "13" })).toBeInTheDocument();
  expect(hardwareProfilesAreValid(inconsistent, boardProfiles)).toBe(false);

  await user.selectOptions(endpoint, "2");
  const repaired = onChange.mock.calls.at(-1)?.[0] as HardwareProfile[];
  expect(repaired[0].inputs[1]).toMatchObject({ keys: { TWO: [1, 2] } });
  expect(hardwareProfilesAreValid(repaired, boardProfiles)).toBe(true);
});

test("edits OLED SDA and OLED SCL through separate controlled selectors", async () => {
  const user = userEvent.setup();
  const profile = {
    id: "oled",
    name: "OLED",
    board_profile_id: "yd-rp2040",
    debounce_ms: 30,
    inputs: [],
    ssd1306: { sda: 18, scl: 19 },
  };
  const onChange = vi.fn<(profiles: HardwareProfile[]) => void>();
  const props = {
    language: "zh-CN" as const,
    layout,
    boardProfiles,
    devices: [] as DeviceStatus[],
    selectedButtonId: null,
    onSelectButton: vi.fn(),
    onChange,
    onSelectionChange: vi.fn(),
    learning: null,
    onBeginLearning: vi.fn(),
    onEndLearning: vi.fn(),
  };
  const { rerender } = render(<HardwareMapping {...props} hardwareProfiles={[profile]} />);

  expect(screen.getByRole("checkbox", { name: "SSD1306 OLED" })).toBeChecked();
  expect(screen.getByText("128x32 / 0x3C")).toBeInTheDocument();
  expect(screen.getByRole("combobox", { name: "OLED SDA" })).toHaveValue("18");
  expect(screen.getByRole("combobox", { name: "OLED SCL" })).toHaveValue("19");

  await user.selectOptions(screen.getByRole("combobox", { name: "OLED SDA" }), "20");
  const afterSda = onChange.mock.calls.at(-1)?.[0] as Array<HardwareProfile & {
    ssd1306?: { sda: number; scl: number };
  }>;
  expect(afterSda[0].ssd1306).toEqual({ sda: 20, scl: 19 });
  rerender(<HardwareMapping {...props} hardwareProfiles={afterSda} />);

  await user.selectOptions(screen.getByRole("combobox", { name: "OLED SCL" }), "21");
  const afterScl = onChange.mock.calls.at(-1)?.[0] as Array<HardwareProfile & {
    ssd1306?: { sda: number; scl: number };
  }>;
  expect(afterScl[0].ssd1306).toEqual({ sda: 20, scl: 21 });
});

test("enables OLED on the first two unowned safe pins", async () => {
  const user = userEvent.setup();
  const directKeys = Object.fromEntries(
    Array.from({ length: 18 }, (_, gpio) => [`KEY_${gpio}`, gpio]),
  );
  const profile: HardwareProfile = {
    id: "eighteen-direct",
    name: "Eighteen direct",
    board_profile_id: "yd-rp2040",
    debounce_ms: 30,
    inputs: [{ type: "direct", id: "direct", keys: directKeys }],
  };
  const onChange = renderMapping({ profiles: [profile] });

  await user.click(screen.getByRole("checkbox", { name: "SSD1306 OLED" }));

  const updated = onChange.mock.calls.at(-1)?.[0] as Array<HardwareProfile & {
    ssd1306?: { sda: number; scl: number };
  }>;
  expect(updated[0].ssd1306).toEqual({ sda: 18, scl: 19 });
});

test("rejects unsupported, unsafe, same-pin, conflicting, and duplicate pin ownership", () => {
  const base = {
    id: "validation",
    name: "Validation",
    board_profile_id: "yd-rp2040",
    debounce_ms: 30,
    inputs: [] as HardwareProfile["inputs"],
  };
  const withOled = (ssd1306: NonNullable<HardwareProfile["ssd1306"]>, inputs = base.inputs) => ({
    ...base,
    inputs,
    ssd1306,
  });

  expect(hardwareProfilesAreValid([
    { ...withOled({ sda: 1, scl: 2 }), board_profile_id: "esp-board" },
  ], boardProfiles)).toBe(false);
  expect(hardwareProfilesAreValid([withOled({ sda: 24, scl: 22 })], boardProfiles)).toBe(false);
  expect(hardwareProfilesAreValid([withOled({ sda: 18, scl: 18 })], boardProfiles)).toBe(false);
  expect(hardwareProfilesAreValid([
    withOled({ sda: 18, scl: 19 }, [{ type: "direct", id: "direct", keys: { ONE: 18 } }]),
  ], boardProfiles)).toBe(false);
  const controlPanel = {
    type: "ec11_confirm_back" as const,
    confirm: 3,
    encoder_press: 4,
    encoder_a: 5,
    encoder_b: 6,
    back: 7,
  };
  expect(hardwareProfilesAreValid([
    withOled({ sda: 18, scl: 19, control_panel: controlPanel }),
  ], boardProfiles)).toBe(true);
  expect(hardwareProfilesAreValid([
    withOled({
      sda: 18,
      scl: 19,
      control_panel: { ...controlPanel, confirm: 18 },
    }),
  ], boardProfiles)).toBe(false);
  expect(hardwareProfilesAreValid([
    withOled({
      sda: 18,
      scl: 19,
      control_panel: { ...controlPanel, back: 24 },
    }),
  ], boardProfiles)).toBe(false);
  expect(hardwareProfilesAreValid([{
    ...base,
    inputs: [
      { type: "direct", id: "first", keys: { ONE: 3 } },
      { type: "direct", id: "second", keys: { TWO: 3 } },
    ],
  }], boardProfiles)).toBe(false);

  renderMapping({
    profiles: [withOled(
      { sda: 18, scl: 19 },
      [{ type: "direct", id: "direct", keys: { ONE: 18 } }],
    )],
  });
  expect(screen.getByRole("combobox", { name: "OLED SDA" })).toHaveAttribute("aria-invalid", "true");
  expect(screen.getByRole("combobox", { name: "1 GPIO" })).toHaveAttribute("aria-invalid", "true");
});

test("labels a matrix pin shared with OLED as a pin conflict", () => {
  renderMapping({
    profiles: [{
      id: "matrix-oled-conflict",
      name: "Matrix OLED conflict",
      board_profile_id: "yd-rp2040",
      debounce_ms: 30,
      ssd1306: { sda: 18, scl: 19 },
      inputs: [{
        type: "contact_matrix",
        id: "matrix",
        pins: [1, 18],
        keys: { ONE: [1, 18] },
      }],
    }],
  });

  const matrixField = screen.getByText("矩阵引脚").closest("label");
  expect(matrixField).not.toBeNull();
  expect(within(matrixField!).getByText("GPIO 18 被重复占用")).toBeInTheDocument();
  expect(within(matrixField!).queryByText("无效 GPIO 18")).toBeNull();
});

test("reserves OLED pins from OLED counterparts, input selectors, and learning while preserving stale values", async () => {
  const user = userEvent.setup();
  const profile = {
    id: "reserved",
    name: "Reserved",
    board_profile_id: "yd-rp2040",
    debounce_ms: 30,
    inputs: [{ type: "direct" as const, id: "direct", keys: { ONE: 0 } }],
    ssd1306: { sda: 18, scl: 19 },
  };
  const onChange = vi.fn<(profiles: HardwareProfile[]) => void>();
  const props = {
    language: "zh-CN" as const,
    layout,
    boardProfiles,
    devices: [] as DeviceStatus[],
    selectedButtonId: null,
    onSelectButton: vi.fn(),
    onChange,
    onSelectionChange: vi.fn(),
    learning: null,
    onBeginLearning: vi.fn(),
    onEndLearning: vi.fn(),
  };
  const { rerender } = render(<HardwareMapping {...props} hardwareProfiles={[profile]} />);

  const sda = screen.getByRole("combobox", { name: "OLED SDA" });
  const scl = screen.getByRole("combobox", { name: "OLED SCL" });
  const direct = screen.getByRole("combobox", { name: "1 GPIO" });
  expect(within(sda).queryByRole("option", { name: "0" })).toBeNull();
  expect(within(sda).queryByRole("option", { name: "19" })).toBeNull();
  expect(within(scl).queryByRole("option", { name: "0" })).toBeNull();
  expect(within(scl).queryByRole("option", { name: "18" })).toBeNull();
  expect(within(direct).queryByRole("option", { name: "18" })).toBeNull();
  expect(within(direct).queryByRole("option", { name: "19" })).toBeNull();

  await user.click(screen.getByText("适配新设备"));
  expect(screen.queryByRole("checkbox", { name: "GPIO 18" })).toBeNull();
  expect(screen.queryByRole("checkbox", { name: "GPIO 19" })).toBeNull();

  await user.selectOptions(screen.getByLabelText("板型"), "esp-board");
  const changed = onChange.mock.calls.at(-1)?.[0] as Array<HardwareProfile & {
    ssd1306?: { sda: number; scl: number };
  }>;
  expect(changed[0].ssd1306).toEqual({ sda: 18, scl: 19 });
  rerender(<HardwareMapping {...props} hardwareProfiles={changed} />);
  expect(screen.getByRole("combobox", { name: "OLED SDA" })).toHaveValue("18");
  expect(screen.getByRole("combobox", { name: "OLED SCL" })).toHaveValue("19");
  expect(screen.getByRole("combobox", { name: "OLED SDA" })).toHaveAttribute("aria-invalid", "true");
  expect(screen.getByRole("combobox", { name: "OLED SCL" })).toHaveAttribute("aria-invalid", "true");
});

test("stacks the Hardware Profile toolbar at compact widths", () => {
  expect(viewCss).toMatch(
    /@media \(max-width: 680px\)[\s\S]*?\.hardware-toolbar\s*\{[^}]*flex-wrap:\s*wrap[^}]*\}/,
  );
  expect(viewCss).toMatch(
    /@media \(max-width: 680px\)[\s\S]*?\.hardware-toolbar > label\s*\{[^}]*min-width:\s*0[^}]*\}/,
  );
});
