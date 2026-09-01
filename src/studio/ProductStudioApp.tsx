import { Bug, LoaderCircle, PackageOpen } from "lucide-react";
import { lazy, Suspense, useState } from "react";
import brandIcon from "../../src-tauri/icons/128x128.png";

type WorkspaceTab = "debug" | "definition";

const DebugWorkspace = lazy(() => import("../App"));
const ProductDefinitionWorkspace = lazy(() => import("./StudioApp"));

function LoadingWorkspace() {
  return (
    <div className="studio-hub-loading" role="status">
      <LoaderCircle className="spin" size={18} />
      正在载入
    </div>
  );
}

export default function ProductStudioApp() {
  const [activeTab, setActiveTab] = useState<WorkspaceTab>("debug");
  const [definitionVisited, setDefinitionVisited] = useState(false);

  const selectTab = (tab: WorkspaceTab) => {
    if (tab === "definition") setDefinitionVisited(true);
    setActiveTab(tab);
  };

  return (
    <main className="studio-hub-shell">
      <header className="studio-hub-header">
        <div className="studio-hub-brand">
          <img src={brandIcon} alt="" />
          <strong>Kivo Product Studio</strong>
        </div>
        <nav className="studio-hub-tabs" role="tablist" aria-label="Studio 工作区">
          <button
            id="studio-debug-tab"
            type="button"
            role="tab"
            aria-controls="studio-debug-panel"
            aria-selected={activeTab === "debug"}
            onClick={() => selectTab("debug")}
          >
            <Bug size={16} />开发调试
          </button>
          <button
            id="studio-definition-tab"
            type="button"
            role="tab"
            aria-controls="studio-definition-panel"
            aria-selected={activeTab === "definition"}
            onClick={() => selectTab("definition")}
          >
            <PackageOpen size={16} />产品定义
          </button>
        </nav>
      </header>

      <section
        id="studio-debug-panel"
        className="studio-hub-panel"
        role="tabpanel"
        aria-labelledby="studio-debug-tab"
        hidden={activeTab !== "debug"}
      >
        <Suspense fallback={<LoadingWorkspace />}>
          <DebugWorkspace embedded />
        </Suspense>
      </section>

      {definitionVisited ? (
        <section
          id="studio-definition-panel"
          className="studio-hub-panel"
          role="tabpanel"
          aria-labelledby="studio-definition-tab"
          hidden={activeTab !== "definition"}
        >
          <Suspense fallback={<LoadingWorkspace />}>
            <ProductDefinitionWorkspace />
          </Suspense>
        </section>
      ) : null}
    </main>
  );
}
