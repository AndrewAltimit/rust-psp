# Maintainer & CI Infrastructure

This page covers how the repository itself is built, tested, and released.
SDK users don't need any of it. Day-to-day agent instructions are in
[`AGENTS.md`](../AGENTS.md) and [`CLAUDE.md`](../CLAUDE.md).

## Workspaces

| Workspace | Manifest | Builds for | Toolchain |
|-----------|----------|------------|-----------|
| PSP workspace | `Cargo.toml` (members: `psp`, `examples/*`, `ci/tests`, `ci/std_verification`) | `mipsel-sony-psp` | nightly + `rust-src` |
| Host tools | `cargo-psp/Cargo.toml` (excluded from the root workspace) | host | stable |

Each workspace has its own `Cargo.lock`, so `cargo deny` and `cargo fmt`
have to be run once per workspace.

## Running CI locally

`ci/run-ci.sh <stage>` runs the same containerized stages that GitHub Actions
runs: `fmt`, `clippy`, `test`, `build`, `deny`, `psp-test`, or `full` for all
of them. The equivalent manual commands:

```bash
# Format / lint / unit tests (host tools)
docker compose --profile ci run --rm rust-ci cargo fmt --manifest-path cargo-psp/Cargo.toml --all -- --check
docker compose --profile ci run --rm rust-ci cargo +nightly fmt --all -- --check
docker compose --profile ci run --rm rust-ci cargo clippy --manifest-path cargo-psp/Cargo.toml --all-targets -- -D warnings
docker compose --profile ci run --rm rust-ci cargo test --manifest-path cargo-psp/Cargo.toml

# Build host tools, the kernel-mode example, and the PSP test EBOOT
docker compose --profile ci run --rm rust-ci cargo build --manifest-path cargo-psp/Cargo.toml --release
docker compose --profile ci run --rm -w /app/examples/kernel-mode rust-ci \
  bash -c 'export PATH="/app/cargo-psp/target/release:$PATH" && cargo +nightly psp'
docker compose --profile ci run --rm -w /app/ci/tests rust-ci \
  bash -c 'export PATH="/app/cargo-psp/target/release:$PATH" && cargo +nightly psp'

# Licenses / advisories
docker compose --profile ci run --rm rust-ci cargo deny check
docker compose --profile ci run --rm rust-ci cargo deny --manifest-path cargo-psp/Cargo.toml check

# Emulator test: copies test_cases.EBOOT.PBP (from target/mipsel-sony-psp-std/
# or target/mipsel-sony-psp/) into target/psp-test/, runs PPSSPPHeadless there,
# and fails unless target/psp-test/psp_output_file.log ends in "FINAL_SUCCESS"
bash ci/psp-test.sh
```

## GitHub Actions

All jobs except `fork-guard` run on a self-hosted runner shared with
[template-repo](https://github.com/AndrewAltimit/template-repo). Rust builds
and tests run inside the `rust-ci` container.

| Workflow | Trigger | Jobs |
|----------|---------|------|
| `main-ci.yml` | push to `main`, `v*` tags, manual (`create_release` input) | `ci` (all stages below), `build-release-binaries` + `create-release` (tags or manual), `deploy-pages` (publishes `site/` to GitHub Pages on `main`), `notify` summary |
| `pr-validation.yml` | pull requests to `main`, manual | `fork-guard`, `ci`, `gemini-review`, `codex-review` (permanently disabled, `if: false`), `agent-review-response`, `agent-failure-handler`, `pr-status` |

CI stages, in order:

1. `cargo fmt --check` (stable for cargo-psp, nightly for the PSP workspace)
2. `cargo clippy -D warnings` (cargo-psp)
3. `cargo test` (cargo-psp)
4. Release build of cargo-psp, then `cargo psp` for `examples/kernel-mode` (checks the `kernel` feature) and `ci/tests`
5. `cargo deny check` for both workspaces
6. The `ci/tests` EBOOT run in PPSSPPHeadless

`ci/std_verification` (a `std` feature test harness) is a workspace member,
but no CI stage builds it.

### PR agent pipeline

- `fork-guard` blocks fork PRs from reaching the self-hosted runner.
- `gemini-review` posts an AI review (`github-agents pr-review`).
- `agent-review-response` applies fixes from review feedback, and
  `agent-failure-handler` fixes CI failures (both use `automation-cli`).
  Each is capped at 5 iterations per agent type. Iterations are tracked with
  `agent-metadata:type=<type>` PR comments. An admin comment `[CONTINUE]`
  extends the cap. The `no-auto-fix` label disables both.

> **OpenAI / Codex disabled.** OpenAI permits government partners
> unrestricted use of its models within their own definition of legality.
> Anthropic maintains explicit prohibitions on mass surveillance and
> autonomous weapons. This project defaults to the stricter policy. The
> `codex-review` job is hard-disabled with `if: false`, and the `mcp-codex`
> compose service sits in a separate `disabled-codex` profile with
> `CODEX_ENABLED=false`.

### Runner dependencies

The runner provides these binaries, built from template-repo. Workflows
degrade gracefully when they are missing.

| Binary | Source (template-repo) | Used by |
|--------|------------------------|---------|
| `github-agents` | `tools/rust/github-agents-cli` | PR reviews, iteration tracking |
| `automation-cli` | `tools/rust/automation-cli` | review response, failure handler |

### Secrets

| Secret | Used by | Purpose |
|--------|---------|---------|
| `GITHUB_TOKEN` | all workflows | standard token |
| `AGENT_TOKEN` | `pr-validation.yml` | PAT for agent commits |
| `GOOGLE_API_KEY` / `GEMINI_API_KEY` | `pr-validation.yml` | Gemini reviews |

## Releases

Pushing a `v*` tag, or running `main-ci.yml` manually with
`create_release`, triggers:

1. full CI,
2. a containerized release build of `cargo-psp`, `prxgen`, `pack-pbp`,
   `mksfo`, and `prxmin`, named `<bin>-linux-<arch>`. `<arch>` is the
   runner's `uname -m`, so releases only contain the runner's architecture
   (the `v1.0.0` assets are `linux-aarch64`),
3. a GitHub Release with those binaries and an auto-generated changelog.

## Docker images

| Compose service | Profile | Source |
|-----------------|---------|--------|
| `rust-ci` | `ci` | `docker/rust-ci.Dockerfile` (this repo) |
| `ppsspp` | `psp` | `ci/Dockerfile-ppsspp` (this repo) |
| `mcp-code-quality`, `mcp-content-creation`, `mcp-gemini`, `mcp-opencode`, `mcp-crush`, `mcp-github-board`, `mcp-reaction-search` | `services` | pre-built `template-repo-mcp-<name>:latest` images |
| `mcp-agentcore-memory` | `services`, `memory` | pre-built template-repo image |
| `mcp-codex` | `disabled-codex` | disabled (see above) |

The MCP images **cannot be built from this repo**. Their source is in
template-repo under `tools/mcp/`. Build them once from a template-repo
checkout with `docker compose --profile services build`. CI and PSP
development don't need them. They only serve interactive agent sessions.

## Local agent tooling

- `tools/cli/agents/`: launchers for Claude Code (`run_claude.sh`,
  `run_claude.bat`, `stop_claude.sh`), Gemini CLI (`run_gemini.sh`), OpenCode
  (`run_opencode.sh`), and Crush (`run_crush.sh`).
- `tools/cli/containers/`: containerized variants (see its README).
