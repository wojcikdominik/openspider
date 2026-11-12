# Security Policy

## Reporting a vulnerability

Please do **not** open a public issue for a security problem.

Report it privately through GitHub: open the [Security tab](https://github.com/wojcikdominik/openspider/security) of this repository and choose **Report a vulnerability** ([direct link](https://github.com/wojcikdominik/openspider/security/advisories/new)). The report is visible only to the maintainers.

Helpful details:

- the OpenSpider version and operating system,
- what an attacker needs (for example a crafted web page, a crawl file, or local access),
- steps to reproduce, and the impact you observed.

You can expect a first answer within 7 days. Once a fix is released, the advisory is published and you are credited, unless you prefer to stay anonymous.

## Supported versions

Security fixes go into the latest release. Until 1.0 that is the latest beta; from 1.0 on it is the latest stable release and the current beta.

| Version | Supported |
| --- | --- |
| Latest release | Yes |
| Older releases | No – please update |

## Scope

In scope are the desktop app, the command-line tool, the MCP server, the installers and the update mechanism. OpenSpider fetches and renders pages from the sites it crawls, so problems that a crawled site can trigger on the user's machine are of particular interest.

Not in scope are findings that only concern a website being crawled, and reports that require an already compromised machine.
