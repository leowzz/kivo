import { invoke } from "@tauri-apps/api/core";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { beforeEach, expect, test, vi } from "vitest";
import StudioApp, { HardwareEditor, LayoutEditor } from "./StudioApp";
import type { ProductDefinition, StudioBoard, StudioSnapshot } from "./types";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

beforeEach(() => {
  vi.clearAllMocks();
});

const board: StudioBoard = {
  id: "yd-rp2040",
  familyId: "rp2040",
  controllerToken: "rp",
  displayName: "YD-RP2040",
  safePins: [0, 1, 2, 3, 4],
  supportsOled: true,
};

const espBoard: StudioBoard = {
  id: "yd-esp32-s3",
  familyId: "esp32s3",
  controllerToken: "s3",
  displayName: "YD-ESP32-S3",
  safePins: [0, 1, 2, 3, 4],
  supportsOled: false,
};

const definition: ProductDefinition = {
  schema_version: 1,
  product: {
    display_name: "Test Product",
    family_id: "test",
    variant_id: "test-rp-k0",
    hardware_revision: 1,
    product_version_id: "test-rp-k0-r01",
    capabilities: [],
  },
  layout: {
    id: "test-rp-k0",
    name: "Test Product",
    groups: ["group-1", "group-2", "group-3"].map((id) => ({
      id,
      columns: 3,
      buttons: [],
    })),
  },
  hardware_profile: {
    id: "hardware",
    name: "Hardware",
    board_profile_id: "yd-rp2040",
    debounce_ms: 30,
    inputs: [],
  },
};

function Harness() {
  const [draft, setDraft] = useState(definition);
  return (
    <LayoutEditor
      definition={draft}
      update={(mutate) => setDraft((current) => {
        const next = structuredClone(current);
        mutate(next);
        return next;
      })}
    />
  );
}

function LayoutGrowthHarness() {
  const [draft, setDraft] = useState<ProductDefinition>({
    ...structuredClone(definition),
    layout: {
      ...definition.layout,
      groups: [{
        id: "keys",
        columns: 1,
        buttons: [{ id: "K1", label: "K1" }],
      }],
    },
  });
  return (
    <LayoutEditor
      definition={draft}
      update={(mutate) => setDraft((current) => {
        const next = structuredClone(current);
        mutate(next);
        return next;
      })}
    />
  );
}

function HardwareHarness() {
  const [draft, setDraft] = useState<ProductDefinition>({
    ...structuredClone(definition),
    product: {
      ...definition.product,
      variant_id: "test-rp-k3",
      product_version_id: "test-rp-k3-r01",
    },
    layout: {
      ...definition.layout,
      id: "test-rp-k3",
      groups: [{
        id: "keys",
        columns: 3,
        buttons: ["K1", "K2", "K3"].map((id) => ({ id, label: id })),
      }],
    },
  });
  return (
    <HardwareEditor
      definition={draft}
      boards={[board]}
      update={(mutate) => setDraft((current) => {
        const next = structuredClone(current);
        mutate(next);
        return next;
      })}
    />
  );
}

function DisplayModuleHarness() {
  const [draft, setDraft] = useState<ProductDefinition>({
    ...structuredClone(definition),
    product: {
      ...definition.product,
      variant_id: "test-rp-k3",
      product_version_id: "test-rp-k3-r01",
    },
    layout: {
      ...definition.layout,
      id: "test-rp-k3",
      groups: [{
        id: "keys",
        columns: 3,
        buttons: ["K1", "K2", "K3"].map((id) => ({ id, label: id })),
      }],
    },
  });
  return (
    <HardwareEditor
      definition={draft}
      boards={[{ ...board, safePins: [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10] }]}
      update={(mutate) => setDraft((current) => {
        const next = structuredClone(current);
        mutate(next);
        return next;
      })}
    />
  );
}

function ExhaustedHardwareHarness() {
  const [draft, setDraft] = useState<ProductDefinition>({
    ...structuredClone(definition),
    layout: {
      ...definition.layout,
      groups: [{
        id: "keys",
        columns: 2,
        buttons: ["K1", "K2"].map((id) => ({ id, label: id })),
      }],
    },
  });
  return (
    <HardwareEditor
      definition={draft}
      boards={[{ ...board, safePins: [0, 1] }]}
      update={(mutate) => setDraft((current) => {
        const next = structuredClone(current);
        mutate(next);
        return next;
      })}
    />
  );
}

