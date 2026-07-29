import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open, save } from "@tauri-apps/plugin-dialog";
import { beforeEach, expect, test, vi } from "vitest";
import App from "./App";
import type { AppSnapshot, ModelConfig } from "./types";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(), save: vi.fn() }));

const model: ModelConfig = {
  schema_version: 1,
  model: {
    id: "tel-carbon-v1",
    name: "碳膜电话键盘",
    groups: [{
      id: "digits",
      columns: 2,
      buttons: [
        { id: "DIGIT_2", label: "2" },
        { id: "ENTER", label: "确认" },
      ],
    }],
  },
  hardware: {
    controller: "esp32s3",
    debounce_ms: 30,
    inputs: [
      { type: "direct", id: "side", keys: { ENTER: 6 } },
      { type: "contact_matrix", id: "carbon", pins: [1, 2, 12, 13], keys: { DIGIT_2: [1, 12] } },
    ],
  },
  actions: {},
};

const baseSnapshot: AppSnapshot = {
  models: [model],
  activeModel: model.model.id,
  language: "zh-CN",
  supportedGpios: [1, 2, 6, 12, 13],
  connection: { state: "connected", port: "/dev/cu.test" },
  runtimeError: null,
  learning: null,
};

let currentSnapshot: AppSnapshot;

beforeEach(() => {
  vi.clearAllMocks();
  currentSnapshot = structuredClone(baseSnapshot);
  HTMLDialogElement.prototype.showModal = function showModal() { this.setAttribute("open", ""); };
  HTMLDialogElement.prototype.close = function close() { this.removeAttribute("open"); };
  vi.mocked(listen).mockResolvedValue(vi.fn());
  vi.mocked(open).mockResolvedValue(null);
  vi.mocked(save).mockResolvedValue(null);
  vi.mocked(invoke).mockImplementation(async (command, args) => {
    if (command === "save_model") {
      const saved = (args as { model: ModelConfig }).model;
      currentSnapshot.models = currentSnapshot.models.map((item) => item.model.id === saved.model.id ? saved : item);
    }
    if (command === "save_settings") {
      const settings = (args as { settings: { active_model: string | null; language: AppSnapshot["language"] } }).settings;
      currentSnapshot.activeModel = settings.active_model;
      currentSnapshot.language = settings.language;
    }
    if (command === "delete_model") {
      currentSnapshot = { ...currentSnapshot, models: [], activeModel: null };
    }
    return structuredClone(currentSnapshot);
  });
});

test("uses Chinese by default with behavior first and no global save button", async () => {
  render(<App />);

  expect(await screen.findByRole("heading", { name: "按键行为" })).toBeInTheDocument();
  expect(screen.getByRole("navigation", { name: "配置" })).toBeInTheDocument();
  expect(screen.getByLabelText("设备型号")).toHaveValue("tel-carbon-v1");
  expect(screen.queryByRole("button", { name: /^保存$/ })).not.toBeInTheDocument();
  expect(screen.getByRole("button", { name: "2，0 项行为" })).toBeInTheDocument();
});

test("switches the complete interface to English", async () => {
  const user = userEvent.setup();
  render(<App />);
  await screen.findByText("配置文件");

  await user.selectOptions(screen.getByLabelText("语言"), "en-US");

  expect(await screen.findByText("Configuration files")).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Button behavior" })).toBeInTheDocument();
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("save_settings", {
    settings: { schema_version: 1, active_model: "tel-carbon-v1", language: "en-US" },
  }));
});

test("builds an ordered action list and autosaves it", async () => {
  const user = userEvent.setup();
  render(<App />);
  await screen.findByRole("button", { name: "2，0 项行为" });
  await screen.findByRole("complementary", { name: "2" });

  await user.click(screen.getByRole("button", { name: "粘贴文本" }));
  await user.type(screen.getByRole("textbox", { name: "文本" }), "你好");
  await user.click(screen.getByRole("button", { name: "按下按键" }));

  await waitFor(() => expect(invoke).toHaveBeenCalledWith("save_model", {
    model: expect.objectContaining({
      actions: {
        DIGIT_2: [
          { type: "paste", text: "你好" },
          { type: "hotkey", keys: ["enter"] },
        ],
      },
    }),
  }), { timeout: 1600 });
  expect(screen.getByRole("button", { name: "2，2 项行为" })).toBeInTheDocument();
});

test("records a shortcut from the application window", async () => {
  const user = userEvent.setup();
  render(<App />);
  const editor = await screen.findByRole("complementary", { name: "2" });

  await user.click(screen.getByRole("button", { name: "按下按键" }));
  await user.click(within(editor).getByRole("button", { name: "录入按键" }));
  fireEvent.keyDown(window, { code: "KeyK", key: "k", metaKey: true, shiftKey: true });

  expect(within(editor).getByText("Command + Shift + K")).toBeInTheDocument();
});

