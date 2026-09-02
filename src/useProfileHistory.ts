import { useCallback, useMemo, useRef, useState } from "react";
import type { DeviceProfile, ProductConfigurationProfile } from "./types";

export const DEFAULT_PROFILE_HISTORY_MAX_SNAPSHOTS = 50;

export interface ProfileHistoryOptions {
  /** Maximum number of snapshots retained for each profile, including the current snapshot. */
  maxSnapshots?: number;
}

export interface ProfileHistory {
  /** Current immutable copies, kept in the same order as the supplied profiles. */
  profiles: DeviceProfile[];
  get(profileId: string): DeviceProfile | undefined;
  canUndo(profileId: string): boolean;
  canRedo(profileId: string): boolean;
  record(profile: DeviceProfile): DeviceProfile;
  update(profileId: string, update: (profile: DeviceProfile) => DeviceProfile): DeviceProfile | undefined;
  undo(profileId: string): DeviceProfile | undefined;
  redo(profileId: string): DeviceProfile | undefined;
  /** Drop navigable history while keeping each profile's current snapshot. */
  clear(profileId?: string): void;
  /**
   * Reconcile an authoritative profile list while preserving timelines whose
   * current snapshots are unchanged. New or changed profiles start fresh.
   */
  sync(profiles: readonly DeviceProfile[]): void;
  /** Replace authoritative snapshots and drop all history for the replaced profiles. */
  reset(profiles: readonly DeviceProfile[]): void;
}

export interface ProductConfigHistoryEntry {
  configurationId: string;
  config: ProductConfigurationProfile;
}

export interface ProductConfigHistory {
  /** Current immutable configs, kept in the same order as the supplied entries. */
  configs: ProductConfigHistoryEntry[];
  get(configurationId: string): ProductConfigurationProfile | undefined;
  canUndo(configurationId: string): boolean;
  canRedo(configurationId: string): boolean;
  record(configurationId: string, config: ProductConfigurationProfile): ProductConfigurationProfile;
  update(
    configurationId: string,
    update: (config: ProductConfigurationProfile) => ProductConfigurationProfile,
  ): ProductConfigurationProfile | undefined;
  undo(configurationId: string): ProductConfigurationProfile | undefined;
  redo(configurationId: string): ProductConfigurationProfile | undefined;
  /** Drop navigable history while keeping each device's current config. */
  clear(configurationId?: string): void;
  /** Reconcile an authoritative config list while preserving unchanged timelines. */
  sync(entries: readonly ProductConfigHistoryEntry[]): void;
  /** Replace authoritative configs and drop all history for the replaced devices. */
  reset(entries: readonly ProductConfigHistoryEntry[]): void;
}

interface Timeline {
  past: DeviceProfile[];
  present: DeviceProfile;
  future: DeviceProfile[];
}

interface ProductTimeline {
  past: ProductConfigurationProfile[];
  present: ProductConfigurationProfile;
  future: ProductConfigurationProfile[];
}

interface Mutation<T> {
  value: T;
  changed: boolean;
}

function cloneProfile(profile: DeviceProfile): DeviceProfile {
  return structuredClone(profile);
}

function cloneProductConfig(config: ProductConfigurationProfile): ProductConfigurationProfile {
  return structuredClone(config);
}

function valuesEqual(left: unknown, right: unknown): boolean {
  if (Object.is(left, right)) return true;
  if (typeof left !== "object" || typeof right !== "object" || left === null || right === null) {
    return false;
  }
  if (Array.isArray(left) || Array.isArray(right)) {
    if (!Array.isArray(left) || !Array.isArray(right) || left.length !== right.length) return false;
    return left.every((value, index) => valuesEqual(value, right[index]));
  }
  const leftRecord = left as Record<string, unknown>;
  const rightRecord = right as Record<string, unknown>;
  const leftKeys = Object.keys(leftRecord);
  const rightKeys = Object.keys(rightRecord);
  return leftKeys.length === rightKeys.length &&
    leftKeys.every((key) => Object.prototype.hasOwnProperty.call(rightRecord, key) && valuesEqual(leftRecord[key], rightRecord[key]));
}

function normalizeMaxSnapshots(maxSnapshots: number | undefined): number {
  const value = maxSnapshots ?? DEFAULT_PROFILE_HISTORY_MAX_SNAPSHOTS;
  if (!Number.isInteger(value) || value < 1) {
    throw new RangeError("maxSnapshots must be a positive integer");
  }
  return value;
}

class ProfileHistoryStore {
  private readonly maxSnapshots: number;
  private timelines: Map<string, Timeline>;

  constructor(profiles: readonly DeviceProfile[], maxSnapshots: number | undefined) {
    this.maxSnapshots = normalizeMaxSnapshots(maxSnapshots);
    this.timelines = this.createTimelines(profiles);
  }

