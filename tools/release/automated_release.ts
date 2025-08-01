#!/usr/bin/env -S deno run -A --quiet --lock=tools/deno.lock.json
// Copyright 2018-2025 the Deno authors. MIT license.

// Automated release orchestrator with Slack integration
// This script automates the Deno release process by triggering GitHub Actions
// and monitoring their completion with Slack notifications.

import { $, createOctoKit } from "./deps.ts";

interface SlackConfig {
  webhookUrl: string;
  channelId: string;
  botToken: string;
}

interface ReleaseConfig {
  version: string;
  releaseType: "patch" | "minor" | "major";
  slackThreadTs?: string;
  bellReactionUsers?: string[];
}

class ReleaseOrchestrator {
  private octoKit = createOctoKit();
  private config: ReleaseConfig;
  private slackConfig: SlackConfig;

  constructor(config: ReleaseConfig, slackConfig: SlackConfig) {
    this.config = config;
    this.slackConfig = slackConfig;
  }

  async startRelease(): Promise<void> {
    try {
      await this.sendSlackMessage("🔒 Starting Deno release process...\n\n*React with 🔔 to get pinged on all updates*", "lock");
      await this.postLockMessage();

      // Start monitoring for bell reactions on the initial message
      this.startBellReactionMonitoring();

      // Phase 1: Version bump
      await this.triggerVersionBump();
      await this.waitForVersionBumpPR();

      // Phase 2: Cargo publish
      await this.triggerCargoPublish();
      await this.waitForCargoPublish();

      // Phase 3: Update repositories
      await this.updateDotcom();
      await this.updateDenoDocs();
      await this.updateDenoDocker();

      // Final steps
      await this.verifyRelease();
      await this.postUnlockMessage();

      await this.sendSlackMessage("✅ Release completed successfully!", "unlock");
    } catch (error) {
      await this.sendSlackMessage(`❌ Release failed: ${error.message}`, "error");
      throw error;
    }
  }

  private async sendSlackMessage(message: string, type: "lock" | "unlock" | "update" | "error"): Promise<void> {
    const emoji = {
      lock: "🔒",
      unlock: "🔓",
      update: "📝",
      error: "❌"
    }[type];

    // Add mentions for bell reaction users on updates (but not the initial lock message)
    let finalMessage = `${emoji} ${message}`;
    if (type === "update" && this.config.bellReactionUsers && this.config.bellReactionUsers.length > 0) {
      const mentions = this.config.bellReactionUsers.map(userId => `<@${userId}>`).join(" ");
      finalMessage = `${emoji} ${message}\n\n${mentions}`;
    }

    const payload = {
      channel: this.slackConfig.channelId,
      text: finalMessage,
      thread_ts: this.config.slackThreadTs,
    };

    try {
      const response = await fetch("https://slack.com/api/chat.postMessage", {
        method: "POST",
        headers: {
          "Authorization": `Bearer ${this.slackConfig.botToken}`,
          "Content-Type": "application/json",
        },
        body: JSON.stringify(payload),
      });

      const result = await response.json();
      if (!result.ok) {
        console.warn(`Slack message failed: ${result.error}`);
      } else if (!this.config.slackThreadTs) {
        // Store thread timestamp for follow-up messages
        this.config.slackThreadTs = result.ts;
      }
    } catch (error) {
      console.warn(`Failed to send Slack message: ${error.message}`);
    }
  }

  private async postLockMessage(): Promise<void> {
    const lockMessage = `🔒

@here

Deno v${this.config.version} is now getting released.

\`denoland/deno\` is now locked.

*DO NOT LAND ANY PRs*

Release being automated...`;

    await this.sendSlackMessage(lockMessage, "lock");
  }

  private async triggerVersionBump(): Promise<void> {
    $.logStep("Triggering version bump workflow...");

    await this.octoKit.request("POST /repos/denoland/deno/actions/workflows/version_bump.yml/dispatches", {
      ref: "main",
      inputs: {
        kind: this.config.releaseType,
      },
    });

    const runUrl = await this.getLatestWorkflowRunUrl("deno", "version_bump.yml");
    await this.sendSlackMessage(`🔄 Version bump workflow triggered: ${runUrl}`, "update");
  }

