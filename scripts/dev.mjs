// Starts the database, the API and the web app on free ports, so a service already listening on
// 3000 or 5432 (another project, another worktree) never stops local development.
import { execFileSync, spawn } from "node:child_process";
import { readFileSync } from "node:fs";
import net from "node:net";

const isFree = (port) =>
  new Promise((resolve) => {
    const server = net.createServer();
    server.once("error", () => resolve(false));
    server.listen(port, "127.0.0.1", () => server.close(() => resolve(true)));
  });

async function freePort(preferred) {
  for (let port = preferred; port < preferred + 100; port++) {
    if (await isFree(port)) return port;
  }
  throw new Error(`no free port from ${preferred}`);
}

// Reuse the port of a database that is already up; asking for another one would recreate it.
function runningDbPort() {
  try {
    const out = execFileSync("docker", ["compose", "port", "db", "5432"], {
      encoding: "utf8",
      stdio: ["ignore", "pipe", "ignore"],
    });
    return Number(out.trim().split(":").pop()) || null;
  } catch {
    return null;
  }
}

const dbPort = runningDbPort() ?? (await freePort(5432));
const apiPort = await freePort(3000);

execFileSync("docker", ["compose", "up", "-d", "--wait", "db"], {
  stdio: "inherit",
  env: { ...process.env, DB_PORT: String(dbPort) },
});

const exampleEnv = Object.fromEntries(
  readFileSync(".env.example", "utf8")
    .split("\n")
    .filter((line) => line.includes("=") && !line.startsWith("#"))
    .map((line) => [line.slice(0, line.indexOf("=")), line.slice(line.indexOf("=") + 1)]),
);

const children = [
  spawn("cargo", ["run", "-p", "conta-verde-api"], {
    stdio: "inherit",
    env: {
      ...process.env,
      ...exampleEnv,
      DATABASE_URL: exampleEnv.DATABASE_URL.replace(":5432/", `:${dbPort}/`),
      BIND_ADDR: `127.0.0.1:${apiPort}`,
    },
  }),
  // Vite moves to the next port on its own when 5173 is taken.
  spawn("pnpm", ["--filter", "conta-verde-web", "dev"], {
    stdio: "inherit",
    env: { ...process.env, VITE_API_TARGET: `http://127.0.0.1:${apiPort}` },
  }),
];

console.log(`db :${dbPort}  api :${apiPort}`);

const stop = () => children.forEach((child) => child.kill("SIGTERM"));
process.on("SIGINT", stop);
process.on("SIGTERM", stop);
// If either side dies the other one is useless; stop it instead of leaving it orphaned.
children.forEach((child) => child.on("exit", stop));
