import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { SerializedSaveQueue, useAutosave } from "./useAutosave";

function deferred<T>() {
  let resolve!: (value: T | PromiseLike<T>) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, resolve, reject };
}

beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());

test("serializes saves and persists the newest revision", async () => {
  const first = deferred<void>();
  const save = vi.fn().mockReturnValueOnce(first.promise).mockResolvedValue(undefined);
  const queue = new SerializedSaveQueue();
  const { rerender, result } = renderHook(
    ({ value }) => useAutosave({ value, valid: true, delayMs: 400, save, queue }),
    { initialProps: { value: 1 } },
  );

  await act(() => vi.advanceTimersByTimeAsync(400));
  rerender({ value: 2 });
  await act(() => vi.advanceTimersByTimeAsync(400));
  expect(save).toHaveBeenCalledTimes(1);
  expect(result.current.status).toBe("saving");

  await act(async () => {
    first.resolve();
    await first.promise;
    await queue.flush();
  });
  expect(save).toHaveBeenLastCalledWith(2);
  expect(result.current.status).toBe("saved");
});

test("retains a failed revision and retries it", async () => {
  const save = vi.fn().mockRejectedValueOnce(new Error("disk full")).mockResolvedValue(undefined);
  const { result } = renderHook(() => useAutosave({
    value: "draft",
    valid: true,
    delayMs: 400,
    save,
    queue: new SerializedSaveQueue(),
  }));

  await act(() => vi.advanceTimersByTimeAsync(400));
  await act(async () => { await Promise.resolve(); });
  expect(result.current.status).toBe("error");
  expect(result.current.error).toBeInstanceOf(Error);

  await act(() => result.current.retry());
  expect(result.current.status).toBe("saved");
  expect(save).toHaveBeenCalledTimes(2);
});

test("reports saved when the persisted value becomes clean during the request", async () => {
  const pending = deferred<void>();
  const save = vi.fn().mockReturnValue(pending.promise);
  const queue = new SerializedSaveQueue();
  const draft = { text: "saved value" };
  const { rerender, result } = renderHook(
    ({ valid }) => useAutosave({ value: draft, valid, delayMs: 400, save, queue }),
    { initialProps: { valid: true } },
  );

  await act(() => vi.advanceTimersByTimeAsync(400));
  expect(result.current.status).toBe("saving");
  rerender({ valid: false });

  await act(async () => {
    pending.resolve();
    await pending.promise;
    await queue.flush();
  });
  expect(result.current.status).toBe("saved");
});
