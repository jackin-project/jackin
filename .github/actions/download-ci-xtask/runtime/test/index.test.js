// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import assert from "node:assert/strict";
import fs from "node:fs/promises";
import path from "node:path";
import test from "node:test";
import {
  exportPrepared,
  latestArtifact,
  preparedToolsComplete,
  splitRepository,
  validateContracts,
  waitForArtifact,
} from "../src/index.js";

const REQUIRED_TOOLS = [
  "sccache",
  "cargo-nextest",
  "cargo-deny",
  "cargo-shear",
  "cargo-audit",
  "cargo-dylint",
  "cargo-fuzz",
  "cargo-hack",
  "cargo-hakari",
  "cargo-llvm-cov",
  "cargo-mutants",
  "cargo-zigbuild",
  "dylint-link",
  "weaver",
];

test("splits an exact owner/repository pair", () => {
  assert.deepEqual(splitRepository("jackin-project/jackin"), {
    owner: "jackin-project",
    repo: "jackin",
  });
  assert.throws(() => splitRepository("jackin"), /invalid repository/);
  assert.throws(() => splitRepository("one/two/three"), /invalid repository/);
});

test("accepts a tools-only artifact contract", () => {
  assert.doesNotThrow(() =>
    validateContracts({
      includeTools: true,
      includeXtask: false,
      toolsContract: "tools",
      xtaskContract: "",
    }),
  );
  assert.throws(
    () =>
      validateContracts({
        includeTools: false,
        includeXtask: true,
        toolsContract: "",
        xtaskContract: "",
      }),
    /xtask-contract is required with xtask/,
  );
});

test("selects the first unexpired repository artifact", async () => {
  const octokit = {
    rest: {
      actions: {
        listArtifactsForRepo: async (input) => {
          assert.equal(input.name, "ci-xtask-contract");
          return {
            data: {
              artifacts: [
                { id: 1, expired: true },
                { id: 2, expired: false },
              ],
            },
          };
        },
      },
    },
  };
  assert.equal(
    (await latestArtifact(octokit, "owner", "repo", "ci-xtask-contract")).id,
    2,
  );
});

test("returns an artifact as soon as it appears", async () => {
  let calls = 0;
  const artifact = await waitForArtifact(
    async () => (++calls === 2 ? { id: 7 } : undefined),
    "ci-xtask-contract",
    Date.now() + 5_000,
    0,
  );
  assert.equal(artifact.id, 7);
  assert.equal(calls, 2);
});

test("rejects an incomplete prepared Cargo tool bundle", async () => {
  const root = await fs.mkdtemp(path.join(process.cwd(), ".test-prepared-tools-"));
  try {
    await fs.writeFile(path.join(root, "cargo-fuzz"), "tool");
    assert.equal(await preparedToolsComplete(root), false);
    await Promise.all(
      REQUIRED_TOOLS.map((tool) => fs.writeFile(path.join(root, tool), "tool")),
    );
    assert.equal(await preparedToolsComplete(root), true);
  } finally {
    await fs.rm(root, { recursive: true, force: true });
  }
});

test("exports separate cache-backed xtask and tool directories", async () => {
  const root = await fs.mkdtemp(path.join(process.cwd(), ".test-prepared-ci-"));
  const tools = path.join(root, "tools");
  const xtask = path.join(root, "xtask");
  await fs.mkdir(tools);
  await fs.mkdir(xtask);
  await fs.writeFile(path.join(tools, "cargo-fuzz"), "tool");
  await fs.writeFile(path.join(xtask, "jackin-xtask"), "xtask");
  await fs.writeFile(path.join(xtask, "workspace-metadata.json"), "{}");

  try {
    await exportPrepared(tools, xtask);
    assert.equal(process.env.CI_TOOLS_PATH, tools);
    assert.equal(process.env.CI_XTASK, path.join(xtask, "jackin-xtask"));
    assert.equal(process.env.CI_CARGO_FUZZ, path.join(tools, "cargo-fuzz"));
    assert.equal(
      process.env.CI_METADATA,
      path.join(xtask, "workspace-metadata.json"),
    );
  } finally {
    await fs.rm(root, { recursive: true, force: true });
  }
});