function groupIds() {
  return Array.from(
    document.querySelectorAll(".group-rail > button span"),
    (element) => element.textContent,
  );
}

test("deleting the first group keeps the remaining groups stable", async () => {
  const user = userEvent.setup();
  render(<Harness />);

  await user.click(screen.getByRole("button", { name: "删除分组" }));
  expect(groupIds()).toEqual(["group-2", "group-3"]);
  expect(screen.getByRole("heading", { name: "group-2" })).toBeInTheDocument();

  await user.click(screen.getByRole("button", { name: "添加分组" }));
  expect(groupIds()).toEqual(["group-2", "group-3", "group-4"]);
  expect(screen.getByRole("heading", { name: "group-4" })).toBeInTheDocument();
});

test("adding keys grows columns while the add button stays above the rows", async () => {
  const user = userEvent.setup();
  render(<LayoutGrowthHarness />);

  const addButton = screen.getByRole("button", { name: "添加按键" });
  const buttonTable = document.querySelector(".button-table");
  expect(addButton.compareDocumentPosition(buttonTable as Node) & Node.DOCUMENT_POSITION_FOLLOWING).not.toBe(0);

  await user.click(addButton);
  expect(screen.getByRole("spinbutton", { name: "Columns" })).toHaveValue(2);
  expect(screen.getByRole("button", { name: "添加按键" })).toBe(addButton);

  await user.click(addButton);
  expect(screen.getByRole("spinbutton", { name: "Columns" })).toHaveValue(3);
  expect(screen.getAllByRole("button", { name: "删除按键" })).toHaveLength(3);
});

test("direct input assigns GPIOs in order and prevents duplicate selections", async () => {
  const user = userEvent.setup();
  render(<HardwareHarness />);

  await user.click(screen.getByRole("button", { name: "Direct" }));

  const key1 = screen.getByRole("combobox", { name: "K1" });
  const key2 = screen.getByRole("combobox", { name: "K2" });
  const key3 = screen.getByRole("combobox", { name: "K3" });
  expect(key1).toHaveValue("1");
  expect(key2).toHaveValue("2");
  expect(key3).toHaveValue("3");
  expect(within(key2).getByRole("option", { name: "GPIO 1" })).toBeDisabled();

  await user.selectOptions(key1, "4");

  expect(key1).toHaveValue("4");
  expect(within(key2).getByRole("option", { name: "GPIO 4" })).toBeDisabled();
  expect(within(key2).getByRole("option", { name: "GPIO 1" })).toBeEnabled();
});

test("automatic GPIO assignment never falls back to GPIO 0", async () => {
  const user = userEvent.setup();
  render(<ExhaustedHardwareHarness />);

  await user.click(screen.getByRole("button", { name: "Direct" }));

  expect(screen.getByRole("combobox", { name: "K1" })).toHaveValue("1");
  expect(screen.getByRole("combobox", { name: "K2" })).toHaveValue("");
  expect(within(screen.getByRole("combobox", { name: "K2" })).getByRole("option", { name: "GPIO 0" })).toBeEnabled();
});

test("the EC11 display module configures seven pins without adding layout keys", async () => {
  const user = userEvent.setup();
  render(<DisplayModuleHarness />);

  await user.click(screen.getByRole("button", { name: "Direct" }));
  await user.selectOptions(
    screen.getByRole("combobox", { name: "显示组件" }),
    "ec11_confirm_back",
  );

  expect(screen.getByRole("combobox", { name: "K1" })).toHaveValue("1");
  expect(screen.getByRole("combobox", { name: "K2" })).toHaveValue("2");
  expect(screen.getByRole("combobox", { name: "K3" })).toHaveValue("3");
  expect(screen.getByRole("combobox", { name: "SDA" })).toHaveValue("9");
  expect(screen.getByRole("combobox", { name: "SCL" })).toHaveValue("10");
  expect(screen.getByRole("combobox", { name: "确认 KEY1" })).toHaveValue("4");
  expect(screen.getByRole("combobox", { name: "编码器按压 PSH" })).toHaveValue("5");
  expect(screen.getByRole("combobox", { name: "编码器 A 相 TRA" })).toHaveValue("6");
  expect(screen.getByRole("combobox", { name: "编码器 B 相 TRB" })).toHaveValue("7");
  expect(screen.getByRole("combobox", { name: "返回 KEY0" })).toHaveValue("8");
  expect(screen.queryByRole("combobox", { name: "PSH" })).toBeNull();
});

