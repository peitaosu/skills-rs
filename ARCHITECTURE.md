# Architecture

How **skills-rs** is structured, what each layer owns, and how to integrate the library into a host application.

For installation and quick start, see [README.md](README.md). For a line-by-line spec coverage checklist, see [SPEC.md](SPEC.md).

## Repository split

```
skills-rs/          Library crate — format + client lib + optional CLI
  src/
host/               Reference host app (`skills` binary)
  src/
examples/           Minimal library-only demo
tests/              Integration tests
agentskills/        Upstream spec submodule
```

| Layer | Crate | Owns |
|-------|-------|------|
| **Library** | `skills-rs` | Parse, validate, discover, catalog, activation payloads, filters, session, CLI tools |
| **Host app** | `skills` | LLM client, tool loop, permissions, script execution, compaction, install UI |

The library produces data (catalog text, activation content, diagnostics). The host decides where that data goes (system prompt, tool result, subagent context).

## Module map

```
src/
  parse.rs           Frontmatter + body parsing (strict and lenient YAML)
  validate.rs        Strict validation; orchestrates lenient runtime checks
  body.rs            Spec advisories: body length, file references
  script.rs          Spec advisories: scripts/ quality heuristics
  prompt.rs          <available_skills> XML (skills-ref parity)
  model.rs           SkillProperties, SkillDocument, SkillEntry, SkillScope
  error.rs           Errors and Diagnostic records
  filter.rs          SkillFilter trait and composable filters
  allowed_tools.rs   Read/Bash pattern matching for host enforcement
  discovery/         Filesystem scan, gitignore, remote registry, archives, merge
  catalog.rs         SkillCatalog, disclosure blocks, CatalogPlacement
  activate.rs        Load body, list resources, structured wrap
  session.rs         Activation tracking, deduplication, protection tags
  bin/skills-rs.rs   CLI (feature `cli`)

host/src/
  main.rs            REPL entrypoint
  host.rs            Bootstrap, discovery, tool wiring
  llm.rs             OpenAI-compatible agent and subagent turns
  subagent.rs        Multi-turn delegated sessions
  install.rs         Directory, archive, and git install
  context.rs         Conversation compaction
  settings.rs        Trust, filters, placement
  watch.rs           Filesystem reload via notify
```

Feature flags: `default = ["runtime"]`. `runtime` enables discovery through remote fetch. `cli` adds the `skills-rs` binary.

## Progressive disclosure

Agent Skills uses three tiers of context:

1. **Catalog** — `name`, `description`, optional `<location>` at session start (`SkillCatalog::disclosure_block`).
2. **Instructions** — full `SKILL.md` body on activation (`SkillActivator`).
3. **Resources** — paths under `scripts/`, `references/`, `assets/` listed at activation but not read until the model or host requests them.

## Core types

| Type | Role |
|------|------|
| `SkillProperties` | Parsed frontmatter: `name`, `description`, optional `license`, `compatibility`, `allowed_tools`, `metadata` |
| `SkillDocument` | Properties + markdown body + paths |
| `SkillEntry` | Discovered skill with `scope` (`Project` \| `User` \| `Organization` \| `Bundled`) |
| `SkillCatalog` | Name-indexed map; `to_prompt_xml()` returns `None` when empty |
| `SkillSource` | Discovery backend trait (`FilesystemDiscovery`, `RemoteRegistrySource`, …) |
| `SkillFilter` | Catalog/activation gating trait (`CompositeFilter`, `TrustedRootsFilter`, …) |
| `ActivatedSkill` | Activation payload: content, `skill_dir`, resource paths |
| `SkillSession` | Tracks activated names; deduplicates re-injection |

## Validation

Three modes serve different callers:

| Mode | Entry points | Use |
|------|--------------|-----|
| **Strict** | `validate`, `parse_frontmatter_strict` | CLI, CI, skills-ref parity |
| **Lenient** | `parse_frontmatter`, `validate_lenient`, discovery | Runtime load; warns instead of failing on cosmetic issues |
| **Advisories** | `validate_advisories`, `validate_body_advisories`, `validate_scripts_advisories` | Authoring guidance as warnings; never fail strict validation |

**Name policies:** `SkillsRef` (default, Unicode letters) or `SpecStrict` (ASCII-only, optional CI mode via `--name-policy spec`).

## Discovery

`DiscoveryConfig` defines scan roots per scope:

| Scope | Config field | Typical paths |
|-------|--------------|---------------|
| Project | `project_roots` | `.agents/skills/`, `.<client>/skills/`, `<project>/skills/` |
| User | `user_roots` | `~/.agents/skills/` |
| Organization | `organization_roots` | Org-config paths |
| Bundled | `bundled_roots` | App-shipped directories |

Notable options: ancestor scan to git root, XDG paths, `.claude/skills/` compat, gitignore (ancestor chain + global excludes), depth/count limits.

**Remote registry:** JSON manifest with local `path`, HTTPS `url`, or HTTPS `archive_url`. Archives supported: `.tar.gz`, `.tar`, `.zip`.

**Merge precedence:** project > organization > user > bundled. Same-scope collisions warn and last wins.

## Activation

| Mode | Catalog behavior | Host responsibility |
|------|------------------|---------------------|
| **File-read** | Includes `<location>`; behavioral text tells model to read `SKILL.md` | Host provides a file-read tool |
| **Dedicated tool** | Behavioral text references `activate_skill(name)` | Host registers tool; library returns body + resource listing |

`ActivationOptions` control frontmatter stripping, `<skill_content>` wrap, and resource listing caps.

## Host integration flow

```text
Session start:
  discover_and_merge_from(sources) → filter → SkillCatalog
  catalog.disclosure_block(mode) → system prompt or tool description

During loop:
  session.try_activate(entry, options, filter)
  host marks activation output protected from compaction
```

The reference host (`host/`, binary `skills`) implements REPL commands, OpenAI tool-calling (`agent`), multi-turn subagent, trust gating, install paths, and filesystem watch reload. See [SPEC.md §3](SPEC.md#3-host-app) for the full host checklist.

`examples/agent_loop.rs` is a minimal library-only demo (discover → catalog → activate one skill).

## CLI

The `skills-rs` binary (feature `cli`) mirrors skills-ref plus a Rust-only `discover` command:

```bash
skills-rs validate <skill-dir> [--name-policy skills-ref|spec]
skills-rs read-properties <skill-dir>
skills-rs to-prompt <skill-dir> ...
skills-rs discover [--project PATH] [--user PATH] [--bundled PATH] [--org PATH] ...
```

## Dependencies

Core: `serde`, `serde_yaml`, `serde_json`, `thiserror`, `unicode-normalization`.

Runtime: `walkdir`, `ignore`, `reqwest`, `flate2`, `tar`, `zip`.

CLI: `clap`.

The library intentionally excludes LLM SDKs, async runtimes, and prompt-template frameworks.
