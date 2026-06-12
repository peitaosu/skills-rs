# skills-rs

Rust implementation of [Agent Skills](https://agentskills.io): parse and validate `SKILL.md` files, discover skills, build model-facing catalogs, and produce activation payloads for AI agent hosts.

## Quick start

Add the library:

```toml
[dependencies]
skills-rs = { path = "../skills-rs" }
```

Discover skills and build a catalog:

```rust
use skills_rs::{
    discover_and_merge, ActivationMode, CompositeFilter, DiscoveryConfig, SkillCatalog,
};

let (entries, _, _) = discover_and_merge(DiscoveryConfig {
    project_roots: vec![project_root],
    user_roots: vec![home.join(".agents/skills")],
    include_agents_convention: true,
    ..Default::default()
})?;

let catalog = SkillCatalog::from_entries_filtered(entries, &CompositeFilter::catalog_default());

if let Some(block) = catalog.disclosure_block(ActivationMode::default()) {
    // inject into system prompt or tool description
}
```

Run the reference host (requires `OPENAI_API_KEY` for LLM commands):

```bash
cargo run -p skills -- --project .
```

Validate a skill directory (skills-ref parity):

```bash
cargo run --features cli -- validate path/to/skill
cargo run --features cli -- to-prompt path/to/skill
```

Run tests:

```bash
cargo test --all-features
```

## Project layout

| Path | Crate | Role |
|------|-------|------|
| `src/` | `skills-rs` | Library: format, client lib, optional CLI |
| `host/` | `skills` | Reference host app (REPL, LLM loop, install) |
| `tests/` | — | Library integration tests |
| `examples/` | — | Minimal library usage (`agent_loop`) |
| `agentskills/` | — | Upstream spec submodule |

## Cargo features

| Feature | Default | Enables |
|---------|---------|---------|
| `runtime` | yes | Discovery, catalog, activation, session, filters, remote registry |
| `cli` | no | `skills-rs` binary (`validate`, `read-properties`, `to-prompt`, `discover`) |

Use `default-features = false` for parse/validate/prompt only (no filesystem or HTTP).

## Documentation

| Document | Purpose |
|----------|---------|
| [ARCHITECTURE.md](ARCHITECTURE.md) | Architecture, module map, integration patterns |
| [SPEC.md](SPEC.md) | Spec and client-guide implementation coverage |

## References

- [Agent Skills specification](https://github.com/agentskills/agentskills/blob/main/docs/specification.mdx)
- [Client implementation guide](https://github.com/agentskills/agentskills/blob/main/docs/client-implementation/adding-skills-support.mdx)
- [skills-ref](https://github.com/agentskills/agentskills/tree/main/skills-ref) (Python parity target)
- Local copies: `agentskills/` submodule

## License

MIT — see [LICENSE](LICENSE).
