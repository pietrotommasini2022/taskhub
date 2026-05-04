# TaskHub

> Personal automation runtime. Single binary. Workflow-as-code. Yours, on your machine.

No cloud account. No Docker. No drag-and-drop editor. Just a binary, a YAML file, and your machine.

---

## Install

### Pre-built binary (recommended)

**macOS / Linux**
```bash
curl -fsSL https://github.com/pietroairoldi/taskhub/releases/latest/download/install.sh | sh
```

**Windows** (PowerShell)
```powershell
irm https://github.com/pietroairoldi/taskhub/releases/latest/download/install.ps1 | iex
```

Or download the archive for your platform directly from the [Releases page](https://github.com/pietroairoldi/taskhub/releases):

| Platform | File |
|---|---|
| Windows x86-64 | `taskhub-windows-x86_64.zip` |
| macOS x86-64 | `taskhub-macos-x86_64.tar.gz` |
| macOS Apple Silicon | `taskhub-macos-aarch64.tar.gz` |
| Linux x86-64 | `taskhub-linux-x86_64.tar.gz` |

Extract and place the `taskhub` binary somewhere in your `PATH`.

### From source

Requires [Rust](https://rustup.rs/) stable.

```bash
git clone https://github.com/pietroairoldi/taskhub
cd taskhub
cargo install --path crates/taskhub-cli
```

---

## Quick start

```bash
# 1. Initialize (~/.taskhub directory + SQLite database)
taskhub init

# 2. Store a secret (encrypted, never stored in plain text)
taskhub secret set GITHUB_TOKEN

# 3. Validate a workflow file
taskhub validate examples/github-notify.yaml

# 4. Run a workflow once
taskhub run examples/github-notify.yaml

# 5. Start the daemon (schedule + webhook + filesystem triggers)
taskhub watch
```

---

## Your first workflow

Create `my-workflow.yaml`:

```yaml
name: rss-digest
description: Post top 5 Hacker News stories to Slack every morning

on:
  trigger: schedule
  cron: "0 9 * * 1-5"   # 9am Monday–Friday

steps:
  - id: feed
    uses: rss/fetch
    with:
      url: https://news.ycombinator.com/rss
      limit: 5

  - id: post
    uses: slack/send
    for_each: ${{ steps.feed.output }}
    with:
      token: ${{ secrets.SLACK_TOKEN }}
      channel: "#digest"
      text: "${{ item.title }} — ${{ item.link }}"
```

```bash
taskhub secret set SLACK_TOKEN
taskhub run my-workflow.yaml
taskhub watch   # keep it running with the cron trigger active
```

---

## CLI reference

```
taskhub init                     Initialize ~/.taskhub (run once)
taskhub run <file>               Execute a workflow immediately
taskhub test <file>              Dry-run — no network calls, no side effects
taskhub validate <file>          Validate YAML syntax and step references
taskhub watch [--tray]           Start daemon (all triggers active)
taskhub list                     List recent workflow runs
taskhub logs <name>              Show step-level logs for a workflow

taskhub secret set <key>         Store an encrypted secret (prompted)
taskhub secret list              List all secret keys (values never shown)
taskhub secret remove <key>      Delete a secret

taskhub plugin install <path>    Install a WASM plugin from a directory
taskhub plugin list              List installed plugins and their actions
```

---

## Workflow format

```yaml
name: my-workflow          # unique identifier
description: What it does  # optional

on:
  trigger: schedule        # manual | schedule | webhook | filesystem
  every: 5m                # for schedule: interval (30s, 5m, 1h, 2h30m, 1d)
  cron: "0 8 * * *"        # for schedule: cron expression (alternative to every)
  path: /my-hook           # for webhook: URL path
  watch_path: ~/Downloads  # for filesystem: directory to watch
  patterns: ["*.pdf"]      # for filesystem: file name glob filter
  events: [create, modify] # for filesystem: create | modify | delete | access
  recursive: false         # for filesystem: watch subdirectories
  debounce: 2s             # for filesystem: quiet period before firing

steps:
  - id: step_name          # used in ${{ steps.step_name.output }}
    uses: plugin/action    # see plugin table below
    with:
      param: value
    if: ${{ steps.prev.output.count > 0 }}   # skip if false
    for_each: ${{ steps.prev.output }}        # repeat per item
    on_error: continue     # fail (default) | continue | retry
    retry:
      max_attempts: 3
      backoff: exponential
      delay: 1s
    timeout: 30s
```

### Template expressions

| Expression | Value |
|---|---|
| `${{ secrets.KEY }}` | Encrypted secret |
| `${{ steps.<id>.output }}` | Full output of a step |
| `${{ steps.<id>.output.field }}` | Nested field in step output |
| `${{ trigger.body }}` | Webhook request body (parsed JSON) |
| `${{ trigger.body.field }}` | Field from webhook body |
| `${{ item }}` | Current element in a `for_each` loop |
| `${{ item.field }}` | Field of the current loop element |

---

## Plugins

### Built-in (always available)

| Action | Description |
|---|---|
| `core/http` | HTTP request (GET, POST, PUT, DELETE, …) |
| `core/shell` | Run a shell command |
| `core/transform` `jq` | JSON query — `.field`, `.nested.path`, `.[0]` |
| `core/transform` `regex.match` | Regex match with capture groups |
| `core/transform` `template` | `{{var}}` string rendering |
| `core/transform` `merge` / `pick` / `omit` | Object manipulation |
| `core/transform` `json.parse` / `json.stringify` | JSON conversion |
| `email_smtp/send` | Send email via SMTP (Gmail, Fastmail, …) |
| `email_imap/inbox.list` | List emails (IMAP) |
| `email_imap/inbox.search` | Search emails |
| `email_imap/mark_read` | Mark email as read |
| `sqlite/query` | SQLite SELECT — returns rows as JSON array |
| `sqlite/execute` | SQLite INSERT / UPDATE / DELETE |
| `sqlite/transaction` | Atomic multi-statement transaction |
| `postgres/query` | PostgreSQL SELECT |
| `postgres/execute` | PostgreSQL INSERT / UPDATE / DELETE |

### WASM plugins (included in the release archive)

Copy the `plugins/` folder from the release archive to `~/.taskhub/plugins/`, or install individually:

```bash
taskhub plugin install path/to/plugin-directory
```

| Plugin | Actions | Auth secret |
|---|---|---|
| `github` | `notifications.list`, `notifications.mark_read`, `issues.list`, `issues.create`, `issues.comment`, `pulls.list`, `pulls.review`, `repo.dispatch` | `GITHUB_TOKEN` |
| `gitlab` | `todos.list`, `todos.mark_done`, `merge_requests.list`, `issues.list` | `GITLAB_TOKEN` |
| `slack` | `send`, `send_thread`, `react`, `upload_file` | `SLACK_TOKEN` |
| `discord` | `webhook.send` | webhook URL in `with.url` |
| `rss` | `fetch`, `check_new` | none |
| `linear` | `issues.list`, `issues.create`, `issues.update`, `issues.comment` | `LINEAR_API_KEY` |
| `notion` | `database.query`, `page.create`, `page.update`, `block.append` | `NOTION_TOKEN` |
| `calendar_caldav` | `events.list`, `events.create` | CalDAV URL + credentials |
| `s3` | `put`, `get`, `list`, `delete` | `AWS_ACCESS_KEY` + `AWS_SECRET_KEY` |
| `llm` | `complete`, `chat`, `embed` | `ANTHROPIC_API_KEY` or `OPENAI_API_KEY` |
| `weather` | `get` | none |

---

## Examples

The `examples/` directory contains one ready-to-use workflow per plugin:

| File | What it does |
|---|---|
| `github-notify.yaml` | Slack alert for unread GitHub notifications (every 5m) |
| `gitlab-todos.yaml` | Daily GitLab todo count to Discord |
| `slack-alert.yaml` | Forward webhook payload to Slack |
| `discord-deploy.yaml` | Discord alert on lock file change |
| `rss-to-slack.yaml` | Hourly HN feed to Slack |
| `llm-summarize.yaml` | Summarize RSS with Claude, post to Slack |
| `linear-triage.yaml` | Daily unstarted issues digest to Slack |
| `notion-log.yaml` | Create Notion page per webhook event |
| `calendar-reminder.yaml` | Slack reminders for upcoming calendar events |
| `s3-backup.yaml` | Back up a file to S3 on change |
| `email-to-linear.yaml` | Create Linear issue per support email |
| `sqlite-log.yaml` | Log webhook events to SQLite |
| `postgres-report.yaml` | Daily DAU query → email report |
| `transform-etl.yaml` | Fetch JSON API, jq-transform, store to SQLite |
| `weather-report.yaml` | Morning weather report to Slack |

---

## Secrets

Secrets are encrypted at rest with [age](https://age-encryption.org) and stored in `~/.taskhub/taskhub.db`. The encryption key lives in your OS keychain (Windows Credential Manager, macOS Keychain, Linux Secret Service) with a file fallback at `~/.taskhub/master.key`.

```bash
taskhub secret set GITHUB_TOKEN   # prompted, never echoed
taskhub secret list               # lists keys only, never values
taskhub secret remove GITHUB_TOKEN
```

Reference in workflows: `${{ secrets.GITHUB_TOKEN }}`

---

## VS Code autocomplete

Add to `.vscode/settings.json`:

```json
{
  "yaml.schemas": {
    "./workflow.schema.json": "**/*.yaml"
  }
}
```

Requires the [YAML extension](https://marketplace.visualstudio.com/items?itemName=redhat.vscode-yaml).

---

## Building WASM plugins

To build the bundled plugins from source (requires `wasm32-unknown-unknown` target):

```bash
rustup target add wasm32-unknown-unknown
powershell -ExecutionPolicy Bypass -File scripts/build-plugins.ps1  # Windows
bash scripts/build-plugins.sh                                        # macOS/Linux
```

To write your own plugin, copy `plugins/_template/` and follow the inline comments.

---

## Status

| Milestone | Status |
|---|---|
| M0 Discovery | ✅ Done |
| M1 Spec + scaffolding | ✅ Done |
| M2 Core engine | ✅ Done |
| M3 WASM plugin system | ✅ Done |
| M4 Triggers + daemon | ✅ Done |
| M5 Essential plugins | ✅ Done |
| M6 Dogfood + closed beta | ⏳ In progress |
| M7 Public launch | ⏳ Planned |

---

## License

MIT — see [LICENSE](LICENSE).
