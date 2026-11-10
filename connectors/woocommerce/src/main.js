import { readFileSync, writeFileSync, renameSync } from "node:fs";
import { createServer } from "node:http";
import { readConfig } from "./config.js";
import { ContaVerde } from "./contaVerde.js";
import { syncOrders } from "./sync.js";
import { createWebhookHandler, seenIds } from "./webhook.js";
import { WooCommerce } from "./woo.js";

const log = (line) => console.log(`${new Date().toISOString()} ${line}`);

function loadState(file, startFrom) {
  try {
    return JSON.parse(readFileSync(file, "utf8"));
  } catch {
    return { cursor: new Date(startFrom).toISOString(), pending: {}, handled: {} };
  }
}

function saveState(file, state) {
  writeFileSync(`${file}.tmp`, JSON.stringify(state));
  renameSync(`${file}.tmp`, file);
}

/** Runs the connector: a poll of the store's orders and the receiver of conta-verde's webhooks. */
export function start(config) {
  const woo = new WooCommerce(config.woo);
  const api = new ContaVerde(config.contaVerde);
  const state = loadState(config.stateFile, config.startFrom);
  const handle = createWebhookHandler({ woo, config, seen: seenIds(), log });

  const server = createServer((request, response) => {
    if (request.method === "GET" && request.url === "/healthz") {
      response.writeHead(200).end("ok");
      return;
    }
    if (request.method !== "POST" || request.url !== "/webhooks/conta-verde") {
      response.writeHead(404).end();
      return;
    }
    const chunks = [];
    let size = 0;
    request.on("data", (chunk) => {
      size += chunk.length;
      if (size > 1_000_000) request.destroy();
      else chunks.push(chunk);
    });
    request.on("end", async () => {
      let status = 500;
      try {
        ({ status } = await handle({ headers: request.headers, body: Buffer.concat(chunks).toString("utf8") }));
      } catch (error) {
        log(`a delivery could not be handled: ${error.message}`);
      }
      response.writeHead(status).end();
    });
  });
  server.listen(config.port, () => log(`listening on ${config.port}`));

  let running = false;
  const pass = async () => {
    if (running) return;
    running = true;
    try {
      const { report, cursor, pending, handled } = await syncOrders({ woo, api, config, cursor: state.cursor, pending: state.pending, handled: state.handled, log });
      state.cursor = cursor;
      state.pending = pending;
      state.handled = handled;
      saveState(config.stateFile, state);
      if (report.made || report.cancelled || report.problems) log(`orders: ${JSON.stringify(report)}`);
    } catch (error) {
      log(`a pass failed and will be tried again: ${error.message}`);
    } finally {
      running = false;
    }
  };
  const timer = setInterval(pass, config.pollSeconds * 1000);
  void pass();
  return { server, stop: () => { clearInterval(timer); server.close(); } };
}

if (import.meta.url === `file://${process.argv[1]}`) {
  try {
    start(readConfig());
  } catch (error) {
    console.error(`cannot start: ${error.message}`);
    process.exit(1);
  }
}
