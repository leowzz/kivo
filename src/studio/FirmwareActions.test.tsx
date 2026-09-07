import { invoke } from "@tauri-apps/api/core";
import { confirm, open, save } from "@tauri-apps/plugin-dialog";
import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { afterAll, beforeAll, beforeEach, expect, test, vi } from "vitest";
import FirmwareActions from "./FirmwareActions";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
  Channel: class {
    onmessage = (_event: unknown) => {};
  },
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({
  confirm: vi.fn(),
  open: vi.fn(),
  save: vi.fn(),
}));

const device = {
  id: "9:yd-rp2040TEST",
  name: "YD-RP2040",
  serial: "TEST",
  boardProfileId: "yd-rp2040",
};
const firmware = {
  path: "/build/firmware.uf2",
  bytes: 1024 * 1024,
  sha256: "abc123",
  productVersionId: "key-rp-k1-r01",
  buildId: "new-build",
};
const busy = vi.fn();
const flashed = vi.fn();

// jsdom does not implement the native dialog lifecycle or top layer.
beforeAll(() => {
  HTMLDialogElement.prototype.showModal = function () {
    this.setAttribute("open", "");
  };
  HTMLDialogElement.prototype.close = function () {
    this.removeAttribute("open");
  };
});
afterAll(() => {
  Reflect.deleteProperty(HTMLDialogElement.prototype, "showModal");
  Reflect.deleteProperty(HTMLDialogElement.prototype, "close");
});

function operations() {
  return vi
    .mocked(invoke)
    .mock.calls.map(([, args]) => args as Record<string, unknown>)
    .filter((request) => request.operation !== "status");
}

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(save).mockResolvedValue("/backups/original.uf2");
  vi.mocked(open).mockResolvedValue(firmware.path);
  vi.mocked(confirm).mockResolvedValue(true);
  vi.mocked(invoke).mockResolvedValue(firmware);
});

function show() {
  return render(
    <FirmwareActions
      device={device}
      disabled={false}
      onBusyChange={busy}
      onFlashed={flashed}
    />,
  );
}

test("backs up the selected device and displays its saved path and checksum", async () => {
  vi.mocked(invoke).mockResolvedValue({
    ...firmware,
    path: "/backups/original.uf2",
  });
  show();
  fireEvent.click(screen.getByRole("button", { name: "备份固件" }));
  await screen.findByText("固件已备份");
  expect(invoke).toHaveBeenCalledWith(
    "studio_firmware_operation",
    expect.objectContaining({
      operation: "backup",
      deviceId: device.id,
      path: "/backups/original.uf2",
    }),
  );
  expect(screen.getByText("/backups/original.uf2")).toBeVisible();
  expect(screen.getByText(/SHA-256 abc123/)).toBeVisible();
  expect(busy.mock.calls).toEqual([[true], [false]]);
  expect(flashed).not.toHaveBeenCalled();
});

test("canceling file selection never contacts the device", async () => {
  vi.mocked(save).mockResolvedValue(null);
  show();
  fireEvent.click(screen.getByRole("button", { name: "备份固件" }));
  await waitFor(() => expect(busy).toHaveBeenLastCalledWith(false));
  expect(operations()).toEqual([]);
  expect(screen.queryByRole("status")).toBeNull();
});

test("confirms the validated product and flashes the confirmed hash", async () => {
  show();
  fireEvent.click(screen.getByRole("button", { name: "刷入固件文件" }));
  await screen.findByText("固件已刷入并校验");
  expect(confirm).toHaveBeenCalledWith(
    expect.stringContaining("key-rp-k1-r01"),
    expect.anything(),
  );
  expect(operations().map((request) => request.operation)).toEqual([
    "inspect",
    "flash",
  ]);
  expect(invoke).toHaveBeenLastCalledWith(
    "studio_firmware_operation",
    expect.objectContaining({
      deviceId: device.id,
      path: firmware.path,
      sha256: "abc123",
    }),
  );
  expect(flashed).toHaveBeenCalledOnce();
  expect(flashed).toHaveBeenCalledWith(false);
  const requests = operations();
  expect(requests[0].progress).not.toBe(requests[1].progress);
});

test("canceling confirmation never flashes", async () => {
  vi.mocked(confirm).mockResolvedValue(false);
  show();
  fireEvent.click(screen.getByRole("button", { name: "刷入固件文件" }));
  await waitFor(() => expect(busy).toHaveBeenLastCalledWith(false));
  expect(operations().map((request) => request.operation)).toEqual(["inspect"]);
  expect(flashed).not.toHaveBeenCalled();
});

test("shows progress and prevents competing operations until a failed upload can be retried", async () => {
  let fail!: (reason: unknown) => void;
  const pending = new Promise((_, reject) => {
    fail = reject;
  });
  vi.mocked(invoke).mockImplementation(async (_, args) => {
    const request = args as Record<string, unknown>;
    if (request.operation === "status") return {};
    if (request.operation === "inspect") return firmware;
    (request.progress as { onmessage: (event: unknown) => void }).onmessage({
      phase: "writing",
    });
    return pending;
  });
  show();
  fireEvent.click(screen.getByRole("button", { name: "刷入固件文件" }));
  await screen.findByText("正在刷入固件，请勿断开设备");
  expect(screen.getByRole("button", { name: "备份固件" })).toBeDisabled();
  expect(screen.getByRole("button", { name: "刷入固件文件" })).toBeDisabled();
  await act(async () => {
    fail({ code: "studio_firmware_failed", detail: "USB disconnected" });
  });
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "USB disconnected",
  );
  expect(screen.getByRole("button", { name: "刷入固件文件" })).toBeEnabled();
  expect(flashed).not.toHaveBeenCalled();
});

