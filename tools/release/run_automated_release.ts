#!/usr/bin/env -S deno run -A --quiet --lock=tools/deno.lock.json
// Copyright 2018-2025 the Deno authors. MIT license.

// Simple runner script for the automated release process
// This provides a user-friendly interface to start the automated release

import { $ } from "./deps.ts";

function printUsage(): void {
  console.log(`
Deno Automated Release Runner

Usage: deno run -A tools/release/run_automated_release.ts [options]

Options:
  --patch     Create a patch release (x.y.Z)
  --minor     Create a minor release (x.Y.0)  
  --major     Create a major release (X.0.0)
  --dry-run   Show what would be done without executing
  --help      Show this help message

Environment Variables Required:
  SLACK_BOT_TOKEN    - Slack bot token for notifications
  SLACK_CHANNEL_ID   - Slack channel ID (e.g., "#cli")
  GITHUB_TOKEN       - GitHub token with workflow permissions

Example:
  export SLACK_BOT_TOKEN="xoxb-..."
  export SLACK_CHANNEL_ID="#cli"  
  export GITHUB_TOKEN="ghp_..."
  
  deno run -A tools/release/run_automated_release.ts --patch
`);
}

function validateEnvironment(): boolean {
  const required = ["SLACK_BOT_TOKEN", "GITHUB_TOKEN"];
  const missing = required.filter(env => !Deno.env.get(env));
  
  if (missing.length > 0) {
    console.error(`❌ Missing required environment variables: ${missing.join(", ")}`);
    return false;
  }
  
  return true;
}

function getCurrentVersion(): string {
  const cargoTomlPath = $.path(import.meta).join("../../cli/Cargo.toml");
  const cargoTomlText = cargoTomlPath.readTextSync();
  
  const result = cargoTomlText.match(/^version\s*=\s*"([^"]+)"$/m);
  if (!result || result.length !== 2) {
    throw new Error("Could not find version in Cargo.toml");
  }
  
  return result[1];
}

function getNextVersion(currentVersion: string, releaseType: "patch" | "minor" | "major"): string {
  const [major, minor, patch] = currentVersion.split('.').map(Number);
  
  switch (releaseType) {
    case "major":
      return `${major + 1}.0.0`;
    case "minor":
      return `${major}.${minor + 1}.0`;
    case "patch":
      return `${major}.${minor}.${patch + 1}`;
  }
}

async function main(): Promise<void> {
  const args = Deno.args;
  
  if (args.includes("--help")) {
    printUsage();
    return;
  }

  const releaseType = args.find(arg => ["--patch", "--minor", "--major"].includes(arg))?.slice(2) as "patch" | "minor" | "major" | undefined;
  const isDryRun = args.includes("--dry-run");

  if (!releaseType) {
    console.error("❌ Must specify release type: --patch, --minor, or --major");
    printUsage();
    Deno.exit(1);  
  }

  if (!validateEnvironment()) {
    Deno.exit(1);
  }

  const currentVersion = getCurrentVersion();
  const nextVersion = getNextVersion(currentVersion, releaseType);

  console.log(`
🚀 Deno Automated Release
========================

Current version: ${currentVersion}
Next version:    ${nextVersion} (${releaseType})
Dry run:         ${isDryRun ? "YES" : "NO"}

This will:
1. Post lock message to Slack  
2. Trigger version bump workflow
3. Wait for PR creation and merge
4. Trigger cargo publish workflow
5. Update deno.com, docs.deno.com, and deno_docker
6. Verify release assets
7. Post unlock message to Slack

Each step will be reported in the Slack thread.
`);

  if (isDryRun) {
    console.log("🔍 Dry run mode - no actions will be taken");
    return;
  }

  const confirmed = confirm("Continue with automated release? (y/N)");
  if (!confirmed) {
    console.log("❌ Release cancelled");
    return;
  }

  console.log("🚀 Starting automated release...");
  
  // Run the actual automation script
  const automatedReleaseScript = $.path(import.meta).join("automated_release.ts");
  
  const result = await $`deno run -A ${automatedReleaseScript} --${releaseType}`.noThrow();
  
  if (result.code !== 0) {
    console.error("❌ Automated release failed!");
    console.error(result.stderr);
    Deno.exit(1);
  }
  
  console.log("✅ Automated release completed successfully!");
}

if (import.meta.main) {
  main().catch((error) => {
    console.error("❌ Runner failed:", error);
    Deno.exit(1);
  });
}