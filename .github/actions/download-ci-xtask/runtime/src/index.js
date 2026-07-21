// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import * as core from "@actions/core";
import * as github from "@actions/github";
import AdmZip from "adm-zip";
import fs from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const WAIT_MILLISECONDS = 2_000;
const DEADLINE_MILLISECONDS = 120_000;
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

async function preparedToolsComplete(destination) {
  try {
    const entries = new Set(await fs.readdir(destination));
    return REQUIRED_TOOLS.every((tool) => entries.has(tool));
  } catch (error) {
    if (error.code === "ENOENT") return false;
    throw error;
  }
}

function splitRepository(repository) {
  const [owner, repo, extra] = repository.split("/");
  if (!owner || !repo || extra) {
    throw new Error(`invalid repository: ${repository}`);
  }
  return { owner, repo };
}

function validateContracts({
  includeTools,
  includeXtask,
  toolsContract,
  xtaskContract,
}) {
  if (includeTools && !toolsContract) {
    throw new Error("tools-contract is required with tools");
  }
  if (includeXtask && !xtaskContract) {
    throw new Error("xtask-contract is required with xtask");
  }
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
      const remainingSeconds = Math.max(
        0,
        Math.ceil((deadline - Date.now()) / 1_000),
      );
      core.notice(
        `waiting up to ${remainingSeconds} seconds for prepared CI artifact: ${name}`,
      );
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
  const workspace = process.env.JACKIN_WORKSPACE;
  const toolsDestination = path.join(workspace, ".ci-prebuilt-tools");
  const xtaskDestination = path.join(workspace, ".ci-prebuilt-xtask");
  const os = process.env.JACKIN_RUNNER_OS;
  const arch = process.env.JACKIN_RUNNER_ARCH;
  const toolsContract = process.env.JACKIN_TOOLS_CONTRACT;
  const xtaskContract = process.env.JACKIN_XTASK_CONTRACT;
  const fallbackXtaskContract = process.env.JACKIN_FALLBACK_XTASK_CONTRACT;
  if (!token) throw new Error("token is required");
  const toolsArtifact = `ci-tools-${os}-${arch}-${toolsContract}`;
  const xtaskArtifact = `ci-xtask-${os}-${arch}-${xtaskContract}`;
  const includeTools = process.env.JACKIN_INCLUDE_TOOLS === "true";
  const includeXtask = process.env.JACKIN_INCLUDE_XTASK !== "false";
  const allowMiss = process.env.JACKIN_ALLOW_MISS === "true";
  validateContracts({
    includeTools,
    includeXtask,
    toolsContract,
    xtaskContract,
  });
  const octokit = github.getOctokit(token);
  const deadline = Date.now() + (allowMiss ? 0 : DEADLINE_MILLISECONDS);
  let toolsHit =
    includeTools && process.env.JACKIN_TOOLS_CACHE_HIT === "true";
  let xtaskHit =
    includeXtask && process.env.JACKIN_XTASK_CACHE_HIT === "true";
  const result = () => {
    const tools = toolsHit ? "true" : "false";
    const xtask = xtaskHit ? "true" : "false";
    if (includeXtask && !xtaskHit) {
      core.exportVariable(
        "CI_XTASK",
        path.join(workspace, "target", "debug", "jackin-xtask"),
      );
    }
    core.exportVariable("CI_TOOLS_PATH", toolsDestination);
    core.exportVariable("CI_TOOLS_HIT", tools);
    if (includeXtask) core.exportVariable("CI_XTASK_HIT", xtask);
    return { tools_hit: tools, xtask_hit: xtask };
  };

  if (toolsHit && !(await preparedToolsComplete(toolsDestination))) {
    core.warning("ignoring incomplete prepared Cargo tools cache");
    toolsHit = false;
  }
  core.setOutput("tools-hit", "false");
  core.setOutput("xtask-hit", "false");
  if (toolsHit) core.setOutput("tools-hit", "true");
  if (xtaskHit) core.setOutput("xtask-hit", "true");
  if (includeTools && !toolsHit) {
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
      await downloadArtifact(
        octokit,
        owner,
        repo,
        tools,
        toolsDestination,
      );
      toolsHit = await preparedToolsComplete(toolsDestination);
      if (!toolsHit && !allowMiss) {
        throw new Error(`prepared CI tools artifact is incomplete: ${toolsArtifact}`);
      }
      core.setOutput("tools-hit", toolsHit ? "true" : "false");
    }
  }

  if (!includeXtask) {
    await exportPrepared(toolsDestination, xtaskDestination, toolsHit, false);
    return result();
  }

  if (!xtaskHit) {
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
    await downloadArtifact(
      octokit,
      owner,
      repo,
      xtask,
      xtaskDestination,
    );
    xtaskHit = true;
    core.setOutput("xtask-hit", "true");
  }
  await exportPrepared(
    toolsDestination,
    xtaskDestination,
    toolsHit,
    xtaskHit,
  );
  return result();
}

async function exportPrepared(
  toolsDestination,
  xtaskDestination,
  includeTools = true,
  includeXtask = true,
) {
  const xtask = path.join(xtaskDestination, "jackin-xtask");
  if (includeTools) {
    const entries = await fs.readdir(toolsDestination, { withFileTypes: true });
    await Promise.all(
      entries
        .filter((entry) => entry.isFile())
        .map((entry) =>
          fs.chmod(path.join(toolsDestination, entry.name), 0o755),
        ),
    );
    const cargoFuzz = path.join(toolsDestination, "cargo-fuzz");
    try {
      await fs.access(cargoFuzz);
      core.exportVariable("CI_CARGO_FUZZ", cargoFuzz);
    } catch (error) {
      if (error.code !== "ENOENT") throw error;
    }
    core.addPath(toolsDestination);
  }
  if (includeXtask) {
    await fs.chmod(xtask, 0o755);
    core.exportVariable("CI_XTASK", xtask);
  }

  const metadata = path.join(xtaskDestination, "workspace-metadata.json");
  try {
    await fs.access(metadata);
    core.exportVariable("CI_METADATA", metadata);
  } catch (error) {
    if (error.code !== "ENOENT") throw error;
  }
  core.exportVariable("CI_TOOLS_PATH", toolsDestination);
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
  exportPrepared,
  latestArtifact,
  preparedToolsComplete,
  splitRepository,
  validateContracts,
  waitForArtifact,
  main,
};
