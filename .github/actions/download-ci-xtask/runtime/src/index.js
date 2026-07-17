// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import * as core from "@actions/core";
import * as github from "@actions/github";
import AdmZip from "adm-zip";
import fs from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const WAIT_MILLISECONDS = 2_000;
const DEADLINE_MILLISECONDS = 55_000;

function splitRepository(repository) {
  const [owner, repo, extra] = repository.split("/");
  if (!owner || !repo || extra) {
    throw new Error(`invalid repository: ${repository}`);
  }
  return { owner, repo };
}

async function latestArtifact(octokit, owner, repo, name) {
  const response = await octokit.rest.actions.listArtifactsForRepo({
    owner,
    repo,
    name,
    per_page: 10,
  });
  return response.data.artifacts.find((artifact) => !artifact.expired);
}

async function currentRunArtifact(octokit, owner, repo, runId, name) {
  const response = await octokit.rest.actions.listWorkflowRunArtifacts({
    owner,
    repo,
    run_id: runId,
    per_page: 100,
  });
  return response.data.artifacts.find(
    (artifact) => artifact.name === name && !artifact.expired,
  );
}

async function waitForArtifact(
  find,
  name,
  deadline,
  waitMilliseconds = WAIT_MILLISECONDS,
) {
  let announced = false;
  while (Date.now() < deadline) {
    const artifact = await find();
    if (artifact) return artifact;
    if (!announced) {
      core.notice(`waiting up to 55 seconds for prepared CI artifact: ${name}`);
      announced = true;
    }
    await new Promise((resolve) => setTimeout(resolve, waitMilliseconds));
  }
  return find();
}

async function downloadArtifact(octokit, owner, repo, artifact, destination) {
  const response = await octokit.rest.actions.downloadArtifact({
    owner,
    repo,
    artifact_id: artifact.id,
    archive_format: "zip",
  });
  await fs.mkdir(destination, { recursive: true });
  new AdmZip(Buffer.from(response.data)).extractAllTo(destination, true);
}

async function run() {
  const token = process.env.JACKIN_TOKEN;
  const { owner, repo } = splitRepository(process.env.JACKIN_REPOSITORY);
  const runId = Number.parseInt(process.env.JACKIN_RUN_ID, 10);
  const lane = process.env.JACKIN_LANE;
  const destination = path.join(process.env.JACKIN_WORKSPACE, ".ci-tools", lane);
  const os = process.env.JACKIN_RUNNER_OS;
  const arch = process.env.JACKIN_RUNNER_ARCH;
  const toolsContract = process.env.JACKIN_TOOLS_CONTRACT;
  const xtaskContract = process.env.JACKIN_XTASK_CONTRACT;
  const fallbackXtaskContract = process.env.JACKIN_FALLBACK_XTASK_CONTRACT;
  if (!token) throw new Error("token is required");
  if (!lane) throw new Error("lane is required");
  if (!xtaskContract) throw new Error("xtask-contract is required");
  const laneArtifact = `ci-xtask-${lane}`;
  const toolsArtifact = `ci-tools-${os}-${arch}-${toolsContract}`;
  const xtaskArtifact = `ci-xtask-${os}-${arch}-${xtaskContract}`;
  const includeTools = process.env.JACKIN_INCLUDE_TOOLS === "true";
  const allowMiss = process.env.JACKIN_ALLOW_MISS === "true";
  const octokit = github.getOctokit(token);
  const deadline = Date.now() + (allowMiss ? 0 : DEADLINE_MILLISECONDS);
  let toolsHit = false;
  let xtaskHit = false;
  const result = () => {
    const tools = toolsHit ? "true" : "false";
    const xtask = xtaskHit ? "true" : "false";
    if (!xtaskHit) {
      core.exportVariable(
        "CI_XTASK",
        path.join(workspace, "target", "debug", "jackin-xtask"),
      );
    }
    core.exportVariable("CI_TOOLS_PATH", destination);
    core.exportVariable("CI_TOOLS_HIT", tools);
    core.exportVariable("CI_XTASK_HIT", xtask);
    return { tools_hit: tools, xtask_hit: xtask };
  };

  core.setOutput("tools-hit", "false");
  core.setOutput("xtask-hit", "false");
  await fs.mkdir(destination, { recursive: true });
  if (includeTools) {
    if (!toolsContract) throw new Error("tools-contract is required with tools");
    const lane = await currentRunArtifact(
      octokit,
      owner,
      repo,
      runId,
      laneArtifact,
    );
    if (lane) {
      await downloadArtifact(octokit, owner, repo, lane, destination);
      toolsHit = true;
      xtaskHit = true;
      core.setOutput("tools-hit", "true");
      core.setOutput("xtask-hit", "true");
      await exportTools(destination);
      return result();
    }

    const tools = await waitForArtifact(
      () => latestArtifact(octokit, owner, repo, toolsArtifact),
      toolsArtifact,
      deadline,
    );
    if (!tools) {
      if (!allowMiss) {
        throw new Error(`prepared CI artifact not found: ${toolsArtifact}`);
      }
    } else {
      await downloadArtifact(octokit, owner, repo, tools, destination);
      toolsHit = true;
      core.setOutput("tools-hit", "true");
    }
  }

  const candidates = [xtaskArtifact];
  if (fallbackXtaskContract && fallbackXtaskContract !== xtaskContract) {
    candidates.push(`ci-xtask-${os}-${arch}-${fallbackXtaskContract}`);
  }
  let xtask;
  for (const name of candidates) {
    xtask = await waitForArtifact(
      () => latestArtifact(octokit, owner, repo, name),
      name,
      deadline,
    );
    if (xtask) break;
  }
  if (!xtask) {
    if (allowMiss) return result();
    throw new Error("prepared CI xtask artifact not found");
  }
  await downloadArtifact(octokit, owner, repo, xtask, destination);
  xtaskHit = true;
  core.setOutput("xtask-hit", "true");
  await exportTools(destination);
  return result();
}

async function exportTools(destination) {
  const xtask = path.join(destination, "jackin-xtask");
  const cargoFuzz = path.join(destination, "cargo-fuzz");
  try {
    await fs.access(cargoFuzz);
    const entries = await fs.readdir(destination, { withFileTypes: true });
    await Promise.all(
      entries
        .filter((entry) => entry.isFile())
        .map((entry) => fs.chmod(path.join(destination, entry.name), 0o755)),
    );
    core.exportVariable("CI_CARGO_FUZZ", cargoFuzz);
  } catch (error) {
    if (error.code !== "ENOENT") throw error;
    await fs.chmod(xtask, 0o755);
  }
  core.exportVariable("CI_XTASK", xtask);

  const metadata = path.join(destination, "workspace-metadata.json");
  try {
    await fs.access(metadata);
    core.exportVariable("CI_METADATA", metadata);
  } catch (error) {
    if (error.code !== "ENOENT") throw error;
  }
  core.addPath(destination);
  core.exportVariable("CI_TOOLS_PATH", destination);
}

async function main() {
  try {
    return await run();
  } catch (error) {
    core.setFailed(error.message);
  }
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  main();
}

export {
  currentRunArtifact,
  latestArtifact,
  splitRepository,
  waitForArtifact,
  main,
};
