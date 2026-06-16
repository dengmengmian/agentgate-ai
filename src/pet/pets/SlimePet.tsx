import type { PetState } from "@/types/pet";

interface Props {
  state: PetState;
}

export function SlimePet({ state }: Props) {
  const bodyColor =
    state === "error" ? "#F85149" : state === "active" ? "#3FB950" : "#7C8CFF";
  const bodyColorDark =
    state === "error" ? "#C13A35" : state === "active" ? "#2D8A3E" : "#5A6AD4";
  const eyeStyle =
    state === "sleep" ? "closed" : state === "error" ? "worried" : "normal";
  const mouthStyle =
    state === "active" ? "happy" : state === "error" ? "sad" : "neutral";

  // Idle body path (round blob)
  const bodyPath =
    "M20,90 Q20,50 30,35 Q40,20 50,20 Q60,20 70,35 Q80,50 80,90 Q65,95 50,95 Q35,95 20,90 Z";
  // Active body path (slightly squished)
  const bodyPathActive =
    "M18,90 Q18,55 32,38 Q42,22 50,22 Q58,22 68,38 Q82,55 82,90 Q65,96 50,96 Q35,96 18,90 Z";

  return (
    <svg
      width="100"
      height="120"
      viewBox="0 0 100 100"
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
    >
      {/* Shadow */}
      <ellipse cx="50" cy="95" rx="28" ry="5" fill="rgba(0,0,0,0.2)" />

      {/* Body */}
      <path d={state === "active" ? bodyPathActive : bodyPath} fill={bodyColor}>
        {state === "active" && (
          <animate
            attributeName="d"
            values={`${bodyPath};${bodyPathActive};${bodyPath}`}
            dur="0.6s"
            repeatCount="indefinite"
          />
        )}
      </path>

      {/* Body highlight */}
      <ellipse
        cx="40"
        cy="45"
        rx="12"
        ry="18"
        fill={bodyColorDark}
        opacity="0.3"
      />

      {/* Body shine */}
      <ellipse cx="38" cy="40" rx="6" ry="10" fill="white" opacity="0.15" />

      {/* Eyes */}
      {eyeStyle === "closed" ? (
        <>
          <path
            d="M36,52 Q40,55 44,52"
            stroke="#fff"
            strokeWidth="2"
            fill="none"
            strokeLinecap="round"
          />
          <path
            d="M56,52 Q60,55 64,52"
            stroke="#fff"
            strokeWidth="2"
            fill="none"
            strokeLinecap="round"
          />
        </>
      ) : eyeStyle === "worried" ? (
        <>
          <ellipse cx="40" cy="50" rx="5" ry="6" fill="white" />
          <circle cx="40" cy="51" r="3" fill="#1E2128" />
          <circle cx="41" cy="50" r="1" fill="#fff" />
          <ellipse cx="60" cy="50" rx="5" ry="6" fill="white" />
          <circle cx="60" cy="51" r="3" fill="#1E2128" />
          <circle cx="61" cy="50" r="1" fill="#fff" />
          {/* Worried eyebrows */}
          <line
            x1="35"
            y1="42"
            x2="44"
            y2="44"
            stroke="#fff"
            strokeWidth="1.5"
            strokeLinecap="round"
          />
          <line
            x1="65"
            y1="42"
            x2="56"
            y2="44"
            stroke="#fff"
            strokeWidth="1.5"
            strokeLinecap="round"
          />
        </>
      ) : (
        <>
          <ellipse cx="40" cy="50" rx="5" ry="6" fill="white" />
          <circle cx="41" cy="51" r="3" fill="#1E2128">
            {state === "active" && (
              <animate
                attributeName="cy"
                values="51;49;51"
                dur="0.6s"
                repeatCount="indefinite"
              />
            )}
          </circle>
          <circle cx="42" cy="50" r="1" fill="#fff" />
          <ellipse cx="60" cy="50" rx="5" ry="6" fill="white" />
          <circle cx="61" cy="51" r="3" fill="#1E2128">
            {state === "active" && (
              <animate
                attributeName="cy"
                values="51;49;51"
                dur="0.6s"
                repeatCount="indefinite"
              />
            )}
          </circle>
          <circle cx="62" cy="50" r="1" fill="#fff" />
        </>
      )}

      {/* Mouth */}
      {mouthStyle === "happy" ? (
        <path
          d="M44,62 Q50,70 56,62"
          stroke="#fff"
          strokeWidth="1.5"
          fill="none"
          strokeLinecap="round"
        />
      ) : mouthStyle === "sad" ? (
        <path
          d="M44,66 Q50,60 56,66"
          stroke="#fff"
          strokeWidth="1.5"
          fill="none"
          strokeLinecap="round"
        />
      ) : (
        <ellipse cx="50" cy="64" rx="4" ry="2" fill="white" opacity="0.5" />
      )}

      {/* Blush spots */}
      <circle cx="30" cy="58" r="4" fill="white" opacity="0.1" />
      <circle cx="70" cy="58" r="4" fill="white" opacity="0.1" />
    </svg>
  );
}
