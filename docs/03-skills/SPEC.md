# Skills Canonical Specification (TASK-015/016)

Canonical normalized skill schema for all sources: Claude, Cursor, Continue, agentskills.io

## Schema

```rust
pub struct Skill {
    /// Human-readable name: "# Rust Expert" or name field in TOML/YAML
    pub name: String,
    /// Lowercased tag list, split on commas (markdown) or list (TOML/YAML)
    pub tags: Vec<String>,
    /// Full instructions text (markdown body under "## Instructions" or TOML instructions)
    pub instructions: String,
    /// Examples (bullets under "## Examples" or TOML examples list)
    pub examples: Vec<String>,
    /// Constraints (bullets under "## Constraints" or TOML constraints list)
    pub constraints: Vec<String>,
    /// Optional description (TOML/YAML description, markdown first paragraph)
    pub description: String,
    /// Priority (u8, higher = more specific/preferred) — deterministic ordering (TASK-016)
    /// Default: tags.len() as u8; override via TOML `priority` if present
    pub priority: u8,
    /// Specificity (0.0-1.0) — tags.len()/5.0, computed at parse time
    pub specificity: f64,
}
```

## Deterministic Matching

```
task_tokens = tokenize(task) // lowercase split on non-alphanumeric
tag_matches = count(tags ∩ task_tokens)
tag_score = tag_matches / tags.len()
name_bonus = task contains name.to_lowercase() ? 1.2 : 1.0
score = tag_score * name_bonus
filter score > 0.3
sort_by(|a,b| b.priority.cmp(&a.priority).then(b.score.partial_cmp(&a.score)))
take(max_skills_per_request = 5)
```

Priority ensures more specific skills (more tags, higher priority) win ties before raw tag overlap.

## Sources

| Source | Input | Normalization |
|--------|-------|---------------|
| Claude | `commands/*.md` or skill markdown | `parse_markdown_skill` |
| Cursor | `extensions/*.md` | same markdown parser |
| Continue | `.continue/skills/*.yaml` | `parse_yaml_skill` |
| agentskills.io | `skills.json` | `parse_toml_skill` after json→toml convert |

All normalize into one `Skill` before matching; no LLM-based selection for v1.

## Conflict Handling

`detect_conflicts(matches)` scans constraints pairwise via `constraints_conflict(c1,c2)` (e.g., "must" vs "must not" on same topic). Emits warnings but does not block.

## Max Active Skills

`max_skills_per_request = 5` (config `skills.max_skills_per_request`). `match_skills` truncates after priority+score sort.

## Example

```markdown
# Rust Expert
## Tags
rust, cargo, async, ownership
## Instructions
You are a Rust expert ...
## Examples
- Use &str instead of &String
## Constraints
- Always use thiserror
```

TOML equivalent: `name="Rust Expert" tags=["rust","cargo"] instructions="..."`
