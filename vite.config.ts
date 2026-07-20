import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  server: { port: 1420, strictPort: true },
  clearScreen: false,
  build: {
    outDir: "dist",
    emptyOutDir: true,
    rolldownOptions: {
      output: {
        codeSplitting: {
          groups: [
            {
              name: "react",
              test: /node_modules[\\/](?:react|react-dom|scheduler)[\\/]/,
              priority: 30,
            },
            {
              name: "editor",
              test: /node_modules[\\/](?:@tiptap[\\/]|prosemirror-|orderedmap[\\/]|rope-sequence[\\/]|w3c-keyname[\\/])/,
              priority: 20,
            },
            {
              name: "markdown",
              test: /node_modules[\\/](?:markdown-it|linkify-it|mdurl|entities|uc.micro)[\\/]/,
              priority: 15,
            },
            {
              name: "vendor",
              test: /node_modules[\\/]/,
              priority: 10,
            },
          ],
        },
      },
    },
  },
});
