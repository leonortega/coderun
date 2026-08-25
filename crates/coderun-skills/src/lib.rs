use std::path::{Path, PathBuf};

use coderun_core::SkillMatch;
use tracing::{debug, info, warn};

// ── Skill Definition — canonical normalized schema (TASK-015)
// All sources (Claude, Cursor, Continue, agentskills.io) normalize into this one format.
// TASK-016: priority/specificity/max_active/conflict handling
#[derive(Debug, Clone)]
pub struct Skill {
    pub name: String,
    pub tags: Vec<String>,
    pub instructions: String,
    pub examples: Vec<String>,
    pub constraints: Vec<String>,
    pub description: String,
    /// Priority (higher = more specific/preferred) — deterministic ordering (TASK-016)
    pub priority: u8,
    /// Specificity score (tags length + name match) — computed at match time
    pub specificity: f64,
}

// ── Skill Engine ────────────────────────────────────────────────────────

pub struct SkillEngine {
    skills: Vec<Skill>,
    skills_dir: PathBuf,
}

impl SkillEngine {
    /// Create a new Skill Engine
    pub fn new(skills_dir: PathBuf) -> Self {
        Self {
            skills: Vec::new(),
            skills_dir,
        }
    }

    /// Create engine from pre-loaded skills (v0.6.0 collapse duplicate scorer)
    pub fn from_skills(skills: Vec<Skill>) -> Self {
        Self { skills, skills_dir: PathBuf::from(".") }
    }

    /// Load all skills from the skills directory
    pub fn load_skills(&mut self) -> Result<usize, String> {
        self.skills.clear();
        let mut seen = std::collections::HashSet::new();
        let count = load_skills_in_dir(&self.skills_dir, &mut self.skills, &mut seen)?;
        info!(count = count, "Loaded skills");
        Ok(count)
    }

    /// Load skills from multiple directories in priority order — first occurrence of a
    /// skill name wins. Lets callers merge e.g. repo-local `.coderun/skills` with the
    /// bundled global `~/.coderun/skills` library without duplicates.
    pub fn load_skills_from_dirs(&mut self, dirs: &[PathBuf]) -> Result<usize, String> {
        self.skills.clear();
        let mut seen = std::collections::HashSet::new();
        let mut count = 0;
        for dir in dirs {
            if dir.is_dir() {
                count += load_skills_in_dir(dir, &mut self.skills, &mut seen)?;
            }
        }
        info!(count = count, dirs = dirs.len(), "Loaded skills (multi-dir)");
        Ok(count)
    }

    /// Match skills against a task description
    pub fn match_skills(&self, task_description: &str, max_skills: usize) -> Vec<SkillMatch> {
        let task_tokens = tokenize(task_description);
        let mut scored: Vec<(f64, &Skill)> = self
            .skills
            .iter()
            .map(|skill| {
                let score = compute_match_score(&task_tokens, skill);
                (score, skill)
            })
            .filter(|(score, _)| *score > 0.3)
            .collect();

        // TASK-015/016: sort by priority (higher first) then score descending before take(max_skills)
        scored.sort_by(|a, b| b.1.priority.cmp(&a.1.priority).then(b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal)));

        // Take top N
        scored
            .into_iter()
            .take(max_skills)
            .map(|(score, skill)| SkillMatch {
                skill_name: skill.name.clone(),
                match_score: score,
                instructions: skill.instructions.clone(),
                examples: skill.examples.clone(),
                constraints: skill.constraints.clone(),
            })
            .collect()
    }

    /// Reload skills from disk
    pub fn reload_skills(&mut self) -> Result<usize, String> {
        self.load_skills()
    }

    /// List all loaded skill names
    pub fn list_skills(&self) -> Vec<String> {
        self.skills.iter().map(|s| s.name.clone()).collect::<Vec<_>>()
    }

    /// Get all loaded skills
    pub fn get_skills(&self) -> &[Skill] {
        &self.skills
    }

    /// Detect conflicting constraints between matched skills
    pub fn detect_conflicts(&self, matches: &[SkillMatch]) -> Vec<String> {
        let mut conflicts = Vec::new();
        let all_constraints: Vec<(&str, &str)> = matches
            .iter()
            .flat_map(|m| {
                m.constraints
                    .iter()
                    .map(move |c| (c.as_str(), m.skill_name.as_str()))
            })
            .collect();

        for i in 0..all_constraints.len() {
            for j in (i + 1)..all_constraints.len() {
                let (c1, s1) = all_constraints[i];
                let (c2, s2) = all_constraints[j];
                if c1 != c2 && constraints_conflict(c1, c2) {
                    conflicts.push(format!(
                        "Skills '{}' and '{}' have conflicting constraints: '{}' vs '{}'",
                        s1, s2, c1, c2
                    ));
                }
            }
        }

        conflicts
    }
}

