import type { PetState } from "@/types/pet";

interface Props {
  state: PetState;
}

export function RobotPet({ state }: Props) {
  const eyeColor =
    state === "error"
      ? "#F85149"
      : state === "active"
        ? "#3FB950"
        : state === "sleep"
          ? "#6B7280"
          : "#7C8CFF";
  const antennaGlow =
    state === "active" ? "#3FB950" : state === "error" ? "#F85149" : "#7C8CFF";

  return (
    <svg
      width="100"
      height="120"
      viewBox="0 0 100 120"
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
    >
      {/* Antenna */}
      <line
        x1="50"
        y1="18"
        x2="50"
        y2="8"
        stroke="#9BA3AF"
        strokeWidth="2"
        strokeLinecap="round"
      />
      <circle cx="50" cy="6" r="4" fill={antennaGlow}>
        {state === "active" && (
          <animate
            attributeName="opacity"
            values="1;0.3;1"
            dur="0.6s"
            repeatCount="indefinite"
          />
        )}
      </circle>

      {/* Head */}
      <rect
        x="25"
        y="18"
        width="50"
        height="36"
        rx="8"
        fill="#1E2128"
        stroke="#7C8CFF"
        strokeWidth="1.5"
      />

      {/* Eyes */}
      {state === "sleep" ? (
        <>
          <line
            x1="34"
            y1="36"
            x2="42"
            y2="36"
            stroke={eyeColor}
            strokeWidth="2"
            strokeLinecap="round"
          />
          <line
            x1="58"
            y1="36"
            x2="66"
            y2="36"
            stroke={eyeColor}
            strokeWidth="2"
            strokeLinecap="round"
          />
        </>
      ) : (
        <>
          <circle cx="38" cy="34" r="5" fill={eyeColor}>
            {state === "active" && (
              <animate
                attributeName="r"
                values="5;4;5"
                dur="0.4s"
                repeatCount="indefinite"
              />
            )}
          </circle>
          <circle cx="62" cy="34" r="5" fill={eyeColor}>
            {state === "active" && (
              <animate
                attributeName="r"
                values="5;4;5"
                dur="0.4s"
                repeatCount="indefinite"
              />
            )}
          </circle>
          {/* Eye shine */}
          <circle cx="40" cy="32" r="1.5" fill="#fff" opacity="0.8" />
          <circle cx="64" cy="32" r="1.5" fill="#fff" opacity="0.8" />
        </>
      )}

      {/* Mouth */}
      {state === "error" ? (
        <path
          d="M40 46 Q50 42 60 46"
          stroke="#F85149"
          strokeWidth="1.5"
          fill="none"
          strokeLinecap="round"
        />
      ) : state === "active" ? (
        <path
          d="M40 43 Q50 49 60 43"
          stroke="#3FB950"
          strokeWidth="1.5"
          fill="none"
          strokeLinecap="round"
        />
      ) : (
        <line
          x1="42"
          y1="45"
          x2="58"
          y2="45"
          stroke="#6B7280"
          strokeWidth="1.5"
          strokeLinecap="round"
        />
      )}

      {/* Body */}
      <rect
        x="20"
        y="58"
        width="60"
        height="40"
        rx="6"
        fill="#1E2128"
        stroke="#7C8CFF"
        strokeWidth="1.5"
      />

      {/* Gateway symbol (arrows) */}
      <path
        d="M38 72 L46 78 L38 84"
        stroke="#7C8CFF"
        strokeWidth="2"
        fill="none"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
      <path
        d="M62 72 L54 78 L62 84"
        stroke="#7C8CFF"
        strokeWidth="2"
        fill="none"
        strokeLinecap="round"
        strokeLinejoin="round"
      />

      {/* Arms */}
      <rect
        x="10"
        y="62"
        width="8"
        height="24"
        rx="4"
        fill="#1E2128"
        stroke="#9BA3AF"
        strokeWidth="1"
      />
      <rect
        x="82"
        y="62"
        width="8"
        height="24"
        rx="4"
        fill="#1E2128"
        stroke="#9BA3AF"
        strokeWidth="1"
      />

      {/* Feet */}
      <rect
        x="28"
        y="100"
        width="16"
        height="8"
        rx="4"
        fill="#1E2128"
        stroke="#9BA3AF"
        strokeWidth="1"
      />
      <rect
        x="56"
        y="100"
        width="16"
        height="8"
        rx="4"
        fill="#1E2128"
        stroke="#9BA3AF"
        strokeWidth="1"
      />

      {/* Status indicator on chest */}
      <circle
        cx="50"
        cy="67"
        r="3"
        fill={
          state === "active"
            ? "#3FB950"
            : state === "error"
              ? "#F85149"
              : "#6B7280"
        }
      >
        {state === "active" && (
          <animate
            attributeName="opacity"
            values="1;0.4;1"
            dur="0.8s"
            repeatCount="indefinite"
          />
        )}
      </circle>
    </svg>
  );
}