  get profiles(): DeviceProfile[] {
    return [...this.timelines.values()].map(({ present }) => cloneProfile(present));
  }

  get(profileId: string): DeviceProfile | undefined {
    const timeline = this.timelines.get(profileId);
    return timeline ? cloneProfile(timeline.present) : undefined;
  }

  canUndo(profileId: string): boolean {
    return (this.timelines.get(profileId)?.past.length ?? 0) > 0;
  }

  canRedo(profileId: string): boolean {
    return (this.timelines.get(profileId)?.future.length ?? 0) > 0;
  }

  record(profile: DeviceProfile): Mutation<DeviceProfile> {
    const next = cloneProfile(profile);
    const profileId = next.profile.id;
    const timeline = this.timelines.get(profileId);
    if (!timeline) {
      this.timelines.set(profileId, { past: [], present: next, future: [] });
      return { value: cloneProfile(next), changed: true };
    }
    if (valuesEqual(timeline.present, next)) {
      return { value: cloneProfile(timeline.present), changed: false };
    }
    timeline.past = this.boundPast([...timeline.past, timeline.present]);
    timeline.present = next;
    timeline.future = [];
    return { value: cloneProfile(next), changed: true };
  }

  update(
    profileId: string,
    update: (profile: DeviceProfile) => DeviceProfile,
  ): Mutation<DeviceProfile | undefined> {
    const timeline = this.timelines.get(profileId);
    if (!timeline) return { value: undefined, changed: false };
    const next = update(cloneProfile(timeline.present));
    if (next.profile.id !== profileId) {
      throw new Error("Profile history updates must retain the profile id");
    }
    return this.record(next);
  }

  undo(profileId: string): Mutation<DeviceProfile | undefined> {
    const timeline = this.timelines.get(profileId);
    if (!timeline || timeline.past.length === 0) return { value: undefined, changed: false };
    const previous = timeline.past[timeline.past.length - 1];
    timeline.past = timeline.past.slice(0, -1);
    timeline.future = [timeline.present, ...timeline.future];
    timeline.present = previous;
    return { value: cloneProfile(previous), changed: true };
  }

  redo(profileId: string): Mutation<DeviceProfile | undefined> {
    const timeline = this.timelines.get(profileId);
    if (!timeline || timeline.future.length === 0) return { value: undefined, changed: false };
    const next = timeline.future[0];
    timeline.future = timeline.future.slice(1);
    timeline.past = this.boundPast([...timeline.past, timeline.present]);
    timeline.present = next;
    return { value: cloneProfile(next), changed: true };
  }

  clear(profileId?: string): Mutation<void> {
    let changed = false;
    const timelines = profileId ? [this.timelines.get(profileId)] : [...this.timelines.values()];
    for (const timeline of timelines) {
      if (!timeline || timeline.past.length === 0 && timeline.future.length === 0) continue;
      timeline.past = [];
      timeline.future = [];
      changed = true;
    }
    return { value: undefined, changed };
  }

  sync(profiles: readonly DeviceProfile[]): Mutation<void> {
    const previous = this.timelines;
    const previousIds = [...previous.keys()];
    const next = new Map<string, Timeline>();
    let changed = previous.size !== profiles.length;
    let index = 0;

    for (const profile of profiles) {
      const snapshot = cloneProfile(profile);
      const existing = previous.get(snapshot.profile.id);
      if (existing && valuesEqual(existing.present, snapshot)) {
        next.set(snapshot.profile.id, existing);
        if (previousIds[index] !== snapshot.profile.id) changed = true;
      } else {
        next.set(snapshot.profile.id, { past: [], present: snapshot, future: [] });
        changed = true;
      }
      index += 1;
    }

    if (!changed && previousIds.some((profileId, currentIndex) =>
      profileId !== profiles[currentIndex]?.profile.id
    )) {
      changed = true;
    }
    this.timelines = next;
    return { value: undefined, changed };
  }

  reset(profiles: readonly DeviceProfile[]): Mutation<void> {
    this.timelines = this.createTimelines(profiles);
    return { value: undefined, changed: true };
  }

  private createTimelines(profiles: readonly DeviceProfile[]): Map<string, Timeline> {
    const timelines = new Map<string, Timeline>();
    for (const profile of profiles) {
      const snapshot = cloneProfile(profile);
      timelines.set(snapshot.profile.id, { past: [], present: snapshot, future: [] });
    }
    return timelines;
  }

  private boundPast(past: DeviceProfile[]): DeviceProfile[] {
    const pastLimit = this.maxSnapshots - 1;
    return pastLimit === 0 ? [] : past.slice(-pastLimit);
  }
}

class ProductConfigHistoryStore {
  private readonly maxSnapshots: number;
  private timelines: Map<string, ProductTimeline>;