  private async waitForVersionBumpPR(): Promise<void> {
    $.logStep("Waiting for version bump PR to be created and merged...");

    let attempt = 0;
    const maxAttempts = 60; // 30 minutes max wait

    while (attempt < maxAttempts) {
      // Check for open PRs with version bump
      const prs = await this.octoKit.request("GET /repos/denoland/deno/pulls", {
        state: "open",
        head: `denoland:version_${this.config.version}`,
      });

      if (prs.data.length > 0) {
        const pr = prs.data[0];
        await this.sendSlackMessage(`📝 Version bump PR created: ${pr.html_url}`, "update");

        // Wait for PR to be merged
        await this.waitForPRMerge(pr.number);
        await this.sendSlackMessage("✅ Version bump PR merged", "update");
        return;
      }

      await new Promise(resolve => setTimeout(resolve, 30000)); // Wait 30 seconds
      attempt++;
    }

    throw new Error("Version bump PR was not created within expected timeframe");
  }

  private async triggerCargoPublish(): Promise<void> {
    $.logStep("Triggering cargo publish workflow...");

    await this.octoKit.request("POST /repos/denoland/deno/actions/workflows/cargo_publish.yml/dispatches", {
      ref: `v${this.config.version.split('.').slice(0, 2).join('.')}`, // Use minor version branch
    });

    const runUrl = await this.getLatestWorkflowRunUrl("deno", "cargo_publish.yml");
    await this.sendSlackMessage(`🚀 Cargo publish workflow triggered: ${runUrl}`, "update");
  }

  private async waitForCargoPublish(): Promise<void> {
    $.logStep("Waiting for cargo publish to complete...");

    // Monitor the workflow run
    await this.waitForWorkflowCompletion("cargo_publish.yml");
    await this.sendSlackMessage("✅ Cargo publish completed", "update");
  }

  private async updateDotcom(): Promise<void> {
    $.logStep("Updating deno.com...");

    await this.octoKit.request("POST /repos/denoland/dotcom/actions/workflows/update_version.yml/dispatches", {
      ref: "main",
    });

    const runUrl = await this.getLatestWorkflowRunUrl("dotcom", "update_version.yml");
    await this.sendSlackMessage(`🌐 Deno.com update triggered: ${runUrl}`, "update");

    // Wait for PR and auto-merge if possible
    await this.waitAndMergePR("denoland/dotcom", "Update Deno version");
  }

  private async updateDenoDocs(): Promise<void> {
    $.logStep("Updating docs.deno.com...");

    await this.octoKit.request("POST /repos/denoland/deno-docs/actions/workflows/update_versions.yml/dispatches", {
      ref: "main",
    });

    const runUrl = await this.getLatestWorkflowRunUrl("deno-docs", "update_versions.yml");
    await this.sendSlackMessage(`📚 Deno docs update triggered: ${runUrl}`, "update");

    // Wait for PR and auto-merge if possible
    await this.waitAndMergePR("denoland/deno-docs", "Update Deno version");
  }

  private async updateDenoDocker(): Promise<void> {
    $.logStep("Updating deno_docker...");

    await this.octoKit.request("POST /repos/denoland/deno_docker/actions/workflows/version_bump.yml/dispatches", {
      ref: "main",
    });

    const runUrl = await this.getLatestWorkflowRunUrl("deno_docker", "version_bump.yml");
    await this.sendSlackMessage(`🐳 Deno Docker update triggered: ${runUrl}`, "update");

    // Wait for PR
    await this.waitAndMergePR("denoland/deno_docker", "Bump Deno version");

    // Create tag
    await this.octoKit.request("POST /repos/denoland/deno_docker/git/refs", {
      ref: `refs/tags/${this.config.version}`,
      sha: await this.getLatestCommitSha("denoland/deno_docker", "main"),
    });

    await this.sendSlackMessage(`🏷️ Docker tag ${this.config.version} created`, "update");
  }

