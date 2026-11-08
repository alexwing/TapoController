import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Tauri expects a fixed port and should not clear the screen.
export default defineConfig({
  base: "./",
  plugins: [react()],
  clearScreen: false,
  server: {
    // 1420 falls in a Windows reserved port range (Hyper-V/WSL) -> EACCES.
    // 5173 is outside the excluded ranges; bind IPv4 to avoid ::1 issues.
    host: "127.0.0.1",
    port: 5173,
    strictPort: true,
    // Rust build output is locked while cargo links; watching it crashes Vite
    // (EBUSY on target/debug/deps/*.dll) whenever the backend rebuilds.
    watch: {
      ignored: ["**/target/**", "**/src-tauri/**", "**/crates/**"],
    },
  },
  build: {
    outDir: "dist",
    target: "esnext",
  },
});
