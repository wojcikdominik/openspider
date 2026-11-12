<p align="center"><img src="assets/brand/openspider-mark.svg" width="160" alt="OpenSpider logo"></p>

<h1 align="center">OpenSpider</h1>

<p align="center"><b>The crawler for the agentic era.</b><br>
Open-source website crawler and technical SEO auditor – built for AI agents and humans alike.</p>

<p align="center">
  <a href="https://github.com/wojcikdominik/openspider/actions/workflows/ci.yml"><img src="https://github.com/wojcikdominik/openspider/actions/workflows/ci.yml/badge.svg?branch=main" alt="CI"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-Apache--2.0-blue.svg" alt="License: Apache-2.0"></a>
  <img src="https://img.shields.io/badge/platforms-macOS%20(Apple%20Silicon%20%26%20Intel)%20%7C%20Windows-lightgrey.svg" alt="Platforms">
  <img src="https://img.shields.io/badge/core-Rust-orange.svg" alt="Rust">
  <img src="https://img.shields.io/badge/MCP-native-8A2BE2.svg" alt="MCP native">
  <a href="https://github.com/wojcikdominik/openspider/releases/latest"><img src="https://img.shields.io/badge/status-beta-orange.svg" alt="Status: beta"></a>
</p>

<p align="center">
  <a href="https://openspider.de">Website</a> ·
  <a href="docs/ARCHITECTURE.md">Architecture</a> ·
  <a href="docs/spec">Specification</a> ·
  <a href="#roadmap">Roadmap</a>
</p>

<p align="center">
  <img src="assets/screenshots/hero-light-dark.jpg" width="900" alt="OpenSpider desktop app after crawling a website: overview with crawl totals, status distribution and top issues, shown in light and dark mode">
</p>

<p align="center">
  <b>Public beta:</b>
  <a href="https://github.com/wojcikdominik/openspider/releases/latest/download/OpenSpider-macOS.dmg"><b>⬇ Download for macOS</b></a> (13+, Apple Silicon &amp; Intel) ·
  <a href="https://github.com/wojcikdominik/openspider/releases/latest/download/OpenSpider-Windows-Setup.exe"><b>⬇ Download for Windows</b></a> (10/11, x64)<br>
  <a href="https://github.com/wojcikdominik/openspider/releases/latest">All files and release notes</a>
</p>

---

OpenSpider crawls websites the way a search engine does and finds the technical SEO problems that cost rankings: broken links, redirect chains, canonical and hreflang errors, indexability conflicts, duplicates, thin content and much more.

What makes it different: **AI agents are first-class users.** A built-in MCP server lets Claude, Codex, Cursor and other agents start crawls, ask precise questions about the results and run full audits – safely: by default an agent can only read; starting crawls, writing files or using paid services has to be enabled explicitly. And every crawl is an open SQLite file, so your data never gets locked into a proprietary format.

## What's new

**1.0.0-beta.4** (12 November 2026): new update dialog, live counters that follow the crawl within a second, clear messages when the start URL cannot be reached, repaired comparison view.

