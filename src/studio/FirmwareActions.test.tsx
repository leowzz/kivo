import { invoke } from "@tauri-apps/api/core";
import { confirm, open, save } from "@tauri-apps/plugin-dialog";
import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { beforeEach, expect, test, vi } from "vitest";
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
  fireEvent.click(screen.getByRole("button", { name: "备份原有固件" }));
  await screen.findByText("原有固件已备份");
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
  fireEvent.click(screen.getByRole("button", { name: "备份原有固件" }));
  await waitFor(() => expect(busy).toHaveBeenLastCalledWith(false));
  expect(invoke).not.toHaveBeenCalled();
  expect(screen.queryByRole("status")).toBeNull();
});

test("confirms the validated product and flashes the confirmed hash", async () => {
  show();
  fireEvent.click(screen.getByRole("button", { name: "刷入新版固件" }));
  await screen.findByText("新版固件已刷入并验证");
  expect(confirm).toHaveBeenCalledWith(
    expect.stringContaining("key-rp-k1-r01"),
    expect.anything(),
  );
  expect(
    vi
      .mocked(invoke)
      .mock.calls.map(
        ([, args]) => (args as Record<string, unknown>)?.operation,
      ),
  ).toEqual(["inspect", "flash"]);
  expect(invoke).toHaveBeenLastCalledWith(
    "studio_firmware_operation",
    expect.objectContaining({
      deviceId: device.id,
      path: firmware.path,
      sha256: "abc123",
    }),
  );
  expect(flashed).toHaveBeenCalledOnce();
  const requests = vi
    .mocked(invoke)
    .mock.calls.map(([, args]) => args as Record<string, unknown>);
  expect(requests[0].progress).not.toBe(requests[1].progress);
});

test("canceling confirmation never flashes", async () => {
  vi.mocked(confirm).mockResolvedValue(false);
  show();
  fireEvent.click(screen.getByRole("button", { name: "刷入新版固件" }));
  await waitFor(() => expect(busy).toHaveBeenLastCalledWith(false));
  expect(invoke).toHaveBeenCalledTimes(1);
  expect(flashed).not.toHaveBeenCalled();
});

test("shows progress and prevents competing operations until a failed upload can be retried", async () => {
  let fail!: (reason: unknown) => void;
  const pending = new Promise((_, reject) => {
    fail = reject;
  });
  vi.mocked(invoke).mockImplementation(async (_, args) => {
    const request = args as Record<string, unknown>;
    if (request.operation === "inspect") return firmware;
    (request.progress as { onmessage: (event: unknown) => void }).onmessage({
      phase: "writing",
    });
    return pending;
  });
  show();
  fireEvent.click(screen.getByRole("button", { name: "刷入新版固件" }));
  await screen.findByText("正在刷入固件，请勿断开设备");
  expect(screen.getByRole("button", { name: "备份原有固件" })).toBeDisabled();
  expect(screen.getByRole("button", { name: "刷入新版固件" })).toBeDisabled();
  await act(async () => {
    fail({ code: "studio_firmware_failed", detail: "USB disconnected" });
  });
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "USB disconnected",
  );
  expect(screen.getByRole("button", { name: "刷入新版固件" })).toBeEnabled();
  expect(flashed).not.toHaveBeenCalled();
});

test("keeps a successful backup visible when reboot fails", async () => {
  vi.mocked(invoke).mockResolvedValue({
    ...firmware,
    warning: "reboot failed",
  });
  show();
  fireEvent.click(screen.getByRole("button", { name: "备份原有固件" }));
  expect(await screen.findByRole("alert")).toHaveTextContent("备份文件已保存");
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
  fireEvent.click(screen.getByRole("button", { name: "备份原有固件" }));
  unmount();
  await act(async () => {
    selected("/backups/original.uf2");
  });
  expect(invoke).not.toHaveBeenCalled();
});
