import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { beforeEach, expect, test, vi } from "vitest";
import { AutostartSettings } from "./AutostartSettings";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
beforeEach(() => { vi.mocked(invoke).mockReset(); });

test("reads the system state and enables and disables startup", async () => {
  let enabled = false;
  vi.mocked(invoke).mockImplementation(async (command) => {
    if (command === "plugin:autostart|enable") enabled = true;
    if (command === "plugin:autostart|disable") enabled = false;
    return enabled;
  });
  const user = userEvent.setup();
  const { unmount } = render(<AutostartSettings language="zh-CN" />);
  const toggle = screen.getByRole("switch", { name: "开机自启" });
  await waitFor(() => expect(toggle).toBeEnabled());
  expect(toggle).not.toBeChecked();
  await user.click(toggle);
  await waitFor(() => expect(toggle).toBeChecked());
  unmount();
  render(<AutostartSettings language="zh-CN" />);
  const reopened = screen.getByRole("switch");
  await waitFor(() => expect(reopened).toBeChecked());
  await user.click(reopened);
  await waitFor(() => expect(reopened).not.toBeChecked());
  expect(invoke).toHaveBeenCalledWith("plugin:autostart|disable");
});

test("keeps the previous state and reports a failed change", async () => {
  vi.mocked(invoke).mockResolvedValueOnce(true).mockRejectedValueOnce("Access denied");
  render(<AutostartSettings language="zh-CN" />);
  const toggle = screen.getByRole("switch");
  await waitFor(() => expect(toggle).toBeChecked());
  await userEvent.setup().click(toggle);
  expect(await screen.findByRole("alert")).toHaveTextContent("Access denied");
  expect(toggle).toBeChecked();
  expect(toggle).toBeEnabled();
});

test("disables the switch while a change is pending", async () => {
  let finish!: () => void;
  vi.mocked(invoke)
    .mockResolvedValueOnce(false)
    .mockImplementationOnce(() => new Promise<void>((resolve) => { finish = resolve; }))
    .mockResolvedValueOnce(true);
  render(<AutostartSettings language="zh-CN" />);
  const toggle = screen.getByRole("switch");
  await waitFor(() => expect(toggle).toBeEnabled());
  const user = userEvent.setup();
  await user.click(toggle);
  expect(toggle).toBeDisabled();
  await user.click(toggle);
  expect(invoke).toHaveBeenCalledTimes(2);
  await act(async () => finish());
  expect(toggle).toBeChecked();
  expect(toggle).toBeEnabled();
});

test("reports a failed initial read without allowing changes", async () => {
  vi.mocked(invoke).mockRejectedValue("Unavailable");
  render(<AutostartSettings language="zh-CN" />);
  expect(await screen.findByRole("alert")).toHaveTextContent("Unavailable");
  expect(screen.getByRole("switch")).toBeDisabled();
});

test("preview does not access system settings", () => {
  render(<AutostartSettings language="zh-CN" preview />);
  expect(screen.getByRole("switch")).toBeDisabled();
  expect(invoke).not.toHaveBeenCalled();
});
