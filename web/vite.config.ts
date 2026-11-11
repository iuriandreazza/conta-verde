import react from "@vitejs/plugin-react";
import { resolve } from "node:path";
import { build, loadEnv, type Plugin } from "vite";
import { defineConfig } from "vitest/config";

// The service worker is its own entry, built after the app into the same folder under a fixed name (a worker's address is its identity,
// so it cannot be hashed). It is not served in development, where the app does not register it.
function serviceWorker(): Plugin {
  let root = process.cwd();
  let outDir = "dist";
  return {
    name: "conta-verde-service-worker",
    apply: "build",
    configResolved(config) {
      root = config.root;
      outDir = resolve(config.root, config.build.outDir);
    },
    async closeBundle() {
      await build({
        configFile: false,
        root,
        publicDir: false,
        logLevel: "warn",
        build: {
          outDir,
          emptyOutDir: false,
          minify: true,
          lib: { entry: resolve(root, "src/pwa/sw.ts"), formats: ["iife"], name: "contaVerdeWorker", fileName: () => "sw.js" },
        },
      });
    },
  };
}

// The browser only talks to its own origin: `/api/*` is proxied to the Rust API with the prefix
// removed. That keeps the session cookie same-origin, so the API needs no CORS. Production uses a
// reverse proxy with the same rule.
export default defineConfig(({ mode }) => {
  // loadEnv also reads .env files, which `process.env` alone would miss.
  const env = loadEnv(mode, process.cwd(), "VITE_");
  const apiTarget = env.VITE_API_TARGET || "http://127.0.0.1:3000";

  return {
    plugins: [react(), serviceWorker()],
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
