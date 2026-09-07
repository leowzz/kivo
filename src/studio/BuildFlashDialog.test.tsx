import { invoke } from "@tauri-apps/api/core";
import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterAll, beforeAll, beforeEach, expect, test, vi } from "vitest";
import BuildFlashDialog from "./BuildFlashDialog";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
  Channel: class {
    onmessage = (_event: unknown) => {};
  },
}));

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

const artifact = {
  productVersionId: "test-rp-k1-r01",
  boardProfileId: "yd-rp2040",
  outputDirectory: "/repo/output/product/dev",
  firmwarePath: "/repo/output/product/dev/firmware.uf2",
  manifestPath: "/repo/output/product/dev/manifest.json",
  definitionPath: "/repo/output/product/dev/product.json",
};
const device = {
  id: "9:yd-rp2040TEST",
  name: "RP2040",
  serial: "TEST",
  boardProfileId: "yd-rp2040",
};
const firmware = {
  path: artifact.firmwarePath,
  bytes: 1024,
  sha256: "confirmed-hash",
  productVersionId: artifact.productVersionId,
  buildId: "dev",
};
const close = vi.fn();
const busy = vi.fn();
const requests = () =>
  vi
    .mocked(invoke)
    .mock.calls.filter(([command]) => command === "studio_firmware_operation")
    .map(([, args]) => args as Record<string, unknown>);

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(invoke).mockImplementation(async (command) => {
    if (command === "studio_list_devices")
      return [
        device,
        { ...device, id: "esp", name: "ESP", boardProfileId: "yd-esp32-s3" },
      ];
    return firmware;
  });
});

function show() {
  return render(
    <BuildFlashDialog
      artifact={artifact}
      onClose={close}
      onBusyChange={busy}
    />,
  );
}

test("filters devices and confirms the built file and hash before flashing", async () => {
  show();
  expect(await screen.findByText("confirmed-hash")).toBeVisible();
  expect(screen.getByRole("combobox", { name: "目标设备" })).toHaveValue(
    device.id,
  );
  expect(screen.queryByRole("option", { name: /ESP/ })).toBeNull();
  expect(screen.getByRole("button", { name: "取消" })).toHaveFocus();
  expect(screen.getByRole("button", { name: "刷入固件" })).toBeDisabled();
  expect(requests().map((request) => request.operation)).toEqual(["inspect"]);
  fireEvent.click(screen.getByRole("checkbox"));
  fireEvent.click(screen.getByRole("button", { name: "刷入固件" }));
  expect(await screen.findByText("固件已刷入并校验")).toBeVisible();
  expect(requests()[1]).toMatchObject({
    operation: "flash",
    deviceId: device.id,
    path: artifact.firmwarePath,
    sha256: "confirmed-hash",
  });
  expect(requests()[0].progress).not.toBe(requests()[1].progress);
  expect(busy.mock.calls).toEqual([[true], [false]]);
});

test("canceling or pressing Escape never flashes", async () => {
  show();
  await screen.findByText("confirmed-hash");
  fireEvent.click(screen.getByRole("button", { name: "取消" }));
  fireEvent(
    screen.getByRole("dialog"),
    new Event("cancel", { cancelable: true }),
  );
  expect(close).toHaveBeenCalledTimes(2);
  expect(requests().map((request) => request.operation)).toEqual(["inspect"]);
});

test("requires an explicit device selection when several matching boards are connected", async () => {
  vi.mocked(invoke).mockImplementation(async (command) =>
    command === "studio_list_devices"
      ? [device, { ...device, id: "other", serial: "OTHER" }]
      : firmware,
  );
  show();
  await screen.findByRole("option", { name: "RP2040 · OTHER" });
  expect(requests()).toEqual([]);
  fireEvent.change(screen.getByRole("combobox"), {
    target: { value: "other" },
  });
  await screen.findByText("confirmed-hash");
  expect(requests()[0]).toMatchObject({
    operation: "inspect",
    deviceId: "other",
  });
});

test("keeps flashing disabled with no device and refresh can find a newly connected board", async () => {
  vi.mocked(invoke).mockResolvedValueOnce([]);
  show();
  await waitFor(() =>
    expect(screen.getByRole("button", { name: "刷新设备列表" })).toBeEnabled(),
  );
  expect(screen.getByRole("button", { name: "刷入固件" })).toBeDisabled();
  expect(requests()).toEqual([]);
  fireEvent.click(screen.getByRole("button", { name: "刷新设备列表" }));
  expect(await screen.findByText("confirmed-hash")).toBeVisible();
});

test("a changed product or invalid firmware blocks flashing", async () => {
  vi.mocked(invoke).mockImplementation(async (command) =>
    command === "studio_list_devices"
      ? [device]
      : { ...firmware, productVersionId: "different-product" },
  );
  show();
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "构建产物与所选产品不一致",
  );
  expect(screen.getByRole("button", { name: "刷入固件" })).toBeDisabled();
});

test("an active flash locks device selection and closing, then permits retry after failure", async () => {
  let fail!: (reason: unknown) => void;
  vi.mocked(invoke).mockImplementation(async (command, args) => {
    if (command === "studio_list_devices") return [device];
    const request = args as Record<string, unknown>;
    if (request.operation === "inspect") return firmware;
    (request.progress as { onmessage: (event: unknown) => void }).onmessage({
      phase: "writing",
    });
    return new Promise((_, reject) => {
      fail = reject;
    });
  });
  show();
  await screen.findByText("confirmed-hash");
  fireEvent.click(screen.getByRole("checkbox"));
  fireEvent.click(screen.getByRole("button", { name: "刷入固件" }));
  expect(await screen.findByText("正在刷入固件，请勿断开设备")).toBeVisible();
  expect(screen.getByRole("combobox")).toBeDisabled();
  expect(screen.getByRole("button", { name: "取消" })).toBeDisabled();
  fireEvent(
    screen.getByRole("dialog"),
    new Event("cancel", { cancelable: true }),
  );
  expect(close).not.toHaveBeenCalled();
  await act(async () =>
    fail({ code: "studio_firmware_failed", detail: "USB disconnected" }),
  );
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "USB disconnected",
  );
  expect(screen.getByRole("button", { name: "取消" })).toBeEnabled();
  expect(screen.getByRole("checkbox")).not.toBeChecked();
  expect(busy).toHaveBeenLastCalledWith(false);
});
