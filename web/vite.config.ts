import react from "@vitejs/plugin-react";
import { loadEnv } from "vite";
import { defineConfig } from "vitest/config";

// The browser only talks to its own origin: `/api/*` is proxied to the Rust API with the prefix
// removed. That keeps the session cookie same-origin, so the API needs no CORS. Production uses a
// reverse proxy with the same rule.
export default defineConfig(({ mode }) => {
  // loadEnv also reads .env files, which `process.env` alone would miss.
  const env = loadEnv(mode, process.cwd(), "VITE_");
  const apiTarget = env.VITE_API_TARGET || "http://127.0.0.1:3000";

  return {
    plugins: [react()],
    server: {
      proxy: {
        "/api": {
          target: apiTarget,
          rewrite: (path: string) => path.replace(/^\/api/, ""),
        },
      },
    },
    test: {
      environment: "jsdom",
      globals: true,
      setupFiles: ["./src/test-setup.ts"],
    },
  };
});
