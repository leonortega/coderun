/**
 * Model Routing Evaluation Provider for Promptfoo
 *
 * Exports a provider object with id() and callApi() methods.
 * Mirrors the Rust implementation in coderun-router.
 */

/**
 * Local model routing (mirrors the Rust implementation)
 */
function localModelRouting(vars) {
  const message = vars.task || "";
  const file_count = vars.file_count || 0;
  const symbol_count = vars.symbol_count || 0;
  const knowledge_entries = vars.knowledge_entries || 0;
  const skills_matched = vars.skills_matched || 0;
  const token_count = vars.token_count || 0;

  // Edge case: empty task
  if (message.trim().length === 0) {
    return "fast";
  }

  // Structural complexity
  const file_score = Math.min(file_count / 20, 1);
  const symbol_score = Math.min(symbol_count / 100, 1);
  const structural = (file_score + symbol_score) / 2;

  // Semantic complexity
  const technicalTerms = [
    "refactor", "migrate", "database", "schema", "api",
    "middleware", "authentication", "authorization", "concurrency",
    "parallel", "async", "distributed", "microservice", "architecture",
    "implement", "system", "design", "configure", "integration",
    "error handling", "comprehensive", "module",
  ];
  const actionVerbs = [
    "implement", "fix", "add", "remove", "refactor", "migrate",
    "optimize", "debug", "test", "deploy", "configure", "integrate",
  ];

  const lowerMessage = message.toLowerCase();
  const techCount = technicalTerms.filter(t => lowerMessage.includes(t)).length;
  const actionCount = actionVerbs.filter(v => lowerMessage.includes(v)).length;
  const wordCount = message.split(/\s+/).length;

  const lengthScore = Math.min(wordCount / 25, 1);
  const techScore = Math.min(techCount / 2, 1);
  const actionScore = Math.min(actionCount / 2, 1);
  const semantic = (lengthScore + techScore + actionScore) / 3;

  // Scope
  const knowledgeScore = Math.min(knowledge_entries / 5, 1);
  const skillScore = Math.min(skills_matched / 2, 1);
  const tokenScore = Math.min(token_count / 5000, 1);
  const scope = (knowledgeScore + skillScore + tokenScore) / 3;

  // Final score (weighted)
  const finalScore = structural * 0.3 + semantic * 0.4 + scope * 0.3;

  // Map to tier with adjusted thresholds
  if (finalScore < 0.25) {
    return "fast";
  } else if (finalScore > 0.55) {
    return "capable";
  } else {
    return "balanced";
  }
}

/**
 * Promptfoo provider
 */
module.exports = class ModelRoutingProvider {
  id() {
    return "model-routing";
  }

  label = "Model Routing";

  async callApi(prompt, context) {
    const vars = context?.vars || {};
    const tier = localModelRouting(vars);
    return {
      output: tier,
    };
  }
};
