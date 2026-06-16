import type { PetState } from "@/types/pet";

interface Props {
  state: PetState;
}

export function OxPet({ state }: Props) {
  const bodyColor = "#8B6B4A";
  const bodyDark = "#6B4F34";
  const bellyColor = "#D4B896";
  const hornColor = "#E8DCC8";
  const eyeColor =
    state === "error" ? "#F85149" : state === "sleep" ? "#6B7280" : "#1E2128";
  const noseRing = "#9BA3AF";

  return (
    <svg
      width="100"
      height="120"
      viewBox="0 0 100 120"
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
    >
      {/* Horns */}
      <path d="M24,28 Q18,16 22,10 Q24,8 26,12 Q28,18 28,28" fill={hornColor} />
      <path d="M66,28 Q72,16 68,10 Q66,8 64,12 Q62,18 62,28" fill={hornColor} />

      {/* Ears */}
      <ellipse
        cx="22"
        cy="32"
        rx="8"
        ry="5"
        fill={bodyColor}
        transform="rotate(-20,22,32)"
      />
      <ellipse
        cx="22"
        cy="32"
        rx="5"
        ry="3"
        fill="#D4A07A"
        transform="rotate(-20,22,32)"
      />
      <ellipse
        cx="68"
        cy="32"
        rx="8"
        ry="5"
        fill={bodyColor}
        transform="rotate(20,68,32)"
      />
      <ellipse
        cx="68"
        cy="32"
        rx="5"
        ry="3"
        fill="#D4A07A"
        transform="rotate(20,68,32)"
      />

      {/* Head */}
      <ellipse cx="45" cy="36" rx="24" ry="20" fill={bodyColor} />

      {/* Face patch */}
      <ellipse cx="45" cy="42" rx="16" ry="12" fill={bellyColor} />

      {/* Eyes */}
      {state === "sleep" ? (
        <>
          <path
            d="M33,33 Q37,36 41,33"
            stroke={eyeColor}
            strokeWidth="2"
            fill="none"
            strokeLinecap="round"
          />
          <path
            d="M49,33 Q53,36 57,33"
            stroke={eyeColor}
            strokeWidth="2"
            fill="none"
            strokeLinecap="round"
          />
          {/* Sweat drop (tired ox) */}
          <path
            d="M62,28 Q64,24 62,20"
            stroke="#7C8CFF"
            strokeWidth="1.5"
            fill="none"
            strokeLinecap="round"
          />
          <circle cx="62" cy="29" r="2" fill="#7C8CFF" opacity="0.6" />
        </>
      ) : (
        <>
          <circle cx="37" cy="32" r="4" fill="white" />
          <circle cx="38" cy="33" r="2.5" fill={eyeColor} />
          <circle cx="39" cy="32" r="1" fill="#fff" />

          <circle cx="53" cy="32" r="4" fill="white" />
          <circle cx="54" cy="33" r="2.5" fill={eyeColor} />
          <circle cx="55" cy="32" r="1" fill="#fff" />

          {/* Tired bags under eyes */}
          <path
            d="M33,37 Q37,38 41,37"
            stroke={bodyDark}
            strokeWidth="0.8"
            fill="none"
            opacity="0.4"
          />
          <path
            d="M49,37 Q53,38 57,37"
            stroke={bodyDark}
            strokeWidth="0.8"
            fill="none"
            opacity="0.4"
          />
        </>
      )}

      {/* Nostrils */}
      <ellipse cx="40" cy="44" rx="3" ry="2" fill={bodyDark} />
      <ellipse cx="50" cy="44" rx="3" ry="2" fill={bodyDark} />

      {/* Nose ring */}
      <path
        d="M42,46 Q45,52 48,46"
        stroke={noseRing}
        strokeWidth="1.5"
        fill="none"
        strokeLinecap="round"
      />

      {/* Mouth */}
      {state === "error" ? (
        <path
          d="M38,50 Q45,47 52,50"
          stroke={bodyDark}
          strokeWidth="1.5"
          fill="none"
          strokeLinecap="round"
        />
      ) : state === "active" ? (
        <path
          d="M40,49 Q45,46 50,49"
          stroke={bodyDark}
          strokeWidth="1"
          fill="none"
          strokeLinecap="round"
        />
      ) : (
        <line
          x1="40"
          y1="49"
          x2="50"
          y2="49"
          stroke={bodyDark}
          strokeWidth="1"
          strokeLinecap="round"
        />
      )}

      {/* Body */}
      <rect x="22" y="56" width="50" height="36" rx="8" fill={bodyColor} />

      {/* Belly */}
      <ellipse cx="47" cy="74" rx="14" ry="12" fill={bellyColor} />

      {/* "996" on belly (hardworking ox!) */}
      <text
        x="47"
        y="76"
        textAnchor="middle"
        fontSize="8"
        fontFamily="monospace"
        fill={bodyDark}
        opacity="0.4"
      >
        996
      </text>

      {/* Legs */}
      <rect x="26" y="88" width="10" height="16" rx="4" fill={bodyColor} />
      <rect x="58" y="88" width="10" height="16" rx="4" fill={bodyColor} />

      {/* Hooves */}
      <rect x="25" y="100" width="12" height="6" rx="3" fill={bodyDark} />
      <rect x="57" y="100" width="12" height="6" rx="3" fill={bodyDark} />

      {/* Tail */}
      <path
        d="M72,62 Q80,58 82,52 Q83,50 81,50"
        stroke={bodyColor}
        strokeWidth="3"
        fill="none"
        strokeLinecap="round"
      >
        {state === "active" && (
          <animateTransform
            attributeName="transform"
            type="rotate"
            values="0 72 62;10 72 62;-10 72 62;0 72 62"
            dur="0.4s"
            repeatCount="indefinite"
          />
        )}
      </path>
      <circle cx="81" cy="50" r="3" fill={bodyDark} />

      {/* Steam from head when active (working hard!) */}
      {state === "active" && (
        <>
          <path
            d="M35,14 Q33,8 36,4"
            stroke="#9BA3AF"
            strokeWidth="1.5"
            fill="none"
            opacity="0.5"
            strokeLinecap="round"
          >
            <animate
              attributeName="opacity"
              values="0.5;0;0.5"
              dur="1s"
              repeatCount="indefinite"
            />
          </path>
          <path
            d="M45,12 Q43,6 46,2"
            stroke="#9BA3AF"
            strokeWidth="1.5"
            fill="none"
            opacity="0.3"
            strokeLinecap="round"
          >
            <animate
              attributeName="opacity"
              values="0.3;0;0.3"
              dur="1.2s"
              repeatCount="indefinite"
            />
          </path>
        </>
      )}

      {/* Sweat when error */}
      {state === "error" && (
        <>
          <path
            d="M64,26 Q66,22 64,18"
            stroke="#7C8CFF"
            strokeWidth="1.5"
            fill="none"
            strokeLinecap="round"
          />
          <circle cx="64" cy="27" r="2.5" fill="#7C8CFF" opacity="0.5" />
          <path
            d="M24,26 Q22,22 24,18"
            stroke="#7C8CFF"
            strokeWidth="1.5"
            fill="none"
            strokeLinecap="round"
          />
          <circle cx="24" cy="27" r="2.5" fill="#7C8CFF" opacity="0.5" />
        </>
      )}
    </svg>
  );
}
