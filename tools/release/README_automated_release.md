# Automated Deno Release Process

This automation system streamlines the Deno release process by orchestrating GitHub Actions workflows and providing real-time Slack notifications.

## Features

- **One-command release**: Start the entire release process with a single command
- **Slack integration**: Real-time updates posted to your Slack channel in a thread
- **Automated workflows**: Triggers and monitors GitHub Actions across multiple repositories
- **Error handling**: Robust error detection and reporting
- **PR monitoring**: Waits for PRs to be created and auto-merges when possible
- **Asset verification**: Checks that release assets are properly generated

## Setup

### 1. Environment Variables

Set the following environment variables:

```bash
# Required
export SLACK_BOT_TOKEN="xoxb-your-slack-bot-token"
export GITHUB_TOKEN="ghp_your-github-token"

# Optional (defaults to "#cli")
export SLACK_CHANNEL_ID="#cli"
```

### 2. Slack Bot Setup

1. Create a Slack app in your workspace
2. Add the `chat:write` and `chat:write.public` bot scopes
3. Install the app to your workspace
4. Copy the Bot User OAuth Token

### 3. GitHub Token

Create a GitHub personal access token with the following permissions:
- `repo` (full repository access)
- `workflow` (trigger workflows)
- `actions:read` (read workflow status)

## Usage

### Basic Usage

```bash
# Patch release (2.4.3 -> 2.4.4)
deno run -A tools/release/run_automated_release.ts --patch

# Minor release (2.4.3 -> 2.5.0)
deno run -A tools/release/run_automated_release.ts --minor

# Major release (2.4.3 -> 3.0.0)
deno run -A tools/release/run_automated_release.ts --major
```

### Dry Run

Test the configuration without making changes:

```bash
deno run -A tools/release/run_automated_release.ts --patch --dry-run
```

## Process Overview

The automated release follows these steps:

### 1. Pre-flight
- Posts lock message to Slack
- Creates a Slack thread for all updates
- Freezes the repository

### 2. Version Bump (Phase 1)
- Triggers `version_bump.yml` workflow
- Monitors for PR creation
- Waits for PR to be merged

### 3. Cargo Publish (Phase 2)
- Triggers `cargo_publish.yml` workflow
- Monitors workflow completion
- Verifies release tag creation

### 4. Repository Updates
- **deno.com**: Triggers version update workflow and merges PR
- **docs.deno.com**: Triggers version update workflow and merges PR
- **deno_docker**: Triggers version bump, merges PR, and creates tag

### 5. Verification
- Checks GitHub release assets (expects 24 assets)
- Reports verification status

### 6. Completion
- Posts unlock message to Slack
- Unfreezes the repository

## Slack Notifications

All updates are posted to a Slack thread with appropriate emojis:

- 🔒 **Lock/Start**: Repository locked, release started
- 🔄 **Workflows**: GitHub Actions triggered (with direct links to workflow runs)
- 📝 **PRs**: Pull requests created/merged
- ✅ **Success**: Steps completed successfully
- ⚠️ **Warnings**: Manual verification needed
- ❌ **Errors**: Failures requiring attention
- 🔓 **Unlock/Complete**: Repository unlocked, release finished

### Bell Reaction Notifications

Users can react with 🔔 (bell emoji) to the initial release message to get mentioned in all subsequent updates. The system:

- Monitors for bell reactions on the initial message
- Updates the subscriber list every 30 seconds
- Mentions all bell reaction users in update messages (not lock/unlock messages)
- Stops monitoring after 2 hours

This allows team members to opt-in to notifications without being spammed if they're not interested.

## Error Handling

The system includes comprehensive error handling:

- **Timeouts**: Maximum wait times for all operations
- **Failure recovery**: Clear error messages with context
- **Slack notifications**: Errors reported to Slack thread
- **Graceful degradation**: Continues where possible, reports issues

## Manual Intervention

Some steps may require manual intervention:

1. **PR Reviews**: Complex PRs may need manual review before merge
2. **Asset Verification**: dl.deno.land assets need manual verification
3. **Workflow Failures**: Failed workflows may need manual restart
4. **MDN Updates**: Manual updates for new APIs (rare)

## Monitoring

Monitor the release through:

1. **Slack thread**: Real-time updates and status
2. **GitHub Actions**: Individual workflow progress
3. **Console output**: Detailed logging during execution

## Troubleshooting

### Common Issues

**Slack token invalid**:
```
❌ Missing required environment variables: SLACK_BOT_TOKEN
```
- Verify your Slack bot token is correct
- Ensure the bot has necessary permissions

**GitHub API rate limits**:
```
❌ API rate limit exceeded
```
- Wait for rate limit reset
- Use a GitHub token with higher limits

**Workflow timeouts**:
```
❌ Workflow version_bump.yml did not complete within expected timeframe
```
- Check GitHub Actions page for workflow status
- May need manual intervention

**PR merge conflicts**:
```
⚠️ denoland/dotcom PR checks timed out, proceeding anyway
```
- Check PR status manually
- Resolve conflicts if needed

### Recovery

If the automated release fails partway through:

1. Check the Slack thread for the last successful step
2. Review GitHub Actions for any failed workflows
3. Continue manually from the failure point using the original release checklist
4. Consider restarting the automation from a clean state

## Comparison to Manual Process

| Aspect | Manual Process | Automated Process |
|--------|---------------|-------------------|
| Time | 2-4 hours | 30-60 minutes |
| Monitoring | Constant attention | Slack notifications |
| Errors | Easy to miss | Automatically detected |
| Documentation | Manual checklist | Automated logging |
| Reproducibility | Varies by operator | Consistent |

## Future Improvements

Potential enhancements:

- **Web dashboard**: Visual progress tracking
- **Rollback automation**: Automated failure recovery
- **Multi-release support**: Coordinate multiple release streams
- **Integration testing**: Automated post-release verification
- **Metrics collection**: Release time and success rate tracking
