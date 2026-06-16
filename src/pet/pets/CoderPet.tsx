import type { PetState } from "@/types/pet";

interface Props {
  state: PetState;
}

export function CoderPet({ state }: Props) {
  const skinColor = "#F0D5B8";
  const hairColor = "#1E2128";
  const shirtColor = "#2D4A3E";
  const glassesColor = "#9BA3AF";
  const screenGlow =
    state === "active" ? "#3FB950" : state === "error" ? "#F85149" : "#7C8CFF";
  const eyeColor =
    state === "error" ? "#F85149" : state === "sleep" ? "#6B7280" : "#1E2128";

  return (
    <svg
      width="100"
      height="120"
      viewBox="0 0 100 120"
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
    >
      {/* Hair */}
      <ellipse cx="50" cy="20" rx="20" ry="10" fill={hairColor} />
      <rect x="32" y="14" width="36" height="12" rx="4" fill={hairColor} />
      {/* Neat side part */}
      <path d="M38,14 Q42,12 46,14" fill="#2A2D35" />

      {/* Head */}
      <ellipse cx="50" cy="34" rx="18" ry="16" fill={skinColor} />

      {/* Glasses — round, classic programmer style */}
      <circle
        cx="42"
        cy="32"
        r="7"
        fill="none"
        stroke={glassesColor}
        strokeWidth="1.5"
      />
      <circle
        cx="58"
        cy="32"
        r="7"
        fill="none"
        stroke={glassesColor}
        strokeWidth="1.5"
      />
      <line
        x1="49"
        y1="32"
        x2="51"
        y2="32"
        stroke={glassesColor}
        strokeWidth="1.5"
      />
      <line
        x1="35"
        y1="30"
        x2="32"
        y2="28"
        stroke={glassesColor}
        strokeWidth="1.5"
      />
      <line
        x1="65"
        y1="30"
        x2="68"
        y2="28"
        stroke={glassesColor}
        strokeWidth="1.5"
      />
      {/* Lens reflection */}
      <path
        d="M38,29 Q39,28 40,29"
        stroke="white"
        strokeWidth="0.8"
        fill="none"
        opacity="0.5"
      />
      <path
        d="M54,29 Q55,28 56,29"
        stroke="white"
        strokeWidth="0.8"
        fill="none"
        opacity="0.5"
      />

      {/* Eyes behind glasses */}
      {state === "sleep" ? (
        <>
          <path
            d="M39,33 Q42,35 45,33"
            stroke={eyeColor}
            strokeWidth="1.5"
            fill="none"
            strokeLinecap="round"
          />
          <path
            d="M55,33 Q58,35 61,33"
            stroke={eyeColor}
            strokeWidth="1.5"
            fill="none"
            strokeLinecap="round"
          />
        </>
      ) : (
        <>
          <circle cx="42" cy="33" r="2" fill={eyeColor} />
          <circle cx="58" cy="33" r="2" fill={eyeColor} />
          {/* Focused look — eyes slightly down (reading code) */}
          {state === "active" && (
            <>
              <circle cx="42" cy="34" r="2" fill={eyeColor} />
              <circle cx="58" cy="34" r="2" fill={eyeColor} />
            </>
          )}
        </>
      )}

      {/* Slight smile — calm, reliable */}
      {state === "error" ? (
        <path
          d="M45,42 L55,42"
          stroke="#8B6B50"
          strokeWidth="1.2"
          strokeLinecap="round"
        />
      ) : (
        <path
          d="M45,41 Q50,44 55,41"
          stroke="#8B6B50"
          strokeWidth="1.2"
          fill="none"
          strokeLinecap="round"
        />
      )}

      {/* Body — plain polo/shirt, no flashy stuff */}
      <rect x="30" y="50" width="40" height="34" rx="6" fill={shirtColor} />

      {/* Collar */}
      <path
        d="M42,50 L50,56 L58,50"
        fill="none"
        stroke="#3D5A4E"
        strokeWidth="1.5"
      />

      {/* Shirt pocket with pen */}
      <rect
        x="54"
        y="58"
        width="8"
        height="9"
        rx="1.5"
        fill="#3D5A4E"
        stroke="#4A6B5A"
        strokeWidth="0.5"
      />
      <line
        x1="57"
        y1="56"
        x2="57"
        y2="62"
        stroke="#7C8CFF"
        strokeWidth="1.5"
        strokeLinecap="round"
      />

      {/* Arms resting on laptop */}
      <rect x="18" y="56" width="12" height="20" rx="5" fill={shirtColor} />
      <rect x="70" y="56" width="12" height="20" rx="5" fill={shirtColor} />

      {/* Hands */}
      <ellipse cx="24" cy="78" rx="5" ry="3.5" fill={skinColor} />
      <ellipse cx="76" cy="78" rx="5" ry="3.5" fill={skinColor} />

      {/* Laptop */}
      <rect x="20" y="78" width="60" height="6" rx="2" fill="#333" />
      {/* Screen */}
      <rect x="26" y="72" width="48" height="6" rx="1" fill="#111">
        {state === "active" && (
          <animate
            attributeName="fill"
            values="#111;#1a2a1a;#111"
            dur="2s"
            repeatCount="indefinite"
          />
        )}
      </rect>
      {/* Code lines on screen */}
      <line
        x1="28"
        y1="74"
        x2="38"
        y2="74"
        stroke={screenGlow}
        strokeWidth="1"
        opacity="0.8"
      >
        {state === "active" && (
          <animate
            attributeName="x2"
            values="38;44;38"
            dur="0.8s"
            repeatCount="indefinite"
          />
        )}
      </line>
      <line
        x1="28"
        y1="76"
        x2="42"
        y2="76"
        stroke={screenGlow}
        strokeWidth="1"
        opacity="0.5"
      />
      {/* Cursor blink */}
      {state !== "sleep" && (
        <rect x="44" y="73" width="1.5" height="4" fill={screenGlow}>
          <animate
            attributeName="opacity"
            values="1;0;1"
            dur="1s"
            repeatCount="indefinite"
          />
        </rect>
      )}

      {/* Legs */}
      <rect x="34" y="84" width="12" height="18" rx="4" fill="#3A3A4A" />
      <rect x="54" y="84" width="12" height="18" rx="4" fill="#3A3A4A" />

      {/* Shoes — plain sneakers */}
      <rect x="32" y="100" width="16" height="7" rx="3.5" fill="#555" />
      <rect x="52" y="100" width="16" height="7" rx="3.5" fill="#555" />

      {/* Coffee mug beside laptop */}
      <rect x="78" y="72" width="8" height="10" rx="2" fill="#D4A574" />
      <path
        d="M86,75 Q90,75 90,78 Q90,81 86,81"
        stroke="#D4A574"
        strokeWidth="1.5"
        fill="none"
      />
      {/* Steam from coffee */}
      {state !== "sleep" && (
        <>
          <path
            d="M80,70 Q79,66 81,63"
            stroke="#9BA3AF"
            strokeWidth="1"
            fill="none"
            opacity="0.4"
            strokeLinecap="round"
          >
            <animate
              attributeName="opacity"
              values="0.4;0;0.4"
              dur="1.5s"
              repeatCount="indefinite"
            />
          </path>
          <path
            d="M84,69 Q83,65 85,62"
            stroke="#9BA3AF"
            strokeWidth="1"
            fill="none"
            opacity="0.3"
            strokeLinecap="round"
          >
            <animate
              attributeName="opacity"
              values="0.3;0;0.3"
              dur="1.8s"
              repeatCount="indefinite"
            />
          </path>
        </>
      )}

      {/* Git commit indicator when active */}
      {state === "active" && (
        <g opacity="0.6">
          <circle cx="14" cy="68" r="3" fill="#3FB950" />
          <line
            x1="14"
            y1="71"
            x2="14"
            y2="78"
            stroke="#3FB950"
            strokeWidth="1.5"
          />
          <circle cx="14" cy="80" r="2" fill="#3FB950" opacity="0.5" />
        </g>
      )}
    </svg>
  );
}
