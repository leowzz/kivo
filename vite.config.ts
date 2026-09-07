import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";

export default defineConfig(({ mode }) => {
  const studio = mode === "studio";
  return {
    plugins: [react()],
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