test("manually selects a multi-modifier shortcut", async () => {
  const user = userEvent.setup();
  render(<App />);
  const editor = await screen.findByRole("complementary", { name: "2" });

  await user.click(screen.getByRole("button", { name: "按下按键" }));
  await user.click(within(editor).getByRole("checkbox", { name: "Cmd" }));
  await user.click(within(editor).getByRole("checkbox", { name: "Ctrl" }));
  await user.click(within(editor).getByRole("checkbox", { name: "Shift" }));
  await user.selectOptions(within(editor).getByRole("combobox", { name: "按键" }), "k");

  expect(within(editor).getByText("Command + Control + Shift + K")).toBeInTheDocument();
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("save_model", {
    model: expect.objectContaining({
      actions: { DIGIT_2: [{ type: "hotkey", keys: ["cmd", "ctrl", "shift", "k"] }] },
    }),
  }), { timeout: 1600 });
});

test("reorders actions from the right editor", async () => {
  const user = userEvent.setup();
  currentSnapshot.models[0].actions.DIGIT_2 = [
    { type: "paste", text: "先粘贴" },
    { type: "hotkey", keys: ["enter"] },
  ];
  render(<App />);
  const editor = await screen.findByRole("complementary", { name: "2" });

  await user.click(within(editor).getAllByRole("button", { name: "上移" })[1]);

  await waitFor(() => expect(invoke).toHaveBeenCalledWith("save_model", {
    model: expect.objectContaining({
      actions: { DIGIT_2: [{ type: "hotkey", keys: ["enter"] }, { type: "paste", text: "先粘贴" }] },
    }),
  }), { timeout: 1600 });
});

test("keeps a failed autosave and exposes retry", async () => {
  const user = userEvent.setup();
  let saveAttempts = 0;
  vi.mocked(invoke).mockImplementation(async (command) => {
    if (command === "save_model" && saveAttempts++ === 0) throw new Error("disk full");
    return structuredClone(currentSnapshot);
  });
  render(<App />);
  const key = await screen.findByRole("button", { name: "2，0 项行为" });

  await user.click(key);
  await user.click(screen.getByRole("button", { name: "按下按键" }));
  expect(await screen.findByText("保存失败", {}, { timeout: 1600 })).toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "重试" }));

  await waitFor(() => expect(vi.mocked(invoke).mock.calls.filter(([command]) => command === "save_model")).toHaveLength(2));
});

test("previews a model before importing it", async () => {
  const user = userEvent.setup();
  vi.mocked(open).mockResolvedValue("/tmp/model.yaml");
  vi.mocked(invoke).mockImplementation(async (command) => {
    if (command === "preview_model_import") return {
      modelId: "tel-carbon-v1",
      modelName: "碳膜电话键盘",
      buttonCount: 22,
      hardwareBindingCount: 22,
      actionCount: 8,
      replacesExisting: true,
    };
    return structuredClone(currentSnapshot);
  });
  render(<App />);
  const dataMenu = (await screen.findByText("配置文件")).closest(".data-menu");
  expect(dataMenu).not.toBeNull();

  await user.click(within(dataMenu as HTMLElement).getByRole("button", { name: "导入型号" }));
  const dialog = await screen.findByRole("dialog", { name: "替换现有型号" });
  expect(within(dialog).getByText("22 个按键，22 项硬件映射，8 项行为")).toBeInTheDocument();
  await user.click(within(dialog).getByRole("button", { name: "确认" }));

  await waitFor(() => expect(invoke).toHaveBeenCalledWith("import_model", { path: "/tmp/model.yaml" }));
});

test("previews a full backup before restoring it", async () => {
  const user = userEvent.setup();
  vi.mocked(open).mockResolvedValue("/tmp/backup.yaml");
  vi.mocked(invoke).mockImplementation(async (command) => {
    if (command === "preview_backup") return {
      modelCount: 3,
      buttonCount: 44,
      hardwareBindingCount: 40,
      actionCount: 19,
    };
    return structuredClone(currentSnapshot);
  });
  render(<App />);
  await screen.findByText("配置文件");

  await user.click(screen.getByRole("button", { name: "恢复备份" }));
  const dialog = await screen.findByRole("dialog", { name: "恢复全量备份" });
  expect(within(dialog).getByText("3 个型号，44 个按键，40 项硬件映射，19 项行为")).toBeInTheDocument();
  await user.click(within(dialog).getByRole("button", { name: "确认" }));

  await waitFor(() => expect(invoke).toHaveBeenCalledWith("restore_backup", { path: "/tmp/backup.yaml" }));
});

test("deletes the last model and keeps import and restore available", async () => {
  const user = userEvent.setup();
  render(<App />);
  await screen.findByText("配置文件");

  await user.click(screen.getByRole("button", { name: "删除型号" }));
  const dialog = await screen.findByRole("dialog", { name: "删除型号" });
  await user.click(within(dialog).getByRole("button", { name: "确认" }));

  expect(await screen.findByRole("heading", { name: "还没有设备型号" })).toBeInTheDocument();
  expect(screen.getAllByRole("button", { name: "导入型号" }).length).toBeGreaterThan(0);
  expect(screen.getAllByRole("button", { name: "恢复备份" }).length).toBeGreaterThan(0);
});

test("keeps key learning secondary and collapsed by default", async () => {
  const user = userEvent.setup();
  render(<App />);
  await screen.findByText("配置文件");

  await user.click(screen.getByRole("button", { name: "硬件映射" }));

  expect(screen.getByText("直连 GPIO")).toBeInTheDocument();
  expect(screen.getByText("接触矩阵")).toBeInTheDocument();
  expect(screen.getByText("适配新设备").closest("details")).not.toHaveAttribute("open");
});
