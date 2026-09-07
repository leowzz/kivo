import { TriangleAlert, Upload, X } from "lucide-react";
import { useEffect, useId, useRef, useState } from "react";

export default function TestFirmwareDialog({
  device,
  onDecision,
}: {
  device: { name: string; serial: string };
  onDecision: (approved: boolean) => void;
}) {
  const dialog = useRef<HTMLDialogElement>(null);
  const cancel = useRef<HTMLButtonElement>(null);
  const [acknowledged, setAcknowledged] = useState(false);
  const titleId = useId();
  const descriptionId = useId();

  useEffect(() => {
    const element = dialog.current!;
    element.showModal();
    cancel.current?.focus();
    return () => element.close();
  }, []);

  return (
    <dialog
      ref={dialog}
      className="studio-modal test-firmware-dialog"
      aria-labelledby={titleId}
      aria-describedby={descriptionId}
      onCancel={(event) => {
        event.preventDefault();
        onDecision(false);
      }}
    >
      <header>
        <strong id={titleId}>
          <TriangleAlert size={17} />
          确认刷入测试固件
        </strong>
        <button
          type="button"
          className="icon-button"
          aria-label="关闭刷写确认"
          title="关闭刷写确认"
          onClick={() => onDecision(false)}
        >
          <X size={16} />
        </button>
      </header>
      <div className="modal-body test-firmware-confirm-body">
        <dl>
          <div>
            <dt>目标设备</dt>
            <dd>{device.name}</dd>
          </div>
          <div>
            <dt>序列号</dt>
            <dd>
              <code>{device.serial}</code>
            </dd>
          </div>
        </dl>
        <div id={descriptionId} className="test-firmware-risks">
          <p>
            <strong>此操作将覆盖设备当前固件。</strong>
          </p>
          <p>临时固件仅用于 I/O 测试，刷入后按键动作和外设功能将停止。</p>
          <p>
            系统会先保存原固件备份，已有备份会保留。备份失败时不会刷写。测试结束后需通过“刷入固件文件”手动恢复。
          </p>
          <p>
            刷写中断可能导致设备无法正常启动，需要进入引导模式恢复。刷写期间请勿断开设备或电源。
          </p>
        </div>
        <label className="test-firmware-acknowledgement">
          <input
            type="checkbox"
            checked={acknowledged}
            onChange={(event) => setAcknowledged(event.target.checked)}
          />
          <span>我已确认目标设备，并了解覆盖固件及手动恢复的风险</span>
        </label>
      </div>
      <footer>
        <button ref={cancel} type="button" onClick={() => onDecision(false)}>
          取消
        </button>
        <button
          type="button"
          className="danger-action"
          disabled={!acknowledged}
          onClick={() => {
            if (acknowledged) onDecision(true);
          }}
        >
          <Upload size={15} />
          备份并刷入
        </button>
      </footer>
    </dialog>
  );
}
