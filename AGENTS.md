# Agent Instructions

- Do not use `mgrep`. Use `lgrep` for semantic search.
- If there is no `lgrep` index for this repo, ask the user whether they want one before indexing.
- For literal text/file searches, use `rg` / `rg --files`.
- Use `tmux` for long-running processes (servers, watchers, daemons).
