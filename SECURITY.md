# Security Policy

## Supported versions

ASCII Forge is pre-1.0. Security fixes are applied to the latest release on the
`main` branch.

## Reporting a vulnerability

Please do **not** open a public issue for security problems.

Use GitHub private vulnerability reporting:

https://github.com/artur282/ascii-forge/security/advisories/new

Include:

- A description of the issue and its impact.
- Steps to reproduce.
- The affected version or commit.
- Any relevant logs, without secrets or personal data.

## Scope

Security-relevant areas include:

- Provider endpoints: HTTPS is required; plain HTTP is limited to loopback
  hosts unless `ASCII_FORGE_ALLOW_INSECURE_HTTP=1` is explicitly set.
- MCP path confinement, overwrite protection and bounded previews.

- Scene parsing and validation (untrusted JSON/YAML).
- AI prompt handling and provider responses.
- FFmpeg invocation and argument handling.
- File path handling for imports and exports.
- Secret handling (API keys, environment variables).
- Generated HTML/SVG output.

## Out of scope

- Vulnerabilities in FFmpeg itself; report them upstream.
- Vulnerabilities in third-party AI providers.
- Issues that require the user to run untrusted code outside ASCII Forge.

## Response

This is a personal project. Reports are handled on a best-effort basis. Please
allow time for a response before public disclosure.
