import { Check, Copy, Upload, X } from "lucide-react";
import { useState } from "react";
import BuildFlashDialog from "./BuildFlashDialog";
import type { BuiltFirmware } from "./types";

export default function BuildResultPanel({
  logs,
  artifact,
  canFlash,
  onClose,
  onFirmwareBusyChange,
}: {
  logs: string[];
  artifact: BuiltFirmware | null;
  canFlash: boolean;
  onClose: () => void;
  onFirmwareBusyChange?: (busy: boolean) => void;
}) {
  const [flashOpen, setFlashOpen] = useState(false);
  const [copied, setCopied] = useState(false);
  const [copyError, setCopyError] = useState("");

  async function copyOutput() {
    if (!artifact) return;
    setCopied(false);
    try {
      await navigator.clipboard.writeText(artifact.outputDirectory);
      setCopied(true);
      setCopyError("");
    } catch {
      setCopyError("复制失败，请重试。");
    }
  }

  return (
    <>
      <section
        className={`build-log${artifact ? " has-result" : ""}`}
        aria-label="构建结果"
      >
        <header>
          <strong>{artifact ? "构建完成" : "构建日志"}</strong>
          <button
            type="button"
            aria-label="关闭构建日志"
            title="关闭构建日志"
            onClick={onClose}
          >
            <X size={14} />
          </button>
        </header>
        <pre>{logs.join("\n")}</pre>
        {artifact && (
          <footer>
            {copyError && <span role="alert">{copyError}</span>}
            <button
              type="button"
              onClick={() => void copyOutput()}
              title="复制输出路径"
            >
              {copied ? <Check size={15} /> : <Copy size={15} />}
              {copied ? "已复制路径" : "复制输出路径"}
            </button>
            <button
              type="button"
              className="primary"
              disabled={!canFlash}
              onClick={() => setFlashOpen(true)}
            >
              <Upload size={15} />
              刷入固件
            </button>
          </footer>
        )}
      </section>
      {flashOpen && artifact && (
        <BuildFlashDialog
          artifact={artifact}
          onClose={() => setFlashOpen(false)}
          onBusyChange={onFirmwareBusyChange}
        />
      )}
    </>
  );
}