// ── Parsers ─────────────────────────────────────────────────────────────

/// Load every skill bundle/file in `dir` into `skills`, skipping names already present in
/// `seen` (first directory in priority order wins). Supports flat `.md`/`.toml`/`.yaml`
/// files and `<name>/SKILL.md` bundles (TASK-037b).
fn load_skills_in_dir(
    dir: &Path,
    skills: &mut Vec<Skill>,
    seen: &mut std::collections::HashSet<String>,
) -> Result<usize, String> {
    if !dir.exists() {
        debug!(path = %dir.display(), "Skills directory not found");
        return Ok(0);
    }

    let mut count = 0;
    let entries: Vec<_> = std::fs::read_dir(dir)
        .map_err(|e| format!("Failed to read skills directory: {}", e))?
        .filter_map(|e| e.ok())
        .collect();

    for entry in entries {
        let path = entry.path();

        // Skill bundles live at <skills>/<name>/SKILL.md (Claude-style folders)
        if path.is_dir() {
            let skill_file_md = path.join("SKILL.md");
            let skill_file = if skill_file_md.exists() {
                skill_file_md
            } else {
                let lower = path.join("skill.md");
                if lower.exists() { lower } else { continue }
            };
            match parse_markdown_skill(&skill_file) {
                Ok(skill) => {
                    if seen.insert(skill.name.to_lowercase()) {
                        debug!(skill = %skill.name, path = %skill_file.display(), "Loaded skill bundle");
                        skills.push(skill);
                        count += 1;
                    }
                }
                Err(e) => {
                    warn!(path = %skill_file.display(), error = %e, "Failed to load skill bundle");
                }
            }
            continue;
        }

        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");

        let result = match ext {
            "md" => parse_markdown_skill(&path),
            "toml" => parse_toml_skill(&path),
            "yaml" | "yml" => parse_yaml_skill(&path),
            _ => continue,
        };

        match result {
            Ok(skill) => {
                if seen.insert(skill.name.to_lowercase()) {
                    debug!(skill = %skill.name, path = %path.display(), "Loaded skill");
                    skills.push(skill);
                    count += 1;
                }
            }
            Err(e) => {
                warn!(path = %path.display(), error = %e, "Failed to load skill");
            }
        }
    }

    Ok(count)
}

/// Parse a Markdown skill file
///
/// Expected format:
/// ```markdown
/// # Skill Name
///
/// ## Tags
/// tag1, tag2, tag3
///
/// ## Instructions
/// Instructions text here...
///
/// ## Examples
/// - Example 1
/// - Example 2
///
/// ## Constraints
/// - Constraint 1
/// - Constraint 2
/// ```
fn parse_markdown_skill(path: &Path) -> Result<Skill, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;

    // TASK-037b: support the Claude-style bundle format — YAML front matter with
    // `name:` / `description:` (+ optional `tags:`) followed by a markdown body.
    if content.trim_start().starts_with("---") {
        return parse_front_matter_skill(path, &content);
    }

    let mut name = String::new();
    let mut tags = Vec::new();
    let mut instructions = String::new();
    let mut examples = Vec::new();
    let mut constraints = Vec::new();
    let mut current_section = String::new();

    for line in content.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with("# ") && !trimmed.starts_with("## ") {
            name = trimmed[2..].trim().to_string();
            continue;
        }

        if let Some(section) = trimmed.strip_prefix("## ") {
            current_section = section.trim().to_lowercase();
            continue;
        }

        match current_section.as_str() {
            "tags" => {
                if !trimmed.is_empty() {
                    tags = trimmed
                        .split(',')
                        .map(|t| t.trim().to_lowercase())
                        .filter(|t| !t.is_empty())
                        .collect();
                }
            }
            "instructions" => {
                if !trimmed.is_empty() {
                    if !instructions.is_empty() {
                        instructions.push('\n');
                    }
                    instructions.push_str(trimmed);
                }
            }
            "examples" => {
                if let Some(item) = trimmed.strip_prefix("- ") {
                    examples.push(item.to_string());
                } else if !trimmed.is_empty() {
                    examples.push(trimmed.to_string());
                }
            }
            "constraints" => {
                if let Some(item) = trimmed.strip_prefix("- ") {
                    constraints.push(item.to_string());
                } else if !trimmed.is_empty() {
                    constraints.push(trimmed.to_string());
                }
            }
            _ => {}
        }
    }

    if name.is_empty() {
        return Err("Missing skill name (# heading)".to_string());
    }
    if tags.is_empty() {
        return Err("Missing tags section".to_string());
    }
    if instructions.is_empty() {
        return Err("Missing instructions section".to_string());
    }

    // TASK-016: priority = tags.len() as specificity, higher tags = more specific
    let priority = tags.len() as u8;
    let specificity = tags.len() as f64 / 5.0;
    Ok(Skill {
        name,
        tags,
        instructions,
        examples,
        constraints,
        description: String::new(),
        priority,
        specificity,
    })
}

