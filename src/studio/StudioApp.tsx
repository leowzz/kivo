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
  X,
} from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import type { ButtonGroup, InputSource } from "../types";
import type {
  NormalizedDefinition,
  ProductBuildResult,
  ProductDefinition,
  StudioBoard,
  StudioError,
  StudioSnapshot,
} from "./types";

type Tab = "identity" | "layout" | "hardware" | "definition";
type CreateMode = "new" | "copy";

const CAPABILITIES = ["mic", "spk", "disp", "enc", "encp"] as const;

function errorText(error: unknown) {
  if (typeof error === "object" && error && "code" in error) {
    const value = error as StudioError;
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

function canonicalIdentity(
  familyId: string,
  keys: number,
  capabilities: string[],
  revision: number,
) {
  const variant = `${familyId}-k${keys}${capabilities.map((token) => `-${token}`).join("")}`;
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
  const identity = canonicalIdentity(familyId, keys, capabilities, revision);
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
          keys: Object.fromEntries(buttons.map((button, index) => [button.id, board.safePins[index] ?? 0])),
        },
      ],
    },
  };
}

function syncNewIdentity(definition: ProductDefinition) {
  const next = clone(definition);
  const identity = canonicalIdentity(
    next.product.family_id,
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
  const [fatal, setFatal] = useState<string | null>(null);

  const dirty = definition ? JSON.stringify(definition) !== saved : false;

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
      return isNew ? syncNewIdentity(next) : next;
    });
  }, [isNew]);

  const save = async () => {
    if (!definition || validationError) return;
    setBusy(true);
    try {
      const next = await invoke<StudioSnapshot>("studio_save_product", {
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
          <button aria-label="复制产品版本" title="复制产品版本" disabled={!definition || dirty} onClick={() => setModal("copy")}><Copy size={15} /></button>
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
            <button disabled={!definition || !dirty || Boolean(validationError) || busy} onClick={save}><Save size={16} />保存</button>
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
              {tab === "identity" ? <IdentityEditor definition={definition} immutable={!isNew} update={update} /> : null}
              {tab === "layout" ? <LayoutEditor definition={definition} update={update} /> : null}
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
          onCreate={(next) => {
            setDefinition(next);
            setSelectedId(null);
            setSaved("");
            setIsNew(true);
            setModal(null);
            setTab("identity");
          }}
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

function IdentityEditor({ definition, immutable, update }: {
  definition: ProductDefinition;
  immutable: boolean;
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
    </section>
  );
}

export function LayoutEditor({ definition, update }: {
  definition: ProductDefinition;
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
          <button className="add-row" onClick={() => update((draft) => { const target = draft.layout.groups[groupIndex]; const id = nextButtonId(); target?.buttons.push({ id, label: id }); })}><CirclePlus size={15} />添加按键</button>
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

function HardwareEditor({ definition, boards, update }: {
  definition: ProductDefinition;
  boards: StudioBoard[];
  update: (mutate: (draft: ProductDefinition) => void) => void;
}) {
  const hardware = definition.hardware_profile;
  const board = boards.find((item) => item.id === hardware.board_profile_id) ?? boards[0];
  const buttons = definition.layout.groups.flatMap((group) => group.buttons);
  const addSource = (type: InputSource["type"]) => update((draft) => {
    const count = draft.hardware_profile.inputs.length + 1;
    if (type === "direct") draft.hardware_profile.inputs.push({ type, id: `direct-${count}`, keys: {} });
    if (type === "contact_matrix") draft.hardware_profile.inputs.push({ type, id: `matrix-${count}`, pins: [], keys: {} });
    if (type === "feature_switch") draft.hardware_profile.inputs.push({ type, id: `switch-${count}`, name: "Feature switch", gpio: board.safePins[0] ?? 0, buttons: [] });
  });
  return (
    <section className="editor-section hardware-editor">
      <h2>硬件实现</h2>
      <div className="form-grid">
        <Field label="Board Profile" wide><select value={hardware.board_profile_id} onChange={(event) => update((draft) => { draft.hardware_profile.board_profile_id = event.target.value; draft.hardware_profile.ssd1306 = undefined; draft.hardware_profile.inputs = []; })}>{boards.map((item) => <option key={item.id} value={item.id}>{item.displayName}</option>)}</select></Field>
        <Field label="Hardware ID"><input value={hardware.id} onChange={(event) => update((draft) => { draft.hardware_profile.id = event.target.value; })} /></Field>
        <Field label="Debounce (ms)"><input type="number" min={1} max={1000} value={hardware.debounce_ms} onChange={(event) => update((draft) => { draft.hardware_profile.debounce_ms = Number(event.target.value); })} /></Field>
      </div>
      <div className="oled-line">
        <label className="switch-control"><input type="checkbox" disabled={!board.supportsOled} checked={Boolean(hardware.ssd1306)} onChange={(event) => update((draft) => { draft.hardware_profile.ssd1306 = event.target.checked ? { sda: board.safePins.at(-2) ?? 0, scl: board.safePins.at(-1) ?? 1 } : undefined; })} /><span />SSD1306</label>
        {hardware.ssd1306 ? <><Field label="SDA"><PinSelect value={hardware.ssd1306.sda} board={board} onChange={(value) => update((draft) => { if (draft.hardware_profile.ssd1306) draft.hardware_profile.ssd1306.sda = value; })} /></Field><Field label="SCL"><PinSelect value={hardware.ssd1306.scl} board={board} onChange={(value) => update((draft) => { if (draft.hardware_profile.ssd1306) draft.hardware_profile.ssd1306.scl = value; })} /></Field></> : null}
      </div>
      <div className="source-heading"><h2>Input Sources</h2><div><button onClick={() => addSource("direct")}>Direct</button><button onClick={() => addSource("contact_matrix")}>Matrix</button><button onClick={() => addSource("feature_switch")}>Switch</button></div></div>
      <div className="source-list">
        {hardware.inputs.map((source, sourceIndex) => (
          <SourceEditor key={`${source.id}-${sourceIndex}`} source={source} sourceIndex={sourceIndex} board={board} buttons={buttons} update={update} />
        ))}
      </div>
    </section>
  );
}

function SourceEditor({ source, sourceIndex, board, buttons, update }: {
  source: InputSource;
  sourceIndex: number;
  board: StudioBoard;
  buttons: { id: string; label: string }[];
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
          {buttons.map((button) => <Field key={button.id} label={button.id}><PinSelect empty value={source.keys[button.id]} board={board} onChange={(value) => change((target) => { if (target.type !== "direct") return; if (Number.isNaN(value)) delete target.keys[button.id]; else target.keys[button.id] = value; })} /></Field>)}
        </div>
      ) : null}
      {open && source.type === "contact_matrix" ? (
        <div className="matrix-editor">
          <Field label="Pins" wide><input value={source.pins.join(", ")} onChange={(event) => change((target) => { if (target.type === "contact_matrix") target.pins = event.target.value.split(",").map(Number).filter(Number.isInteger); })} /></Field>
          <div className="binding-grid">{buttons.map((button) => <Field key={button.id} label={button.id}><input placeholder="row,col" value={source.keys[button.id]?.join(",") ?? ""} onChange={(event) => change((target) => { if (target.type !== "contact_matrix") return; const pins = event.target.value.split(",").map(Number); if (pins.length === 2 && pins.every(Number.isInteger)) target.keys[button.id] = [pins[0], pins[1]]; else delete target.keys[button.id]; })} /></Field>)}</div>
        </div>
      ) : null}
      {open && source.type === "feature_switch" ? (
        <div className="switch-editor"><Field label="Name"><input value={source.name} onChange={(event) => change((target) => { if (target.type === "feature_switch") target.name = event.target.value; })} /></Field><Field label="GPIO"><PinSelect value={source.gpio} board={board} onChange={(value) => change((target) => { if (target.type === "feature_switch") target.gpio = value; })} /></Field><div className="button-checks">{buttons.map((button) => <label key={button.id}><input type="checkbox" checked={source.buttons.includes(button.id)} onChange={(event) => change((target) => { if (target.type !== "feature_switch") return; target.buttons = event.target.checked ? [...target.buttons, button.id] : target.buttons.filter((id) => id !== button.id); })} />{button.id}</label>)}</div></div>
      ) : null}
    </section>
  );
}

function PinSelect({ value, board, onChange, empty = false }: { value: number | undefined; board: StudioBoard; onChange: (value: number) => void; empty?: boolean }) {
  return <select value={value ?? ""} onChange={(event) => onChange(event.target.value === "" ? Number.NaN : Number(event.target.value))}>{empty ? <option value="">Unbound</option> : null}{board.safePins.map((pin) => <option key={pin} value={pin}>GPIO {pin}</option>)}</select>;
}

function DefinitionPreview({ normalized }: { normalized: NormalizedDefinition | null }) {
  return <section className="definition-preview"><header><div><h2>product.json</h2><span>{normalized?.byteLength ?? 0} bytes</span></div><code>{normalized?.sha256 ?? "Invalid definition"}</code></header><pre>{normalized ? JSON.stringify(JSON.parse(normalized.json), null, 2) : ""}</pre></section>;
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
      const identity = canonicalIdentity(next.product.family_id, buttonCount(next), next.product.capabilities, revision);
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