  private async verifyRelease(): Promise<void> {
    $.logStep("Verifying release assets...");

    // Check GitHub release
    try {
      const release = await this.octoKit.request("GET /repos/denoland/deno/releases/tags/v{tag}", {
        tag: this.config.version,
      });

      if (release.data.assets.length < 24) {
        throw new Error(`Expected 24 assets, found ${release.data.assets.length}`);
      }

      await this.sendSlackMessage(`✅ GitHub release verified (${release.data.assets.length} assets)`, "update");
    } catch (error) {
      throw new Error(`Failed to verify GitHub release: ${error.message}`);
    }

    // Check dl.deno.land (would need GCP API or web scraping)
    await this.sendSlackMessage("⚠️ Please manually verify dl.deno.land assets", "update");
  }

  private async postUnlockMessage(): Promise<void> {
    const unlockMessage = `🔓

@here

\`denoland/deno\` is now unlocked.

*You can land PRs now*

Deno v${this.config.version} has been released.`;

    await this.sendSlackMessage(unlockMessage, "unlock");
  }

  // Helper methods

  private async waitForPRMerge(prNumber: number): Promise<void> {
    let attempt = 0;
    const maxAttempts = 120; // 1 hour max wait

    while (attempt < maxAttempts) {
      const pr = await this.octoKit.request("GET /repos/denoland/deno/pulls/{pull_number}", {
        pull_number: prNumber,
      });

      if (pr.data.merged) {
        return;
      }

      if (pr.data.state === "closed" && !pr.data.merged) {
        throw new Error(`PR #${prNumber} was closed without merging`);
      }

      await new Promise(resolve => setTimeout(resolve, 30000));
      attempt++;
    }

    throw new Error(`PR #${prNumber} was not merged within expected timeframe`);
  }

  private async waitForWorkflowCompletion(workflowFile: string): Promise<void> {
    let attempt = 0;
    const maxAttempts = 240; // 2 hours max wait

    while (attempt < maxAttempts) {
      const runs = await this.octoKit.request("GET /repos/denoland/deno/actions/workflows/{workflow_id}/runs", {
        workflow_id: workflowFile,
        per_page: 1,
      });

      if (runs.data.workflow_runs.length > 0) {
        const run = runs.data.workflow_runs[0];

        if (run.status === "completed") {
          if (run.conclusion === "success") {
            return;
          } else {
            throw new Error(`Workflow ${workflowFile} failed: ${run.conclusion}`);
          }
        }
      }

      await new Promise(resolve => setTimeout(resolve, 30000));
      attempt++;
    }

    throw new Error(`Workflow ${workflowFile} did not complete within expected timeframe`);
  }

  private async waitAndMergePR(repo: string, titlePattern: string): Promise<void> {
    let attempt = 0;
    const maxAttempts = 60;

    while (attempt < maxAttempts) {
      const prs = await this.octoKit.request("GET /repos/{owner}/{repo}/pulls", {
        owner: repo.split('/')[0],
        repo: repo.split('/')[1],
        state: "open",
      });

      const pr = prs.data.find(pr => pr.title.includes(titlePattern));
      if (pr) {
        await this.sendSlackMessage(`📝 ${repo} PR created: ${pr.html_url}`, "update");

        // Wait for checks to pass before merging
        await this.waitForChecks(repo, pr.number);

        // Auto-merge the PR
        await this.octoKit.request("PUT /repos/{owner}/{repo}/pulls/{pull_number}/merge", {
          owner: repo.split('/')[0],
          repo: repo.split('/')[1],
          pull_number: pr.number,
          commit_title: pr.title,
          merge_method: "squash",
        });

        await this.sendSlackMessage(`✅ ${repo} PR merged`, "update");
        return;
      }

      await new Promise(resolve => setTimeout(resolve, 30000));
      attempt++;
    }

    throw new Error(`${repo} PR was not created within expected timeframe`);
  }

  private async waitForChecks(repo: string, prNumber: number): Promise<void> {
    let attempt = 0;
    const maxAttempts = 120;

    while (attempt < maxAttempts) {
      const pr = await this.octoKit.request("GET /repos/{owner}/{repo}/pulls/{pull_number}", {
        owner: repo.split('/')[0],
        repo: repo.split('/')[1],
        pull_number: prNumber,
      });

      if (pr.data.mergeable_state === "clean") {
        return;
      }

      await new Promise(resolve => setTimeout(resolve, 30000));
      attempt++;
    }

    // Proceed anyway after timeout - manual intervention may be needed
    await this.sendSlackMessage(`⚠️ ${repo} PR checks timed out, proceeding anyway`, "update");
  }

