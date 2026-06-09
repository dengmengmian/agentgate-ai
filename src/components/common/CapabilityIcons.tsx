import { Eye, EyeOff, Mic, Speaker, Video, Brain, Globe } from "lucide-react";
import { useI18n } from "@/lib/i18n";

/**
 * Render a row of capability icons inferred from a provider's
 * `model_capabilities` JSON matrix. Used in both the Providers card and the
 * Routes provider chain so users see the same signal everywhere.
 *
 * The icon set intentionally omits "text" and "tools" because those are the
 * default for any chat provider and would just clutter the row.
 *
 * `legacyVision` is a fallback only used when no matrix is present — covers
 * older providers created before the matrix existed.
 */

interface CapabilityIconsProps {
  modelCapabilities: string | null;
  legacyVision?: boolean | null;
  /** Compact mode hides the "no support" greyed-out icons; useful for tight rows. */
  compact?: boolean;
  size?: "xs" | "sm";
}

const ICON_SIZE = { xs: "h-3 w-3", sm: "h-3.5 w-3.5" };

interface IconSpec {
  cap: string;
  Icon: typeof Eye;
  labelKey: string;
  noLabelKey: string;
}

const ICONS: IconSpec[] = [
  { cap: "vision", Icon: Eye, labelKey: "providers.cap.vision", noLabelKey: "providers.cap.vision_no" },
  { cap: "audio_in", Icon: Mic, labelKey: "providers.cap.audio_in", noLabelKey: "providers.cap.audio_in_no" },
  { cap: "tts", Icon: Speaker, labelKey: "providers.cap.tts", noLabelKey: "providers.cap.tts_no" },
  { cap: "video_in", Icon: Video, labelKey: "providers.cap.video_in", noLabelKey: "providers.cap.video_in_no" },
  { cap: "reasoning", Icon: Brain, labelKey: "providers.cap.reasoning", noLabelKey: "providers.cap.reasoning_no" },
  { cap: "web_search", Icon: Globe, labelKey: "providers.cap.web_search", noLabelKey: "providers.cap.web_search_no" },
];

function parseMatrix(json: string | null): Record<string, string[]> | null {
  if (!json) return null;
  try {
    const parsed = JSON.parse(json);
    return typeof parsed === "object" && parsed !== null ? (parsed as Record<string, string[]>) : null;
  } catch {
    return null;
  }
}

function anyModelHas(matrix: Record<string, string[]>, capability: string): boolean {
  return Object.values(matrix).some((caps) => Array.isArray(caps) && caps.includes(capability));
}

export function CapabilityIcons({ modelCapabilities, legacyVision, compact = true, size = "sm" }: CapabilityIconsProps) {
  const { t } = useI18n();
  const matrix = parseMatrix(modelCapabilities);
  const cls = ICON_SIZE[size];

  // Build the per-capability state. If matrix unset, only vision can be derived
  // from the legacy boolean; other caps stay "unknown" (hidden).
  const states: { spec: IconSpec; state: "yes" | "no" | "unknown" }[] = ICONS.map((spec) => {
    if (matrix) {
      return { spec, state: anyModelHas(matrix, spec.cap) ? "yes" : "no" };
    }
    if (spec.cap === "vision" && typeof legacyVision === "boolean") {
      return { spec, state: legacyVision ? "yes" : "no" };
    }
    return { spec, state: "unknown" };
  });

  const visible = states.filter(({ state }) => state !== "unknown" && (!compact || state === "yes"));
  if (visible.length === 0) return null;

  return (
    <span className="inline-flex items-center gap-1.5">
      {visible.map(({ spec, state }) => {
        const label = state === "yes" ? t(spec.labelKey) : t(spec.noLabelKey);
        return (
          <spec.Icon
            key={spec.cap}
            className={`${cls} ${state === "yes" ? "text-accent" : "text-text-muted/60"}`}
            aria-label={label}
          >
            <title>{label}</title>
          </spec.Icon>
        );
      })}
    </span>
  );
}

// Re-export EyeOff so callers can use the same "vision unknown" placeholder if desired
export { EyeOff };
