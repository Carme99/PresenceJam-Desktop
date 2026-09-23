# CLAUDE.md - DEPRECATED

This file is **deprecated**. The single source of truth for AI coding agents
(Claude Code, Codex, Cursor, aider, OpenClaw, omp, and any other tool that
reads context from the repo root) is:

> **[AGENTS.md](./AGENTS.md)**

It contains the full operating contract for working in this repository:
toolchain pins, layout, gates, authoring rules (Rust + frontend), i18n
contract, security/storage contract, CI gate map, things agents must not do,
the per-issue workflow recipe, and the symbol glossary.

If your tool reads only `CLAUDE.md`, point it at `AGENTS.md` explicitly. New
agents should default to `AGENTS.md` from the start. This stub remains in
the tree as a redirect; it will be removed in a follow-up once every external
tool that read it has been re-pointed.
