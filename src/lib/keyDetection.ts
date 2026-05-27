// Shared API-key → provider-type heuristic. Identical wording across the
// onboarding wizard, the quick-setup page, and the provider form dialog,
// so all three surfaces produce the same answer for the same paste.
//
// Detection is intentionally prefix-first with one exact-shape disambiguator
// for DeepSeek, whose keys (`sk-` + 32 lowercase hex) would otherwise be
// indistinguishable from OpenAI's older `sk-…` format. Heuristic only —
// users can still override the inferred type in the next step.

export interface DetectedProvider {
  type: string;
  label: string;
}

/** Map provider-type to its display label (shown to the user as "Detected: X"). */
const LABELS: Record<string, string> = {
  anthropic: "Anthropic",
  deepseek: "DeepSeek",
  openai: "OpenAI",
  openrouter: "OpenRouter",
  groq: "Groq",
  xai: "xAI",
  perplexity: "Perplexity",
  mimo: "MiMo (小米)",
  kimi: "Kimi (Moonshot)",
};

// DeepSeek's documented key shape: `sk-` followed by exactly 32 lowercase hex
// characters. Anchored regex so longer OpenAI keys that happen to start with
// 32 hex chars don't false-positive.
const DEEPSEEK_KEY_RE = /^sk-[a-f0-9]{32}$/;

// Moonshot/Kimi keys observed in the wild: `sk-` + 48 base64-ish chars
// (mixed-case + digits + no special chars). Slightly looser than the
// DeepSeek anchor because Moonshot doesn't publish an exact spec.
const KIMI_KEY_RE = /^sk-[A-Za-z0-9]{48}$/;

/**
 * Return the most likely provider type for a given API key, or `null` when
 * the input is empty or doesn't match any known shape. The check runs
 * specific-prefix-first (sk-ant-, sk-proj-, deepseek-, …) and falls back to
 * generic `sk-` → OpenAI only after the tighter regexes have had a chance.
 */
export function detectProviderType(key: string): string | null {
  const k = key.trim();
  if (!k) return null;

  // Explicit-prefix providers — unambiguous.
  if (k.startsWith("sk-ant-")) return "anthropic";
  if (k.startsWith("sk-or-")) return "openrouter";
  if (k.startsWith("gsk_")) return "groq";
  if (k.startsWith("xai-")) return "xai";
  if (k.startsWith("pplx-")) return "perplexity";
  if (k.startsWith("tp-")) return "mimo"; // Token Plan
  if (k.startsWith("deepseek-")) return "deepseek"; // Legacy / alt format

  // OpenAI's new structured prefixes — distinguish from DeepSeek's `sk-…hex…`.
  if (
    k.startsWith("sk-proj-") ||
    k.startsWith("sk-svcacct-") ||
    k.startsWith("sk-admin-")
  ) {
    return "openai";
  }

  // Exact-shape disambiguators on bare `sk-…` keys.
  if (DEEPSEEK_KEY_RE.test(k)) return "deepseek";
  if (KIMI_KEY_RE.test(k)) return "kimi";

  // Generic `sk-` fallback — most often OpenAI, but could be SiliconFlow,
  // MiMo PAYG, or any number of OpenAI-compatible aggregators. We still
  // return "openai" to give the user a sensible starting preset; they pick
  // the actual provider in the next wizard step if it differs.
  if (k.startsWith("sk-")) return "openai";

  return null;
}

/** Convenience: return `{type, label}` ready to render. */
export function detectProvider(key: string): DetectedProvider | null {
  const type = detectProviderType(key);
  if (!type) return null;
  return { type, label: LABELS[type] ?? type };
}