test("switching controller families updates the product ID token", async () => {
  const snapshot: StudioSnapshot = {
    repoRoot: "/repo",
    boards: [espBoard, board],
    products: [],
  };
  vi.mocked(invoke).mockImplementation(async (command, args) => {
    if (command === "studio_get_snapshot") return snapshot;
    if (command === "studio_validate_product") {
      const current = (args as { definition: ProductDefinition }).definition;
      return {
        definition: current,
        json: JSON.stringify(current),
        sha256: "valid",
        byteLength: 1,
      };
    }
    throw new Error(`Unexpected command: ${command}`);
  });
  const user = userEvent.setup();
  render(<StudioApp />);

  await user.click(await screen.findByRole("button", { name: "新建" }));
  await user.click(within(screen.getByRole("dialog")).getByRole("button", { name: "创建草稿" }));
  expect(screen.getByRole("textbox", { name: "Product Version ID" })).toHaveValue(
    "kivo-product-s3-k1-r01",
  );

  await user.click(screen.getByRole("tab", { name: "硬件引脚" }));
  await user.selectOptions(screen.getByRole("combobox", { name: "Board Profile" }), board.id);
  expect(screen.getByText("kivo-product-rp-k1-r01")).toBeInTheDocument();
});

test("a conflicting draft offers the next available revision before saving", async () => {
  const snapshot: StudioSnapshot = {
    repoRoot: "/repo",
    boards: [board],
    products: [{
      productVersionId: "kivo-product-rp-k1-r01",
      displayName: "Existing Product",
      boardProfileId: board.id,
      sha256: "saved",
      error: null,
    }],
  };
  const createdSnapshot: StudioSnapshot = {
    ...snapshot,
    products: [
      ...snapshot.products,
      {
        productVersionId: "kivo-product-rp-k1-r02",
        displayName: "Kivo Product",
        boardProfileId: board.id,
        sha256: "created",
        error: null,
      },
    ],
  };
  vi.mocked(invoke).mockImplementation(async (command, args) => {
    if (command === "studio_get_snapshot") return snapshot;
    if (command === "studio_validate_product") {
      const current = (args as { definition: ProductDefinition }).definition;
      return {
        definition: current,
        json: JSON.stringify(current),
        sha256: "valid",
        byteLength: 1,
      };
    }
    if (command === "studio_save_product") return createdSnapshot;
    throw new Error(`Unexpected command: ${command}`);
  });
  const user = userEvent.setup();
  render(<StudioApp />);

  await user.click(await screen.findByRole("button", { name: "新建" }));
  await user.click(within(screen.getByRole("dialog")).getByRole("button", { name: "创建草稿" }));

  expect(await screen.findByRole("alert")).toHaveTextContent("该版本已存在：Existing Product");
  expect(screen.getByRole("textbox", { name: "Product Version ID" })).toHaveValue("kivo-product-rp-k1-r01");

  await user.click(screen.getByRole("button", { name: "保存" }));
  expect(vi.mocked(invoke).mock.calls.some(([command]) => command === "studio_save_product")).toBe(false);

  await user.click(screen.getByRole("button", { name: "改用 r02" }));
  expect(screen.getByRole("textbox", { name: "Product Version ID" })).toHaveValue("kivo-product-rp-k1-r02");
  expect(screen.queryByRole("alert")).toBeNull();

  await user.click(screen.getByRole("button", { name: "保存" }));
  await waitFor(() => expect(vi.mocked(invoke)).toHaveBeenCalledWith(
    "studio_save_product",
    expect.objectContaining({
      create: true,
      definition: expect.objectContaining({
        product: expect.objectContaining({
          hardware_revision: 2,
          product_version_id: "kivo-product-rp-k1-r02",
        }),
      }),
    }),
  ));
});

