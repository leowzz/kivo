import { useCallback, useEffect, useRef, useState } from "react";

export type SaveStatus = "idle" | "saving" | "saved" | "error";

export class SerializedSaveQueue {
  private tail: Promise<void> = Promise.resolve();

  enqueue<T>(task: () => Promise<T>): Promise<T> {
    const result = this.tail.then(task);
    this.tail = result.then(() => undefined, () => undefined);
    return result;
  }

  flush() {
    return this.tail;
  }
}

interface AutosaveOptions<T> {
  value: T;
  valid: boolean;
  delayMs?: number;
  save: (value: T) => Promise<unknown>;
  queue: SerializedSaveQueue;
}

interface Attempt {
  revision: number;
  pending: boolean;
  promise: Promise<void>;
}

export function useAutosave<T>({
  value,
  valid,
  delayMs = 400,
  save,
  queue,
}: AutosaveOptions<T>) {
  const [status, setStatus] = useState<SaveStatus>("idle");
  const [error, setError] = useState<unknown>(null);
  const mounted = useRef(true);
  const revision = useRef(0);
  const latest = useRef({ revision: 0, value, valid });
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const attempt = useRef<Attempt | null>(null);
  const saveRef = useRef(save);
  const queueRef = useRef(queue);
  saveRef.current = save;
  queueRef.current = queue;

  useEffect(() => () => {
    mounted.current = false;
    if (timer.current) clearTimeout(timer.current);
  }, []);

  const persist = useCallback((target: typeof latest.current, force = false) => {
    if (!target.valid) return Promise.resolve();
    const currentAttempt = attempt.current;
    if (currentAttempt?.revision === target.revision && currentAttempt.pending) {
      return currentAttempt.promise;
    }
    if (!force && currentAttempt?.revision === target.revision) {
      return currentAttempt.promise;
    }
    if (mounted.current && target.revision === revision.current) {
      setStatus("saving");
      setError(null);
    }
    const queued = queueRef.current
      .enqueue(() => saveRef.current(target.value))
      .then(() => {
        if (mounted.current && target.revision === revision.current) {
          setStatus("saved");
          setError(null);
        }
      })
      .catch((saveError) => {
        if (mounted.current && target.revision === revision.current) {
          setStatus("error");
          setError(saveError);
        }
        throw saveError;
      });
    const nextAttempt: Attempt = {
      revision: target.revision,
      pending: true,
      promise: queued,
    };
    attempt.current = nextAttempt;
    void queued.finally(() => {
      nextAttempt.pending = false;
    }).catch(() => undefined);
    return queued;
  }, []);

  useEffect(() => {
    revision.current += 1;
    const target = { revision: revision.current, value, valid };
    latest.current = target;
    if (timer.current) clearTimeout(timer.current);
    if (!valid) {
      setStatus("idle");
      setError(null);
      return;
    }
    if (status === "error") {
      setStatus("idle");
      setError(null);
    }
    timer.current = setTimeout(() => {
      timer.current = null;
      void persist(target).catch(() => undefined);
    }, delayMs);
    return () => {
      if (timer.current) clearTimeout(timer.current);
    };
  }, [delayMs, persist, valid, value]);

  const retry = useCallback(() => persist(latest.current, true), [persist]);
  const flush = useCallback(() => {
    if (timer.current) {
      clearTimeout(timer.current);
      timer.current = null;
    }
    return persist(latest.current, true);
  }, [persist]);

  return { status, error, retry, flush };
}
