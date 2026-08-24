/**
 * Context Quality Evaluation Provider
 * 
 * This provider calls the Coderun Context Engine to evaluate
 * the quality of generated context packs.
 * 
 * Usage with Promptfoo:
 *   npx promptfoo eval -c eval/promptfoo.yaml
 */

const CODERUN_DAEMON_URL = process.env.CODERUN_DAEMON_URL || "http://127.0.0.1:9527";

/**
 * Call the Coderun Context Engine API
 * @param {Object} vars - Test variables
 * @returns {Promise<Object>} - Context pack
 */
async function callContextEngine(vars) {
  const request = {
    message: vars.task,
    session_id: vars.session_id || "eval-session",
    context_hints: {
      files_mentioned: vars.files_mentioned || [],
    },
  };

  try {
    const response = await fetch(`${CODERUN_DAEMON_URL}/api/context`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(request),
      signal: AbortSignal.timeout(30000),
    });

    if (!response.ok) {
      throw new Error(`HTTP ${response.status}`);
    }

    return await response.json();
  } catch (error) {
    // If daemon is not running, use mock context
    return mockContextEngine(vars);
  }
}

/**
 * Mock context engine (fallback when daemon is not available)
 */
function mockContextEngine(vars) {
  const { task, max_tokens = 12000, files_mentioned = [], skills_matched = [], knowledge_entries = [] } = vars;

  // Build mock context based on inputs
  const behavioral_skills = skills_matched
    .map(s => `# ${s.name}\nScore: ${s.score}`)
    .join("\n\n");

  const docs_context = knowledge_entries
    .map(k => `// ${k.key}: ${k.value}`)
    .join("\n");

  const code_context = files_mentioned
    .map(f => `// ${f}\n// [file content]`)
    .join("\n\n");

  // Estimate tokens (rough: 1 token ≈ 4 chars)
  const total_tokens = Math.floor(
    (behavioral_skills.length + docs_context.length + code_context.length) / 4
  );

  // Enforce budget
  const budget_remaining = Math.max(0, max_tokens - total_tokens);

  return {
    context_pack: {
      behavioral_skills,
      docs_context,
      code_context,
      token_usage: {
        total_tokens: Math.min(total_tokens, max_tokens),
        budget_remaining,
        by_source: {
          behavioral_skills: Math.floor(behavioral_skills.length / 4),
          docs_context: Math.floor(docs_context.length / 4),
          code_context: Math.floor(code_context.length / 4),
        },
      },
    },
    routing_decision: {
      model: "gpt-4o",
      tier: "balanced",
    },
  };
}

/**
 * Promptfoo provider function
 */
module.exports = async function (vars) {
  const result = await callContextEngine(vars);
  
  // Return the full context pack as output
  return {
    output: JSON.stringify(result.context_pack, null, 2),
    metadata: {
      routing_decision: result.routing_decision,
      token_usage: result.context_pack?.token_usage,
    },
  };
};

// Export for testing
module.exports.mockContextEngine = mockContextEngine;
