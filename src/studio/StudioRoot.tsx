import { CircuitBoard, PackageOpen } from "lucide-react";
import { lazy, Suspense, useState } from "react";
import brandIcon from "../../src-tauri/icons/128x128.png";
import StudioApp from "./StudioApp";

const GpioMonitor = lazy(() => import("./GpioMonitor"));

export default function StudioRoot() {
  const [view, setView] = useState<"definition" | "gpio">("definition");
  const [firmwareBusy, setFirmwareBusy] = useState(false);
  return (
    <div className="studio-hub-shell">
      <header className="studio-hub-header">
        <div className="studio-hub-brand">
          <img src={brandIcon} alt="" />
          <strong>Kivo Studio</strong>
        </div>
        <nav className="studio-hub-tabs" aria-label="Studio 工作区">
          <button
            aria-pressed={view === "definition"}
            disabled={firmwareBusy}
            onClick={() => setView("definition")}
          >
            <PackageOpen size={16} />
            产品定义
          </button>
          <button
            aria-pressed={view === "gpio"}
            disabled={firmwareBusy}
            onClick={() => setView("gpio")}
          >
            <CircuitBoard size={16} />
            硬件测试
          </button>
        </nav>
      </header>
      <div className="studio-hub-panel" hidden={view !== "definition"}>
        <StudioApp />
      </div>
      {view === "gpio" && (
        <div className="studio-hub-panel">
          <Suspense
            fallback={
              <div className="studio-hub-loading" role="status">
                正在载入
              </div>
            }
          >
            <GpioMonitor onBusyChange={setFirmwareBusy} />
          </Suspense>
        </div>
      )}
    </div>
  );
}
