import { describe, expect, test } from "vitest";
import { messages, t } from "./i18n";

describe("i18n", () => {
  test("defaults to complete Chinese product labels", () => {
    expect(t("zh-CN", "nav.behavior")).toBe("按键行为");
    expect(t("zh-CN", "save.failed")).toBe("保存失败");
    expect(t("en-US", "nav.behavior")).toBe("Button behavior");
  });

  test("keeps Chinese and English dictionaries structurally complete", () => {
    expect(Object.keys(messages["zh-CN"]).sort()).toEqual(
      Object.keys(messages["en-US"]).sort(),
    );
    expect(t("zh-CN", "model.actionCount", { count: 2 })).toBe("2 项行为");
  });

  test("includes translated action picker labels", () => {
    expect(t("zh-CN", "behavior.trigger")).toBe("触发方式");
    expect(t("en-US", "behavior.recordShortcut")).toBe("Record shortcut");
    expect(t("en-US", "behavior.removeKey", { key: "A" })).toBe("Remove A");
  });

  test("uses the approved device registry glossary without visible model terminology", () => {
    expect(t("zh-CN", "model.label")).toBe("设备配置");
    expect(t("zh-CN", "model.select")).toBe("当前编辑配置");
    expect(t("zh-CN", "hardware.title")).toBe("硬件配置");
    expect(t("zh-CN", "device.runtimeAssignment")).toBe("运行分配");
    expect(t("zh-CN", "device.boardProfile")).toBe("板型");
    expect(Object.values(messages["zh-CN"]).join(" ")).not.toContain("型号");
  });

  test("includes physical keyboard switcher labels", () => {
    expect(t("zh-CN", "device.current")).toBe("当前键盘");
    expect(t("zh-CN", "device.connect")).toBe("连接键盘");
    expect(t("en-US", "device.current")).toBe("Current keyboard");
  });

  test("includes default keyboard workspace states", () => {
    expect(t("zh-CN", "workspace.connectTitle")).toBe("连接你的键盘");
    expect(t("zh-CN", "workspace.unconfigured")).toBe("未配置");
    expect(t("en-US", "workspace.repair")).toBe("Repair setup");
  });
});