Full list: [openspider.de/en/changelog](https://openspider.de/en/changelog/) · [GitHub Releases](https://github.com/wojcikdominik/openspider/releases)

## Why OpenSpider?

| | |
|---|---|
| 🤖 **Agent-native** | Built-in MCP server (STDIO and authenticated local HTTP). Tools are grouped into `read`, `crawl`, `write` and `cost` classes – only `read` is enabled by default. Crawled content is treated as untrusted data, never as instructions (prompt-injection safe outputs). |
| 🐘 **Built for large sites** | Disk-first storage with constant memory and crash-safe resume. **Measured: 1 million URLs in about 4.5 minutes at ~3,600 URLs/s with under 0.5 GB RAM** (Apple M2, synthetic site with 30 links per page). |
| 📂 **Open data** | One plain SQLite file per crawl (`.ospider`). Query it with SQL, Python, DuckDB, Excel or any BI tool – no export step needed. |
| 🔍 **Complete technical SEO** | Status codes, redirects, canonicals, hreflang, directives, robots.txt, sitemaps, duplicates, content, images, links, pagination, security headers, structured data – with filters, issue explanations, reports and exports (CSV/XLSX). |
| 🆓 **Free, no limits** | Apache-2.0. No URL cap, no license key, headless CLI and scheduled crawls included – perfect for CI/CD pipelines. |
| ⚡ **Native and fast** | Rust core, Tauri desktop app, small installers for macOS (Apple Silicon & Intel) and Windows. |

## Screenshots

<table>
  <tr>
    <td width="50%">
      <picture>
        <source media="(prefers-color-scheme: dark)" srcset="assets/screenshots/dashboard-dark.jpg">
        <img src="assets/screenshots/dashboard-light.jpg" alt="Overview after a crawl: totals, status distribution and top issues">
      </picture>
      <p align="center"><b>Overview</b> – totals, status codes and the most important issues at a glance</p>
    </td>
    <td width="50%">
      <picture>
        <source media="(prefers-color-scheme: dark)" srcset="assets/screenshots/urls-dark.jpg">
        <img src="assets/screenshots/urls-light.jpg" alt="URL table filtered to HTML pages with the SERP snippet preview of the selected page">
      </picture>
      <p align="center"><b>URL table and details</b> – filters, columns and a SERP snippet preview per page</p>
    </td>
  </tr>
</table>

<sub>Screenshots: OpenSpider 1.0.0-beta.1 on macOS, crawling <a href="https://caloone.de">caloone.de</a> (4,674 URLs in 46 seconds). The images follow your GitHub light/dark theme.</sub>

## Performance

Measured on 4 November 2026 on a Mac mini (Apple M2, 16 GB) with the released OpenSpider 1.0.0-beta.2 CLI, a synthetic local test site (30 links per page) and default settings. Time runs from start until the crawl is saved and the internal URL table is exported; memory is the peak resident set size (RSS).

**Crawling**

| Crawl | Time (median of 3) | Throughput | Peak memory |
|---|---|---|---|
| 100,000 URLs | **48 s** | ~2,080 URLs/s | **0.85 GB** |
| 1,000,000 URLs | **12 min 51 s** | ~1,300 URLs/s | **~1.05 GB** |

Crash safety: 10 of 10 hard kills during a crawl resumed cleanly from the crawl file (development build, October 2026).

**Side by side with a widely used commercial desktop SEO crawler** – same machine, same local test site with 100,000 URLs, both with default settings (5 threads), crawl saved and the internal URL table exported:

| | OpenSpider 1.0.0-beta.2 | Commercial crawler |
|---|---|---|
| Total time | **48 s** | 333 s |
| Peak memory | **0.85 GB** | 2.28 GB |

About 7× faster with roughly a third of the memory. A local test site has no network latency, so on real websites politeness limits and server response times narrow the gap. You can reproduce the OpenSpider side with the released CLI and the test site in `crates/os-testsite` (`os-testsite synthetic:n=100000,links=30`).

**Working with the results – 5,000,000 URLs in one crawl file** (development build, October 2026)

| Operation | Measured | Budget |
|---|---|---|
| Fetch a page of 200 rows × 20 columns | **0.7 ms** | ≤ 30 ms |
| Switch filter (cached) | **36 ms** | ≤ 200 ms |
| Reverse sort order | **35–42 ms** | ≤ 200 ms |
| Search as a filter | **53 ms** | ≤ 1 s |
| Scroll the table (desktop app grid) | **~60 fps**, no frame over 50 ms | ≥ 55 fps |

Memory for an open view over 5 million rows stays around 60 MB. The table never loads the whole crawl – it streams the visible window from the crawl file.

## Installation

The public beta is live. Download one file; the links always point to the latest release:

| System | Download |
|---|---|
| macOS 13+ (Apple Silicon and Intel) | [OpenSpider-macOS.dmg](https://github.com/wojcikdominik/openspider/releases/latest/download/OpenSpider-macOS.dmg) (universal DMG) |
| Windows 10 1809+ / 11 (x64, also Windows on ARM via emulation) | [OpenSpider-Windows-Setup.exe](https://github.com/wojcikdominik/openspider/releases/latest/download/OpenSpider-Windows-Setup.exe) (per-user setup) |

The [latest release](https://github.com/wojcikdominik/openspider/releases/latest) lists the same installers
under their versioned names (`OpenSpider_<version>_universal.dmg`, `OpenSpider_<version>_x64-setup.exe`)
plus the files for automatic updates, the CLI, managed installs (MSI) and checksums (`SHA256SUMS`,
which also covers the two versionless copies). See also the [beta page](https://openspider.de/en/beta/).
Linux x86_64 with glibc supports the core and CLI only; Linux desktop,
.deb/.rpm and arm64 releases remain unsupported.
Check a download against `SHA256SUMS` before opening it (commands on the [download page](https://openspider.de/en/download/#sha256)).
The macOS app is signed and notarized; on first launch Gatekeeper asks once, and Windows SmartScreen may warn while the
signing certificate is new (see the user guide for what to check).
Version 1.0 has not been released yet; beta users receive it as a normal update.

**Packages.** The Homebrew tap `wojcikdominik/homebrew-openspider` provides the cask `openspider@beta` for macOS.
Homebrew loads casks from third-party taps only after you trust the tap explicitly:

```sh
brew tap wojcikdominik/openspider
brew trust wojcikdominik/openspider
brew install --cask wojcikdominik/openspider/openspider@beta
```

The cask installs the same signed DMG as the download. The app updates itself afterwards, so an upgrade through
Homebrew needs `brew upgrade --cask --greedy wojcikdominik/openspider/openspider@beta`.
winget is planned from 1.0 on; until then use the Windows installers above.

**JavaScript rendering.** Chromium is not part of the installer. On request the app downloads a verified Chromium
(Configuration → Spider → Rendering, about 200 MB, SHA-256 check); an installed Chrome works as well.

**Docker (self-hosted server).** A [Dockerfile](packaging/docker/Dockerfile) builds a container image with the Linux CLI that covers crawls, exports,
scheduled runs (cron on the host) and MCP over STDIO. It has no web interface: results are files
and `.ospider` crawl files that you open in the desktop app. The image runs as UID/GID 10001,
uses `/data` as output folder and `/config` (`OPENSPIDER_HOME`) for crawl management; keep
`/config` to compare with the previous crawl. There is no public image: build it yourself from
[`packaging/docker`](packaging/docker/Dockerfile) at the release tag, with the archive checksum from `SHA256SUMS`
of the same release.

```sh
docker build -t openspider:<version> --build-arg OPENSPIDER_VERSION=<version> \
  --build-arg OPENSPIDER_SHA256=<sha256 of openspider_<version>_linux-x86_64.tar.gz> packaging/docker
docker run --rm -v "$PWD/data:/data" -v "$PWD/config:/config" openspider:<version> \
  --headless --crawl https://example.com --export-tabs "Internal:All" \
  --save-crawl --output-folder /data
```

On Linux create `data` and `config` for UID 10001 first or add `--user "$(id -u):$(id -g)"`.
Comparison runs, cron and MCP configurations for Claude Code, Codex and Cursor: see the user guide,
section [Docker](https://openspider.de/en/manual/#docker).

The [German user guide](https://openspider.de/handbuch/) and
[English user guide](https://openspider.de/en/manual/) cover DMG, NSIS and silent MSI
installation, Gatekeeper/SmartScreen, Chromium, CLI/MCP, scheduling, updates and uninstalling.
Automatic update checks send only version, OS and architecture to GitHub Releases
(which sees the connection IP), without telemetry or crawl data. Disable them in
Settings → Updates or with `OPENSPIDER_DISABLE_UPDATE_CHECK=1`. From beta.4 the app shows what is new once after an update.

## Documentation

- [Benutzerhandbuch (Deutsch)](https://openspider.de/handbuch/)
- [User guide (English)](https://openspider.de/en/manual/)
- [Download and checksums](https://openspider.de/en/download/) ([Deutsch](https://openspider.de/download/)) and the [FAQ](https://openspider.de/en/faq/) ([Deutsch](https://openspider.de/faq/))
- [Open beta](https://openspider.de/en/beta/) ([Deutsch](https://openspider.de/beta/)) — everyone can join; send feedback through the form on that page or open a [GitHub issue](https://github.com/wojcikdominik/openspider/issues/new/choose).

### Build the CLI from source

```bash
git clone https://github.com/wojcikdominik/openspider.git
cd openspider
cargo build --release -p os-cli          # binary: target/release/openspider
```

**Crawl a site and export results**

```bash
openspider --crawl https://example.com --headless \
  --output-folder ./out --export-tabs "Internal:All" --save-crawl
```

**Crawl a list of URLs or a sitemap**

```bash
openspider --crawl-list https://example.com/sitemap.xml --headless --output-folder ./out
```

**Work with saved crawls**

```bash
openspider --list-crawls --json
openspider --load-crawl <crawl-id> --headless --output-folder ./out --export-tabs "Internal:HTML"
openspider --help export-tabs            # all valid tab:filter names
```

## Use it from your AI agent

The desktop app connects Claude Desktop, Claude Code, Codex and Cursor with one click (menu **MCP**). It backs up the client's config file, adds only its own `openspider` entry and saves your permission settings in the same step. On a server or without the app, the CLI prints a ready-to-paste configuration:

```bash
openspider mcp --print-config claude-code     # also: claude-desktop, codex, cursor
```

Agents can only read by default. Starting crawls requires the permission **Agent may start crawls** in the connect dialog (or `--allow read,crawl` on the CLI); tools that write files or use paid services have to be enabled separately.

Then just ask your agent, for example:

> *“List my OpenSpider crawls, show the overview of the latest one and explain the five most important issues with example URLs.”*

Read tools: `list_crawls`, `crawl_overview`, `list_issues`, `query_urls`, `url_details`. With the crawl permission an agent can also use `start_crawl` and `crawl_status`; from beta.4 on, `pause_crawl`, `resume_crawl` and `stop_crawl` are added. A Claude Code plugin with slash commands (`/openspider-audit`, `/openspider-compare`, `/openspider-issues`) and a skill follow in the next milestone.

## Your crawl is a database

```bash
sqlite3 example.ospider \
  "SELECT address, status_code, title_1 FROM urls
   WHERE is_internal = 1 AND status_code >= 400
   ORDER BY address LIMIT 20;"
```

The file format is documented and versioned, so scripts and dashboards keep working across releases.

## How it works

```mermaid
flowchart LR
    A[Frontier &amp; robots.txt] --> B[Fetch<br/>HTTP · TLS]
    B --> C[Parse<br/>HTML · headers · sitemaps]
    C --> R[Render<br/>headless Chromium · JavaScript]
    C --> D[(SQLite crawl file<br/>.ospider)]
    R --> D
    D --> E[Analyze<br/>filters · issues · reports]
    E --> F[Desktop app]
    E --> G[CLI &amp; exports]
    E --> H[MCP server<br/>for AI agents]
```

A Cargo workspace of focused crates (`os-fetch`, `os-parse`, `os-render`, `os-store`, `os-engine`, `os-analyze`, `os-export`, `os-mcp`, `os-cli`, …) with a Tauri v2 + React desktop app on top. Details: [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## Roadmap

| Milestone | Content | Status |
|---|---|---|
| **M1** | Crawl core, storage, CLI, MCP (read-only), core analyses | ✅ done |
| **M2** | Desktop app, all analysis tabs, reports, JavaScript rendering, Google APIs | ✅ done |
| **M3** | Signed installers, auto-update, public beta → **1.0** | 🚧 in progress – public beta since November 2026 |
| **M4** | AI features: page classification, optimization suggestions, “ask your crawl”, AI search visibility (Claude, OpenAI, OpenRouter, Ollama) | after 1.0 |
| **M5** | External SEO data (keywords, backlinks, SERP, LLM mentions) | after 1.0 |

OpenSpider stays fully usable without any account. AI and external data features will work with your own API keys or local models. More: [roadmap after 1.0](docs/plan/ROADMAP-nach-1.0.md).

## Contributing

Everyone is welcome to help: report bugs and request features via [Issues](https://github.com/wojcikdominik/openspider/issues/new/choose), ask questions and share ideas in [Discussions](https://github.com/wojcikdominik/openspider/discussions), or send a pull request. Contributions follow a few simple rules: work from the specification in [`docs/spec`](docs/spec), reference spec IDs in tests, keep `fmt`/`clippy`/tests green before every commit, and use only permissively licensed dependencies.

## License

Apache-2.0 – see [LICENSE](LICENSE).

The OpenSpider name and logo (`assets/brand/`) are not covered by the Apache-2.0 license; please do not use them to brand forks or derived products without permission.