  private async getLatestCommitSha(repo: string, branch: string): Promise<string> {
    const ref = await this.octoKit.request("GET /repos/{owner}/{repo}/git/refs/heads/{ref}", {
      owner: repo.split('/')[0],
      repo: repo.split('/')[1],
      ref: branch,
    });

    return ref.data.object.sha;
  }

  private async getLatestWorkflowRunUrl(repo: string, workflowFile: string): Promise<string> {
    // Wait a moment for the workflow run to be created
    await new Promise(resolve => setTimeout(resolve, 2000));

    try {
      const runs = await this.octoKit.request("GET /repos/denoland/{repo}/actions/workflows/{workflow_id}/runs", {
        repo,
        workflow_id: workflowFile,
        per_page: 1,
      });

      if (runs.data.workflow_runs.length > 0) {
        return runs.data.workflow_runs[0].html_url;
      }
    } catch (error) {
      console.warn(`Failed to get workflow run URL: ${error.message}`);
    }

    // Fallback to workflow page if specific run not found
    return `https://github.com/${repo}/actions/workflows/${workflowFile}`;
  }

  private startBellReactionMonitoring(): void {
    // Start a background task to periodically check for bell reactions
    setTimeout(() => this.checkForBellReactions(), 5000); // Check after 5 seconds initially
    
    // Set up periodic checking every 30 seconds
    const intervalId = setInterval(() => this.checkForBellReactions(), 30000);
    
    // Stop monitoring after 2 hours (release should be done by then)
    setTimeout(() => clearInterval(intervalId), 2 * 60 * 60 * 1000);
  }

  private async checkForBellReactions(): Promise<void> {
    if (!this.config.slackThreadTs) return;

    try {
      const response = await fetch(`https://slack.com/api/reactions.get?channel=${this.slackConfig.channelId}&timestamp=${this.config.slackThreadTs}`, {
        headers: {
          "Authorization": `Bearer ${this.slackConfig.botToken}`,
        },
      });

      const result = await response.json();
      if (result.ok && result.message?.reactions) {
        const bellReaction = result.message.reactions.find((reaction: any) => reaction.name === "bell");
        if (bellReaction && bellReaction.users) {
          const currentBellUsers = bellReaction.users;
          
          // Update our list if it changed
          if (!this.config.bellReactionUsers || 
              JSON.stringify(currentBellUsers.sort()) !== JSON.stringify(this.config.bellReactionUsers.sort())) {
            this.config.bellReactionUsers = currentBellUsers;
            console.log(`Updated bell reaction users: ${currentBellUsers.length} users subscribed to notifications`);
          }
        }
      }
    } catch (error) {
      console.warn(`Failed to check bell reactions: ${error.message}`);
    }
  }
}

// Main script execution
async function main(): Promise<void> {
  const releaseType = Deno.args.find(arg => ["--patch", "--minor", "--major"].includes(arg))?.slice(2) as "patch" | "minor" | "major";

  if (!releaseType) {
    console.error("Usage: automated_release.ts [--patch|--minor|--major]");
    Deno.exit(1);
  }

  // Get current version and calculate next version
  const currentVersion = getCurrentVersion();
  const nextVersion = getNextVersion(currentVersion, releaseType);

  // Slack configuration from environment variables
  const slackConfig: SlackConfig = {
    webhookUrl: Deno.env.get("SLACK_WEBHOOK_URL") || "",
    channelId: Deno.env.get("SLACK_CHANNEL_ID") || "#cli",
    botToken: Deno.env.get("SLACK_BOT_TOKEN") || "",
  };

  if (!slackConfig.botToken) {
    console.error("SLACK_BOT_TOKEN environment variable is required");
    Deno.exit(1);
  }

  const releaseConfig: ReleaseConfig = {
    version: nextVersion,
    releaseType,
  };

  const orchestrator = new ReleaseOrchestrator(releaseConfig, slackConfig);
  await orchestrator.startRelease();
}

function getCurrentVersion(): string {
  const cargoTomlText = $.path(import.meta)
    .join("../../cli/Cargo.toml")
    .readTextSync();

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

if (import.meta.main) {
  main().catch((error) => {
    console.error("Release failed:", error);
    Deno.exit(1);
  });
}
