import { defineConfig } from "vite";
import { resolve } from "node:path";

export default defineConfig({
  root: "ui",
  build: {
    outDir: resolve(__dirname, "dist"),
    emptyOutDir: !process.argv.includes("--watch"),
  },
  server: {
    port: 1420,
    strictPort: true,
  },
});