test("keeps a successful backup visible when reboot fails", async () => {
  vi.mocked(invoke).mockResolvedValue({
    ...firmware,
    warning: "reboot failed",
  });
  show();
  fireEvent.click(screen.getByRole("button", { name: "备份固件" }));
  expect(await screen.findByRole("alert")).toHaveTextContent("reboot failed");
  expect(screen.getByText(firmware.path)).toBeVisible();
});

test("does not start if the view disappears during the file dialog", async () => {
  let selected!: (path: string) => void;
  vi.mocked(save).mockReturnValue(
    new Promise((resolve) => {
      selected = resolve;
    }),
  );
  const { unmount } = show();
  fireEvent.click(screen.getByRole("button", { name: "备份固件" }));
  unmount();
  await act(async () => {
    selected("/backups/original.uf2");
  });
  expect(operations()).toEqual([]);
});

test("requires risk acknowledgement and explicit confirmation before installing test firmware", async () => {
  vi.mocked(invoke).mockImplementation(async (_, args) => {
    const request = args as Record<string, unknown>;
    if (request.operation === "status") return {};
    return {
      ...firmware,
      productVersionId: undefined,
      buildId: "io-test-unit",
      backupPath: "/recovery/original.uf2",
    };
  });
  show();
  fireEvent.click(screen.getByRole("button", { name: "刷入测试固件" }));
  const dialog = screen.getByRole("dialog", { name: "确认刷入测试固件" });
  expect(within(dialog).getByText(device.name)).toBeVisible();
  expect(within(dialog).getByText(device.serial)).toBeVisible();
  expect(within(dialog).getByText("此操作将覆盖设备当前固件。")).toBeVisible();
  expect(within(dialog).getByRole("button", { name: "取消" })).toHaveFocus();
  const install = within(dialog).getByRole("button", { name: "备份并刷入" });
  expect(install).toBeDisabled();
  fireEvent.click(install);
  expect(operations()).toEqual([]);
  const acknowledge = within(dialog).getByRole("checkbox");
  fireEvent.click(acknowledge);
  expect(install).toBeEnabled();
  expect(operations()).toEqual([]);
  fireEvent.click(acknowledge);
  expect(install).toBeDisabled();
  fireEvent.click(acknowledge);
  fireEvent.click(install);
  await screen.findByText("I/O 测试固件已刷入");
  expect(open).not.toHaveBeenCalled();
  expect(confirm).not.toHaveBeenCalled();
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(operations().map((request) => request.operation)).toEqual([
    "install_test",
  ]);
  expect(flashed).toHaveBeenCalledWith(true);
  expect(screen.getByText("原固件备份：/recovery/original.uf2")).toBeVisible();
});

test("restoring through the file picker starts at the original device backup", async () => {
  vi.mocked(invoke).mockImplementation(async (_, args) => {
    const request = args as Record<string, unknown>;
    return request.operation === "status"
      ? { backupPath: "/recovery/original.uf2" }
      : firmware;
  });
  show();
  await screen.findByText("原固件备份：/recovery/original.uf2");
  fireEvent.click(screen.getByRole("button", { name: "刷入固件文件" }));
  await waitFor(() =>
    expect(open).toHaveBeenCalledWith(
      expect.objectContaining({ defaultPath: "/recovery/original.uf2" }),
    ),
  );
});

test("canceling temporary firmware confirmation does not build or touch hardware", async () => {
  show();
  fireEvent.click(screen.getByRole("button", { name: "刷入测试固件" }));
  fireEvent.click(screen.getByRole("button", { name: "取消" }));
  await waitFor(() => expect(busy).toHaveBeenLastCalledWith(false));
  expect(operations()).toEqual([]);
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(screen.getByRole("button", { name: "刷入测试固件" })).toBeEnabled();
});

test.each(["close", "escape"])(
  "dismissing the test firmware dialog with %s never starts installation",
  async (dismiss) => {
    show();
    fireEvent.click(screen.getByRole("button", { name: "刷入测试固件" }));
    if (dismiss === "close") {
      fireEvent.click(screen.getByRole("button", { name: "关闭刷写确认" }));
    } else {
      fireEvent(
        screen.getByRole("dialog"),
        new Event("cancel", { cancelable: true }),
      );
    }
    await waitFor(() => expect(busy).toHaveBeenLastCalledWith(false));
    expect(operations()).toEqual([]);
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(flashed).not.toHaveBeenCalled();
  },
);

test("reopening confirmation requires a new acknowledgement", async () => {
  show();
  fireEvent.click(screen.getByRole("button", { name: "刷入测试固件" }));
  fireEvent.click(screen.getByRole("checkbox"));
  fireEvent.click(screen.getByRole("button", { name: "取消" }));
  await waitFor(() => expect(busy).toHaveBeenLastCalledWith(false));
  fireEvent.click(screen.getByRole("button", { name: "刷入测试固件" }));
  expect(screen.getByRole("checkbox")).not.toBeChecked();
  expect(screen.getByRole("button", { name: "备份并刷入" })).toBeDisabled();
  expect(operations()).toEqual([]);
});

test("unmounting during test firmware confirmation never starts installation", async () => {
  const { unmount } = show();
  fireEvent.click(screen.getByRole("button", { name: "刷入测试固件" }));
  unmount();
  await act(async () => {});
  expect(operations()).toEqual([]);
  expect(flashed).not.toHaveBeenCalled();
});
