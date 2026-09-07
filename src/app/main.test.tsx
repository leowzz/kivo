import type { ReactElement } from "react";
import { expect, test, vi } from "vitest";

const renderRoot = vi.hoisted(() => vi.fn());

vi.mock("react-dom/client", () => ({
  createRoot: () => ({ render: renderRoot }),
}));
vi.mock("./App", () => ({ default: () => null }));

test("mounts the full device workspace in the entry app", async () => {
  document.body.innerHTML = '<div id="root"></div>';

  await import("./main");

  const strictMode = renderRoot.mock.calls[0]?.[0] as ReactElement<{
    children: ReactElement<{ client?: boolean }>;
  }>;
  expect(strictMode.props.children.props.client).toBeUndefined();
});
