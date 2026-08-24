/**
 * Context Quality Evaluation Provider for Promptfoo
 *
 * Exports a provider object with id() and callApi() methods.
 * Simulates the Coderun context engine behavior.
 */

/**
 * Mock context engine (simulates Coderun context building)
 */
function mockContextEngine(vars) {
  const task = vars.task || "";
  const max_tokens = vars.max_tokens || 12000;

  // Parse comma-separated strings
  const filesStr = vars.files_mentioned || "";
  const skillsStr = vars.skills_matched || "";
  const knowledgeStr = vars.knowledge_entries || "";

  const files = filesStr ? filesStr.split(",").map(s => s.trim()).filter(Boolean) : [];
  const skills = skillsStr ? skillsStr.split(",").map(s => {
    const [name, score] = s.split(":");
    return { name: name?.trim(), score: parseFloat(score) || 0.5 };
  }) : [];
  const knowledge = knowledgeStr ? knowledgeStr.split(",").map(s => {
    const [key, value] = s.split(":");
    return { key: key?.trim(), value: value?.trim() };
  }) : [];

  // Build mock context based on inputs
  const behavioral_skills = skills
    .filter(s => s.name)
    .map(s => `# ${s.name}\nScore: ${s.score}`)
    .join("\n\n");

  const docs_context = knowledge
    .filter(k => k.key)
    .map(k => `// ${k.key}: ${k.value}`)
    .join("\n");

  const code_context = files
    .map(f => `// ${f}\n// [file content]`)
    .join("\n\n");

  // Estimate tokens (rough: 1 token ≈ 4 chars)
  const total_content = behavioral_skills.length + docs_context.length + code_context.length;
  const total_tokens = Math.floor(total_content / 4);

  // Enforce budget
  const budget_remaining = Math.max(0, max_tokens - total_tokens);

  // Determine section ordering
  const has_skills = behavioral_skills.length > 0;
  const has_docs = docs_context.length > 0;
  const has_code = code_context.length > 0;

  return {
    behavioral_skills: behavioral_skills || "",
    docs_context: docs_context || "",
    code_context: code_context || "",
    token_usage: {
      total_tokens: Math.min(total_tokens, max_tokens),
      budget_remaining,
      by_source: {
        behavioral_skills: Math.floor(behavioral_skills.length / 4),
        docs_context: Math.floor(docs_context.length / 4),
        code_context: Math.floor(code_context.length / 4),
      },
    },
    // Metadata for assertions
    metadata: {
      has_skills,
      has_docs,
      has_code,
      task_length: task.length,
    },
  };
}

/**
 * Promptfoo provider
 */
module.exports = class ContextQualityProvider {
  id() {
    return "context-quality";
  }

  label = "Context Quality";

  async callApi(prompt, context) {
    const vars = context?.vars || {};
    const result = mockContextEngine(vars);
    return {
      output: JSON.stringify(result, null, 2),
    };
  }
};