  constructor(entries: readonly ProductConfigHistoryEntry[], maxSnapshots: number | undefined) {
    this.maxSnapshots = normalizeMaxSnapshots(maxSnapshots);
    this.timelines = this.createTimelines(entries);
  }

  get configs(): ProductConfigHistoryEntry[] {
    return [...this.timelines.entries()].map(([configurationId, { present }]) => ({
      configurationId,
      config: cloneProductConfig(present),
    }));
  }

  get(configurationId: string): ProductConfigurationProfile | undefined {
    const timeline = this.timelines.get(configurationId);
    return timeline ? cloneProductConfig(timeline.present) : undefined;
  }

  canUndo(configurationId: string): boolean {
    return (this.timelines.get(configurationId)?.past.length ?? 0) > 0;
  }

  canRedo(configurationId: string): boolean {
    return (this.timelines.get(configurationId)?.future.length ?? 0) > 0;
  }

  record(configurationId: string, config: ProductConfigurationProfile): Mutation<ProductConfigurationProfile> {
    const next = cloneProductConfig(config);
    const timeline = this.timelines.get(configurationId);
    if (!timeline) {
      this.timelines.set(configurationId, { past: [], present: next, future: [] });
      return { value: cloneProductConfig(next), changed: true };
    }
    if (valuesEqual(timeline.present, next)) {
      return { value: cloneProductConfig(timeline.present), changed: false };
    }
    timeline.past = this.boundPast([...timeline.past, timeline.present]);
    timeline.present = next;
    timeline.future = [];
    return { value: cloneProductConfig(next), changed: true };
  }

  update(
    configurationId: string,
    update: (config: ProductConfigurationProfile) => ProductConfigurationProfile,
  ): Mutation<ProductConfigurationProfile | undefined> {
    const timeline = this.timelines.get(configurationId);
    if (!timeline) return { value: undefined, changed: false };
    return this.record(configurationId, update(cloneProductConfig(timeline.present)));
  }

  undo(configurationId: string): Mutation<ProductConfigurationProfile | undefined> {
    const timeline = this.timelines.get(configurationId);
    if (!timeline || timeline.past.length === 0) return { value: undefined, changed: false };
    const previous = timeline.past[timeline.past.length - 1];
    timeline.past = timeline.past.slice(0, -1);
    timeline.future = [timeline.present, ...timeline.future];
    timeline.present = previous;
    return { value: cloneProductConfig(previous), changed: true };
  }

  redo(configurationId: string): Mutation<ProductConfigurationProfile | undefined> {
    const timeline = this.timelines.get(configurationId);
    if (!timeline || timeline.future.length === 0) return { value: undefined, changed: false };
    const next = timeline.future[0];
    timeline.future = timeline.future.slice(1);
    timeline.past = this.boundPast([...timeline.past, timeline.present]);
    timeline.present = next;
    return { value: cloneProductConfig(next), changed: true };
  }

  clear(configurationId?: string): Mutation<void> {
    let changed = false;
    const timelines = configurationId ? [this.timelines.get(configurationId)] : [...this.timelines.values()];
    for (const timeline of timelines) {
      if (!timeline || timeline.past.length === 0 && timeline.future.length === 0) continue;
      timeline.past = [];
      timeline.future = [];
      changed = true;
    }
    return { value: undefined, changed };
  }

  sync(entries: readonly ProductConfigHistoryEntry[]): Mutation<void> {
    const previous = this.timelines;
    const previousIds = [...previous.keys()];
    const next = new Map<string, ProductTimeline>();
    let changed = previous.size !== entries.length;

    entries.forEach(({ configurationId, config }, index) => {
      const snapshot = cloneProductConfig(config);
      const existing = previous.get(configurationId);
      if (existing && valuesEqual(existing.present, snapshot)) {
        next.set(configurationId, existing);
        if (previousIds[index] !== configurationId) changed = true;
      } else {
        next.set(configurationId, { past: [], present: snapshot, future: [] });
        changed = true;
      }
    });
    if (!changed && previousIds.some((configurationId, index) => configurationId !== entries[index]?.configurationId)) {
      changed = true;
    }
    this.timelines = next;
    return { value: undefined, changed };
  }

  reset(entries: readonly ProductConfigHistoryEntry[]): Mutation<void> {
    this.timelines = this.createTimelines(entries);
    return { value: undefined, changed: true };
  }

  private createTimelines(entries: readonly ProductConfigHistoryEntry[]): Map<string, ProductTimeline> {
    const timelines = new Map<string, ProductTimeline>();
    for (const { configurationId, config } of entries) {
      const snapshot = cloneProductConfig(config);
      timelines.set(configurationId, { past: [], present: snapshot, future: [] });
    }
    return timelines;
  }

