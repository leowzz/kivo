import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { expect, test } from "vitest";
import { LayoutEditor } from "./StudioApp";
import type { ProductDefinition } from "./types";

const definition: ProductDefinition = {
  schema_version: 1,
  product: {
    display_name: "Test Product",
    family_id: "test",
    variant_id: "test-k0",
    hardware_revision: 1,
    product_version_id: "test-k0-r01",
    capabilities: [],
  },
  layout: {
    id: "test-k0",
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
