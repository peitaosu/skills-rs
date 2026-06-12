# Spec coverage

Checklist mapping [Agent Skills](https://agentskills.io) requirements to this repository. Use this to audit spec and client-guide parity — not as end-user documentation.

| Document | Audience | Content |
|----------|----------|---------|
| [README.md](README.md) | New users | Quick start, layout, feature flags |
| [ARCHITECTURE.md](ARCHITECTURE.md) | Integrators, contributors | Architecture, types, flows |
| **SPEC.md** (this file) | Maintainers | Requirement → status → code location |

**Status key:** **done** · **partial** · **missing** · **—** (host/model behavior, not library code)

**Code locations:**

| Part | Path |
|------|------|
| Format + client lib | `src/` (`skills-rs` crate) |
| Host app | `host/` (`skills` binary) |

---

## 1. Format

Skill artifact definition. Target: strict parity with the [specification](agentskills/docs/specification.mdx) and [skills-ref](agentskills/skills-ref/) validator.

| Requirement | Source | Status | Implementation |
|-------------|--------|--------|----------------|
| Skill directory with required `SKILL.md` | Spec | done | `find_skill_md`, `validate` |
| Accept lowercase `skill.md` | skills-ref | done | Prefers `SKILL.md` |
| YAML frontmatter between `---` delimiters | Spec | done | `parse_frontmatter` |
| Markdown body after frontmatter | Spec | done | `read_document` |
| Parse `name` (required) | Spec | done | `SkillProperties` |
| Parse `description` (required) | Spec | done | |
| Parse `license` (optional) | Spec | done | |
| Parse `compatibility` (optional, max 500) | Spec | done | Rejects empty when present |
| Parse `metadata` map (string values) | Spec | done | Coerces numeric scalars to string |
| Parse `allowed-tools` (optional) | Spec | done | Parsed; host enforces via `AllowedToolsPolicy` |
| Strict: name length ≤ 64 | Spec | done | `validate` |
| Strict: lowercase name | Spec | done | `NameValidationPolicy::SkillsRef` (default) |
| Strict: ASCII-only names (optional) | — | done | `NameValidationPolicy::SpecStrict` |
| Strict: no leading/trailing/consecutive hyphens | Spec | done | |
| Strict: name matches directory | Spec | done | NFKC-normalized |
| Strict: description length ≤ 1024 | Spec | done | |
| Strict: reject unknown frontmatter fields | skills-ref | done | |
| Strict: path / missing file errors | skills-ref | done | |
| Optional dirs `scripts/`, `references/`, `assets/` | Spec | done | Listed at activation |
| Relative file references in body | Spec | done | `validate_body_advisories` |
| Progressive disclosure authoring guidance | Spec | done | Body advisories + lenient discovery |
| Script quality authoring guidance | Spec | done | `validate_scripts_advisories` (warnings only) |

Advisories are warnings only — they do not fail `validate()` or block discovery.

### Name validation policies

| Policy | Use | Behavior |
|--------|-----|----------|
| `SkillsRef` (default) | `validate()`, runtime discovery | Unicode letters, digits, hyphens |
| `SpecStrict` | `--name-policy spec` | ASCII `a-z`, `0-9`, hyphens only |

---

## 2. Client library

Reusable lifecycle: discover → parse → disclose → activate → session.

### 2.1 Parse and validate (runtime)

| Requirement | Source | Status | Implementation |
|-------------|--------|--------|----------------|
| Extract frontmatter + body | Client guide Step 2 | done | |
| Strict parse for CLI/CI | skills-ref | done | `parse_frontmatter_strict`, `validate` |
| Lenient YAML: quote values containing `:` | Client guide | done | `parse_frontmatter` |
| Lenient YAML: block-scalar retry | Client guide | done | `repair_block_scalar_values` |
| Lenient: warn on name/dir mismatch | Client guide | done | `validate_lenient` |
| Lenient: warn on name too long | Client guide | done | `validate_lenient` |
| Lenient: skip if description missing | Client guide | done | `merge_skills` filter |
| Lenient: skip if YAML unparseable | Client guide | done | |
| Collect `Diagnostic` records | Client guide | done | Returned from discovery |
| Surface diagnostics | Client guide | done | `format_diagnostics`, CLI `discover` |
| Skill record: `name`, `description`, `location` | Client guide | done | `SkillEntry` |
| Derive `skill_dir` from `location` | Client guide | done | |
| Read body at activation, not in catalog | Client guide | done | Body at activation; advisories at discovery |

### 2.2 Discovery

| Requirement | Source | Status | Implementation |
|-------------|--------|--------|----------------|
| Project / user / organization / bundled scope | Client guide Step 1 | done | `DiscoveryConfig` |
| Scan `.agents/skills/`, `.<client>/skills/` | Client guide | done | |
| Scan `<project>/skills/` | Extension | done | Project scope only |
| Scan `~/.agents/skills/` | Client guide | done | via `user_roots` |
| Scan `.claude/skills/` (compat) | Client guide | done | `include_claude_compat` (default off) |
| Ancestor directories up to git root | Client guide | done | `include_ancestor_scan` |
| XDG config directories | Client guide | done | `include_xdg` |
| User-configured extra scan paths | Client guide | done | `extra_roots` |
| Find subdirs containing `SKILL.md` | Client guide | done | Also accepts `skill.md` |
| Skip `.git`, `node_modules`, etc. | Client guide | done | `skip_dir_names` |
| Respect `.gitignore` | Client guide | done | Ancestor chain, excludes, global |
| Max depth / max directory bounds | Client guide | done | `max_depth`, `max_directories` |
| Merge precedence | Client guide | done | project > org > user > bundled |
| Collision warnings | Client guide | done | `CollisionWarning` |
| `SkillSource` trait | Client guide | done | |
| `FilesystemDiscovery` | Client guide | done | |
| `StaticSkillSource` | Client guide | done | |
| `MultiSkillSource` | Client guide | done | |
| `RemoteRegistrySource` | Client guide | done | Manifest paths, HTTPS url/archive, advisories |

### 2.3 Catalog and disclosure

| Requirement | Source | Status | Implementation |
|-------------|--------|--------|----------------|
| `<available_skills>` XML catalog | skills-ref | done | `to_prompt`, `SkillCatalog::to_prompt_xml` |
| XML-escape name and description | skills-ref | done | |
| Include `<location>` per skill | Client guide Step 3 | done | Absolute path to `SKILL.md` |
| Behavioral instructions (file-read mode) | Client guide Step 3 | done | `ActivationMode::FileRead` |
| Behavioral instructions (dedicated-tool mode) | Client guide Step 3 | done | `ActivationMode::DedicatedTool` |
| Combined `disclosure_block` | Client guide Step 3 | done | |
| Omit catalog when no skills | Client guide | done | Returns `None` |
| Empty `<available_skills>` for CLI | skills-ref | done | CLI parity |
| Catalog placement (prompt vs tool) | Client guide Step 3 | done | `CatalogPlacement` |
| `allowed-tools` enforcement helper | Spec | done | `AllowedToolsPolicy` |

### 2.4 Filtering

| Requirement | Source | Status | Implementation |
|-------------|--------|--------|----------------|
| `SkillFilter` trait | Client guide | done | |
| Hide `metadata.disable-model-invocation` | Client guide | done | `HideModelDisabled` |
| Trust gate for project skills | Client guide | done | `TrustedRootsFilter` |
| Composable filters | — | done | `CompositeFilter` |
| User disabled skills | Client guide | done | `DisabledSkillsFilter` |
| Permission-denied skills | Client guide | done | `PermissionDeniedFilter` |

### 2.5 Activation

| Requirement | Source | Status | Implementation |
|-------------|--------|--------|----------------|
| Load full instructions on activation | Client guide Step 4 | done | `SkillActivator` |
| Strip frontmatter (default) | Client guide | done | `ActivationOptions` |
| Return full file including frontmatter | Client guide | done | `strip_frontmatter: false` |
| `<skill_content>` structured wrap | Client guide | done | `WrapStyle::SkillContent` |
| List `scripts/`, `references/`, `assets/` | Client guide | done | `list_resources` |
| Do not eagerly read bundled files | Client guide | done | |
| Cap resource listing | Client guide | done | `max_listed_resources` |
| Note listing may be incomplete | Client guide | done | `resources_truncated` |
| Resolve paths under `skill_dir` only | Client guide | done | `resolve_under_skill_dir` |
| File-read activation support | Client guide | done | Catalog includes `<location>` |
| Dedicated-tool activation payload | Client guide | done | `SkillActivator` |
| Model-driven activation | Client guide | — | Host responsibility |

### 2.6 Session

| Requirement | Source | Status | Implementation |
|-------------|--------|--------|----------------|
| Track activated skill names | Client guide Step 5 | done | `SkillSession` |
| Deduplicate re-activation | Client guide Step 5 | done | `try_activate` |
| Protection tags for compaction | Client guide Step 5 | done | `protection_tags()` |

### 2.7 CLI

| Requirement | Source | Status | Implementation |
|-------------|--------|--------|----------------|
| `validate <skill-dir>` | skills-ref | done | `--name-policy`; prints advisories on success |
| `read-properties <skill-dir>` | skills-ref | done | JSON output |
| `to-prompt <skill-dir>...` | skills-ref | done | Propagates parse errors |
| `discover` | Extension | done | `--project`, `--user`, `--bundled`, `--org`, … |
| Accept path to `SKILL.md` directly | skills-ref | done | |

### 2.8 Tests

| Requirement | Source | Status | Implementation |
|-------------|--------|--------|----------------|
| skills-ref validator/parser tests | skills-ref | done | `tests/integration.rs` |
| Client-guide behavior tests | Client guide | done | gitignore, scopes, filters, i18n, advisories |
| Minimal library example | — | done | `examples/agent_loop.rs` |

---

## 3. Host app

Reference REPL at `host/` (binary `skills`). Demonstrates client-guide host patterns; not a production agent.

| Requirement | Source | Lib hook | Host |
|-------------|--------|----------|------|
| LLM API client | Client guide | — | OpenAI-compatible (`OPENAI_API_KEY`) |
| Register `activate_skill` tool | Client guide Step 4 | `SkillCatalog::names()` | `agent` command (function-calling) |
| Constrain tool `name` to enum | Client guide Step 4 | `SkillCatalog::names()` | yes |
| Omit tool when no skills | Client guide Step 4 | `SkillCatalog::is_empty()` | yes |
| Model-driven skill selection | Client guide | — | `llm` / `llm-run` / `agent` |
| File-read tool for resources | Client guide | `<location>` in catalog | `read` + `read_file` in agent loop |
| Execute scripts in `scripts/` | Spec | — | `run` (bash/python/node) |
| Enforce `allowed-tools` | Spec | `AllowedToolsPolicy` | gates read/run |
| User-explicit activation (`/skill-name`) | Client guide | `SkillActivator` | yes |
| Skill autocomplete | Client guide | `SkillCatalog::names()` | `complete` |
| User disable / enable skills | Client guide | `SkillFilter` | `disable` / `enable` |
| Diagnostics command | Client guide | `Diagnostic` | `diagnostics`, startup |
| Trust prompt for project skills | Client guide | `TrustedRootsFilter` | `trust on\|off` |
| Allowlist skill dirs for reads | Client guide | `SkillEntry::skill_dir` | yes |
| Activation consent | Client guide Step 4 | — | `activate --yes` |
| Permission-denied filter | Client guide | `SkillFilter` | `deny` |
| Inject catalog into system prompt | Client guide Step 3 | `disclosure_block()` | `placement prompt` |
| Catalog in tool description | Client guide Step 3 | `to_prompt_xml()` | `placement tool` |
| Context compaction | Client guide Step 5 | `protection_tags()` | `compact` |
| Subagent delegation | Client guide Step 5 | `SkillSession` | multi-turn subagent |
| Activation analytics | Client guide Step 4 | — | `analytics` |
| Remote registry provisioning | Client guide | `RemoteRegistrySource` | `registry/manifest.json` |
| Bundled skill provisioning | Client guide | bundled roots | `host/skills/bundled/` |
| Install skill directory | Client guide | — | `install <dir>` |
| Install from archive | Client guide | `unpack_skill_archive` | `install-archive` |
| Git clone install | Client guide | — | `install-git` |
| Reload on file change | Client guide | lib re-reads on reload | `watch on` (notify) |

Host LLM env: `OPENAI_API_KEY`, optional `OPENAI_BASE_URL`, `OPENAI_MODEL`.
