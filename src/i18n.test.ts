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
});
