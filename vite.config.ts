import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// @ts-expect-error process is a nodejs global
const host = process.env.TAURI_DEV_HOST;

export default defineConfig(async () => ({
  plugins: [react()],
  // Let Rust compiler errors survive on screen instead of being cleared away.
  clearScreen: false,
  server: {
    // 1431 avoids both Tauri's 1420 default (VS Code auto-forwards it) and Remota's 1430.
    port: 1431,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: "ws", host, port: 1432 } : undefined,
    watch: { ignored: ["**/src-tauri/**"] },
  },
}));
