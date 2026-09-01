/* istanbul ignore file -- declarative build/test configuration */
/* v8 ignore file */
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";
import { viteSingleFile } from "vite-plugin-singlefile";

export default defineConfig({
  plugins: [react(), viteSingleFile()],
  build: {
    outDir: "../../src/docs/apps",
    emptyOutDir: false,
    rollupOptions: {
      input: "mcp-usage-overview.html",
    },
  },
});