/// Parse a Claude-style skill bundle: YAML front matter (`name`, `description`, optional
/// `tags`) followed by a markdown body (TASK-037b — the shipped skills library format).
fn parse_front_matter_skill(_path: &Path, content: &str) -> Result<Skill, String> {
    let mut name = String::new();
    let mut description = String::new();
    let mut tags: Vec<String> = Vec::new();

    // Split front matter from body on the second `---` line
    let after_leading = content.trim_start().trim_start_matches("---\n").trim_start_matches("---\r\n");
    let (fm_block, body) = match after_leading.find("\n---") {
        Some(idx) => {
            let fm = &after_leading[..idx];
            let rest = &after_leading[idx..];
            // skip the closing delimiter line itself
            let body_start = rest.find('\n').map(|i| i + 1).unwrap_or(rest.len());
            (fm, rest[body_start..].to_string())
        }
        None => (after_leading, String::new()),
    };

    let mut in_block_scalar = false;
    for line in fm_block.lines() {
        let indented = line.starts_with(' ') || line.starts_with('\t');
        let trimmed = line.trim();
        if !indented {
            in_block_scalar = false;
            if let Some(v) = trimmed.strip_prefix("name:") {
                name = v.trim().trim_matches('"').trim_matches('\'').to_string();
            } else if let Some(v) = trimmed.strip_prefix("tags:") {
                tags = v
                    .split(|c| c == ',' || c == ';')
                    .map(|t| t.trim().to_lowercase())
                    .filter(|t| !t.is_empty())
                    .collect();
            } else if trimmed.starts_with("description:") {
                let v = trimmed["description:".len()..].trim().trim_matches('"').trim_matches('\'');
                if matches!(v, ">-" | ">" | "|-" | "|" | "") {
                    // YAML block scalar — value continues on following indented lines
                    in_block_scalar = true;
                } else {
                    description = v.to_string();
                }
            }
        } else if in_block_scalar && !trimmed.is_empty() && !looks_like_key(trimmed) {
            // Strip stray block-scalar indicators inside continued lines
            let cleaned = trimmed
                .strip_prefix(">-\n")
                .or_else(|| trimmed.strip_prefix(">"))
                .unwrap_or(trimmed)
                .trim_start_matches(['>', '-', '|'])
                .trim();
            if !cleaned.is_empty() {
                if !description.is_empty() {
                    description.push(' ');
                }
                description.push_str(cleaned);
            }
        }
    }

    if name.is_empty() {
        return Err("Missing skill name in front matter".to_string());
    }
    // Derive tags from the skill name when the bundle has no explicit tags so tag-overlap
    // matching still works ("kubernetes-manifest-authoring" → kubernetes, manifest, authoring)
    if tags.is_empty() {
        tags = tokenize(&name.replace(['-', '_'], " "));
    }
    if tags.is_empty() {
        return Err("No tags derivable for skill".to_string());
    }
    // Instructions: the markdown body; fall back to the description when the body is empty
    let instructions_body = body.trim();
    let instructions = if instructions_body.is_empty() { description.clone() } else { format!("{description}\n\n{instructions_body}") };
    if instructions.trim().is_empty() {
        return Err("Empty skill body".to_string());
    }

    let priority = tags.len() as u8;
    let specificity = tags.len() as f64 / 5.0;
    Ok(Skill {
        name,
        tags,
        instructions,
        examples: Vec::new(),
        constraints: Vec::new(),
        description,
        priority,
        specificity,
    })
}

