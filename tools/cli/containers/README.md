# Agent Container Scripts

These scripts run AI coding agents inside Docker containers. They were copied
from [template-repo](https://github.com/AndrewAltimit/template-repo), and only
some of them work as-is in this repository.

| Script | Status in this repo | What it does |
|--------|---------------------|--------------|
| `run_opencode_simple.sh` | Works | Builds a standalone `opencode-simple:latest` image (`node:20-slim` + `opencode-ai`) on first run and starts OpenCode. Needs only Docker. |
| `run_gemini_container.sh` | Works | Runs the Gemini CLI in a container (building a small `node:20-slim` image if needed), reusing host Gemini authentication. |
| `run_opencode_container.sh` | **Needs template-repo** | Runs OpenCode through an `openrouter-agents` compose service. That service and its `docker/openrouter-agents.Dockerfile` exist in template-repo, not here. |
| `run_crush_container.sh` | **Needs template-repo** | Runs Crush through the same `openrouter-agents` service. |

To use the template-repo-backed scripts, run them from a template-repo
checkout, or build the `template-repo-mcp-opencode`, `template-repo-mcp-crush`,
and `template-repo-openrouter-agents` images there first. See
[docs/maintainers.md](../../../docs/maintainers.md#docker-images) for how the
MCP images relate to this repo.

Host-side (non-container) launchers are in [`../agents/`](../agents/).

## Environment variables

- `OPENROUTER_API_KEY` is required by the OpenCode and Crush scripts. It is
  read from the environment or a `.env` file in the current directory.
  `run_opencode_simple.sh` prompts for it if it's missing.
- `OPENCODE_MODEL` sets the OpenCode model (default
  `qwen/qwen-2.5-coder-32b-instruct`).

## Usage

```bash
# Interactive
./run_opencode_simple.sh

# Single query
./run_opencode_simple.sh -q "Explain psp/src/alloc_impl.rs"

# Full-infrastructure variant (template-repo images required)
./run_opencode_container.sh -q "Refactor this code" -c existing_code.py
```

Each script prints its full option list with `-h`.

## Troubleshooting

- **API key errors:** get a key at https://openrouter.ai/keys, then run
  `export OPENROUTER_API_KEY=...` or add it to `.env`.
- **Permission errors:** make the scripts executable (`chmod +x *.sh`) and
  check that your user can reach the Docker daemon (`docker ps`).
- **`openrouter-agents` service not found:** see the table above. This repo
  doesn't define that service.
