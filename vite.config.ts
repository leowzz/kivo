import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";

export default defineConfig(({ mode }) => {
  const studio = mode === "studio";
  // Tauri's production asset protocol loads the frontend root as index.html.
  const studioEntryPlugin = {
    name: "studio-tauri-entry",
    enforce: "post" as const,
    generateBundle(
      _options: unknown,
      bundle: Record<string, { fileName: string; source?: string | Uint8Array }>,
    ) {
      const studioEntry = bundle["studio.html"];
      if (!studioEntry?.source) return;
      this.emitFile({
        type: "asset",
        fileName: "index.html",
        source: studioEntry.source,
      });
    },
  };
  return {
    plugins: [react(), ...(studio ? [studioEntryPlugin] : [])],
    clearScreen: false,
    test: {
      environment: "jsdom",
      setupFiles: "./src/test/setup.ts",
    },
    server: {
      port: studio ? 1421 : 1420,
      strictPort: true,
    },
    build: studio
      ? {
          outDir: "dist-studio",
          rollupOptions: { input: "studio.html" },
        }
      : undefined,
  };
});
