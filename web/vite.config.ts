import react from "@vitejs/plugin-react";
import { defineConfig } from "vitest/config";

// The browser only talks to its own origin: `/api/*` is proxied to the Rust API with the prefix
// removed. That keeps the session cookie same-origin, so the API needs no CORS. Production uses a
// reverse proxy with the same rule.
const apiTarget = process.env.VITE_API_TARGET ?? "http://127.0.0.1:3000";

export default defineConfig({
  plugins: [react()],
  server: {
    proxy: {
      "/api": {
        target: apiTarget,
        rewrite: (path) => path.replace(/^\/api/, ""),
      },
    },
  },
  test: {
    environment: "jsdom",
    globals: true,
    setupFiles: ["./src/test-setup.ts"],
  },
});
