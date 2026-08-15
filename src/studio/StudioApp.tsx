import { invoke } from "@tauri-apps/api/core";
import {
  Box,
  Braces,
  Check,
  ChevronDown,
  ChevronRight,
  CirclePlus,
  CircuitBoard,
  Copy,
  Hammer,
  KeyRound,
  LayoutGrid,
  LoaderCircle,
  Plus,
  RefreshCw,
  Save,
  Trash2,
  TriangleAlert,
  X,
} from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import type { ButtonGroup, HardwareProfile, InputSource } from "../types";
import type {
  NormalizedDefinition,
  ProductBuildResult,
  ProductDefinition,
  ProductSummary,
  StudioBoard,
  StudioError,
  StudioSnapshot,
} from "./types";

type Tab = "identity" | "layout" | "hardware" | "definition";
type CreateMode = "new" | "copy";
type DisplayComponent = "none" | "ssd1306" | "sh1106_ec11";

const CAPABILITIES = ["mic", "spk", "disp", "enc", "encp"] as const;

function errorText(error: unknown) {
  if (typeof error === "object" && error && "code" in error) {
    const value = error as StudioError;
    if (value.code === "product_already_exists") {
      const id = value.params?.productVersionId;
      return id
        ? `产品版本 ${id} 已存在，请提高 Hardware Revision 后再保存。`
        : "该产品版本已存在，请提高 Hardware Revision 后再保存。";
    }
    return value.detail ? `${value.code}: ${value.detail}` : value.code;
  }
  return String(error);
}

function clone<T>(value: T): T {
  return structuredClone(value);
}

function buttonCount(definition: ProductDefinition) {
  return definition.layout.groups.reduce((sum, group) => sum + group.buttons.length, 0);
}

function nextAvailableRevision(
  definition: ProductDefinition,
  controllerToken: string,
  products: ProductSummary[],
) {
  const existingIds = new Set(products.map((product) => product.productVersionId));
  let revision = Math.max(1, definition.product.hardware_revision + 1);
  while (existingIds.has(canonicalIdentity(
    definition.product.family_id,
    controllerToken,
    buttonCount(definition),
    definition.product.capabilities,
    revision,
  ).version)) {
    revision += 1;
  }
  return revision;
}

function orderedAssignablePins(safePins: number[]) {
  return [...safePins].sort((left, right) => left - right).filter((pin) => pin > 0);
}

function usedHardwarePins(hardware: HardwareProfile) {
  const used = new Set<number>();
  if (hardware.ssd1306) {
    used.add(hardware.ssd1306.sda);
    used.add(hardware.ssd1306.scl);
  }
  if (hardware.sh1106) {
    used.add(hardware.sh1106.sda);
    used.add(hardware.sh1106.scl);
    if (hardware.sh1106.control_panel) {
      const panel = hardware.sh1106.control_panel;
      [panel.confirm, panel.encoder_press, panel.encoder_a, panel.encoder_b, panel.back]
        .forEach((pin) => used.add(pin));
    }
  }
  for (const source of hardware.inputs) {
    if (source.type === "direct") Object.values(source.keys).forEach((pin) => used.add(pin));
    if (source.type === "contact_matrix") source.pins.forEach((pin) => used.add(pin));
    if (source.type === "feature_switch") used.add(source.gpio);
  }
  return used;
}

function usedInputPins(hardware: HardwareProfile) {
  const withoutDisplay = { ...hardware, ssd1306: undefined, sh1106: undefined };
  return usedHardwarePins(withoutDisplay);
}

function selectedDisplayComponent(hardware: HardwareProfile): DisplayComponent {
  if (hardware.sh1106) return "sh1106_ec11";
  if (hardware.ssd1306) return "ssd1306";
  return "none";
}

function displayComponentCanFit(
  component: Exclude<DisplayComponent, "none">,
  hardware: HardwareProfile,
  board: StudioBoard,
) {
  if (!board.supportsOled) return false;
  const requiredPins = component === "sh1106_ec11" ? 7 : 2;
  const occupied = usedInputPins(hardware);
  return orderedAssignablePins(board.safePins).filter((pin) => !occupied.has(pin)).length
    >= requiredPins;
}

function configureDisplayComponent(
  definition: ProductDefinition,
  board: StudioBoard,
  component: DisplayComponent,
) {
  const hardware = definition.hardware_profile;
  if (component === "none") {
    hardware.ssd1306 = undefined;
    hardware.sh1106 = undefined;
    return;
  }

  const capabilities = new Set(definition.product.capabilities);
  capabilities.add("disp");
  if (component === "sh1106_ec11") {
    capabilities.add("encp");
    capabilities.delete("enc");
  }
  definition.product.capabilities = CAPABILITIES.filter((token) => capabilities.has(token));

  const occupied = usedInputPins(hardware);
  const available = orderedAssignablePins(board.safePins).filter((pin) => !occupied.has(pin));
  const existing = hardware.sh1106 ?? hardware.ssd1306;
  const canKeepBus = existing
    && existing.sda !== existing.scl
    && available.includes(existing.sda)
    && available.includes(existing.scl);
  const sda = canKeepBus ? existing.sda : available.at(-2);
  const scl = canKeepBus ? existing.scl : available.at(-1);
  if (sda === undefined || scl === undefined) return;

  if (component === "ssd1306") {
    hardware.ssd1306 = { sda, scl };
    hardware.sh1106 = undefined;
    return;
  }

  const controlPins = available.filter((pin) => pin !== sda && pin !== scl);
  const existingPanel = existing?.control_panel;
  const existingControlPins = existingPanel
    ? [
        existingPanel.confirm,
        existingPanel.encoder_press,
        existingPanel.encoder_a,
        existingPanel.encoder_b,
        existingPanel.back,
      ]
    : [];
  const canKeepControls = existingControlPins.length === 5
    && new Set(existingControlPins).size === 5
    && existingControlPins.every((pin) => controlPins.includes(pin));
  const [confirm, encoderPress, encoderA, encoderB, back] = canKeepControls
    ? existingControlPins
    : controlPins.slice(0, 5);
  if ([confirm, encoderPress, encoderA, encoderB, back].some((pin) => pin === undefined)) return;
  hardware.sh1106 = {
    sda,
    scl,
    control_panel: {
      type: "ec11_confirm_back",
      confirm,
      encoder_press: encoderPress,
      encoder_a: encoderA,
      encoder_b: encoderB,
      back,
    },
  };
  hardware.ssd1306 = undefined;
}

