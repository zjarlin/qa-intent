import { test } from "node:test";
import assert from "node:assert/strict";
import { releaseVisible, waitForRegistry } from "./registry.mjs";

test("native dependencies must all become public before the main release", async () => {
  const packages = [{ name: "a", version: "1.0.0" }, { name: "b", version: "1.0.0" }];
  let polls = 0;
  let waits = 0;
  await waitForRegistry(packages, {
    attempts: 2,
    pause: async () => { waits++; },
    request: async url => {
      const name = url.includes("/a/") ? "a" : "b";
      polls++;
      if (polls <= 2 && name === "b") return { status: 404 };
      return { status: 200, ok: true, json: async () => ({ name, version: "1.0.0", dist: { integrity: "sha512-test" } }) };
    }
  });
  assert.equal(waits, 1);
  assert.equal(polls, 4);
});

test("authorization and malformed releases are not treated as absent packages", async () => {
  const pkg = { name: "a", version: "1.0.0" };
  await assert.rejects(releaseVisible(pkg, async () => ({ status: 403 })), /403/);
  await assert.rejects(releaseVisible(pkg, async () => ({ status: 200, ok: true, json: async () => ({}) })), /元数据/);
  await assert.rejects(waitForRegistry([pkg], { attempts: 1, request: async () => ({ status: 404 }) }), /仍在处理/);
});
