import { invoke } from "@tauri-apps/api/core";
import {
  CircleAlert,
  LoaderCircle,
  Play,
  RefreshCw,
  Square,
} from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";

interface GpioDevice {
  id: string;
  port: string;
  boardProfileId: string;
  name: string;
  serial: string;
}

interface GpioSnapshot {
  sessionId: number;
  deviceId: string;
  firmwareBuildId: string;
  pins: { gpio: number; high: boolean }[];
}

const errors: Record<string, string> = {
  gpio_port_unavailable: "串口被占用或无法打开。请先退出 Kivo 或其他串口工具。",
  gpio_device_missing: "设备已断开",
  gpio_disconnected: "设备已断开",
  gpio_monitor_unsupported: "当前固件不支持 GPIO 测试，请刷入新版固件。",
  gpio_incompatible_firmware: "设备固件不兼容",
  gpio_invalid_response: "设备返回了无效的 GPIO 状态",
  gpio_response_timeout: "设备响应超时",
  gpio_handshake_failed: "设备握手失败",
  gpio_session_active: "已有硬件测试连接",
  gpio_session_closed: "测试连接已关闭",
  gpio_enumeration_failed: "无法获取串口设备",
  duplicate_identity: "设备身份重复，请只连接一台具有此序列号的设备。",
};

function errorText(error: unknown) {
  const code =
    typeof error === "object" && error && "code" in error
      ? String(error.code)
      : String(error);
  return errors[code] ?? code;
}

export default function GpioMonitor() {
  const [devices, setDevices] = useState<GpioDevice[]>([]);
  const [selectedId, setSelectedId] = useState("");
  const [refreshing, setRefreshing] = useState(false);
  const [running, setRunning] = useState(false);
  const [snapshot, setSnapshot] = useState<GpioSnapshot | null>(null);
  const [error, setError] = useState<string | null>(null);
  const connectionQueue = useRef(Promise.resolve());
  const mounted = useRef(false);
  const refreshVersion = useRef(0);

  const refresh = useCallback(async () => {
    const version = ++refreshVersion.current;
    setRefreshing(true);
    setError(null);
    try {
      const next = await invoke<GpioDevice[]>("studio_list_devices");
      if (!mounted.current || version !== refreshVersion.current) return;
      setDevices(next);
      setSelectedId((current) =>
        next.some((device) => device.id === current)
          ? current
          : (next[0]?.id ?? ""),
      );
    } catch (reason) {
      if (mounted.current && version === refreshVersion.current)
        setError(errorText(reason));
    } finally {
      if (mounted.current && version === refreshVersion.current)
        setRefreshing(false);
    }
  }, []);

  useEffect(() => {
    mounted.current = true;
    void refresh();
    return () => {
      mounted.current = false;
      refreshVersion.current++;
    };
  }, [refresh]);

  useEffect(() => {
    if (!running || !selectedId) return;
    let active = true;
    let sessionId: number | null = null;
    let timer: ReturnType<typeof setTimeout> | undefined;
    setSnapshot(null);
    setError(null);

    const fail = (reason: unknown) => {
      if (!active) return;
      setSnapshot(null);
      setError(errorText(reason));
      setRunning(false);
    };
    const poll = async () => {
      try {
        const next = await invoke<GpioSnapshot>("studio_read_gpio", {
          sessionId,
        });
        if (!active) return;
        setSnapshot(next);
        timer = setTimeout(() => void poll(), 125);
      } catch (reason) {
        fail(reason);
      }
    };
    // Serialize connection changes, including a connect that finishes after unmount.
    connectionQueue.current = connectionQueue.current.then(async () => {
      if (!active) return;
      try {
        const next = await invoke<GpioSnapshot>("studio_connect_gpio", {
          deviceId: selectedId,
        });
        sessionId = next.sessionId;
        if (!active) return;
        setSnapshot(next);
        timer = setTimeout(() => void poll(), 125);
      } catch (reason) {
        fail(reason);
      }
    });
    return () => {
      active = false;
      clearTimeout(timer);
      connectionQueue.current = connectionQueue.current.then(async () => {
        if (sessionId !== null) {
          await invoke("studio_disconnect_gpio", { sessionId }).catch(() => {});
        }
      });
    };
  }, [running, selectedId]);

  const selectedDevice = devices.find((device) => device.id === selectedId);
  const highCount = snapshot?.pins.filter((pin) => pin.high).length ?? 0;

  return (
    <main className="gpio-monitor">
      <header className="gpio-toolbar">
        <h1>硬件测试</h1>
        <select
          aria-label="测试设备"
          value={selectedId}
          disabled={running || refreshing}
          onChange={(event) => {
            setSelectedId(event.target.value);
            setSnapshot(null);
            setError(null);
          }}
        >
          {devices.length === 0 ? (
            <option value="">未发现设备</option>
          ) : (
            devices.map((device) => (
              <option key={`${device.id}:${device.port}`} value={device.id}>
                {device.name} · {device.serial}
              </option>
            ))
          )}
        </select>
        <button
          className="icon-button"
          title="刷新设备"
          aria-label="刷新设备"
          disabled={running || refreshing}
          onClick={() => void refresh()}
        >
          <RefreshCw size={16} className={refreshing ? "spin" : ""} />
        </button>
        <button
          className="gpio-run-button"
          disabled={!selectedId || refreshing}
          onClick={() => {
            setSnapshot(null);
            setRunning((current) => !current);
          }}
        >
          {running ? <Square size={15} /> : <Play size={15} />}
          {running ? "停止测试" : "开始测试"}
        </button>
      </header>
      <div className="gpio-status" role="status">
        <span>
          {running ? (
            snapshot ? (
              "实时采样"
            ) : (
              <>
                <LoaderCircle className="spin" size={14} />
                正在连接
              </>
            )
          ) : (
            "未连接"
          )}
        </span>
        {snapshot && (
          <>
            <span>
              <i className="gpio-high-swatch" />
              高电平 {highCount}
            </span>
            <span>
              <i className="gpio-low-swatch" />
              低电平 {snapshot.pins.length - highCount}
            </span>
          </>
        )}
        <span className="gpio-port">{selectedDevice?.port}</span>
      </div>
      {error && (
        <div className="gpio-error" role="alert">
          <CircleAlert size={16} />
          {error}
        </div>
      )}
      {snapshot ? (
        <section className="gpio-grid" aria-label="GPIO 电平">
          {snapshot.pins.map((pin) => (
            <div
              className={`gpio-pin${pin.high ? " is-high" : " is-low"}`}
              key={pin.gpio}
              aria-label={`GPIO ${pin.gpio} ${pin.high ? "高电平" : "低电平"}`}
            >
              <span>GPIO {pin.gpio}</span>
              <strong>{pin.high ? "HIGH" : "LOW"}</strong>
              <i aria-hidden="true" />
            </div>
          ))}
        </section>
      ) : (
        <div className="gpio-empty">
          {running
            ? "等待设备电平"
            : devices.length
              ? "测试未启动"
              : "未发现设备"}
        </div>
      )}
      <footer className="gpio-footer">
        {snapshot?.firmwareBuildId ?? selectedDevice?.boardProfileId}
      </footer>
    </main>
  );
}