test("adding a key to a saved product derives a new ID and remains saveable", async () => {
  const savedProduct: ProductDefinition = {
    ...structuredClone(definition),
    product: {
      ...definition.product,
      variant_id: "test-rp-k1",
      product_version_id: "test-rp-k1-r01",
    },
    layout: {
      ...definition.layout,
      id: "test-rp-k1",
      groups: [{
        id: "keys",
        columns: 3,
        buttons: [{ id: "K1", label: "K1" }],
      }],
    },
    hardware_profile: {
      ...definition.hardware_profile,
      inputs: [{ type: "direct", id: "direct-1", keys: { K1: 1 } }],
    },
  };
  const snapshot: StudioSnapshot = {
    repoRoot: "/repo",
    boards: [board],
    products: [{
      productVersionId: "test-rp-k1-r01",
      displayName: "Test Product",
      boardProfileId: board.id,
      sha256: "saved",
      error: null,
    }],
  };
  vi.mocked(invoke).mockImplementation(async (command, args) => {
    if (command === "studio_get_snapshot") return snapshot;
    if (command === "studio_load_product") return savedProduct;
    if (command === "studio_validate_product") {
      const current = (args as { definition: ProductDefinition }).definition;
      return {
        definition: current,
        json: JSON.stringify(current),
        sha256: "valid",
        byteLength: 1,
      };
    }
    if (command === "studio_copy_product") return snapshot;
    throw new Error(`Unexpected command: ${command}`);
  });
  const user = userEvent.setup();
  render(<StudioApp />);

  await user.click(await screen.findByRole("button", { name: /Test Product/ }));
  await user.click(screen.getByRole("tab", { name: "按键布局" }));
  await user.click(screen.getByRole("button", { name: "添加按键" }));

  expect(screen.getByText("test-rp-k2-r01")).toBeInTheDocument();
  const save = screen.getByRole("button", { name: "保存" });
  expect(save).toBeEnabled();
  await user.click(save);

  await waitFor(() => expect(vi.mocked(invoke)).toHaveBeenCalledWith(
    "studio_copy_product",
    expect.objectContaining({
      sourceProductVersionId: "test-rp-k1-r01",
      definition: expect.objectContaining({
        product: expect.objectContaining({ product_version_id: "test-rp-k2-r01" }),
      }),
    }),
  ));
});

test("deleting a saved product confirms and clears the editor", async () => {
  const savedProduct: ProductDefinition = {
    ...structuredClone(definition),
    product: {
      ...definition.product,
      variant_id: "test-rp-k1",
      product_version_id: "test-rp-k1-r01",
    },
    layout: {
      ...definition.layout,
      id: "test-rp-k1",
      groups: [{
        id: "keys",
        columns: 1,
        buttons: [{ id: "K1", label: "K1" }],
      }],
    },
  };
  const snapshot: StudioSnapshot = {
    repoRoot: "/repo",
    boards: [board],
    products: [{
      productVersionId: "test-rp-k1-r01",
      displayName: "Test Product",
      boardProfileId: board.id,
      sha256: "saved",
      error: null,
    }],
  };
  vi.mocked(invoke).mockImplementation(async (command, args) => {
    if (command === "studio_get_snapshot") return snapshot;
    if (command === "studio_load_product") return savedProduct;
    if (command === "studio_validate_product") {
      const current = (args as { definition: ProductDefinition }).definition;
      return {
        definition: current,
        json: JSON.stringify(current),
        sha256: "valid",
        byteLength: 1,
      };
    }
    if (command === "studio_delete_product") return { ...snapshot, products: [] };
    throw new Error(`Unexpected command: ${command}`);
  });
  const user = userEvent.setup();
  render(<StudioApp />);

  await user.click(await screen.findByRole("button", { name: /Test Product/ }));
  const deleteButton = screen.getByRole("button", { name: "删除产品版本" });
  expect(within(screen.getByRole("navigation", { name: "产品版本" })).queryByRole("button", { name: "删除产品版本" })).toBeNull();
  expect(deleteButton.closest(".identity-delete")).not.toBeNull();
  await user.type(screen.getByRole("textbox", { name: "Display Name" }), " Copy");
  expect(deleteButton).toBeEnabled();
  await user.click(deleteButton);

  const dialog = screen.getByRole("dialog", { name: "删除产品版本" });
  expect(within(dialog).getByText("test-rp-k1-r01")).toBeInTheDocument();
  expect(within(dialog).getByText("当前未保存的修改也会一并丢失。")).toBeInTheDocument();
  expect(vi.mocked(invoke).mock.calls.some(([command]) => command === "studio_delete_product")).toBe(false);
  await user.click(within(dialog).getByRole("button", { name: "确认删除" }));

  await waitFor(() => expect(vi.mocked(invoke)).toHaveBeenCalledWith(
    "studio_delete_product",
    { productVersionId: "test-rp-k1-r01" },
  ));
  expect(await screen.findByText("暂无产品版本")).toBeInTheDocument();
  expect(screen.getByText("选择或新建产品版本")).toBeInTheDocument();
  expect(deleteButton).toBeDisabled();
});

