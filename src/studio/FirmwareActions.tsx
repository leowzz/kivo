import { Channel, invoke } from "@tauri-apps/api/core";
import { confirm, open, save } from "@tauri-apps/plugin-dialog";
import { Download, LoaderCircle, Upload } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import TestFirmwareDialog from "./TestFirmwareDialog";

interface Device {
  id: string;
  name: string;
  serial: string;
  boardProfileId: string;
}

interface FirmwareResult {
  path: string;
  bytes: number;
  sha256: string;
  productVersionId?: string;
  buildId?: string;
  warning?: string;
  backupPath?: string;
}

interface Progress {
  phase: string;
  backupPath?: string;
}

const phases: Record<string, string> = {
  checking: "正在核对设备产品",
  connecting: "正在进入刷写模式",
  reading: "正在读取原有固件",
  rebooting: "正在重启设备",
  writing: "正在刷入固件，请勿断开设备",
  verifying: "正在验证设备固件",
  building: "正在构建 I/O 测试固件",
  backup_saved: "原固件备份已保存",
};

type Operation = "inspect" | "backup" | "flash" | "install_test" | "status";

function invokeFirmware<T = FirmwareResult>(
  deviceId: string,
  operation: Operation,
  onProgress: (event: Progress) => void,
  path?: string,
  sha256?: string,
) {
  const progress = new Channel<Progress>();
  progress.onmessage = onProgress;
  return invoke<T>("studio_firmware_operation", {
    operation,
    deviceId,
    path: path ?? null,
    sha256: sha256 ?? null,
    progress,
  });
}

function failureText(reason: unknown): string {
  if (typeof reason === "object" && reason && "code" in reason) {
    if (reason.code === "studio_repository_not_configured")
      return "请先在产品定义中选择 Kivo 仓库。";
    if (reason.code === "studio_firmware_busy") return "已有固件操作正在进行。";
    if ("detail" in reason && reason.detail) return String(reason.detail);
    return String(reason.code);
  }
  return String(reason);
}