function boundHardwareButtons(inputs: InputSource[]) {
  const bound = new Set<string>();
  for (const source of inputs) {
    if (source.type === "direct" || source.type === "contact_matrix") {
      Object.keys(source.keys).forEach((button) => bound.add(button));
    }
  }
  return bound;
}

function automaticDirectKeys(
  buttons: { id: string }[],
  safePins: number[],
  occupiedPins: ReadonlySet<number>,
  boundButtons: ReadonlySet<string> = new Set(),
) {
  const available = orderedAssignablePins(safePins).filter((pin) => !occupiedPins.has(pin));
  const keys: Record<string, number> = {};
  let pinIndex = 0;
  for (const button of buttons) {
    if (boundButtons.has(button.id) || pinIndex >= available.length) continue;
    keys[button.id] = available[pinIndex];
    pinIndex += 1;
  }
  return keys;
}

function nextSourceId(inputs: InputSource[], prefix: string) {
  const ids = new Set(inputs.map((source) => source.id));
  for (let index = 1; ; index += 1) {
    const candidate = `${prefix}-${index}`;
    if (!ids.has(candidate)) return candidate;
  }
}

function canonicalIdentity(
  familyId: string,
  controllerToken: string,
  keys: number,
  capabilities: string[],
  revision: number,
) {
  const variant = `${familyId}-${controllerToken}-k${keys}${capabilities.map((token) => `-${token}`).join("")}`;
  return {
    variant,
    version: `${variant}-r${String(revision).padStart(2, "0")}`,
  };
}

function emptyDefinition(
  displayName: string,
  familyId: string,
  keys: number,
  capabilities: string[],
  revision: number,
  board: StudioBoard,
): ProductDefinition {
  const identity = canonicalIdentity(
    familyId,
    board.controllerToken,
    keys,
    capabilities,
    revision,
  );
  const buttons = Array.from({ length: keys }, (_, index) => ({
    id: `K${index + 1}`,
    label: `K${index + 1}`,
  }));
  return {
    schema_version: 1,
    product: {
      display_name: displayName,
      family_id: familyId,
      variant_id: identity.variant,
      hardware_revision: revision,
      product_version_id: identity.version,
      capabilities,
    },
    layout: {
      id: identity.variant,
      name: displayName,
      groups: [{ id: "keys", columns: Math.min(4, Math.max(1, keys)), buttons }],
    },
    hardware_profile: {
      id: "hardware",
      name: "Default hardware",
      board_profile_id: board.id,
      debounce_ms: 30,
      inputs: [
        {
          type: "direct",
          id: "direct-1",
          keys: automaticDirectKeys(buttons, board.safePins, new Set()),
        },
      ],
    },
  };
}

function syncIdentity(definition: ProductDefinition, boards: StudioBoard[]) {
  const next = clone(definition);
  const board = boards.find((item) => item.id === next.hardware_profile.board_profile_id);
  if (!board) return next;
  const identity = canonicalIdentity(
    next.product.family_id,
    board.controllerToken,
    buttonCount(next),
    next.product.capabilities,
    next.product.hardware_revision,
  );
  next.product.variant_id = identity.variant;
  next.product.product_version_id = identity.version;
  next.layout.id = identity.variant;
  next.layout.name = next.product.display_name;
  return next;
}

function nextGroupId(groups: ButtonGroup[]) {
  const ids = new Set(groups.map((group) => group.id));
  let index = groups.length + 1;
  while (ids.has(`group-${index}`)) index += 1;
  return `group-${index}`;
}

