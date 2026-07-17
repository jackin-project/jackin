// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import assert from "node:assert/strict";
import test from "node:test";
import {
  currentRunArtifact,
  latestArtifact,
  splitRepository,
  waitForArtifact,
} from "../src/index.js";

test("splits an exact owner/repository pair", () => {
  assert.deepEqual(splitRepository("jackin-project/jackin"), {
    owner: "jackin-project",
    repo: "jackin",
  });
  assert.throws(() => splitRepository("jackin"), /invalid repository/);
  assert.throws(() => splitRepository("one/two/three"), /invalid repository/);
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

test("selects the named current-run artifact", async () => {
  const octokit = {
    rest: {
      actions: {
        listWorkflowRunArtifacts: async () => ({
          data: {
            artifacts: [
              { id: 1, name: "other", expired: false },
              { id: 2, name: "ci-xtask-GitHub", expired: false },
            ],
          },
        }),
      },
    },
  };
  assert.equal(
    (
      await currentRunArtifact(
        octokit,
        "owner",
        "repo",
        42,
        "ci-xtask-GitHub",
      )
    ).id,
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