/// Heuristic: an indented front-matter line that itself looks like a new top-level key
/// (`foo: bar` with no space before the colon) ends the current block scalar.
fn looks_like_key(trimmed: &str) -> bool {
    match trimmed.find(':') {
        Some(idx) => {
            let head = &trimmed[..idx];
            !head.is_empty()
                && head.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        }
        None => false,
    }
}

/// Parse a TOML skill file
fn parse_toml_skill(path: &Path) -> Result<Skill, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;

    #[derive(serde::Deserialize)]
    struct TomlSkill {
        name: String,
        tags: Vec<String>,
        instructions: String,
        #[serde(default)]
        examples: Vec<String>,
        #[serde(default)]
        constraints: Vec<String>,
        #[serde(default)]
        description: String,
    }

    let skill: TomlSkill =
        toml::from_str(&content).map_err(|e| format!("TOML parse error: {}", e))?;

    if skill.name.is_empty() {
        return Err("Empty skill name".to_string());
    }
    if skill.tags.is_empty() {
        return Err("No tags defined".to_string());
    }

    // TASK-016: priority from TOML, default tags.len()
    let tags_norm: Vec<String> = skill.tags.into_iter().map(|t| t.to_lowercase()).collect();
    let priority = tags_norm.len() as u8;
    let specificity = priority as f64 / 5.0;
    Ok(Skill {
        name: skill.name,
        tags: tags_norm,
        instructions: skill.instructions,
        examples: skill.examples,
        constraints: skill.constraints,
        description: skill.description,
        priority,
        specificity,
    })
}

/// Parse a YAML skill file
fn parse_yaml_skill(path: &Path) -> Result<Skill, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;

    #[derive(serde::Deserialize)]
    struct YamlSkill {
        name: String,
        tags: Vec<String>,
        instructions: String,
        #[serde(default)]
        examples: Vec<String>,
        #[serde(default)]
        constraints: Vec<String>,
        #[serde(default)]
        description: String,
    }

    let skill: YamlSkill =
        serde_yaml::from_str(&content).map_err(|e| format!("YAML parse error: {}", e))?;

    if skill.name.is_empty() {
        return Err("Empty skill name".to_string());
    }
    if skill.tags.is_empty() {
        return Err("No tags defined".to_string());
    }

    let tags_norm: Vec<String> = skill.tags.into_iter().map(|t| t.to_lowercase()).collect();
    let prio = tags_norm.len() as u8;
    let spec = prio as f64 / 5.0;
    Ok(Skill {
        name: skill.name,
        tags: tags_norm,
        instructions: skill.instructions,
        examples: skill.examples,
        constraints: skill.constraints,
        description: skill.description,
        priority: prio,
        specificity: spec,
    })
}

// ── Matching ────────────────────────────────────────────────────────────

/// Tokenize a string into lowercase words
fn tokenize(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(|s| s.to_string())
        .collect()
}

/// Compute match score between a task and a skill
fn compute_match_score(task_tokens: &[String], skill: &Skill) -> f64 {
    let task_set: std::collections::HashSet<&str> =
        task_tokens.iter().map(|s| s.as_str()).collect();

    // Tag overlap score
    let tag_matches = skill
        .tags
        .iter()
        .filter(|tag| task_set.contains(tag.as_str()))
        .count();

    let tag_score = if skill.tags.is_empty() {
        0.0
    } else {
        tag_matches as f64 / skill.tags.len() as f64
    };

    // Category bonus: check if skill name appears in task
    let name_bonus = if task_set.contains(skill.name.to_lowercase().as_str()) {
        1.2
    } else {
        1.0
    };

    tag_score * name_bonus
}

