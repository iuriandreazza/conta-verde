import assert from "node:assert/strict";
import { readFileSync, readdirSync } from "node:fs";
import { dirname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { test } from "node:test";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");

const files = (dir) => readdirSync(dir, { withFileTypes: true, recursive: true }).filter((entry) => entry.isFile() && entry.name.endsWith(".js") && !join(entry.parentPath, entry.name).includes("node_modules")).map((entry) => join(entry.parentPath, entry.name));

test("the package imports nothing from outside its own directory, and only the standard library from outside", () => {
  const imports = [];
  for (const file of files(root)) {
    for (const match of readFileSync(file, "utf8").matchAll(/(?:import|export)[^"'\n]*?from\s+["']([^"']+)["']|import\(\s*["']([^"']+)["']\s*\)/g)) imports.push({ file, spec: match[1] ?? match[2] });
  }
  assert.ok(imports.length > 10);
  for (const { file, spec } of imports) {
    if (spec.startsWith(".")) assert.ok(!relative(root, resolve(dirname(file), spec)).startsWith(".."), `${file} reaches outside the package: ${spec}`);
    else assert.ok(spec.startsWith("node:"), `${file} imports a dependency: ${spec}`);
  }
});

test("it has no dependencies at all, so it has no lockfile to keep", () => {
  const manifest = JSON.parse(readFileSync(join(root, "package.json"), "utf8"));
  assert.equal(manifest.dependencies, undefined);
  assert.equal(manifest.devDependencies, undefined);
});
