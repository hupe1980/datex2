# Security policy

## Reporting a vulnerability

Please report a suspected vulnerability **privately** through GitHub's private vulnerability
reporting: <https://github.com/hupe1980/datex2/security/advisories/new>.

Do not open a public issue for it. Include the `datex2` version and the smallest input, or the
smallest program, that shows the problem.

## Scope

What `datex2` is meant to defend against, and what it does not, is described in the
[threat model](THREAT_MODEL.md), which lists every known threat with its control and its tests.
Typical reports in scope: an input that makes the reader crash, hang or use memory out of
proportion to its size; input text that reaches an error or warning unescaped; a value that adds
markup to a written document; a written document the official schema rejects.

## Supported versions

Before 1.0 only the latest minor version gets fixes.
