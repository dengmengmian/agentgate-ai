import type { PetState } from "@/types/pet";

interface Props {
  state: PetState;
}

export function FoxPet({ state }: Props) {
  const suitColor = "#1E2128";
  const shirtColor = "#E8EAF0";
  const tieColor = state === "error" ? "#F85149" : "#FFD700";
  const skinColor = "#F5D6B8";
  const eyeColor = state === "error" ? "#F85149" : state === "sleep" ? "#6B7280" : "#1E2128";
  const cheekColor = state === "active" ? "#FF9999" : "transparent";

  return (
    <svg width="100" height="120" viewBox="0 0 100 120" fill="none" xmlns="http://www.w3.org/2000/svg">
      {/* Hair */}
      <ellipse cx="50" cy="22" rx="22" ry="10" fill="#2A2520" />
      <rect x="30" y="15" width="40" height="10" rx="5" fill="#2A2520" />

      {/* Head */}
      <ellipse cx="50" cy="35" rx="20" ry="18" fill={skinColor} />

      {/* Hair fringe */}
      <path d="M32,25 Q40,18 50,22 Q55,18 60,22 Q65,18 68,25" fill="#2A2520" />

      {/* Sunglasses (CEO style) */}
      {state === "sleep" ? (
        <>
          <path d="M36,34 Q40,37 44,34" stroke={eyeColor} strokeWidth="2" fill="none" strokeLinecap="round" />
          <path d="M56,34 Q60,37 64,34" stroke={eyeColor} strokeWidth="2" fill="none" strokeLinecap="round" />
        </>
      ) : (
        <>
          <rect x="33" y="30" width="14" height="10" rx="2" fill="#1a1a1a" stroke="#333" strokeWidth="0.5" />
          <rect x="53" y="30" width="14" height="10" rx="2" fill="#1a1a1a" stroke="#333" strokeWidth="0.5" />
          <line x1="47" y1="35" x2="53" y2="35" stroke="#333" strokeWidth="1" />
          {/* Lens glint */}
          <rect x="35" y="32" width="4" height="2" rx="1" fill="#444" opacity="0.6" />
          <rect x="55" y="32" width="4" height="2" rx="1" fill="#444" opacity="0.6" />
          {/* Temple arms */}
          <line x1="33" y1="33" x2="28" y2="31" stroke="#333" strokeWidth="1" />
          <line x1="67" y1="33" x2="72" y2="31" stroke="#333" strokeWidth="1" />
        </>
      )}

      {/* Cheeks */}
      <circle cx="30" cy="40" r="3" fill={cheekColor} opacity="0.5" />
      <circle cx="70" cy="40" r="3" fill={cheekColor} opacity="0.5" />

      {/* Mouth */}
      {state === "error" ? (
        <path d="M44,46 Q50,43 56,46" stroke="#C06060" strokeWidth="1.5" fill="none" strokeLinecap="round" />
      ) : state === "active" ? (
        <path d="M44,44 Q50,49 56,44" stroke="#8B6B50" strokeWidth="1.5" fill="none" strokeLinecap="round" />
      ) : (
        <line x1="45" y1="45" x2="55" y2="45" stroke="#8B6B50" strokeWidth="1.5" strokeLinecap="round" />
      )}

      {/* Body — suit */}
      <rect x="28" y="54" width="44" height="40" rx="6" fill={suitColor} />

      {/* Shirt V */}
      <path d="M42,54 L50,68 L58,54" fill={shirtColor} />

      {/* Tie */}
      <polygon points="50,58 47,66 50,78 53,66" fill={tieColor}>
        {state === "active" && (
          <animateTransform attributeName="transform" type="rotate" values="0 50 68;-3 50 68;3 50 68;0 50 68" dur="0.5s" repeatCount="indefinite" />
        )}
      </polygon>
      <rect x="47" y="56" width="6" height="4" rx="1" fill={tieColor} />

      {/* Suit lapels */}
      <path d="M42,54 L36,62 L38,62 L44,56" fill="#2A2D35" />
      <path d="M58,54 L64,62 L62,62 L56,56" fill="#2A2D35" />

      {/* Suit pocket square */}
      <rect x="56" y="64" width="6" height="4" rx="1" fill={shirtColor} opacity="0.7" />

      {/* Arms */}
      <rect x="18" y="58" width="10" height="24" rx="5" fill={suitColor} />
      <rect x="72" y="58" width="10" height="24" rx="5" fill={suitColor} />

      {/* Hands */}
      <ellipse cx="23" cy="84" rx="5" ry="4" fill={skinColor} />
      <ellipse cx="77" cy="84" rx="5" ry="4" fill={skinColor} />

      {/* Shoes */}
      <rect x="32" y="94" width="14" height="7" rx="3" fill="#1a1a1a" />
      <rect x="54" y="94" width="14" height="7" rx="3" fill="#1a1a1a" />
      {/* Shoe shine */}
      <rect x="34" y="95" width="6" height="2" rx="1" fill="#333" />
      <rect x="56" y="95" width="6" height="2" rx="1" fill="#333" />

      {/* Status indicator — tie pin */}
      <circle cx="50" cy="64" r="2" fill={state === "active" ? "#3FB950" : state === "error" ? "#F85149" : "#6B7280"}>
        {state === "active" && (
          <animate attributeName="opacity" values="1;0.4;1" dur="0.8s" repeatCount="indefinite" />
        )}
      </circle>
    </svg>
  );
}
