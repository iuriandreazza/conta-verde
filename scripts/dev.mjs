// Starts the database, the API and the web app on free ports, so a service already listening on
// 3000 or 5432 (another project, another worktree) never stops local development.
import { execFileSync, spawn } from "node:child_process";
import { readFileSync } from "node:fs";
import net from "node:net";

// Vite listens on `localhost`, which may be IPv6 only, so a port is free only if both loopbacks are.
const isFreeOn = (port, host) =>
  new Promise((resolve) => {
    const server = net.createServer();
    server.once("error", (error) => resolve(error.code !== "EADDRINUSE"));
    server.listen(port, host, () => server.close(() => resolve(true)));
  });

const isFree = async (port) => (await isFreeOn(port, "127.0.0.1")) && (await isFreeOn(port, "::1"));

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

// The same for a service with one or two published ports: reuse them when it is already up.
function runningPort(service, containerPort) {
  try {
    const out = execFileSync("docker", ["compose", "port", service, String(containerPort)], {
      encoding: "utf8",
      stdio: ["ignore", "pipe", "ignore"],
    });
    return Number(out.trim().split(":").pop()) || null;
  } catch {
    return null;
  }
}

const dbPort = runningDbPort() ?? (await freePort(5432));
const mailSmtpPort = runningPort("mailpit", 1025) ?? (await freePort(1025));
const mailUiPort = runningPort("mailpit", 8025) ?? (await freePort(8025));
const apiPort = await freePort(3000);
// The invitation link points at the web app, so the port Vite really gets has to be known up front.
const webPort = await freePort(5173);

execFileSync("docker", ["compose", "up", "-d", "--wait", "db", "mailpit"], {
  stdio: "inherit",
  env: {
    ...process.env,
    DB_PORT: String(dbPort),
    MAIL_SMTP_PORT: String(mailSmtpPort),
    MAIL_UI_PORT: String(mailUiPort),
  },
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
      DATABASE_MIGRATE_URL: exampleEnv.DATABASE_MIGRATE_URL.replace(":5432/", `:${dbPort}/`),
      BIND_ADDR: `127.0.0.1:${apiPort}`,
      PUBLIC_URL: `http://localhost:${webPort}`,
      // Everything the app mails lands in Mailpit, never in a real mailbox.
      SMTP_URL: `smtp://127.0.0.1:${mailSmtpPort}`,
      SMTP_FROM: "conta-verde <no-reply@localhost>",
    },
  }),
  spawn("pnpm", ["--filter", "conta-verde-web", "dev", "--port", String(webPort), "--strictPort"], {
    stdio: "inherit",
    env: { ...process.env, VITE_API_TARGET: `http://127.0.0.1:${apiPort}` },
  }),
];

console.log(`db :${dbPort}  api :${apiPort}  web http://localhost:${webPort}  mail http://localhost:${mailUiPort}`);

const stop = () => children.forEach((child) => child.kill("SIGTERM"));
process.on("SIGINT", stop);
process.on("SIGTERM", stop);
// If either side dies the other one is useless; stop it instead of leaving it orphaned.
children.forEach((child) => child.on("exit", stop));
