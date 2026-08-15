import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn(),
  save: vi.fn(),
}));

beforeEach(() => {
  vi.clearAllMocks();
  window.history.replaceState({}, "", "/?preview");
});

afterEach(() => {
  window.history.replaceState({}, "", "/");
});

test("preview exposes an editable RP2040 SSD1306 configuration", async () => {
  const { previewSnapshot } = await import("./preview");
  const rp2040 = previewSnapshot.boardProfiles.find(
    ({ id }) => id === "yd-rp2040",
  );
  const hardware = previewSnapshot.deviceProfiles
    .flatMap(({ hardware_profiles }) => hardware_profiles)
    .find(({ id }) => id === "phone-rp-workbench");

  expect(rp2040?.supportsOled).toBe(true);
  expect(rp2040?.safePins).toContain(23);
  expect(hardware?.ssd1306).toEqual({ sda: 18, scl: 19 });
});

test("creates a blank profile locally in preview mode", async () => {
  const user = userEvent.setup();
  const { default: App } = await import("./App");
  render(<App />);

  const setup = await screen.findByRole("dialog", { name: "添加键盘" });
  await user.click(within(setup).getByRole("button", { name: "先新建配置" }));
  await user.click(within(setup).getByRole("radio", { name: "空白配置" }));
  await user.type(
    within(setup).getByRole("textbox", { name: "配置名称" }),
    "验收空白配置",
  );
  await user.click(within(setup).getByRole("button", { name: "创建配置" }));

  await waitFor(() =>
    expect(within(setup).getByText("设备身份无效")).toBeInTheDocument(),
  );
  expect(within(setup).queryByRole("alert")).toBeNull();
  expect(invoke).not.toHaveBeenCalled();

  await user.click(within(setup).getByRole("button", { name: "关闭" }));
  await user.click(screen.getByRole("button", { name: "数据与备份" }));
  expect(screen.getByText("验收空白配置")).toBeInTheDocument();
  expect(screen.queryByLabelText("当前编辑配置")).not.toBeInTheDocument();
});
