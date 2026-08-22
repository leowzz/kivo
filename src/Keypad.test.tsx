import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
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
      onSelect={vi.fn()}
    />,
  );
  const groups = container.querySelectorAll<HTMLElement>(".key-group");

  expect(groups[0].style.gridTemplateRows).toBe("repeat(1, minmax(72px, 1fr))");
  expect(groups[0].style.flexGrow).toBe("0.2");
  expect(groups[1].style.gridTemplateRows).toBe("repeat(4, minmax(72px, 1fr))");
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
      onSelect={vi.fn()}
    />,
  );

  expect(getByRole("button", { name: "A，2 actions" })).toBeInTheDocument();
});

test("keeps the full key label available as a tooltip", () => {
  const layout: ModelLayout = {
    id: "label-visibility-test",
    name: "Label visibility test",
    groups: [{ id: "keys", columns: 2, buttons: [
      { id: "A", label: "打开工作台" },
      { id: "B", label: "Open calendar" },
    ]}],
  };
  render(
    <Keypad
      layout={layout}
      actions={{ A: { press: [{ type: "delay", duration_ms: 1 }], release: [], long_press: [], double_press: [] } }}
      selectedButtonId={null}
      pressedButtonIds={new Set()}
      actionCountLabel={(count) => `${count} actions`}
      onSelect={vi.fn()}
    />,
  );

  const button = screen.getByRole("button", { name: "打开工作台，1 actions" });
  expect(button).toHaveAttribute("title", "打开工作台");
  expect(button.querySelector(".key-button-label")).toHaveTextContent("打开工作台");
  expect(button.querySelector(".key-button-count")).toHaveTextContent("1 actions");
  expect(screen.getByText("Open calendar", { selector: ".key-button-label" })).toHaveClass("is-medium");
});

test("separates selected, physical pressed, and action summary semantics", () => {
  const layout: ModelLayout = {
    id: "semantic-test",
    name: "Semantic test",
    groups: [{ id: "keys", columns: 2, buttons: [
      { id: "A", label: "A" },
      { id: "B", label: "B" },
    ]}],
  };
  render(
    <Keypad
      layout={layout}
      actions={{ A: { press: [{ type: "delay", duration_ms: 1 }], release: [], long_press: [], double_press: [] } }}
      selectedButtonId="A"
      pressedButtonIds={new Set(["B"])}
      actionCountLabel={(count) => `${count} actions`}
      onSelect={vi.fn()}
    />,
  );

  const selected = screen.getByRole("button", { name: "A，1 actions" });
  const pressed = screen.getByRole("button", { name: "B，0 actions" });
  expect(selected).toHaveAttribute("aria-selected", "true");
  expect(selected).toHaveAttribute("aria-current", "true");
  expect(selected).toHaveAttribute("aria-pressed", "false");
  expect(pressed).toHaveAttribute("aria-selected", "false");
  expect(pressed).toHaveAttribute("aria-pressed", "true");
  expect(selected).toHaveAttribute("aria-describedby");
  expect(document.getElementById(selected.getAttribute("aria-describedby")!)).toHaveTextContent("1 actions");
});

test("moves focus by layout columns and across groups", () => {
  const layout: ModelLayout = {
    id: "navigation-test",
    name: "Navigation test",
    groups: [
      { id: "top", columns: 3, buttons: [
        { id: "A", label: "A" },
        { id: "B", label: "B" },
        { id: "C", label: "C" },
        { id: "D", label: "D" },
        { id: "E", label: "E" },
      ]},
      { id: "bottom", columns: 2, buttons: [
        { id: "F", label: "F" },
        { id: "G", label: "G" },
        { id: "H", label: "H" },
        { id: "I", label: "I" },
      ]},
    ],
  };
  render(
    <Keypad
      layout={layout}
      actions={{}}
      selectedButtonId={null}
      pressedButtonIds={new Set()}
      actionCountLabel={(count) => `${count}`}
      onSelect={vi.fn()}
    />,
  );

  const button = (id: string) => screen.getByRole("button", { name: `${id}，0` });
  button("A").focus();
  fireEvent.keyDown(button("A"), { key: "ArrowRight" });
  expect(button("B")).toHaveFocus();
  fireEvent.keyDown(button("B"), { key: "ArrowDown" });
  expect(button("E")).toHaveFocus();
  fireEvent.keyDown(button("E"), { key: "ArrowDown" });
  expect(button("G")).toHaveFocus();
  fireEvent.keyDown(button("G"), { key: "ArrowUp" });
  expect(button("E")).toHaveFocus();
  button("G").focus();
  fireEvent.keyDown(button("G"), { key: "ArrowLeft" });
  expect(button("F")).toHaveFocus();
});

test("selects a focused button with Enter and Space", async () => {
  const user = userEvent.setup();
  const onSelect = vi.fn();
  const layout: ModelLayout = {
    id: "activation-test",
    name: "Activation test",
    groups: [{ id: "keys", columns: 1, buttons: [{ id: "A", label: "A" }] }],
  };
  render(
    <Keypad
      layout={layout}
      actions={{}}
      selectedButtonId={null}
      pressedButtonIds={new Set()}
      actionCountLabel={(count) => `${count}`}
      onSelect={onSelect}
    />,
  );

  const button = screen.getByRole("button", { name: "A，0" });
  button.focus();
  await user.keyboard("{Enter}");
  await user.keyboard(" ");
  expect(onSelect).toHaveBeenNthCalledWith(1, "A");
  expect(onSelect).toHaveBeenNthCalledWith(2, "A");
});

test("routes Escape separately from button selection", () => {
  const onSelect = vi.fn();
  const onEscape = vi.fn();
  const layout: ModelLayout = {
    id: "escape-test",
    name: "Escape test",
    groups: [{ id: "keys", columns: 1, buttons: [{ id: "A", label: "A" }] }],
  };
  render(
    <Keypad
      layout={layout}
      actions={{}}
      selectedButtonId={null}
      pressedButtonIds={new Set()}
      actionCountLabel={(count) => `${count}`}
      onSelect={onSelect}
      onEscape={onEscape}
    />,
  );

  const button = screen.getByRole("button", { name: "A，0" });
  button.focus();
  fireEvent.keyDown(button, { key: "Escape" });

  expect(onEscape).toHaveBeenCalledOnce();
  expect(onSelect).not.toHaveBeenCalled();
});

test("keeps roving focus independent from physical press state", () => {
  const layout: ModelLayout = {
    id: "focus-test",
    name: "Focus test",
    groups: [{ id: "keys", columns: 2, buttons: [
      { id: "A", label: "A" },
      { id: "B", label: "B" },
    ]}],
  };
  const { rerender } = render(
    <Keypad
      layout={layout}
      actions={{}}
      selectedButtonId="A"
      pressedButtonIds={new Set()}
      actionCountLabel={(count) => `${count}`}
      onSelect={vi.fn()}
    />,
  );

  const selected = screen.getByRole("button", { name: "A，0" });
  const focused = screen.getByRole("button", { name: "B，0" });
  focused.focus();
  rerender(
    <Keypad
      layout={layout}
      actions={{}}
      selectedButtonId="A"
      pressedButtonIds={new Set(["A"])}
      actionCountLabel={(count) => `${count}`}
      onSelect={vi.fn()}
    />,
  );

  expect(focused).toHaveFocus();
  expect(selected).toHaveAttribute("aria-pressed", "true");
  expect(focused).toHaveAttribute("aria-pressed", "false");
});