test("creating a copy saves it immediately and keeps product navigation responsive", async () => {
  const savedProduct: ProductDefinition = {
    ...structuredClone(definition),
    product: {
      ...definition.product,
      variant_id: "test-rp-k1",
      product_version_id: "test-rp-k1-r01",
    },
    layout: {
      ...definition.layout,
      id: "test-rp-k1",
      groups: [{
        id: "keys",
        columns: 1,
        buttons: [{ id: "K1", label: "K1" }],
      }],
    },
  };
  const copiedProduct: ProductDefinition = {
    ...structuredClone(savedProduct),
    product: {
      ...savedProduct.product,
      hardware_revision: 2,
      product_version_id: "test-rp-k1-r02",
    },
  };
  const snapshot: StudioSnapshot = {
    repoRoot: "/repo",
    boards: [board],
    products: [{
      productVersionId: "test-rp-k1-r01",
      displayName: "Test Product",
      boardProfileId: board.id,
      sha256: "saved",
      error: null,
    }],
  };
  const copiedSnapshot: StudioSnapshot = {
    ...snapshot,
    products: [
      ...snapshot.products,
      {
        productVersionId: "test-rp-k1-r02",
        displayName: "Test Product",
        boardProfileId: board.id,
        sha256: "copied",
        error: null,
      },
    ],
  };
  vi.mocked(invoke).mockImplementation(async (command, args) => {
    if (command === "studio_get_snapshot") return snapshot;
    if (command === "studio_load_product") {
      return (args as { productVersionId: string }).productVersionId === "test-rp-k1-r02"
        ? copiedProduct
        : savedProduct;
    }
    if (command === "studio_validate_product") {
      const current = (args as { definition: ProductDefinition }).definition;
      return {
        definition: current,
        json: JSON.stringify(current),
        sha256: "valid",
        byteLength: 1,
      };
    }
    if (command === "studio_copy_product") return copiedSnapshot;
    throw new Error(`Unexpected command: ${command}`);
  });
  const confirm = vi.spyOn(window, "confirm").mockReturnValue(true);
  const user = userEvent.setup();
  render(<StudioApp />);

  await user.click(await screen.findByRole("button", { name: /test-rp-k1-r01/ }));
  await user.click(screen.getByRole("button", { name: "复制产品版本" }));
  const dialog = screen.getByRole("dialog");
  await user.click(within(dialog).getByRole("button", { name: "创建副本" }));

  await waitFor(() => expect(vi.mocked(invoke)).toHaveBeenCalledWith(
    "studio_copy_product",
    expect.objectContaining({
      sourceProductVersionId: "test-rp-k1-r01",
      definition: expect.objectContaining({
        product: expect.objectContaining({ product_version_id: "test-rp-k1-r02" }),
      }),
    }),
  ));
  expect(await screen.findByRole("button", { name: /test-rp-k1-r02/ })).toBeInTheDocument();
  expect(screen.getByText("已保存")).toBeInTheDocument();

  await user.click(screen.getByRole("button", { name: /test-rp-k1-r01/ }));
  await waitFor(() => expect(vi.mocked(invoke)).toHaveBeenCalledWith(
    "studio_load_product",
    { productVersionId: "test-rp-k1-r01" },
  ));
  expect(confirm).not.toHaveBeenCalled();
  confirm.mockRestore();
});
