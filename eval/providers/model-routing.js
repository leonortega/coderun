/**
 * Model Routing Evaluation Provider
 * 
 * This provider calls the Coderun Model Router to evaluate
 * routing accuracy for different task types.
 * 
 * Usage with Promptfoo:
 *   npx promptfoo eval -c eval/promptfoo.yaml
 */

const CODERUN_DAEMON_URL = process.env.CODERUN_DAEMON_URL || "http://127.0.0.1:9527";

/**
 * Call the Coderun Model Router API
 * @param {Object} vars - Test variables
 * @returns {Promise<Object>} - Routing decision
 */
async function callModelRouter(vars) {
  const request = {
    message: vars.task,
    file_count: vars.file_count || 0,
    symbol_count: vars.symbol_count || 0,
    knowledge_entries: vars.knowledge_entries || 0,
    skills_matched: vars.skills_matched || 0,
    token_count: vars.token_count || 0,
  };

  try {
    const response = await fetch(`${CODERUN_DAEMON_URL}/api/route`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(request),
      signal: AbortSignal.timeout(5000),
    });

    if (!response.ok) {
      throw new Error(`HTTP ${response.status}`);
    }

    return await response.json();
  } catch (error) {
    // If daemon is not running, use local scoring
    return localModelRouting(request);
  }
}

/**
 * Local model routing (fallback when daemon is not available)
 * Mirrors the Rust implementation in coderun-router
 */
function localModelRouting(request) {
  const { message, file_count, symbol_count, knowledge_entries, skills_matched, token_count } = request;

  // Structural complexity
  const file_score = Math.min(file_count / 20, 1);
  const symbol_score = Math.min(symbol_count / 100, 1);
  const structural = (file_score + symbol_score) / 2;

  // Semantic complexity
  const technicalTerms = [
    "refactor", "migrate", "database", "schema", "api",
    "middleware", "authentication", "authorization", "concurrency",
    "parallel", "async", "distributed", "microservice", "architecture",
  ];
  const actionVerbs = [
    "implement", "fix", "add", "remove", "refactor", "migrate",
    "optimize", "debug", "test", "deploy", "configure", "integrate",
  ];

  const lowerMessage = message.toLowerCase();
  const techCount = technicalTerms.filter(t => lowerMessage.includes(t)).length;
  const actionCount = actionVerbs.filter(v => lowerMessage.includes(v)).length;
  const wordCount = message.split(/\s+/).length;

  const lengthScore = Math.min(wordCount / 50, 1);
  const techScore = Math.min(techCount / 5, 1);
  const actionScore = Math.min(actionCount / 3, 1);
  const semantic = (lengthScore + techScore + actionScore) / 3;

  // Scope
  const knowledgeScore = Math.min(knowledge_entries / 10, 1);
  const skillScore = Math.min(skills_matched / 3, 1);
  const tokenScore = Math.min(token_count / 8000, 1);
  const scope = (knowledgeScore + skillScore + tokenScore) / 3;

  // Final score
  const finalScore = structural * 0.3 + semantic * 0.4 + scope * 0.3;

  // Map to tier
  let tier;
  if (finalScore < 0.3) {
    tier = "fast";
  } else if (finalScore > 0.7) {
    tier = "capable";
  } else {
    tier = "balanced";
  }

  return {
    tier,
    scores: { structural, semantic, scope, final: finalScore },
  };
}

/**
 * Promptfoo provider function
 */
module.exports = async function (vars) {
  const result = await callModelRouter(vars);
  
  return {
    output: result.tier,
    metadata: {
      scores: result.scores,
    },
  };
};

// Export for testing
module.exports.localModelRouting = localModelRouting;
