# MCP tool reference

This table mirrors `crates/os-mcp/src/tools/mod.rs::all()`. The validator checks names
and classes against that register. Live `tools/list` remains authoritative for schemas
and per-installation availability; disabled classes/tools are omitted.

| Tool | Class | Use |
| --- | --- | --- |
| `compare_crawls` | `read` | Compare saved crawls by tab/filter; change detection requires its available analysis. `changes=true` returns the Crawl Changes report (pages gone, new errors, new redirects and chains, changed canonicals, indexability changes), paginated; `change` selects one section. |
| `fetch_link_metrics` | `cost` | Estimate provider usage for saved-crawl URLs; execute only after explicit confirmation. |
| `list_crawls` | `read` | Discover saved crawl IDs, dates and states. |
| `crawl_overview` | `read` | Crawl counts, filter counts, top issues and analysis gaps. |
| `query_urls` | `read` | Paginated tab/filter rows, sorting and selected columns. |
| `url_details` | `read` | A URL's details, inlinks/outlinks, headers and directives. |
| `list_issues` | `read` | Issue priorities, counts, explanations, examples and query references. |
| `sql_query` | `read` | Bounded, read-only SQL when tab queries cannot answer the question. |
| `list_catalog` | `read` | Canonical tabs/filters, columns, reports, bulk exports and export formats. |
| `url_content` | `read` | Saved HTML or extracted text of a crawled URL (untrusted data). |
| `url_screenshot` | `read` | Saved render screenshot of a URL as image content. |
| `start_crawl` | `crawl` | Start a spider/list crawl with a saved profile and safe overrides. |
| `crawl_status` | `crawl` | Poll progress and completion for a crawl. |
| `pause_crawl` | `crawl` | Pause a running crawl so it can be resumed later; poll until the state is Paused. |
| `stop_crawl` | `crawl` | End a running or paused crawl for good; its data stays saved, but it can never be resumed. |
| `resume_crawl` | `crawl` | Resume a paused, interrupted or limit-ended crawl; stopped crawls are rejected. |
| `export` | `write` | Save a requested CSV/XLSX tab, bulk export or report, or page content as NDJSON. |

Crawl control errors with code `busy` (also in `structuredContent.error`) are retryable: a pause or
stop is still in progress, or the app is running another operation. Wait briefly and call again.

The `cost` class contains paid provider operations. `fetch_link_metrics` first returns
an estimate; executing it requires the same arguments with explicit confirmation. A server
started with `--read-only` exposes only `read`, including for existing saved crawls.
Otherwise `mcp.toml` or `--allow crawl,write` can enable more classes. The plugin invokes
`openspider mcp` with no permission overrides and inherits the operator's settings.

Examples below use `CRAWL_ID`/`PAGE_URL` returned by discovery, never guessed IDs:

```json
{"tool":"list_crawls","arguments":{}}
{"tool":"crawl_overview","arguments":{"crawl_id":"CRAWL_ID"}}
{"tool":"list_issues","arguments":{"crawl_id":"CRAWL_ID","lang":"de","limit":50}}
{"tool":"query_urls","arguments":{"crawl_id":"CRAWL_ID","tab":"Response Codes","filter":"Client Error (4xx)","limit":50}}
{"tool":"url_details","arguments":{"crawl_id":"CRAWL_ID","url":"PAGE_URL","sections":["inlinks","directives"],"limit":50}}
{"tool":"start_crawl","arguments":{"url":"https://example.com/","overrides":{"limits.max_urls":100,"limits.max_depth":3,"speed.max_urls_per_sec":2}}}
{"tool":"crawl_status","arguments":{"crawl_id":"CRAWL_ID"}}
```

Paginate with the returned `next_cursor` as `cursor` and keep all filters/sort parameters
unchanged. `url_details` returns a cursor for each link section; use `inlinks_cursor` and
`outlinks_cursor` when requesting both. `query_urls` returns `filter_state`; unavailable
analysis is a reason to report uncertainty. `list_issues` supports `lang: de|en`,
`priority: high|medium|low` and `type: issue|warning|opportunity`.
