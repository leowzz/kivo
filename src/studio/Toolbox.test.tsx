import { invoke } from "@tauri-apps/api/core";
import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import Toolbox from "./Toolbox";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
  Channel: class {
    onmessage = () => {};
  },
}));

const device = {
  id: "yd-rp2040:TEST",
  port: "/dev/cu.test",
  boardProfileId: "yd-rp2040",
  name: "YD-RP2040",
  serial: "TEST",
};
const sample = (high = true) => ({
  sessionId: 7,
  deviceId: device.id,
  firmwareBuildId: "test-build",
  pins: [
    { gpio: 1, high },
    { gpio: 2, high: false },
  ],
});

beforeEach(() => {
  vi.mocked(invoke).mockReset();
  vi.mocked(invoke).mockImplementation(async (command) => {
    if (command === "studio_list_devices") return [device];
    if (command === "studio_connect_gpio") return sample();
    if (command === "studio_read_gpio") return sample(false);
    return undefined;
  });
});
afterEach(() => {
  vi.useRealTimers();
});

async function start() {
  await screen.findByRole("option", { name: "YD-RP2040 · TEST" });
  fireEvent.click(screen.getByRole("button", { name: "开始测试" }));
}

test("samples actual high and low transitions and releases the connection on stop", async () => {
  render(<Toolbox />);
  await start();
  expect(await screen.findByLabelText("GPIO 1 高电平")).toBeInTheDocument();
  expect(screen.getByText("高电平 1")).toBeInTheDocument();
  expect(await screen.findByLabelText("GPIO 1 低电平")).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "停止测试" }));
  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith("studio_disconnect_gpio", {
      sessionId: 7,
    }),
  );
  expect(screen.queryByLabelText("GPIO 电平")).toBeNull();
  expect(screen.getByRole("combobox", { name: "测试设备" })).toBeEnabled();
});

test("clears stale pin levels and reports a disconnected device", async () => {
  vi.mocked(invoke).mockImplementation(async (command) => {
    if (command === "studio_list_devices") return [device];
    if (command === "studio_connect_gpio") return sample();
    if (command === "studio_read_gpio") throw { code: "gpio_disconnected" };
  });
  render(<Toolbox />);
  await start();
  await screen.findByLabelText("GPIO 1 高电平");
  expect(await screen.findByRole("alert")).toHaveTextContent("设备已断开");
  expect(screen.queryByLabelText("GPIO 电平")).toBeNull();
  expect(screen.getByRole("button", { name: "开始测试" })).toBeEnabled();
});

test("reports unsupported firmware without showing fabricated readings", async () => {
  vi.mocked(invoke).mockImplementation(async (command) => {
    if (command === "studio_list_devices") return [device];
    if (command === "studio_connect_gpio")
      throw { code: "gpio_monitor_unsupported" };
  });
  render(<Toolbox />);
  await start();
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "当前固件不支持 GPIO 测试",
  );
  expect(screen.getByRole("button", { name: "备份固件" })).toBeEnabled();
  expect(screen.getByRole("button", { name: "刷入固件文件" })).toBeEnabled();
  expect(screen.getByRole("button", { name: "刷入测试固件" })).toBeEnabled();
  expect(screen.queryByLabelText("GPIO 电平")).toBeNull();
});

test("closes a connection that finishes after the view unmounts", async () => {
  let connected!: (value: ReturnType<typeof sample>) => void;
  const pending = new Promise<ReturnType<typeof sample>>((resolve) => {
    connected = resolve;
  });
  vi.mocked(invoke).mockImplementation(async (command) => {
    if (command === "studio_list_devices") return [device];
    if (command === "studio_connect_gpio") return pending;
  });
  const { unmount } = render(<Toolbox />);
  await start();
  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith("studio_connect_gpio", {
      deviceId: device.id,
    }),
  );
  unmount();
  await act(async () => {
    connected(sample());
  });
  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith("studio_disconnect_gpio", {
      sessionId: 7,
    }),
  );
  expect(invoke).not.toHaveBeenCalledWith(
    "studio_read_gpio",
    expect.anything(),
  );
});

test("stops polling and closes the serial session when leaving the view", async () => {
  const { unmount } = render(<Toolbox />);
  await start();
  await screen.findByLabelText("GPIO 1 高电平");
  vi.useFakeTimers();
  unmount();
  await act(async () => {
    await vi.advanceTimersByTimeAsync(1000);
  });
  expect(invoke).toHaveBeenCalledWith("studio_disconnect_gpio", {
    sessionId: 7,
  });
  expect(invoke).not.toHaveBeenCalledWith(
    "studio_read_gpio",
    expect.anything(),
  );
});

test("keeps the test disabled when no supported device is connected", async () => {
  vi.mocked(invoke).mockResolvedValue([]);
  render(<Toolbox />);
  await waitFor(() =>
    expect(screen.getByRole("button", { name: "刷新设备列表" })).toBeEnabled(),
  );
  expect(screen.getByRole("button", { name: "开始测试" })).toBeDisabled();
});

test("offers input pull modes only on the temporary I/O firmware", async () => {
  let inputMode = "input";
  vi.mocked(invoke).mockImplementation(async (command, args) => {
    if (command === "studio_list_devices") return [device];
    if (command === "studio_set_gpio_input_mode")
      inputMode = (args as { mode: string }).mode;
    if (
      [
        "studio_connect_gpio",
        "studio_read_gpio",
        "studio_set_gpio_input_mode",
      ].includes(command)
    )
      return { ...sample(), firmwareBuildId: "io-test-unit", inputMode };
    return {};
  });
  render(<Toolbox />);
  await start();
  const select = await screen.findByRole("combobox", { name: "输入模式" });
  expect(select).toHaveValue("input");
  fireEvent.change(select, { target: { value: "pull_up" } });
  await waitFor(() => expect(select).toHaveValue("pull_up"));
  expect(invoke).toHaveBeenCalledWith("studio_set_gpio_input_mode", {
    sessionId: 7,
    mode: "pull_up",
  });
});