/// Check if two constraints conflict with each other
fn constraints_conflict(c1: &str, c2: &str) -> bool {
    let c1_lower = c1.to_lowercase();
    let c2_lower = c2.to_lowercase();

    // Simple conflict detection: one says "do X" and the other says "don't do X"
    let negation_pairs = [
        ("must", "must not"),
        ("should", "should not"),
        ("always", "never"),
        ("use", "don't use"),
        ("prefer", "avoid"),
    ];

    for (positive, negative) in &negation_pairs {
        if (c1_lower.contains(positive) && c2_lower.contains(negative))
            || (c1_lower.contains(negative) && c2_lower.contains(positive))
        {
            // Check if they're about the same topic
            let words1: Vec<&str> = c1_lower.split_whitespace().collect();
            let words2: Vec<&str> = c2_lower.split_whitespace().collect();
            let overlap = words1
                .iter()
                .filter(|w| words2.contains(w) && w.len() > 3)
                .count();
            if overlap > 0 {
                return true;
            }
        }
    }

    false
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn create_test_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "coderun_skills_test_{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// TASK-037b: <name>/SKILL.md bundles with YAML front matter must load and match
    #[test]
    fn test_loads_front_matter_skill_bundles_from_subdirs() {
        let dir = create_test_dir();
        let bundle = dir.join("kubernetes-manifest-authoring");
        fs::create_dir_all(&bundle).unwrap();
        fs::write(
            bundle.join("SKILL.md"),
            "---\nname: kubernetes-manifest-authoring\ndescription: >-\n  Write and maintain Kubernetes manifests\n  following best practices.\n---\n\n# Body\n\nDeployment yaml guidance.",
        )
        .unwrap();
        let flat = dir.join("legacy-style.md");
        fs::write(
            &flat,
            "# Legacy Skill\n\n## Tags\nrust, cargo\n\n## Instructions\nDo rust things.\n",
        )
        .unwrap();

        let mut engine = SkillEngine::new(dir.clone());
        let count = engine.load_skills().unwrap();
        assert_eq!(count, 2, "both bundle and flat skills should load");

        // Tag-overlap matching works via name-derived tags
        let matches = engine.match_skills("help me write kubernetes manifests", 5);
        assert!(!matches.is_empty(), "front-matter bundle should match by derived tags");
        assert!(matches[0].skill_name.contains("kubernetes"));
        cleanup(&dir);
    }

    fn cleanup(dir: &Path) {
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn test_parse_markdown_skill() {
        let dir = create_test_dir();
        let skill_path = dir.join("test-skill.md");
        fs::write(
            &skill_path,
            r#"# Rust Expert

## Tags
rust, cargo, async, ownership

## Instructions
You are a Rust expert. Help with ownership, borrowing, and lifetimes.

## Examples
- Use `&str` instead of `&String`
- Prefer `iter()` over `into_iter()` when possible

## Constraints
- Always use `thiserror` for error types
- Never use `unwrap()` in production code
"#,
        )
        .unwrap();

        let skill = parse_markdown_skill(&skill_path).unwrap();
        assert_eq!(skill.name, "Rust Expert");
        assert_eq!(skill.tags, vec!["rust", "cargo", "async", "ownership"]);
        assert!(skill.instructions.contains("Rust expert"));
        assert_eq!(skill.examples.len(), 2);
        assert_eq!(skill.constraints.len(), 2);

        cleanup(&dir);
    }

    #[test]
    fn test_parse_toml_skill() {
        let dir = create_test_dir();
        let skill_path = dir.join("test-skill.toml");
        fs::write(
            &skill_path,
            r#"
name = "Python Expert"
tags = ["python", "django", "fastapi"]
instructions = "You are a Python expert."
examples = ["Use type hints", "Prefer dataclasses"]
constraints = ["Always use snake_case"]
"#,
        )
        .unwrap();

        let skill = parse_toml_skill(&skill_path).unwrap();
        assert_eq!(skill.name, "Python Expert");
        assert_eq!(skill.tags, vec!["python", "django", "fastapi"]);

        cleanup(&dir);
    }

    #[test]
    fn test_parse_yaml_skill() {
        let dir = create_test_dir();
        let skill_path = dir.join("test-skill.yaml");
        fs::write(
            &skill_path,
            r#"
name: "Go Expert"
tags: ["go", "goroutines", "channels"]
instructions: "You are a Go expert."
examples:
  - "Use errgroup for concurrency"
  - "Prefer context for cancellation"
constraints:
  - "Always check errors"
"#,
        )
        .unwrap();

        let skill = parse_yaml_skill(&skill_path).unwrap();
        assert_eq!(skill.name, "Go Expert");
        assert_eq!(skill.tags, vec!["go", "goroutines", "channels"]);

        cleanup(&dir);
    }

    #[test]
    fn test_skill_matching() {
        let dir = create_test_dir();
        fs::write(
            dir.join("rust.md"),
            r#"# Rust Expert
## Tags
rust, cargo, ownership, borrowing
## Instructions
Rust expert instructions.
## Examples
- Example 1
## Constraints
- Always use thiserror
"#,
        )
        .unwrap();
        fs::write(
            dir.join("python.md"),
            r#"# Python Expert
## Tags
python, django, fastapi
## Instructions
Python expert instructions.
## Examples
- Example 1
## Constraints
- Use type hints
"#,
        )
        .unwrap();

        let mut engine = SkillEngine::new(dir.clone());
        engine.load_skills().unwrap();

        let matches = engine.match_skills("help with rust ownership", 5);
        assert!(!matches.is_empty());
        assert_eq!(matches[0].skill_name, "Rust Expert");
        assert!(matches[0].match_score > 0.3);

        cleanup(&dir);
    }

    #[test]
    fn test_skill_matching_no_match() {
        let dir = create_test_dir();
        fs::write(
            dir.join("rust.md"),
            r#"# Rust Expert
## Tags
rust, cargo
## Instructions
Instructions.
## Examples
- Example
## Constraints
- Constraint
"#,
        )
        .unwrap();

        let mut engine = SkillEngine::new(dir.clone());
        engine.load_skills().unwrap();

        let matches = engine.match_skills("cook a meal", 5);
        assert!(matches.is_empty());

        cleanup(&dir);
    }

    #[test]
    fn test_list_skills() {
        let dir = create_test_dir();
        fs::write(
            dir.join("a.md"),
            r#"# Skill A
## Tags
a
## Instructions
Instructions.
## Examples
- Example
## Constraints
- Constraint
"#,
        )
        .unwrap();
        fs::write(
            dir.join("b.md"),
            r#"# Skill B
## Tags
b
## Instructions
Instructions.
## Examples
- Example
## Constraints
- Constraint
"#,
        )
        .unwrap();

        let mut engine = SkillEngine::new(dir.clone());
        engine.load_skills().unwrap();

        let names = engine.list_skills();
        assert_eq!(names.len(), 2);
        assert!(names.contains(&"Skill A".to_string()));
        assert!(names.contains(&"Skill B".to_string()));

        cleanup(&dir);
    }

    #[test]
    fn test_tokenize() {
        let tokens = tokenize("Hello, World! This is a test.");
        assert_eq!(
            tokens,
            vec!["hello", "world", "this", "is", "a", "test"]
        );
    }

    #[test]
    fn test_constraint_conflict_detection() {
        let dir = create_test_dir();
        fs::write(
            dir.join("a.md"),
            r#"# Skill A
## Tags
a
## Instructions
Instructions.
## Examples
- Example
## Constraints
- Always use semicolons
"#,
        )
        .unwrap();
        fs::write(
            dir.join("b.md"),
            r#"# Skill B
## Tags
b
## Instructions
Instructions.
## Examples
- Example
## Constraints
- Never use semicolons
"#,
        )
        .unwrap();

        let mut engine = SkillEngine::new(dir.clone());
        engine.load_skills().unwrap();

        let matches = vec![
            SkillMatch {
                skill_name: "Skill A".to_string(),
                match_score: 0.8,
                instructions: "".to_string(),
                examples: vec![],
                constraints: vec!["Always use semicolons".to_string()],
            },
            SkillMatch {
                skill_name: "Skill B".to_string(),
                match_score: 0.7,
                instructions: "".to_string(),
                examples: vec![],
                constraints: vec!["Never use semicolons".to_string()],
            },
        ];

        let conflicts = engine.detect_conflicts(&matches);
        assert_eq!(conflicts.len(), 1);
        assert!(conflicts[0].contains("conflicting"));

        cleanup(&dir);
    }
}