export default function StudioApp() {
  const [snapshot, setSnapshot] = useState<StudioSnapshot | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [definition, setDefinition] = useState<ProductDefinition | null>(null);
  const [saved, setSaved] = useState("");
  const [isNew, setIsNew] = useState(false);
  const [tab, setTab] = useState<Tab>("identity");
  const [normalized, setNormalized] = useState<NormalizedDefinition | null>(null);
  const [validationError, setValidationError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [buildLogs, setBuildLogs] = useState<string[]>([]);
  const [modal, setModal] = useState<CreateMode | null>(null);
  const [deleteOpen, setDeleteOpen] = useState(false);
  const [fatal, setFatal] = useState<string | null>(null);

  const dirty = definition ? JSON.stringify(definition) !== saved : false;
  const conflictingProduct = definition && selectedId !== definition.product.product_version_id
    ? snapshot?.products.find(
        (product) => product.productVersionId === definition.product.product_version_id,
      ) ?? null
    : null;
  const definitionBoard = definition
    ? snapshot?.boards.find((board) => board.id === definition.hardware_profile.board_profile_id)
    : null;
  const suggestedRevision = definition && conflictingProduct && snapshot && definitionBoard
    ? nextAvailableRevision(definition, definitionBoard.controllerToken, snapshot.products)
    : null;

  const refresh = useCallback(async () => {
    const next = await invoke<StudioSnapshot>("studio_get_snapshot");
    setSnapshot(next);
    return next;
  }, []);

  useEffect(() => {
    refresh().catch((error) => setFatal(errorText(error)));
  }, [refresh]);

  const selectProduct = useCallback(async (id: string) => {
    if (dirty && !window.confirm("放弃未保存的修改？")) return;
    setBusy(true);
    try {
      const next = await invoke<ProductDefinition>("studio_load_product", {
        productVersionId: id,
      });
      setSelectedId(id);
      setDefinition(next);
      setSaved(JSON.stringify(next));
      setIsNew(false);
      setTab("identity");
      setBuildLogs([]);
    } catch (error) {
      setFatal(errorText(error));
    } finally {
      setBusy(false);
    }
  }, [dirty]);

  useEffect(() => {
    if (!definition) {
      setNormalized(null);
      setValidationError(null);
      return;
    }
    let active = true;
    const timer = window.setTimeout(() => {
      invoke<NormalizedDefinition>("studio_validate_product", { definition })
        .then((value) => {
          if (!active) return;
          setNormalized(value);
          setValidationError(null);
        })
        .catch((error) => {
          if (!active) return;
          setNormalized(null);
          setValidationError(errorText(error));
        });
    }, 160);
    return () => {
      active = false;
      window.clearTimeout(timer);
    };
  }, [definition]);

  const update = useCallback((mutate: (draft: ProductDefinition) => void) => {
    setDefinition((current) => {
      if (!current) return current;
      const next = clone(current);
      mutate(next);
      return syncIdentity(next, snapshot?.boards ?? []);
    });
  }, [snapshot?.boards]);

  const save = async () => {
    if (!definition) return;
    if (conflictingProduct) {
      setFatal(`产品版本 ${definition.product.product_version_id} 已存在，请使用建议的硬件版本号后再保存。`);
      return;
    }
    if (validationError) {
      setFatal(`无法保存：${validationError}`);
      return;
    }
    setBusy(true);
    try {
      const identityChanged = !isNew
        && selectedId !== null
        && selectedId !== definition.product.product_version_id;
      const next = identityChanged
        ? await invoke<StudioSnapshot>("studio_copy_product", {
            sourceProductVersionId: selectedId,
            definition,
          })
        : await invoke<StudioSnapshot>("studio_save_product", {
            definition,
            create: isNew,
          });
      setSnapshot(next);
      setSelectedId(definition.product.product_version_id);
      setSaved(JSON.stringify(definition));
      setIsNew(false);
    } catch (error) {
      setFatal(errorText(error));
    } finally {
      setBusy(false);
    }
  };

  const deleteProduct = async () => {
    if (!definition || busy) return;
    if (!selectedId) {
      setDeleteOpen(false);
      setDefinition(null);
      setSaved("");
      setIsNew(false);
      setBuildLogs([]);
      return;
    }
    setBusy(true);
    try {
      const next = await invoke<StudioSnapshot>("studio_delete_product", {
        productVersionId: selectedId,
      });
      setSnapshot(next);
      setSelectedId(null);
      setDefinition(null);
      setSaved("");
      setIsNew(false);
      setBuildLogs([]);
      setDeleteOpen(false);
    } catch (error) {
      setFatal(errorText(error));
      setDeleteOpen(false);
    } finally {
      setBusy(false);
    }
  };

  const createProduct = async (nextDefinition: ProductDefinition) => {
    if (modal !== "copy") {
      setDefinition(nextDefinition);
      setSelectedId(null);
      setSaved("");
      setIsNew(true);
      setModal(null);
      setTab("identity");
      return;
    }
    if (!selectedId) return;
    const sourceProductVersionId = selectedId;
    setModal(null);
    setBusy(true);
    try {
      const next = await invoke<StudioSnapshot>("studio_copy_product", {
        sourceProductVersionId,
        definition: nextDefinition,
      });
      setSnapshot(next);
      setSelectedId(nextDefinition.product.product_version_id);
      setDefinition(nextDefinition);
      setSaved(JSON.stringify(nextDefinition));
      setIsNew(false);
      setTab("identity");
      setBuildLogs([]);
    } catch (error) {
      setFatal(errorText(error));
    } finally {
      setBusy(false);
    }
  };

  const build = async () => {
    if (!definition || dirty || validationError) return;
    setBusy(true);
    setBuildLogs([`Building ${definition.product.product_version_id}`]);
    try {
      const result = await invoke<ProductBuildResult>("studio_build_product", {
        productVersionId: definition.product.product_version_id,
      });
      setBuildLogs([...result.logs, `Output: ${result.output.outputDirectory}`]);
    } catch (error) {
      setBuildLogs((logs) => [...logs, `Error: ${errorText(error)}`]);
    } finally {
      setBusy(false);
    }
  };

  if (!snapshot) {
    return <div className="studio-loading"><LoaderCircle className="spin" /> {fatal ?? "Loading"}</div>;
  }

  return (
    <div className="studio-shell">
      <aside className="studio-sidebar">
        <div className="studio-brand">
          <Box size={20} />
          <div><strong>Kivo Product Studio</strong><span>{snapshot.repoRoot}</span></div>
        </div>
        <div className="sidebar-actions">
          <button className="primary" onClick={() => setModal("new")}><Plus size={15} />新建</button>
          <button aria-label="复制产品版本" title="复制产品版本" disabled={!definition || dirty || busy} onClick={() => setModal("copy")}><Copy size={15} /></button>
          <button aria-label="刷新" title="刷新" onClick={() => refresh()}><RefreshCw size={15} /></button>
        </div>
        <nav className="product-list" aria-label="产品版本">
          {snapshot.products.map((product) => (
            <button
              key={product.productVersionId}
              className={selectedId === product.productVersionId ? "selected" : ""}
              onClick={() => selectProduct(product.productVersionId)}
            >
              <CircuitBoard size={16} />
              <span><strong>{product.displayName}</strong><small>{product.productVersionId}</small></span>
              {product.error ? <X className="error-icon" size={14} /> : <ChevronRight size={14} />}
            </button>
          ))}
          {snapshot.products.length === 0 ? <p className="empty-list">暂无产品版本</p> : null}
        </nav>
      </aside>

      <main className="studio-main">
        <header className="studio-toolbar">
          <div className="document-title">
            <strong>{definition?.product.display_name ?? "选择产品版本"}</strong>
            <span>{definition?.product.product_version_id ?? ""}</span>
            {dirty ? <i>未保存</i> : definition ? <i className="saved"><Check size={12} />已保存</i> : null}
          </div>
          <div className="toolbar-actions">
            <button
              disabled={!definition || !dirty || busy}
              title={conflictingProduct
                ? `无法保存：产品版本 ${conflictingProduct.productVersionId} 已存在`
                : validationError ? `无法保存：${validationError}` : "保存"}
              onClick={save}
            ><Save size={16} />保存</button>
            <button className="primary" disabled={!definition || dirty || Boolean(validationError) || busy} onClick={build}>
              {busy ? <LoaderCircle className="spin" size={16} /> : <Hammer size={16} />}构建
            </button>
          </div>
        </header>

        {definition ? (
          <>
            <div className="studio-tabs" role="tablist">
              <TabButton active={tab === "identity"} icon={<Box size={15} />} label="产品身份" onClick={() => setTab("identity")} />
              <TabButton active={tab === "layout"} icon={<LayoutGrid size={15} />} label="按键布局" onClick={() => setTab("layout")} />
              <TabButton active={tab === "hardware"} icon={<CircuitBoard size={15} />} label="硬件引脚" onClick={() => setTab("hardware")} />
              <TabButton active={tab === "definition"} icon={<Braces size={15} />} label="规范化定义" onClick={() => setTab("definition")} />
            </div>
            <div className="studio-workspace">
              {tab === "identity" ? (
                <IdentityEditor
                  definition={definition}
                  immutable={!isNew}
                  deleteLabel={selectedId ? "删除产品版本" : "丢弃草稿"}
                  deleting={busy}
                  conflictingProduct={conflictingProduct}
                  suggestedRevision={suggestedRevision}
                  onDelete={() => setDeleteOpen(true)}
                  onUseSuggestedRevision={() => {
                    if (suggestedRevision === null) return;
                    setFatal(null);
                    update((draft) => { draft.product.hardware_revision = suggestedRevision; });
                  }}
                  update={update}
                />
              ) : null}
              {tab === "layout" ? (
                <LayoutEditor
                  definition={definition}
                  board={snapshot.boards.find((board) => board.id === definition.hardware_profile.board_profile_id)}
                  update={update}
                />
              ) : null}
              {tab === "hardware" ? <HardwareEditor definition={definition} boards={snapshot.boards} update={update} /> : null}
              {tab === "definition" ? <DefinitionPreview normalized={normalized} /> : null}
            </div>
            <footer className="studio-status">
              <span className={validationError ? "invalid" : "valid"}>{validationError ?? `Valid · ${normalized?.byteLength ?? 0} bytes · ${normalized?.sha256.slice(0, 12) ?? ""}`}</span>
              <span>{definition.hardware_profile.board_profile_id}</span>
            </footer>
            {buildLogs.length > 0 ? (
              <section className="build-log">
                <header><strong>构建日志</strong><button aria-label="关闭构建日志" onClick={() => setBuildLogs([])}><X size={14} /></button></header>
                <pre>{buildLogs.join("\n")}</pre>
              </section>
            ) : null}
          </>
        ) : (
          <div className="studio-empty"><CircuitBoard size={36} /><strong>选择或新建产品版本</strong></div>
        )}
      </main>

      {modal ? (
        <CreateDialog
          mode={modal}
          source={definition}
          boards={snapshot.boards}
          onClose={() => setModal(null)}
          onCreate={createProduct}
        />
      ) : null}
      {deleteOpen && definition ? (
        <DeleteDialog
          productVersionId={selectedId}
          dirty={dirty}
          busy={busy}
          onClose={() => setDeleteOpen(false)}
          onConfirm={deleteProduct}
        />
      ) : null}
      {fatal ? <div className="fatal-banner"><span>{fatal}</span><button onClick={() => setFatal(null)}><X size={14} /></button></div> : null}
    </div>
  );
}

function TabButton(props: { active: boolean; icon: React.ReactNode; label: string; onClick: () => void }) {
  return <button role="tab" aria-selected={props.active} className={props.active ? "active" : ""} onClick={props.onClick}>{props.icon}{props.label}</button>;
}

function Field(props: { label: string; children: React.ReactNode; wide?: boolean }) {
  return <label className={props.wide ? "field wide" : "field"}><span>{props.label}</span>{props.children}</label>;
}

function IdentityEditor({
  definition,
  immutable,
  deleteLabel,
  deleting,
  conflictingProduct,
  suggestedRevision,
  onDelete,
  onUseSuggestedRevision,
  update,
}: {
  definition: ProductDefinition;
  immutable: boolean;
  deleteLabel: string;
  deleting: boolean;
  conflictingProduct: ProductSummary | null;
  suggestedRevision: number | null;
  onDelete: () => void;
  onUseSuggestedRevision: () => void;
  update: (mutate: (draft: ProductDefinition) => void) => void;
}) {
  return (
    <section className="editor-section">
      <h2>产品身份</h2>
      <div className="form-grid">
        <Field label="Display Name" wide><input value={definition.product.display_name} onChange={(event) => update((draft) => { draft.product.display_name = event.target.value; draft.layout.name = event.target.value; })} /></Field>
        <Field label="Product Family ID"><input disabled={immutable} value={definition.product.family_id} onChange={(event) => update((draft) => { draft.product.family_id = event.target.value; })} /></Field>
        <Field label="Hardware Revision"><input disabled={immutable} type="number" min={1} value={definition.product.hardware_revision} onChange={(event) => update((draft) => { draft.product.hardware_revision = Number(event.target.value); })} /></Field>
        <Field label="Product Variant ID" wide><input disabled value={definition.product.variant_id} /></Field>
        <Field label="Product Version ID" wide><input disabled value={definition.product.product_version_id} /></Field>
        {conflictingProduct && suggestedRevision !== null ? (
          <div className="identity-conflict" role="alert">
            <span><TriangleAlert size={16} />该版本已存在：<code>{conflictingProduct.displayName}</code></span>
            <button type="button" onClick={onUseSuggestedRevision}>
              改用 r{String(suggestedRevision).padStart(2, "0")}
            </button>
          </div>
        ) : null}
      </div>
      <h3>Capabilities</h3>
      <div className="capability-row">
        {CAPABILITIES.map((token) => (
          <label key={token} className="check-control"><input type="checkbox" disabled={immutable} checked={definition.product.capabilities.includes(token)} onChange={(event) => update((draft) => {
            const selected = new Set(draft.product.capabilities);
            if (token === "enc" && event.target.checked) selected.delete("encp");
            if (token === "encp" && event.target.checked) selected.delete("enc");
            event.target.checked ? selected.add(token) : selected.delete(token);
            draft.product.capabilities = CAPABILITIES.filter((value) => selected.has(value));
          })} /><span>{token}</span></label>
        ))}
      </div>
      <div className="identity-delete">
        <button disabled={deleting} onClick={onDelete}><Trash2 size={15} />{deleteLabel}</button>
      </div>
    </section>
  );
}

export function LayoutEditor({ definition, board, update }: {
  definition: ProductDefinition;
  board?: StudioBoard;
  update: (mutate: (draft: ProductDefinition) => void) => void;
}) {
  const [selectedGroupIndex, setSelectedGroupIndex] = useState(0);
  const groupIndex = definition.layout.groups.length === 0
    ? -1
    : Math.min(selectedGroupIndex, definition.layout.groups.length - 1);
  const group = groupIndex >= 0 ? definition.layout.groups[groupIndex] : undefined;
  const allButtons = definition.layout.groups.flatMap((item) => item.buttons);
  const nextButtonId = () => {
    const ids = new Set(allButtons.map((button) => button.id));
    for (let index = 1; ; index += 1) if (!ids.has(`K${index}`)) return `K${index}`;
  };
  const addGroup = () => {
    const id = nextGroupId(definition.layout.groups);
    const nextIndex = definition.layout.groups.length;
    update((draft) => { draft.layout.groups.push({ id, columns: 3, buttons: [] }); });
    setSelectedGroupIndex(nextIndex);
  };
  const deleteGroup = () => {
    if (groupIndex < 0) return;
    const nextCount = definition.layout.groups.length - 1;
    update((draft) => { draft.layout.groups.splice(groupIndex, 1); });
    setSelectedGroupIndex(Math.max(0, Math.min(groupIndex, nextCount - 1)));
  };
  const addButton = () => {
    const id = nextButtonId();
    update((draft) => {
      const target = draft.layout.groups[groupIndex];
      if (!target) return;
      target.buttons.push({ id, label: id });
      target.columns = Math.max(target.columns, target.buttons.length);
      if (!board) return;
      const directSources = draft.hardware_profile.inputs.filter(
        (source): source is Extract<InputSource, { type: "direct" }> => source.type === "direct",
      );
      if (directSources.length !== 1) return;
      const binding = automaticDirectKeys(
        [{ id }],
        board.safePins,
        usedHardwarePins(draft.hardware_profile),
        boundHardwareButtons(draft.hardware_profile.inputs),
      );
      Object.assign(directSources[0].keys, binding);
    });
  };
  return (
    <section className="layout-editor">
      <div className="group-rail">
        <header><h2>分组</h2><button title="添加分组" aria-label="添加分组" onClick={addGroup}><Plus size={15} /></button></header>
        {definition.layout.groups.map((item, itemIndex) => <button key={`${item.id}-${itemIndex}`} className={itemIndex === groupIndex ? "selected" : ""} onClick={() => setSelectedGroupIndex(itemIndex)}><span>{item.id}</span><small>{item.buttons.length}</small></button>)}
      </div>
      {group ? (
        <div className="group-editor">
          <header>
            <div><h2>{group.id}</h2><span>{group.buttons.length} keys</span></div>
            <button className="danger-icon" title="删除分组" aria-label="删除分组" onClick={deleteGroup}><Trash2 size={15} /></button>
          </header>
          <div className="inline-fields">
            <Field label="Group ID"><input value={group.id} onChange={(event) => { const value = event.target.value; update((draft) => { const target = draft.layout.groups[groupIndex]; if (target) target.id = value; }); }} /></Field>
            <Field label="Columns"><input type="number" min={1} value={group.columns} onChange={(event) => update((draft) => { const target = draft.layout.groups[groupIndex]; if (target) target.columns = Number(event.target.value); })} /></Field>
          </div>
          <button className="add-row" onClick={addButton}><CirclePlus size={15} />添加按键</button>
          <div className="button-table">
            <div className="table-head"><span>ID</span><span>Label</span><span /></div>
            {group.buttons.map((button, buttonIndex) => (
              <div className="table-row" key={`${button.id}-${buttonIndex}`}>
                <input value={button.id} onChange={(event) => { const value = event.target.value; update((draft) => { const target = draft.layout.groups[groupIndex]?.buttons[buttonIndex]; if (target) target.id = value; }); }} />
                <input value={button.label} onChange={(event) => { const value = event.target.value; update((draft) => { const target = draft.layout.groups[groupIndex]?.buttons[buttonIndex]; if (target) target.label = value; }); }} />
                <button title="删除按键" aria-label="删除按键" onClick={() => update((draft) => { draft.layout.groups[groupIndex]?.buttons.splice(buttonIndex, 1); })}><Trash2 size={14} /></button>
              </div>
            ))}
          </div>
        </div>
      ) : null}
      <div className="layout-preview">
        <h2>布局预览</h2>
        {definition.layout.groups.map((item, itemIndex) => (
          <div key={`${item.id}-${itemIndex}`} className="preview-group" style={{ gridTemplateColumns: `repeat(${item.columns}, minmax(42px, 1fr))` }}>
            {item.buttons.map((button, buttonIndex) => <div key={`${button.id}-${buttonIndex}`}><KeyRound size={14} /><span>{button.label}</span><small>{button.id}</small></div>)}
          </div>
        ))}
      </div>
    </section>
  );
}

export function HardwareEditor({ definition, boards, update }: {
  definition: ProductDefinition;
  boards: StudioBoard[];
  update: (mutate: (draft: ProductDefinition) => void) => void;
}) {
  const hardware = definition.hardware_profile;
  const board = boards.find((item) => item.id === hardware.board_profile_id) ?? boards[0];
  const buttons = definition.layout.groups.flatMap((group) => group.buttons);
  const unavailablePins = usedHardwarePins(hardware);
  const displayComponent = selectedDisplayComponent(hardware);
  const activeDisplay = hardware.sh1106 ?? hardware.ssd1306;
  const controlPanel = hardware.sh1106?.control_panel;
  const addSource = (type: InputSource["type"]) => update((draft) => {
    const inputs = draft.hardware_profile.inputs;
    if (type === "direct") {
      const draftButtons = draft.layout.groups.flatMap((group) => group.buttons);
      inputs.push({
        type,
        id: nextSourceId(inputs, "direct"),
        keys: automaticDirectKeys(
          draftButtons,
          board.safePins,
          usedHardwarePins(draft.hardware_profile),
          boundHardwareButtons(inputs),
        ),
      });
    }
    if (type === "contact_matrix") {
      inputs.push({ type, id: nextSourceId(inputs, "matrix"), pins: [], keys: {} });
    }
    if (type === "feature_switch") {
      const used = usedHardwarePins(draft.hardware_profile);
      const gpio = orderedAssignablePins(board.safePins).find((pin) => !used.has(pin));
      if (gpio === undefined) return;
      inputs.push({
        type,
        id: nextSourceId(inputs, "switch"),
        name: "Feature switch",
        gpio,
        buttons: [],
      });
    }
  });
  return (
    <section className="editor-section hardware-editor">
      <h2>硬件实现</h2>
      <div className="form-grid">
        <Field label="Board Profile" wide><select value={hardware.board_profile_id} onChange={(event) => update((draft) => { draft.hardware_profile.board_profile_id = event.target.value; draft.hardware_profile.ssd1306 = undefined; draft.hardware_profile.sh1106 = undefined; draft.hardware_profile.inputs = []; })}>{boards.map((item) => <option key={item.id} value={item.id}>{item.displayName}</option>)}</select></Field>
        <Field label="Hardware ID"><input value={hardware.id} onChange={(event) => update((draft) => { draft.hardware_profile.id = event.target.value; })} /></Field>
        <Field label="Debounce (ms)"><input type="number" min={1} max={1000} value={hardware.debounce_ms} onChange={(event) => update((draft) => { draft.hardware_profile.debounce_ms = Number(event.target.value); })} /></Field>
      </div>
      <div className="display-module">
        <Field label="显示组件" wide>
          <select value={displayComponent} onChange={(event) => update((draft) => {
            configureDisplayComponent(draft, board, event.target.value as DisplayComponent);
          })}>
            <option value="none">无</option>
            <option value="ssd1306" disabled={!displayComponentCanFit("ssd1306", hardware, board)}>SSD1306 128x32 @ 0x3C（2 IO）</option>
            <option value="sh1106_ec11" disabled={!displayComponentCanFit("sh1106_ec11", hardware, board)}>SH1106 1.3 英寸 128x64 + EC11 + 确认/返回（7 IO）</option>
          </select>
        </Field>
        {activeDisplay ? (
          <div className="display-pin-grid">
            <Field label="SDA"><PinSelect value={activeDisplay.sda} board={board} unavailablePins={unavailablePins} onChange={(value) => update((draft) => { const display = draft.hardware_profile.sh1106 ?? draft.hardware_profile.ssd1306; if (display) display.sda = value; })} /></Field>
            <Field label="SCL"><PinSelect value={activeDisplay.scl} board={board} unavailablePins={unavailablePins} onChange={(value) => update((draft) => { const display = draft.hardware_profile.sh1106 ?? draft.hardware_profile.ssd1306; if (display) display.scl = value; })} /></Field>
            {controlPanel ? <>
              <Field label="确认 KEY1"><PinSelect value={controlPanel.confirm} board={board} unavailablePins={unavailablePins} onChange={(value) => update((draft) => { if (draft.hardware_profile.sh1106?.control_panel) draft.hardware_profile.sh1106.control_panel.confirm = value; })} /></Field>
              <Field label="编码器按压 PSH"><PinSelect value={controlPanel.encoder_press} board={board} unavailablePins={unavailablePins} onChange={(value) => update((draft) => { if (draft.hardware_profile.sh1106?.control_panel) draft.hardware_profile.sh1106.control_panel.encoder_press = value; })} /></Field>
              <Field label="编码器 A 相 TRA"><PinSelect value={controlPanel.encoder_a} board={board} unavailablePins={unavailablePins} onChange={(value) => update((draft) => { if (draft.hardware_profile.sh1106?.control_panel) draft.hardware_profile.sh1106.control_panel.encoder_a = value; })} /></Field>
              <Field label="编码器 B 相 TRB"><PinSelect value={controlPanel.encoder_b} board={board} unavailablePins={unavailablePins} onChange={(value) => update((draft) => { if (draft.hardware_profile.sh1106?.control_panel) draft.hardware_profile.sh1106.control_panel.encoder_b = value; })} /></Field>
              <Field label="返回 KEY0"><PinSelect value={controlPanel.back} board={board} unavailablePins={unavailablePins} onChange={(value) => update((draft) => { if (draft.hardware_profile.sh1106?.control_panel) draft.hardware_profile.sh1106.control_panel.back = value; })} /></Field>
            </> : null}
          </div>
        ) : null}
      </div>
      <div className="source-heading"><h2>Input Sources</h2><div><button onClick={() => addSource("direct")}>Direct</button><button onClick={() => addSource("contact_matrix")}>Matrix</button><button onClick={() => addSource("feature_switch")}>Switch</button></div></div>
      <div className="source-list">
        {hardware.inputs.map((source, sourceIndex) => (
          <SourceEditor key={`${source.id}-${sourceIndex}`} source={source} sourceIndex={sourceIndex} board={board} buttons={buttons} unavailablePins={unavailablePins} update={update} />
        ))}
      </div>
    </section>
  );
}

function SourceEditor({ source, sourceIndex, board, buttons, unavailablePins, update }: {
  source: InputSource;
  sourceIndex: number;
  board: StudioBoard;
  buttons: { id: string; label: string }[];
  unavailablePins: ReadonlySet<number>;
  update: (mutate: (draft: ProductDefinition) => void) => void;
}) {
  const [open, setOpen] = useState(true);
  const change = (mutate: (source: InputSource) => void) => update((draft) => mutate(draft.hardware_profile.inputs[sourceIndex]));
  return (
    <section className="source-panel">
      <header>
        <button className="disclosure" onClick={() => setOpen((value) => !value)}>{open ? <ChevronDown size={15} /> : <ChevronRight size={15} />}{source.type}</button>
        <input value={source.id} onChange={(event) => change((target) => { target.id = event.target.value; })} />
        <button title="删除输入源" aria-label="删除输入源" onClick={() => update((draft) => { draft.hardware_profile.inputs.splice(sourceIndex, 1); })}><Trash2 size={14} /></button>
      </header>
      {open && source.type === "direct" ? (
        <div className="binding-grid">
          {buttons.map((button) => <Field key={button.id} label={button.id}><PinSelect empty value={source.keys[button.id]} board={board} unavailablePins={unavailablePins} onChange={(value) => change((target) => { if (target.type !== "direct") return; if (Number.isNaN(value)) delete target.keys[button.id]; else target.keys[button.id] = value; })} /></Field>)}
        </div>
      ) : null}
      {open && source.type === "contact_matrix" ? (
        <div className="matrix-editor">
          <Field label="Pins" wide><input value={source.pins.join(", ")} onChange={(event) => change((target) => { if (target.type === "contact_matrix") target.pins = event.target.value.split(",").map(Number).filter(Number.isInteger); })} /></Field>
          <div className="binding-grid">{buttons.map((button) => <Field key={button.id} label={button.id}><input placeholder="row,col" value={source.keys[button.id]?.join(",") ?? ""} onChange={(event) => change((target) => { if (target.type !== "contact_matrix") return; const pins = event.target.value.split(",").map(Number); if (pins.length === 2 && pins.every(Number.isInteger)) target.keys[button.id] = [pins[0], pins[1]]; else delete target.keys[button.id]; })} /></Field>)}</div>
        </div>
      ) : null}
      {open && source.type === "feature_switch" ? (
        <div className="switch-editor"><Field label="Name"><input value={source.name} onChange={(event) => change((target) => { if (target.type === "feature_switch") target.name = event.target.value; })} /></Field><Field label="GPIO"><PinSelect value={source.gpio} board={board} unavailablePins={unavailablePins} onChange={(value) => change((target) => { if (target.type === "feature_switch") target.gpio = value; })} /></Field><div className="button-checks">{buttons.map((button) => <label key={button.id}><input type="checkbox" checked={source.buttons.includes(button.id)} onChange={(event) => change((target) => { if (target.type !== "feature_switch") return; target.buttons = event.target.checked ? [...target.buttons, button.id] : target.buttons.filter((id) => id !== button.id); })} />{button.id}</label>)}</div></div>
      ) : null}
    </section>
  );
}

function PinSelect({ value, board, unavailablePins, onChange, empty = false }: {
  value: number | undefined;
  board: StudioBoard;
  unavailablePins?: ReadonlySet<number>;
  onChange: (value: number) => void;
  empty?: boolean;
}) {
  return (
    <select value={value ?? ""} onChange={(event) => onChange(event.target.value === "" ? Number.NaN : Number(event.target.value))}>
      {empty ? <option value="">Unbound</option> : null}
      {board.safePins.map((pin) => (
        <option key={pin} value={pin} disabled={pin !== value && unavailablePins?.has(pin)}>
          GPIO {pin}
        </option>
      ))}
    </select>
  );
}

function DefinitionPreview({ normalized }: { normalized: NormalizedDefinition | null }) {
  return <section className="definition-preview"><header><div><h2>product.json</h2><span>{normalized?.byteLength ?? 0} bytes</span></div><code>{normalized?.sha256 ?? "Invalid definition"}</code></header><pre>{normalized ? JSON.stringify(JSON.parse(normalized.json), null, 2) : ""}</pre></section>;
}

function DeleteDialog({ productVersionId, dirty, busy, onClose, onConfirm }: {
  productVersionId: string | null;
  dirty: boolean;
  busy: boolean;
  onClose: () => void;
  onConfirm: () => void;
}) {
  const deletingSavedProduct = productVersionId !== null;
  const title = deletingSavedProduct ? "删除产品版本" : "丢弃草稿";
  return (
    <div className="modal-backdrop" role="presentation" onMouseDown={(event) => { if (!busy && event.target === event.currentTarget) onClose(); }}>
      <div className="studio-modal" role="dialog" aria-modal="true" aria-labelledby="delete-product-title">
        <header><strong id="delete-product-title">{title}</strong><button aria-label="关闭删除确认" disabled={busy} onClick={onClose}><X size={15} /></button></header>
        <div className="modal-body delete-confirm-body">
          {deletingSavedProduct ? <p>确定删除产品版本 <code>{productVersionId}</code>？</p> : <p>确定丢弃当前未保存的产品草稿？</p>}
          {dirty && deletingSavedProduct ? <p>当前未保存的修改也会一并丢失。</p> : null}
          {deletingSavedProduct ? <p>对应的产品目录将被删除，此操作无法撤销。</p> : null}
        </div>
        <footer>
          <button disabled={busy} onClick={onClose}>取消</button>
          <button className="danger-action" disabled={busy} onClick={onConfirm}>
            {busy ? <LoaderCircle className="spin" size={15} /> : <Trash2 size={15} />}{deletingSavedProduct ? "确认删除" : "丢弃草稿"}
          </button>
        </footer>
      </div>
    </div>
  );
}

function CreateDialog({ mode, source, boards, onClose, onCreate }: {
  mode: CreateMode;
  source: ProductDefinition | null;
  boards: StudioBoard[];
  onClose: () => void;
  onCreate: (definition: ProductDefinition) => void;
}) {
  const [displayName, setDisplayName] = useState(source?.product.display_name ?? "Kivo Product");
  const [family, setFamily] = useState(source?.product.family_id ?? "kivo-product");
  const [keys, setKeys] = useState(source ? buttonCount(source) : 1);
  const [revision, setRevision] = useState((source?.product.hardware_revision ?? 0) + 1);
  const [capabilities, setCapabilities] = useState<string[]>(source?.product.capabilities ?? []);
  const submit = () => {
    if (mode === "copy" && source) {
      const next = clone(source);
      next.product.display_name = displayName;
      next.product.hardware_revision = revision;
      next.layout.name = displayName;
      const board = boards.find(
        (item) => item.id === next.hardware_profile.board_profile_id,
      );
      if (!board) return;
      const identity = canonicalIdentity(
        next.product.family_id,
        board.controllerToken,
        buttonCount(next),
        next.product.capabilities,
        revision,
      );
      next.product.variant_id = identity.variant;
      next.product.product_version_id = identity.version;
      onCreate(next);
      return;
    }
    onCreate(emptyDefinition(displayName, family, keys, capabilities, revision, boards[0]));
  };
  return (
    <div className="modal-backdrop" role="presentation" onMouseDown={(event) => { if (event.target === event.currentTarget) onClose(); }}>
      <div className="studio-modal" role="dialog" aria-modal="true">
        <header><strong>{mode === "copy" ? "复制产品版本" : "新建产品版本"}</strong><button onClick={onClose}><X size={15} /></button></header>
        <div className="modal-body">
          <Field label="Display Name" wide><input autoFocus value={displayName} onChange={(event) => setDisplayName(event.target.value)} /></Field>
          <Field label="Product Family ID"><input disabled={mode === "copy"} value={family} onChange={(event) => setFamily(event.target.value)} /></Field>
          <Field label="Hardware Revision"><input type="number" min={1} value={revision} onChange={(event) => setRevision(Number(event.target.value))} /></Field>
          {mode === "new" ? <Field label="Key Count"><input type="number" min={1} value={keys} onChange={(event) => setKeys(Number(event.target.value))} /></Field> : null}
          {mode === "new" ? <div className="capability-row wide">{CAPABILITIES.map((token) => <label key={token} className="check-control"><input type="checkbox" checked={capabilities.includes(token)} onChange={(event) => setCapabilities((current) => event.target.checked ? CAPABILITIES.filter((item) => item === token || current.includes(item)).filter((item) => !(token === "enc" && item === "encp") && !(token === "encp" && item === "enc")) : current.filter((item) => item !== token))} /><span>{token}</span></label>)}</div> : null}
        </div>
        <footer><button onClick={onClose}>取消</button><button className="primary" disabled={!displayName.trim() || !family.trim() || revision < 1 || keys < 1} onClick={submit}>{mode === "copy" ? "创建副本" : "创建草稿"}</button></footer>
      </div>
    </div>
  );
}