export default function FirmwareActions({
  device,
  disabled,
  onBusyChange,
  onFlashed,
}: {
  device: Device;
  disabled: boolean;
  onBusyChange: (busy: boolean) => void;
  onFlashed: (testing: boolean) => void;
}) {
  const [busy, setBusy] = useState(false);
  const [status, setStatus] = useState("");
  const [error, setError] = useState("");
  const [result, setResult] = useState<FirmwareResult | null>(null);
  const [backupPath, setBackupPath] = useState<string>();
  const [confirmTest, setConfirmTest] = useState(false);
  const testConfirmation = useRef<((approved: boolean) => void) | null>(null);
  const pending = useRef(false);
  const mounted = useRef(false);
  useEffect(() => {
    mounted.current = true;
    let active = true;
    void invokeFirmware<{ backupPath?: string }>(device.id, "status", () => {})
      .then((status) => {
        if (active && !pending.current) setBackupPath(status.backupPath);
      })
      .catch(() => {});
    return () => {
      active = false;
      mounted.current = false;
      testConfirmation.current?.(false);
      testConfirmation.current = null;
    };
  }, [device.id]);

  async function operate(operation: "backup" | "flash" | "install_test") {
    if (pending.current) return;
    pending.current = true;
    setBusy(true);
    onBusyChange(true);
    setError("");
    setResult(null);
    setStatus(operation === "install_test" ? "等待刷写确认" : "正在选择文件");
    const run = (action: Operation, path?: string, sha256?: string) => {
      return invokeFirmware(
        device.id,
        action,
        ({ phase, backupPath }) => {
          if (mounted.current && phases[phase]) setStatus(phases[phase]);
          if (mounted.current && backupPath) setBackupPath(backupPath);
        },
        path,
        sha256,
      );
    };
    let flashed = false;
    try {
      const extension = device.boardProfileId === "yd-rp2040" ? "uf2" : "bin";
      let completed: FirmwareResult;
      if (operation === "backup") {
        const serial = device.serial.replace(/[^A-Za-z0-9_.-]/g, "_");
        const path = await save({
          title: "备份固件",
          defaultPath: `${device.boardProfileId}-${serial}-${Date.now()}.${extension}`,
          filters: [{ name: "Flash 备份", extensions: [extension] }],
        });
        if (!path || !mounted.current) return;
        setStatus("正在准备备份");
        completed = await run("backup", path);
      } else if (operation === "install_test") {
        const approved = await new Promise<boolean>((resolve) => {
          testConfirmation.current = resolve;
          setConfirmTest(true);
        });
        if (!approved || !mounted.current) return;
        setStatus("正在准备测试固件");
        completed = await run("install_test");
        flashed = true;
      } else {
        const path = await open({
          title: "刷入固件文件",
          defaultPath: backupPath,
          multiple: false,
          filters: [{ name: "固件文件", extensions: [extension] }],
        });
        if (typeof path !== "string" || !mounted.current) return;
        setStatus("正在校验固件文件");
        const firmware = await run("inspect", path);
        if (!mounted.current) return;
        const approved = await confirm(
          `设备：${device.name} · ${device.serial}\n文件：${firmware.path}${firmware.productVersionId ? `\n产品：${firmware.productVersionId}` : ""}${firmware.buildId ? `\n构建：${firmware.buildId}` : ""}${device.boardProfileId === "yd-esp32-s3" ? "\n写入地址：0x0" : ""}\n\n将覆盖设备现有固件。刷写期间请勿断开设备。`,
          {
            title: "刷入固件文件",
            kind: "warning",
            okLabel: "刷入固件",
            cancelLabel: "取消",
          },
        );
        if (!approved || !mounted.current) return;
        setStatus("正在准备刷写");
        completed = await run("flash", firmware.path, firmware.sha256);
        if (!completed.warning) setBackupPath(undefined);
        flashed = true;
      }
      if (mounted.current) {
        setResult(completed);
        setStatus(
          operation === "backup"
            ? "固件已备份"
            : operation === "install_test"
              ? "I/O 测试固件已刷入"
              : "固件已刷入并校验",
        );
        if (completed.backupPath) setBackupPath(completed.backupPath);
      }
    } catch (reason) {
      if (mounted.current) {
        setError(failureText(reason));
        setStatus(operation === "backup" ? "备份失败" : "刷写失败");
      }
    } finally {
      pending.current = false;
      if (mounted.current) {
        setBusy(false);
        setStatus((current) =>
          current === "正在选择文件" ||
          current === "正在校验固件文件" ||
          current === "等待刷写确认"
            ? ""
            : current,
        );
        onBusyChange(false);
        if (flashed) onFlashed(operation === "install_test");
      }
    }
  }

  return (
    <section
      className="firmware-actions"
      aria-label="设备固件"
      aria-busy={busy && !confirmTest}
    >
      <div className="firmware-buttons">
        <button
          disabled={disabled || busy}
          onClick={() => void operate("backup")}
        >
          <Download size={15} />
          备份固件
        </button>
        <button
          disabled={disabled || busy}
          onClick={() => void operate("flash")}
        >
          <Upload size={15} />
          刷入固件文件
        </button>
        <button
          disabled={disabled || busy}
          onClick={() => void operate("install_test")}
        >
          <Upload size={15} />
          刷入测试固件
        </button>
        {status && !error && (
          <span role="status">
            {busy && <LoaderCircle size={14} className="spin" />}
            {status}
          </span>
        )}
      </div>
      {backupPath && (
        <div className="firmware-result">
          <span>原固件备份：{backupPath}</span>
        </div>
      )}
      {result && (
        <div className="firmware-result">
          <span>{result.path}</span>
          <small>
            {(result.bytes / 1024 / 1024).toFixed(2)} MB · SHA-256{" "}
            {result.sha256}
          </small>
          {result.warning && <p role="alert">{result.warning}</p>}
        </div>
      )}
      {error && (
        <div className="firmware-failure" role="alert">
          <strong>{status}</strong>
          <pre>{error}</pre>
        </div>
      )}
      {confirmTest && (
        <TestFirmwareDialog
          device={device}
          onDecision={(approved) => {
            const resolve = testConfirmation.current;
            testConfirmation.current = null;
            setConfirmTest(false);
            resolve?.(approved);
          }}
        />
      )}
    </section>
  );
}
