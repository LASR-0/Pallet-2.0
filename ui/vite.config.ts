import { defineConfig } from "vite";

// Tauri serves this over a custom protocol in production and from the dev
// server while developing, so assets must be referenced relatively.
export default defineConfig({
  base: "/",
  build: {
    outDir: "dist",
    emptyOutDir: true,
    target: "es2022",
  },
  server: {
    // Bound to IPv4 explicitly. Vite otherwise listens on [::1] only, and
    // WebKitGTK resolves localhost to 127.0.0.1 first, so the window loads
    // nothing and shows a connection error.
    host: "127.0.0.1",
    port: 5173,
    strictPort: true,
  },
  clearScreen: false,
});
