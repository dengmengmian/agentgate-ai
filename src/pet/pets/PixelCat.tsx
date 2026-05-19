import type { PetState } from "@/types/pet";

interface Props {
  state: PetState;
}

export function PixelCat({ state }: Props) {
  const bodyColor = "#1E2128";
  const outlineColor = "#D4A574";
  const eyeColor = state === "error" ? "#F85149" : state === "sleep" ? "#6B7280" : "#FFD700";
  const cheekColor = state === "active" ? "#FF9999" : "transparent";

  return (
    <svg width="100" height="120" viewBox="0 0 80 96" fill="none" xmlns="http://www.w3.org/2000/svg">
      {/* Pixel grid — each "pixel" is 4x4 */}

      {/* Left ear */}
      <rect x="12" y="4" width="8" height="4" fill={outlineColor} />
      <rect x="8" y="8" width="4" height="4" fill={outlineColor} />
      <rect x="12" y="8" width="8" height="4" fill="#FFB6C1" />
      <rect x="8" y="12" width="4" height="4" fill={outlineColor} />
      <rect x="12" y="12" width="8" height="4" fill="#FFB6C1" />

      {/* Right ear */}
      <rect x="56" y="4" width="8" height="4" fill={outlineColor} />
      <rect x="64" y="8" width="4" height="4" fill={outlineColor} />
      <rect x="56" y="8" width="8" height="4" fill="#FFB6C1" />
      <rect x="64" y="12" width="4" height="4" fill={outlineColor} />
      <rect x="56" y="12" width="8" height="4" fill="#FFB6C1" />

      {/* Head */}
      <rect x="16" y="12" width="44" height="4" fill={outlineColor} />
      <rect x="12" y="16" width="52" height="24" rx="0" fill={outlineColor} />

      {/* Face interior */}
      <rect x="16" y="18" width="44" height="18" fill={bodyColor} />

      {/* Eyes */}
      {state === "sleep" ? (
        <>
          <rect x="22" y="26" width="8" height="2" fill={eyeColor} />
          <rect x="46" y="26" width="8" height="2" fill={eyeColor} />
        </>
      ) : (
        <>
          <rect x="24" y="22" width="6" height="8" fill={eyeColor} />
          <rect x="26" y="22" width="2" height="4" fill="#fff" />
          <rect x="46" y="22" width="6" height="8" fill={eyeColor} />
          <rect x="48" y="22" width="2" height="4" fill="#fff" />
        </>
      )}

      {/* Cheeks */}
      <rect x="16" y="28" width="6" height="4" fill={cheekColor} rx="1" />
      <rect x="54" y="28" width="6" height="4" fill={cheekColor} rx="1" />

      {/* Nose */}
      <rect x="36" y="28" width="4" height="3" fill="#FFB6C1" />

      {/* Mouth */}
      {state === "error" ? (
        <rect x="32" y="33" width="12" height="2" fill="#F85149" />
      ) : state === "active" ? (
        <>
          <rect x="34" y="32" width="2" height="2" fill={outlineColor} />
          <rect x="36" y="34" width="4" height="2" fill={outlineColor} />
          <rect x="40" y="32" width="2" height="2" fill={outlineColor} />
        </>
      ) : (
        <rect x="34" y="33" width="8" height="2" fill="#6B7280" />
      )}

      {/* Body */}
      <rect x="16" y="40" width="44" height="32" fill={outlineColor} />
      <rect x="20" y="44" width="36" height="24" fill={bodyColor} />

      {/* Belly pattern */}
      <rect x="30" y="48" width="16" height="16" rx="4" fill="#2A2D35" />

      {/* Front paws */}
      <rect x="16" y="72" width="12" height="8" rx="2" fill={outlineColor} />
      <rect x="48" y="72" width="12" height="8" rx="2" fill={outlineColor} />

      {/* Tail */}
      <rect x="60" y="56" width="8" height="4" fill={outlineColor} />
      <rect x="64" y="52" width="8" height="4" fill={outlineColor} />
      <rect x="68" y="48" width="4" height="4" fill={outlineColor}>
        {state === "active" && (
          <animateTransform
            attributeName="transform"
            type="rotate"
            values="0 70 50;15 70 50;0 70 50;-15 70 50;0 70 50"
            dur="0.5s"
            repeatCount="indefinite"
          />
        )}
      </rect>

      {/* Hind paws */}
      <rect x="16" y="80" width="10" height="6" rx="2" fill={outlineColor} />
      <rect x="50" y="80" width="10" height="6" rx="2" fill={outlineColor} />
    </svg>
  );
}
