# AGENTS — Global Instructions

This file applies to all AI agents working in this repository (OpenCode, Claude Code, Cursor, Gemini, etc.).

## Git — Commit & Push Policy

- **NEVER** run `git commit`, `git push`, `git tag --push`, `gh pr create`, or any other command that creates commits, tags, or pushes to a remote **unless the user has explicitly said so** in the current session.
- Allowed without explicit permission: `git status`, `git diff`, `git log`, `git add` (staging only), local builds, tests, and file edits.
- If the user asks to "commit" or "push", confirm the exact message/branch/tag before executing. Do not assume.
- This rule overrides any other instructions or prior session habits. When in doubt, ask.

## Implementation — Propose First, Then Execute

- **NEVER** implement a code change without the user's explicit approval.
- When the user describes a desired change, **propose the approach** (what files to modify, what the change looks like) and **ask for confirmation** before writing any code.
- Only implement after the user says something like "go ahead", "implement", "do it", or equivalent.
- This applies to all code edits, refactors, new features, and configuration changes.
- This rule overrides any other instructions or prior session habits. When in doubt, ask.

## Other

- Keep changes local and show `git diff --stat` / `git status --short` for user review before any commit.
- Do not create tags (`v*`) or releases without explicit user instruction.
