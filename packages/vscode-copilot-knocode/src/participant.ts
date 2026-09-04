import * as vscode from "vscode";
import { ensureDaemonReady, requestContextEnrichment } from "./daemon";

/**
 * Registers the `@knocode` chat participant.
 *
 * On every invocation (each user turn):
 *   1. resolves the active workspace root,
 *   2. fetches repository context from the Knocode daemon (`knocode_context`),
 *   3. assembles the prompt (history + [repository context] + user prompt),
 *   4. sends it to the user's own Copilot model via `request.model.sendRequest`,
 *   5. streams the response as markdown.
 *
 * This is the faithful analog of opencode's `chat.message` enrichment: inside
 * the participant, we own the prompt assembly, so context is injected on every
 * turn. Fail-open: if the daemon is down/mid-index it simply runs with the
 * bare prompt.
 */
export function registerKnocodeParticipant(context: vscode.ExtensionContext): void {
  const participant = vscode.chat.createChatParticipant(
    "chat.knocode",
    handler,
  );
  context.subscriptions.push(participant);
}

const handler: vscode.ChatRequestHandler = async (request, chatContext, stream, token) => {
  const cwd = vscode.workspace.workspaceFolders?.[0]?.uri.fsPath ?? process.cwd();
  stream.progress("Gathering repository context from the Knocode daemon…");

  // Bounded, fail-open readiness gate (cold-start indexing).
  await ensureDaemonReady();

  const contextText = await requestContextEnrichment(
    request.prompt,
    cwd,
  );

  const messages: vscode.LanguageModelChatMessage[] = [];

  // Conversation history (matches the chat-tutorial pattern).
  for (const turn of chatContext.history) {
    if (turn instanceof vscode.ChatResponseTurn) {
      const text = turn.response
        .filter((p) => p instanceof vscode.ChatResponseMarkdownPart)
        .map((p) => (p as vscode.ChatResponseMarkdownPart).value.value)
        .join("\n");
      if (text) messages.push(vscode.LanguageModelChatMessage.Assistant(text));
    } else if (turn instanceof vscode.ChatRequestTurn) {
      messages.push(vscode.LanguageModelChatMessage.User(turn.prompt));
    }
  }

  // Inject repository context, then the user's prompt.
  if (contextText) {
    messages.push(
      vscode.LanguageModelChatMessage.User(
        `[Repository context from Knocode]\n${contextText}`,
      ),
    );
  }
  messages.push(vscode.LanguageModelChatMessage.User(request.prompt));

  // Send to the user's own Copilot model and stream the reply.
  try {
    const response = await request.model.sendRequest(messages, {}, token);
    for await (const fragment of response.text) {
      stream.markdown(fragment);
    }
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err);
    throw new Error(`Knocode model request failed: ${msg}`);
  }

  return { metadata: { knocodeContext: contextText ? "attached" : "none" } };
};