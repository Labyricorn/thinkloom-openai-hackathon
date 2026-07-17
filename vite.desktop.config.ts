import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  root: "desktop",
  build: { outDir: "../desktop-dist", emptyOutDir: true },
  server: { port: 1420, strictPort: true },
  clearScreen: false,
});