  private boundPast(past: ProductConfigurationProfile[]): ProductConfigurationProfile[] {
    const pastLimit = this.maxSnapshots - 1;
    return pastLimit === 0 ? [] : past.slice(-pastLimit);
  }
}

/**
 * Keep independent, bounded undo/redo timelines for each DeviceProfile.
 * Initial profiles are read once; call `reset` when an authoritative snapshot replaces them.
 */
export function useProfileHistory(
  initialProfiles: readonly DeviceProfile[] = [],
  options?: ProfileHistoryOptions,
): ProfileHistory {
  const storeRef = useRef<ProfileHistoryStore | null>(null);
  if (storeRef.current === null) {
    storeRef.current = new ProfileHistoryStore(initialProfiles, options?.maxSnapshots);
  }
  const store = storeRef.current;
  const [version, setVersion] = useState(0);
  const commit = useCallback(<T,>(operation: () => Mutation<T>): T => {
    const mutation = operation();
    if (mutation.changed) setVersion((version) => version + 1);
    return mutation.value;
  }, []);

  const get = useCallback((profileId: string) => store.get(profileId), [store]);
  const canUndo = useCallback((profileId: string) => store.canUndo(profileId), [store]);
  const canRedo = useCallback((profileId: string) => store.canRedo(profileId), [store]);
  const record = useCallback((profile: DeviceProfile) => commit(() => store.record(profile)), [commit, store]);
  const update = useCallback(
    (profileId: string, updater: (profile: DeviceProfile) => DeviceProfile) => commit(() => store.update(profileId, updater)),
    [commit, store],
  );
  const undo = useCallback((profileId: string) => commit(() => store.undo(profileId)), [commit, store]);
  const redo = useCallback((profileId: string) => commit(() => store.redo(profileId)), [commit, store]);
  const clear = useCallback((profileId?: string) => { commit(() => store.clear(profileId)); }, [commit, store]);
  const sync = useCallback((profiles: readonly DeviceProfile[]) => { commit(() => store.sync(profiles)); }, [commit, store]);
  const reset = useCallback((profiles: readonly DeviceProfile[]) => { commit(() => store.reset(profiles)); }, [commit, store]);
  return useMemo(() => ({
    profiles: store.profiles,
    get,
    canUndo,
    canRedo,
    record,
    update,
    undo,
    redo,
    clear,
    sync,
    reset,
  }), [canRedo, canUndo, clear, get, record, redo, reset, store, sync, undo, update, version]);
}

/** Keep independent, bounded undo/redo timelines for each product device config. */
export function useProductConfigHistory(
  initialEntries: readonly ProductConfigHistoryEntry[] = [],
  options?: ProfileHistoryOptions,
): ProductConfigHistory {
  const storeRef = useRef<ProductConfigHistoryStore | null>(null);
  if (storeRef.current === null) {
    storeRef.current = new ProductConfigHistoryStore(initialEntries, options?.maxSnapshots);
  }
  const store = storeRef.current;
  const [version, setVersion] = useState(0);
  const commit = useCallback(<T,>(operation: () => Mutation<T>): T => {
    const mutation = operation();
    if (mutation.changed) setVersion((current) => current + 1);
    return mutation.value;
  }, []);

  const get = useCallback((configurationId: string) => store.get(configurationId), [store]);
  const canUndo = useCallback((configurationId: string) => store.canUndo(configurationId), [store]);
  const canRedo = useCallback((configurationId: string) => store.canRedo(configurationId), [store]);
  const record = useCallback(
    (configurationId: string, config: ProductConfigurationProfile) => commit(() => store.record(configurationId, config)),
    [commit, store],
  );
  const update = useCallback(
    (configurationId: string, updater: (config: ProductConfigurationProfile) => ProductConfigurationProfile) => commit(() => store.update(configurationId, updater)),
    [commit, store],
  );
  const undo = useCallback((configurationId: string) => commit(() => store.undo(configurationId)), [commit, store]);
  const redo = useCallback((configurationId: string) => commit(() => store.redo(configurationId)), [commit, store]);
  const clear = useCallback((configurationId?: string) => { commit(() => store.clear(configurationId)); }, [commit, store]);
  const sync = useCallback((entries: readonly ProductConfigHistoryEntry[]) => { commit(() => store.sync(entries)); }, [commit, store]);
  const reset = useCallback((entries: readonly ProductConfigHistoryEntry[]) => { commit(() => store.reset(entries)); }, [commit, store]);
  return useMemo(() => ({
    configs: store.configs,
    get,
    canUndo,
    canRedo,
    record,
    update,
    undo,
    redo,
    clear,
    sync,
    reset,
  }), [canRedo, canUndo, clear, get, record, redo, reset, store, sync, undo, update, version]);
}
