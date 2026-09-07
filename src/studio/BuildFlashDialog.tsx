import { invoke } from "@tauri-apps/api/core";
import {
  LoaderCircle,
  RefreshCw,
  TriangleAlert,
  Upload,
  X,
} from "lucide-react";
import { useCallback, useEffect, useId, useRef, useState } from "react";
import {
  firmwareFailureText,
  firmwarePhases,
  invokeFirmware,
  type FirmwareDevice,
  type FirmwareResult,
} from "./firmware";
import type { BuiltFirmware } from "./types";

export default function BuildFlashDialog({
  artifact,
  onClose,
  onBusyChange,
}: {
  artifact: BuiltFirmware;
  onClose: () => void;
  onBusyChange?: (busy: boolean) => void;
}) {
  const dialog = useRef<HTMLDialogElement>(null);
  const cancel = useRef<HTMLButtonElement>(null);
  const mounted = useRef(false);
  const pending = useRef(false);
  const refreshVersion = useRef(0);
  const [devices, setDevices] = useState<FirmwareDevice[]>([]);
  const [selectedId, setSelectedId] = useState("");
  const [refreshing, setRefreshing] = useState(true);
  const [enumeration, setEnumeration] = useState(0);
  const [inspected, setInspected] = useState<{
    deviceId: string;
    firmware: FirmwareResult;
  } | null>(null);
  const [acknowledged, setAcknowledged] = useState(false);
  const [busy, setBusy] = useState(false);
  const [status, setStatus] = useState("");
  const [error, setError] = useState("");
  const [completed, setCompleted] = useState<FirmwareResult | null>(null);
  const titleId = useId();
  const device = devices.find((item) => item.id === selectedId);
  const firmware =
    inspected?.deviceId === selectedId ? inspected.firmware : null;

  const refresh = useCallback(async () => {
    const version = ++refreshVersion.current;
    setRefreshing(true);
    setError("");
    setAcknowledged(false);
    try {
      const next = await invoke<FirmwareDevice[]>("studio_list_devices");
      if (!mounted.current || version !== refreshVersion.current) return;
      const matching = next.filter(
        (item) => item.boardProfileId === artifact.boardProfileId,
      );
      setDevices(matching);
      setEnumeration((current) => current + 1);
      setSelectedId((current) =>
        matching.some((item) => item.id === current)
          ? current
          : matching.length === 1
            ? matching[0].id
            : "",
      );
    } catch (reason) {
      if (mounted.current && version === refreshVersion.current) {
        setDevices([]);
        setSelectedId("");
        setError(firmwareFailureText(reason));
      }
    } finally {
      if (mounted.current && version === refreshVersion.current)
        setRefreshing(false);
    }
  }, [artifact.boardProfileId]);

  useEffect(() => {
    mounted.current = true;
    const element = dialog.current!;
    element.showModal();
    cancel.current?.focus();
    void refresh();
    return () => {
      mounted.current = false;
      refreshVersion.current++;
      element.close();
    };
  }, [refresh]);

  useEffect(() => {
    let active = true;
    setInspected(null);
    setAcknowledged(false);
    setCompleted(null);
    setStatus("");
    if (!selectedId) return;
    setError("");
    setStatus("正在校验固件文件");
    void invokeFirmware(selectedId, "inspect", () => {}, artifact.firmwarePath)
      .then((result) => {
        if (!active) return;
        if (result.productVersionId !== artifact.productVersionId) {
          throw new Error("构建产物与所选产品不一致，请重新构建。");
        }
        setInspected({ deviceId: selectedId, firmware: result });
        setStatus("");
      })
      .catch((reason) => {
        if (active) {
          setError(firmwareFailureText(reason));
          setStatus("");
        }
      });
    return () => {
      active = false;
    };
  }, [
    selectedId,
    enumeration,
    artifact.firmwarePath,
    artifact.productVersionId,
  ]);

  async function flash() {
    if (
      pending.current ||
      refreshing ||
      !device ||
      !firmware ||
      !acknowledged ||
      completed
    )
      return;
    pending.current = true;
    setBusy(true);
    setError("");
    setAcknowledged(false);
    setStatus("正在准备刷写");
    onBusyChange?.(true);
    try {
      const result = await invokeFirmware(
        device.id,
        "flash",
        ({ phase }) => {
          if (mounted.current && firmwarePhases[phase])
            setStatus(firmwarePhases[phase]);
        },
        firmware.path,
        firmware.sha256,
      );
      if (mounted.current) {
        setCompleted(result);
        setStatus("固件已刷入并校验");
      }
    } catch (reason) {
      if (mounted.current) {
        setError(firmwareFailureText(reason));
        setStatus("刷写失败");
      }
    } finally {
      pending.current = false;
      onBusyChange?.(false);
      if (mounted.current) setBusy(false);
    }
  }

  function close() {
    if (!pending.current) onClose();
  }

  return (
    <dialog
      ref={dialog}
      className="studio-modal build-flash-dialog"
      aria-labelledby={titleId}
      onCancel={(event) => {
        event.preventDefault();
        close();
      }}
    >
      <header>
        <strong id={titleId}>确认刷入构建固件</strong>
        <button
          type="button"
          className="icon-button"
          aria-label="关闭刷写窗口"
          title="关闭刷写窗口"
          disabled={busy}
          onClick={close}
        >
          <X size={16} />
        </button>
      </header>
      <div className="modal-body build-flash-body">
        <label className="field">
          <span>目标设备</span>
          <div className="build-flash-devices">
            <select
              aria-label="目标设备"
              value={selectedId}
              disabled={busy || refreshing || Boolean(completed)}
              onChange={(event) => setSelectedId(event.target.value)}
            >
              <option value="">
                {devices.length ? "选择设备" : "未发现匹配板卡的设备"}
              </option>
              {devices.map((item) => (
                <option key={item.id} value={item.id}>
                  {item.name} · {item.serial}
                </option>
              ))}
            </select>
            <button
              type="button"
              className="icon-button"
              aria-label="刷新设备列表"
              title="刷新设备列表"
              disabled={busy || refreshing || Boolean(completed)}
              onClick={() => void refresh()}
            >
              <RefreshCw size={16} className={refreshing ? "spin" : ""} />
            </button>
          </div>
        </label>
        <dl>
          <div>
            <dt>产品</dt>
            <dd>{artifact.productVersionId}</dd>
          </div>
          <div>
            <dt>固件文件</dt>
            <dd>{artifact.firmwarePath}</dd>
          </div>
          {firmware && (
            <>
              <div>
                <dt>构建</dt>
                <dd>{firmware.buildId}</dd>
              </div>
              <div>
                <dt>SHA-256</dt>
                <dd>
                  <code>{firmware.sha256}</code>
                </dd>
              </div>
            </>
          )}
        </dl>
        {!completed && (
          <>
            <p className="build-flash-warning">
              <TriangleAlert size={16} />
              将覆盖所选设备现有固件。刷写期间请勿断开设备或电源。
            </p>
            <label className="test-firmware-acknowledgement">
              <input
                type="checkbox"
                checked={acknowledged}
                disabled={!firmware || busy || refreshing}
                onChange={(event) => setAcknowledged(event.target.checked)}
              />
              <span>我已确认目标设备，同意覆盖现有固件</span>
            </label>
          </>
        )}
        {status && (
          <p className="build-flash-status" role="status">
            {busy && <LoaderCircle size={15} className="spin" />}
            {status}
          </p>
        )}
        {error && (
          <p className="build-flash-error" role="alert">
            {error}
          </p>
        )}
        {completed?.warning && (
          <p className="build-flash-warning" role="alert">
            {completed.warning}
          </p>
        )}
      </div>
      <footer>
        <button ref={cancel} type="button" disabled={busy} onClick={close}>
          {completed ? "关闭" : "取消"}
        </button>
        {!completed && (
          <button
            type="button"
            className="danger-action"
            disabled={
              !device || !firmware || !acknowledged || busy || refreshing
            }
            onClick={() => void flash()}
          >
            {busy ? (
              <LoaderCircle size={15} className="spin" />
            ) : (
              <Upload size={15} />
            )}
            刷入固件
          </button>
        )}
      </footer>
    </dialog>
  );
}
