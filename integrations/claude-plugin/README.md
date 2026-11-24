# OpenSpider plugin for Claude

OpenSpider is an open-source desktop SEO crawler for macOS and Windows. This plugin
connects Claude to the OpenSpider program installed on your computer, so Claude can
read your saved crawls, explain and prioritize technical SEO issues, compare two
crawls and, if you allow it in OpenSpider, start and monitor new crawls.

The plugin contains:

- a local MCP server entry that starts `openspider mcp` on your computer
  (through the small launcher `scripts/launch.mjs`),
- skills: `technical-seo-audit`, `issue-triage`, `crawl-comparison`,
  `crawl-monitoring`, and the general `openspider` skill with tool references,
- commands: `/openspider:openspider-audit`, `/openspider:openspider-issues`,
  `/openspider:openspider-compare`.

## Requirements

- **OpenSpider 1.0 or later**, installed from
  [GitHub Releases](https://github.com/wojcikdominik/openspider/releases/latest) or
  [openspider.de](https://openspider.de): macOS DMG or Homebrew cask, Windows setup or MSI.
  The plugin does not install OpenSpider and does not download anything.
- **Node.js 22 or later** (`node` on PATH) to run the launcher.
- A Claude app that runs local MCP servers: **Claude Code** (macOS, Windows, Linux with
  the OpenSpider CLI) and **Cowork when the session runs on your computer**. In Claude
  chat (web, desktop and mobile) the MCP server is ignored, so the skills have no data
  to work with there.

## Setup

1. Install OpenSpider (no further setup in the app is needed for reading crawls).
2. Install this plugin and restart Claude (or reload plugins).
3. Ask, for example: "List my OpenSpider crawls", "Audit my latest crawl of
   example.com", "What should I fix first?", "Compare the last two crawls".

The launcher looks for the OpenSpider program in this order: the `OPENSPIDER_BINARY`
environment variable (absolute path to the executable), every folder on PATH, then the
default install locations:

| Installation | Program path |
| --- | --- |
| macOS DMG | `/Applications/OpenSpider.app/Contents/MacOS/openspider` (also `~/Applications`) |
| macOS Homebrew cask | same app; the cask also links `openspider` into Homebrew's `bin` |
| Windows setup (per user) | `%LOCALAPPDATA%\OpenSpider\openspider.exe` (also added to PATH) |
| Windows MSI (per machine) | `%ProgramFiles%\OpenSpider\openspider.exe` |

If nothing is found, the MCP server fails to start and the error (visible in Claude
Code under `/mcp`) names the download page, the searched paths and how to set
`OPENSPIDER_BINARY`.

## Permissions

The plugin starts `openspider mcp` without extra flags, so OpenSpider's own settings
decide what Claude may do. Out of the box only **read** tools are enabled. Crawl tools
(`start_crawl`, `crawl_status`, `pause_crawl`, `resume_crawl`, `stop_crawl`) appear only
after you tick "Agent may start crawls" in OpenSpider's MCP menu (or add `crawl` to
`classes` in OpenSpider's `mcp.toml`). Export (`write`) and paid provider calls (`cost`)
need their own classes, which the plugin's skills do not use. The plugin never changes
these settings. Private and local network targets stay blocked unless you allow them
in OpenSpider.

## What this plugin runs, sends and fetches

- **Runs:** `node scripts/launch.mjs`, which starts the installed program as
  `openspider mcp` (MCP over STDIO, no shell, no extra arguments) and passes its
  input and output through. The launcher only checks whether files exist at the paths
  listed above; it opens no network connection and writes no files.
- **Reads:** the OpenSpider MCP server reads your saved crawl files (`.ospider`) from
  OpenSpider's data folder and its settings (`mcp.toml`, crawl profiles).
- **Fetches:** only when crawl tools are enabled and Claude calls `start_crawl` or
  `resume_crawl`, OpenSpider sends HTTP requests from your computer to the website being
  crawled (and to the sites it links to, within the crawl's scope and limits). Crawled
  data is saved locally in a new crawl file. The read tools never fetch anything.
- **Sends to Claude:** tool results (crawl data such as URLs, status codes, titles,
  headers, issue lists and, on request, saved page text, HTML or screenshots) go to the
  Claude model as part of your conversation. Content that comes from crawled websites is
  returned inside a block marked `<<<UNTRUSTED_CONTENT>>>`; the server and the skills
  tell Claude to treat it as data, never as instructions.
- **Third parties:** the plugin itself sends no data to anyone else and contains no
  analytics or telemetry. OpenSpider's own optional features (update check, paid
  integrations) are configured in OpenSpider and are not used by this plugin.
- **Credentials:** none. The plugin asks for no keys or passwords.

## Troubleshooting

- `/mcp` in Claude Code shows the server as failed: open the error; usually OpenSpider
  is not installed or not found. Set `OPENSPIDER_BINARY` to the full path of the
  executable and restart Claude.
- "node: not found": install Node.js 22 or later, or connect OpenSpider directly with
  OpenSpider's menu "MCP" -> "Connect Claude Code..." instead of this plugin.
- No crawls listed: OpenSpider reads its standard data folder, or
  `<OPENSPIDER_HOME>/crawls` when `OPENSPIDER_HOME` is set. Claude and the OpenSpider app
  must use the same folder.
- Crawl tools missing: enable "Agent may start crawls" in OpenSpider, then restart the
  MCP connection.

## License and support

Apache-2.0, same as OpenSpider (see `LICENSE` in the repository root). Source and
issues: [github.com/wojcikdominik/openspider](https://github.com/wojcikdominik/openspider).
Privacy notice: [openspider.de/en/privacy](https://openspider.de/en/privacy/).
