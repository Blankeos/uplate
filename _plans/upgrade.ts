#!/usr/bin/env bun
// Upgrade your solid-launch template against the upstream pinned in .solid-launch.json.
// See the README "Updating" section for the design.

import { $ } from "bun";
import { existsSync } from "node:fs";
import { readFile, writeFile, unlink } from "node:fs/promises";
import { stdin, stdout } from "node:process";
import { createInterface } from "node:readline/promises";

const STATE_FILE = ".solid-launch.json";
const DIFF_FILE = ".solid-launch-upgrade.diff";
const PROMPT_FILE = "UPGRADE_PROMPT.md";
const DEFAULT_UPSTREAM = "https://github.com/Blankeos/solid-launch.git";
const DEFAULT_BRANCH = "main";

const c = {
  dim: (s: string) => `\x1b[2m${s}\x1b[0m`,
  bold: (s: string) => `\x1b[1m${s}\x1b[0m`,
  green: (s: string) => `\x1b[32m${s}\x1b[0m`,
  red: (s: string) => `\x1b[31m${s}\x1b[0m`,
  yellow: (s: string) => `\x1b[33m${s}\x1b[0m`,
  cyan: (s: string) => `\x1b[36m${s}\x1b[0m`,
};

type State = {
  upstream: { repo: string; branch: string; sha: string };
};

async function readState(): Promise<State | null> {
  if (!existsSync(STATE_FILE)) return null;
  return JSON.parse(await readFile(STATE_FILE, "utf8")) as State;
}

async function writeState(state: State): Promise<void> {
  await writeFile(STATE_FILE, `${JSON.stringify(state, null, 2)}\n`);
}

async function workingTreeClean(): Promise<boolean> {
  const out = await $`git status --porcelain`.text();
  return out.trim() === "";
}

async function getRemoteSha(repo: string, branch: string): Promise<string> {
  const out = await $`git ls-remote ${repo} ${branch}`.text();
  const sha = out.split(/\s+/)[0];
  if (!sha || sha.length < 40) {
    throw new Error(`Could not resolve ${repo}#${branch}`);
  }
  return sha;
}

async function fetchObjects(repo: string, shas: string[]): Promise<void> {
  // Bring upstream commits into the local object store without registering a remote.
  // GitHub allows fetch-by-SHA, so no refspec is needed.
  await $`git fetch ${repo} ${{ raw: shas.join(" ") }}`.quiet();
}

async function showChangelog(from: string, to: string): Promise<void> {
  const log = await $`git log --oneline ${from}..${to}`.text();
  const stat = await $`git diff --shortstat ${from}..${to}`.text();
  console.log(c.bold("\nCommits since your pin:\n"));
  console.log(log.trim() || c.dim("  (no commits)"));
  if (stat.trim()) console.log(c.dim(`\n  ${stat.trim()}`));
}

async function writeDiff(from: string, to: string): Promise<string> {
  const diff = await $`git diff ${from}..${to}`.text();
  await writeFile(DIFF_FILE, diff);
  return DIFF_FILE;
}

async function findConflicts(): Promise<string[]> {
  // 3-way merge writes <<<<<<< markers inline; fully-failed hunks write .rej files.
  const seen = new Set<string>();
  const grep = await $`git grep -l '<<<<<<< ' -- :^${STATE_FILE}`.nothrow().text();
  grep.split("\n").filter(Boolean).forEach((f) => seen.add(f));
  const rej = await $`find . -name '*.rej' -not -path './node_modules/*' -not -path './.git/*'`.nothrow().text();
  rej.split("\n").filter(Boolean).forEach((f) => seen.add(f));
  return [...seen];
}

async function applyDiff(diffPath: string): Promise<{ conflicts: string[]; failed: boolean }> {
  const result = await $`git apply --3way --whitespace=nowarn ${diffPath}`.nothrow().quiet();
  const conflicts = await findConflicts();
  if (result.exitCode !== 0 && conflicts.length === 0) {
    console.error(c.red("\ngit apply failed:\n") + result.stderr.toString());
    return { conflicts: [], failed: true };
  }
  return { conflicts, failed: false };
}

function buildAiPrompt(diffPath: string, from: string, to: string): string {
  return [
    `I'm upgrading my solid-launch boilerplate from upstream SHA \`${from}\` to \`${to}\`.`,
    ``,
    `The diff is in \`${diffPath}\` at the repo root.`,
    ``,
    `Please:`,
    `1. Read the diff to understand what changed upstream.`,
    `2. Apply those changes to my repo, preserving any modifications I've made on top of the template.`,
    `3. Where my changes and upstream changes touch the same code, reconcile them — and explain non-obvious decisions.`,
    `4. After applying, update \`.solid-launch.json\`'s \`upstream.sha\` field to \`${to}\`.`,
    `5. Do NOT commit; leave the changes unstaged so I can review.`,
    ``,
    `Flag anything you can't confidently resolve.`,
  ].join("\n");
}

async function copyToClipboard(text: string): Promise<boolean> {
  const cmd =
    process.platform === "darwin"
      ? ["pbcopy"]
      : process.platform === "linux"
        ? ["xclip", "-selection", "clipboard"]
        : ["clip"];
  try {
    const proc = Bun.spawn(cmd, { stdin: "pipe", stdout: "ignore", stderr: "ignore" });
    proc.stdin.write(text);
    await proc.stdin.end();
    await proc.exited;
    return proc.exitCode === 0;
  } catch {
    return false;
  }
}

