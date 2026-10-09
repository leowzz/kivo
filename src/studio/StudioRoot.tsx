import { Wrench, PackageOpen, Download } from "lucide-react";
import { lazy, Suspense, useState } from "react";
import brandIcon from "../../src-tauri/icons/128x128.png";
import StudioApp from "./StudioApp";
import { UpdateSettings, UpdateProgressDialog } from "../shared/UpdateSettings";
import { updateIsBusy } from "../shared/releaseUpdate";
import { useReleaseUpdate } from "../shared/useReleaseUpdate";

const Toolbox = lazy(() => import("./Toolbox"));

export default function StudioRoot() {
  const [view, setView] = useState<"definition" | "tools" | "update">("definition");
  const [firmwareBusy, setFirmwareBusy] = useState(false);
  const [draftBlocked, setDraftBlocked] = useState(false);
  const updateBlocked = firmwareBusy || draftBlocked;
  const update = useReleaseUpdate({
    beforeInstall: async () => {
      if (updateBlocked) throw new Error("Unsaved draft or active hardware operation");
    },
  });
  return (
    <>
    <div className="studio-hub-shell" inert={updateIsBusy(update.phase)}>
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
            aria-pressed={view === "tools"}
            disabled={firmwareBusy}
            onClick={() => setView("tools")}
          >
            <Wrench size={16} />
            工具台
          </button>
          <button aria-pressed={view === "update"} disabled={firmwareBusy}
            onClick={() => { setView("update"); update.check(); }}>
            <Download size={16} />
            应用更新
            {update.hasUpdate && <span className="update-badge" aria-label="发现新版本" />}
          </button>
        </nav>
      </header>
      <div className="studio-hub-panel" hidden={view !== "definition"}>
        <StudioApp onFirmwareBusyChange={setFirmwareBusy} onUpdateBlockedChange={setDraftBlocked} />
      </div>
      {view === "tools" && (
        <div className="studio-hub-panel">
          <Suspense
            fallback={
              <div className="studio-hub-loading" role="status">
                正在载入
              </div>
            }
          >
            <Toolbox onBusyChange={setFirmwareBusy} />
          </Suspense>
        </div>
      )}
      {view === "update" && <div className="studio-update-page">
        <UpdateSettings update={update} language="zh-CN" blocked={updateBlocked} productName="Kivo Product Studio" />
      </div>}
    </div>
    <UpdateProgressDialog update={update} language="zh-CN" productName="Kivo Product Studio" />
    </>
  );
}
