import type { PetState } from "@/types/pet";

interface Props {
  state: PetState;
}

export function OctopusPet({ state }: Props) {
  const bodyColor = state === "error" ? "#E88B8B" : "#C4A6E8";
  const bodyDark = state === "error" ? "#C06060" : "#9B7DC8";
  const eyeColor = state === "sleep" ? "#6B7280" : "#FFFFFF";
  const tentacleColor = state === "error" ? "#D47070" : "#B090D8";

  const tentacleWave = state === "active" ? "0.4s" : "1.5s";

  return (
    <svg
      width="100"
      height="120"
      viewBox="0 0 100 120"
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
    >
      {/* Head / Body (dome shape) */}
      <ellipse cx="50" cy="42" rx="30" ry="32" fill={bodyColor} />

      {/* Head highlight */}
      <ellipse cx="42" cy="30" rx="12" ry="16" fill={bodyDark} opacity="0.2" />
      <ellipse cx="40" cy="28" rx="6" ry="10" fill="white" opacity="0.1" />

      {/* Spots */}
      <circle cx="62" cy="28" r="3" fill={bodyDark} opacity="0.3" />
      <circle cx="68" cy="38" r="2" fill={bodyDark} opacity="0.3" />
      <circle cx="35" cy="22" r="2.5" fill={bodyDark} opacity="0.3" />

      {/* Eyes */}
      {state === "sleep" ? (
        <>
          <path
            d="M36,42 Q40,45 44,42"
            stroke={eyeColor}
            strokeWidth="2"
            fill="none"
            strokeLinecap="round"
          />
          <path
            d="M56,42 Q60,45 64,42"
            stroke={eyeColor}
            strokeWidth="2"
            fill="none"
            strokeLinecap="round"
          />
        </>
      ) : (
        <>
          <ellipse cx="40" cy="42" rx="7" ry="8" fill="white" />
          <circle cx="41" cy="43" r="4" fill="#1E2128">
            {state === "active" && (
              <animate
                attributeName="cx"
                values="41;39;43;41"
                dur="0.6s"
                repeatCount="indefinite"
              />
            )}
          </circle>
          <circle cx="42.5" cy="41.5" r="1.5" fill="#fff" />

          <ellipse cx="60" cy="42" rx="7" ry="8" fill="white" />
          <circle cx="61" cy="43" r="4" fill="#1E2128">
            {state === "active" && (
              <animate
                attributeName="cx"
                values="61;59;63;61"
                dur="0.6s"
                repeatCount="indefinite"
              />
            )}
          </circle>
          <circle cx="62.5" cy="41.5" r="1.5" fill="#fff" />
        </>
      )}

      {/* Mouth */}
      {state === "error" ? (
        <ellipse cx="50" cy="56" rx="4" ry="3" fill="#1E2128" />
      ) : state === "active" ? (
        <path
          d="M45,54 Q50,60 55,54"
          stroke="#1E2128"
          strokeWidth="1.5"
          fill="none"
          strokeLinecap="round"
        />
      ) : (
        <path
          d="M46,55 Q50,58 54,55"
          stroke="#1E2128"
          strokeWidth="1.5"
          fill="none"
          strokeLinecap="round"
        />
      )}

      {/* Tentacles */}
      {/* Left outer */}
      <path
        d="M22,65 Q14,80 18,95 Q20,100 24,96"
        stroke={tentacleColor}
        strokeWidth="5"
        fill="none"
        strokeLinecap="round"
      >
        <animateTransform
          attributeName="transform"
          type="rotate"
          values="0 22 65;-8 22 65;0 22 65;8 22 65;0 22 65"
          dur={tentacleWave}
          repeatCount="indefinite"
        />
      </path>

      {/* Left inner */}
      <path
        d="M32,68 Q28,82 30,95 Q31,100 34,96"
        stroke={tentacleColor}
        strokeWidth="5"
        fill="none"
        strokeLinecap="round"
      >
        <animateTransform
          attributeName="transform"
          type="rotate"
          values="0 32 68;5 32 68;0 32 68;-5 32 68;0 32 68"
          dur={tentacleWave}
          repeatCount="indefinite"
        />
      </path>

      {/* Center left */}
      <path
        d="M42,70 Q40,85 42,98 Q43,102 45,98"
        stroke={tentacleColor}
        strokeWidth="5"
        fill="none"
        strokeLinecap="round"
      >
        <animateTransform
          attributeName="transform"
          type="rotate"
          values="0 42 70;-3 42 70;3 42 70;0 42 70"
          dur={tentacleWave}
          repeatCount="indefinite"
        />
      </path>

      {/* Center right */}
      <path
        d="M58,70 Q60,85 58,98 Q57,102 55,98"
        stroke={tentacleColor}
        strokeWidth="5"
        fill="none"
        strokeLinecap="round"
      >
        <animateTransform
          attributeName="transform"
          type="rotate"
          values="0 58 70;3 58 70;-3 58 70;0 58 70"
          dur={tentacleWave}
          repeatCount="indefinite"
        />
      </path>

      {/* Right inner */}
      <path
        d="M68,68 Q72,82 70,95 Q69,100 66,96"
        stroke={tentacleColor}
        strokeWidth="5"
        fill="none"
        strokeLinecap="round"
      >
        <animateTransform
          attributeName="transform"
          type="rotate"
          values="0 68 68;-5 68 68;0 68 68;5 68 68;0 68 68"
          dur={tentacleWave}
          repeatCount="indefinite"
        />
      </path>

      {/* Right outer */}
      <path
        d="M78,65 Q86,80 82,95 Q80,100 76,96"
        stroke={tentacleColor}
        strokeWidth="5"
        fill="none"
        strokeLinecap="round"
      >
        <animateTransform
          attributeName="transform"
          type="rotate"
          values="0 78 65;8 78 65;0 78 65;-8 78 65;0 78 65"
          dur={tentacleWave}
          repeatCount="indefinite"
        />
      </path>

      {/* Suction cups (small dots on two tentacles) */}
      <circle cx="18" cy="88" r="1.5" fill={bodyDark} opacity="0.4" />
      <circle cx="20" cy="82" r="1.5" fill={bodyDark} opacity="0.4" />
      <circle cx="80" cy="82" r="1.5" fill={bodyDark} opacity="0.4" />
      <circle cx="82" cy="88" r="1.5" fill={bodyDark} opacity="0.4" />
    </svg>
  );
}
