import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, test, vi } from "vitest";
import { UsageSettingsPanel } from "./UsageSettingsPanel";
import type { UsageView } from "./types";

function usageView(overrides: Partial<UsageView> = {}): UsageView {
  return {
    settings: {
      enabled: true,
      baseUrl: "https://sub2api.example.com",
      email: "leo@example.com",
      intervalSeconds: 60,
      passwordRequired: false,
    },
    snapshot: {
      state: "ready",
      hasData: true,
      costMicros: 12_345_678,
      todayTokens: 1_234_567,
      tpm: 98_765,
      updatedAtMs: 1_788_224_400_000,
    },
    ...overrides,
  };
}

test("shows today's Cost, Token, and TPM together and saves app settings", async () => {
  const user = userEvent.setup();
  const onSave = vi.fn().mockResolvedValue(undefined);
  render(
    <UsageSettingsPanel
      language="zh-CN"
      usage={usageView()}
      onSave={onSave}
    />,
  );

  expect(screen.getByRole("heading", { name: "Sub2API 用量显示" })).toBeInTheDocument();
  expect(screen.getByText("$12.35")).toBeInTheDocument();
  expect(screen.getByText("1,234,567")).toBeInTheDocument();
  expect(screen.getByText("98,765")).toBeInTheDocument();

  await user.clear(screen.getByLabelText("刷新间隔（秒）"));
  await user.type(screen.getByLabelText("刷新间隔（秒）"), "30");
  await user.click(screen.getByRole("button", { name: "保存 SUB2API" }));

  expect(onSave).toHaveBeenCalledWith({
    enabled: true,
    baseUrl: "https://sub2api.example.com",
    email: "leo@example.com",
    password: "",
    intervalSeconds: 30,
  });
});

test("requires a one-time password when no stored session is available", async () => {
  const user = userEvent.setup();
  const onSave = vi.fn().mockResolvedValue(undefined);
  const usage = usageView();
  usage.settings.passwordRequired = true;
  render(
    <UsageSettingsPanel
      language="zh-CN"
      usage={usage}
      onSave={onSave}
    />,
  );

  expect(screen.getByRole("alert")).toHaveTextContent("首次连接或更换账号时需要登录密码");
  expect(screen.getByRole("button", { name: "保存 SUB2API" })).toBeDisabled();

  await user.type(screen.getByLabelText("登录密码"), "temporary-secret");
  await user.click(screen.getByRole("button", { name: "保存 SUB2API" }));
  expect(onSave).toHaveBeenCalledWith(expect.objectContaining({ password: "temporary-secret" }));
});
