import { render } from "@testing-library/react";
import { expect, test, vi } from "vitest";
import { Keypad } from "./Keypad";
import type { ModelLayout } from "./types";

test("sizes keypad groups by their row and column counts", () => {
  const layout: ModelLayout = {
    id: "test",
    name: "Test",
    groups: [
      { id: "top", columns: 5, buttons: [{ id: "UP", label: "UP" }] },
      {
        id: "digits",
        columns: 3,
        buttons: Array.from({ length: 12 }, (_, index) => ({
          id: String(index),
          label: String(index),
        })),
      },
    ],
  };
  const { container } = render(
    <Keypad
      layout={layout}
      actions={{}}
      selectedButtonId={null}
      pressedButtonIds={new Set()}
      actionCountLabel={(count) => `${count}`}
      unconfiguredLabel="Unconfigured"
      onSelect={vi.fn()}
    />,
  );
  const groups = container.querySelectorAll<HTMLElement>(".key-group");

  expect(groups[0].style.gridTemplateRows).toBe("repeat(1, minmax(0, 1fr))");
  expect(groups[0].style.flexGrow).toBe("0.2");
  expect(groups[1].style.gridTemplateRows).toBe("repeat(4, minmax(0, 1fr))");
  expect(groups[1].style.flexGrow).toBe(String(4 / 3));
});

test("counts actions in every trigger group", () => {
  const layout: ModelLayout = {
    id: "test",
    name: "Test",
    groups: [{ id: "keys", columns: 1, buttons: [{ id: "A", label: "A" }] }],
  };
  const { getByRole } = render(
    <Keypad
      layout={layout}
      actions={{
        A: {
          press: [{ type: "delay", duration_ms: 1 }],
          release: [],
          long_press: [{ type: "delay", duration_ms: 2 }],
          double_press: [],
        },
      }}
      selectedButtonId={null}
      pressedButtonIds={new Set()}
      actionCountLabel={(count) => `${count} actions`}
      unconfiguredLabel="Unconfigured"
      onSelect={vi.fn()}
    />,
  );

  expect(getByRole("button", { name: "A，2 actions" })).toBeInTheDocument();
});

test("marks unconfigured keys without replacing their labels", () => {
  const layout: ModelLayout = {
    id: "test",
    name: "Test",
    groups: [{ id: "keys", columns: 1, buttons: [{ id: "A", label: "Copy" }] }],
  };
  const { getByRole, getByText } = render(
    <Keypad
      layout={layout}
      actions={{}}
      selectedButtonId={null}
      pressedButtonIds={new Set()}
      actionCountLabel={(count) => `${count} actions`}
      unconfiguredLabel="Unconfigured"
      onSelect={vi.fn()}
    />,
  );

  expect(getByRole("button", { name: "Copy，0 actions" })).toHaveClass("is-unconfigured");
  expect(getByText("Copy")).toBeInTheDocument();
  expect(getByText("Unconfigured")).toBeInTheDocument();
});
