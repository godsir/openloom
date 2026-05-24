/**
 * Anthropic Messages prompt-cache compatibility layer.
 *
 * Chat requests normally get cache_control from Pi SDK before this layer runs.
 * Utility requests are direct HTTP calls, so this module makes the same marker
 * contract available through normalizeProviderPayload.
 */

const CACHE_CONTROL = { type: "ephemeral" };

function lower(value) {
  return typeof value === "string" ? value.toLowerCase() : "";
}

function hasCacheControl(block) {
  return Boolean(block && typeof block === "object" && block.cache_control);
}

function shouldCacheContentBlock(block) {
  return block && typeof block === "object"
    && (block.type === "text" || block.type === "image" || block.type === "tool_result");
}

function withCacheControl(block) {
  if (!shouldCacheContentBlock(block) || hasCacheControl(block)) return block;
  return { ...block, cache_control: { ...CACHE_CONTROL } };
}

function normalizeSystem(system) {
  if (typeof system === "string") {
    return {
      value: [{ type: "text", text: system, cache_control: { ...CACHE_CONTROL } }],
      changed: true,
    };
  }

  if (!Array.isArray(system)) {
    return { value: system, changed: false };
  }

  let lastIndex = -1;
  for (let i = system.length - 1; i >= 0; i--) {
    if (system[i]?.type === "text") {
      lastIndex = i;
      break;
    }
  }
  if (lastIndex < 0 || hasCacheControl(system[lastIndex])) {
    return { value: system, changed: false };
  }

  const next = system.slice();
  next[lastIndex] = withCacheControl(system[lastIndex]);
  return { value: next, changed: true };
}

function normalizeUserMessage(message) {
  if (!message || message.role !== "user") {
    return { value: message, changed: false, cacheable: false };
  }

  if (typeof message.content === "string") {
    if (message.content.trim().length === 0) {
      return { value: message, changed: false, cacheable: false };
    }
    return {
      value: {
        ...message,
        content: [{
          type: "text",
          text: message.content,
          cache_control: { ...CACHE_CONTROL },
        }],
      },
      changed: true,
      cacheable: true,
    };
  }

  if (!Array.isArray(message.content) || message.content.length === 0) {
    return { value: message, changed: false, cacheable: false };
  }

  const blockIndex = message.content.length - 1;
  const lastBlock = message.content[blockIndex];
  if (!shouldCacheContentBlock(lastBlock)) {
    return { value: message, changed: false, cacheable: false };
  }
  if (hasCacheControl(lastBlock)) {
    return { value: message, changed: false, cacheable: true };
  }

  const nextContent = message.content.slice();
  nextContent[blockIndex] = withCacheControl(lastBlock);
  return {
    value: { ...message, content: nextContent },
    changed: true,
    cacheable: true,
  };
}

function normalizeRecentUserMessages(messages) {
  if (!Array.isArray(messages) || messages.length === 0) {
    return { value: messages, changed: false };
  }

  let next = messages;
  let changed = false;
  let marked = 0;
  for (let i = messages.length - 1; i >= 0 && marked < 2; i--) {
    const result = normalizeUserMessage(next[i]);
    if (!result.cacheable) continue;
    marked++;
    if (result.changed) {
      if (next === messages) next = messages.slice();
      next[i] = result.value;
      changed = true;
    }
  }
  return { value: next, changed };
}

export function matches(model) {
  if (!model || typeof model !== "object") return false;
  if (lower(model.api) !== "anthropic-messages") return false;
  if (lower(model.provider) === "anthropic") return true;
  if (lower(model.id).startsWith("claude-")) return true;
  return model.compat?.cacheControlFormat === "anthropic";
}

export function apply(payload) {
  let result = payload;

  if (Object.prototype.hasOwnProperty.call(payload, "system")) {
    const system = normalizeSystem(payload.system);
    if (system.changed) result = { ...result, system: system.value };
  }

  const messages = normalizeRecentUserMessages(result.messages);
  if (messages.changed) result = { ...result, messages: messages.value };

  return result;
}