async function prompt(question: string): Promise<string> {
  const rl = createInterface({ input: stdin, output: stdout });
  try {
    return await rl.question(question);
  } finally {
    rl.close();
  }
}

async function cmdInit(): Promise<void> {
  if (existsSync(STATE_FILE)) {
    console.log(c.yellow(`${STATE_FILE} already exists. Delete it first if you want to re-init.`));
    process.exit(1);
  }
  console.log(c.bold(`Initializing ${STATE_FILE}...`));
  const sha = await getRemoteSha(DEFAULT_UPSTREAM, DEFAULT_BRANCH);
  await writeState({ upstream: { repo: DEFAULT_UPSTREAM, branch: DEFAULT_BRANCH, sha } });
  console.log(c.green(`✓ Pinned to ${sha.slice(0, 7)}`));
}

async function cmdUpgrade(): Promise<void> {
  console.log(c.bold("📦 solid-launch upgrade\n"));

  const state = await readState();
  if (!state) {
    console.log(c.red(`No ${STATE_FILE} found.`));
    console.log(`Run ${c.cyan("bun run upgrade --init")} to pin against current upstream HEAD.`);
    process.exit(1);
  }

  if (!(await workingTreeClean())) {
    console.log(c.red("Working tree is not clean. Commit or stash your changes first."));
    process.exit(1);
  }

  console.log(`Upstream:  ${c.dim(`${state.upstream.repo}#${state.upstream.branch}`)}`);
  console.log(`Pinned:    ${c.dim(state.upstream.sha.slice(0, 7))}`);

  const latest = await getRemoteSha(state.upstream.repo, state.upstream.branch);
  console.log(`Latest:    ${c.dim(latest.slice(0, 7))}\n`);

  if (latest === state.upstream.sha) {
    console.log(c.green("Already up to date."));
    return;
  }

  console.log(c.dim("Fetching upstream objects..."));
  await fetchObjects(state.upstream.repo, [state.upstream.sha, latest]);
  await showChangelog(state.upstream.sha, latest);

  console.log(c.bold("\nHow would you like to proceed?\n"));
  console.log(`  [1] Apply diff via 3-way merge ${c.dim("(recommended)")}`);
  console.log(`      ${c.dim("Clean hunks apply silently; conflicts land as <<<<<<< markers.")}`);
  console.log(`  [2] Write diff + copy AI prompt to clipboard`);
  console.log(`      ${c.dim("Hand the diff to Claude Code / Cursor / etc.")}`);
  console.log(`  [3] Show full diff and exit`);
  console.log(`  [4] Abort\n`);

  const choice = (await prompt("Choice [1-4]: ")).trim();

  switch (choice) {
    case "1": {
      await writeDiff(state.upstream.sha, latest);
      const { conflicts, failed } = await applyDiff(DIFF_FILE);
      if (failed) {
        await unlink(DIFF_FILE).catch(() => {});
        process.exit(1);
      }
      await writeState({ ...state, upstream: { ...state.upstream, sha: latest } });
      await unlink(DIFF_FILE).catch(() => {});
      if (conflicts.length === 0) {
        console.log(c.green("\n✓ Applied cleanly."));
      } else {
        console.log(c.yellow(`\n⚠ Applied with ${conflicts.length} conflict(s):\n`));
        for (const f of conflicts) console.log(`    ${f}`);
        console.log(c.dim("\nResolve markers / .rej files, then review with `git diff`."));
      }
      console.log(c.dim(`Updated ${STATE_FILE} to ${latest.slice(0, 7)} (unstaged).`));
      console.log(c.dim("To abort: `git restore .` and `git clean -f`."));
      break;
    }
    case "2": {
      const diffPath = await writeDiff(state.upstream.sha, latest);
      const text = buildAiPrompt(diffPath, state.upstream.sha.slice(0, 7), latest.slice(0, 7));
      await writeFile(PROMPT_FILE, `${text}\n`);
      const copied = await copyToClipboard(text);
      console.log(c.green(`\n✓ Wrote diff to ${diffPath}`));
      console.log(c.green(`✓ Wrote prompt to ${PROMPT_FILE}`));
      if (copied) console.log(c.green("✓ Copied prompt to clipboard"));
      console.log(c.dim(`\nOpen your AI tool of choice and paste, or reference ${PROMPT_FILE}.`));
      console.log(c.dim(`Delete ${DIFF_FILE} and ${PROMPT_FILE} when done.`));
      break;
    }
    case "3": {
      const diff = await $`git diff ${state.upstream.sha}..${latest}`.text();
      console.log(diff);
      break;
    }
    default:
      console.log("Aborted.");
      return;
  }
}

const arg = process.argv[2];
if (arg === "--init") {
  await cmdInit();
} else if (arg === "--help" || arg === "-h") {
  console.log(`Usage: bun run upgrade [--init]\n\n  --init   Create ${STATE_FILE} pinned to current upstream HEAD\n`);
} else {
  await cmdUpgrade();
}
