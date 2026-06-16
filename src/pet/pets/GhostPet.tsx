import type { PetState } from "@/types/pet";

interface Props {
  state: PetState;
}

export function GhostPet({ state }: Props) {
  const bodyColor = state === "error" ? "#FFB0B0" : "#E8EAF0";
  const blushColor = state === "active" ? "#FFB6C1" : "transparent";
  const eyeColor = state === "error" ? "#F85149" : "#1E2128";

  return (
    <svg
      width="100"
      height="120"
      viewBox="0 0 100 115"
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
    >
      {/* Body */}
      <path
        d={`M20,90 L20,45 Q20,15 50,15 Q80,15 80,45 L80,90 Q76,98 68,90 Q60,82 52,90 Q44,98 36,90 Q28,82 20,90Z`}
        fill={bodyColor}
      >
        {state === "active" && (
          <animate
            attributeName="d"
            values="M20,90 L20,45 Q20,15 50,15 Q80,15 80,45 L80,90 Q76,98 68,90 Q60,82 52,90 Q44,98 36,90 Q28,82 20,90Z;M20,88 L20,45 Q20,15 50,15 Q80,15 80,45 L80,88 Q76,96 68,88 Q60,80 52,88 Q44,96 36,88 Q28,80 20,88Z;M20,90 L20,45 Q20,15 50,15 Q80,15 80,45 L80,90 Q76,98 68,90 Q60,82 52,90 Q44,98 36,90 Q28,82 20,90Z"
            dur="0.8s"
            repeatCount="indefinite"
          />
        )}
      </path>

      {/* Body shine */}
      <ellipse cx="40" cy="40" rx="10" ry="18" fill="white" opacity="0.3" />
      <ellipse cx="38" cy="35" rx="5" ry="10" fill="white" opacity="0.2" />

      {/* Eyes */}
      {state === "sleep" ? (
        <>
          <path
            d="M35,48 Q39,52 43,48"
            stroke={eyeColor}
            strokeWidth="2"
            fill="none"
            strokeLinecap="round"
          />
          <path
            d="M55,48 Q59,52 63,48"
            stroke={eyeColor}
            strokeWidth="2"
            fill="none"
            strokeLinecap="round"
          />
        </>
      ) : (
        <>
          <ellipse cx="39" cy="48" rx="6" ry="7" fill={eyeColor} />
          <circle cx="41" cy="46" r="2.5" fill="#fff" opacity="0.8" />
          <circle cx="37" cy="50" r="1" fill="#fff" opacity="0.5" />

          <ellipse cx="59" cy="48" rx="6" ry="7" fill={eyeColor} />
          <circle cx="61" cy="46" r="2.5" fill="#fff" opacity="0.8" />
          <circle cx="57" cy="50" r="1" fill="#fff" opacity="0.5" />
        </>
      )}

      {/* Cheeks */}
      <ellipse cx="30" cy="56" rx="5" ry="3" fill={blushColor} opacity="0.6" />
      <ellipse cx="68" cy="56" rx="5" ry="3" fill={blushColor} opacity="0.6" />

      {/* Mouth */}
      {state === "error" ? (
        <path
          d="M43,62 Q49,58 55,62"
          stroke={eyeColor}
          strokeWidth="1.5"
          fill="none"
          strokeLinecap="round"
        />
      ) : state === "active" ? (
        <ellipse cx="49" cy="62" rx="5" ry="4" fill={eyeColor} opacity="0.6" />
      ) : (
        <path
          d="M44,61 Q49,66 54,61"
          stroke={eyeColor}
          strokeWidth="1.5"
          fill="none"
          strokeLinecap="round"
        />
      )}

      {/* Little arms */}
      <ellipse
        cx="18"
        cy="60"
        rx="6"
        ry="4"
        fill={bodyColor}
        transform="rotate(-20, 18, 60)"
      >
        {state === "active" && (
          <animateTransform
            attributeName="transform"
            type="rotate"
            values="-20 18 60;-30 18 60;-10 18 60;-20 18 60"
            dur="0.5s"
            repeatCount="indefinite"
          />
        )}
      </ellipse>
      <ellipse
        cx="82"
        cy="60"
        rx="6"
        ry="4"
        fill={bodyColor}
        transform="rotate(20, 82, 60)"
      >
        {state === "active" && (
          <animateTransform
            attributeName="transform"
            type="rotate"
            values="20 82 60;30 82 60;10 82 60;20 82 60"
            dur="0.5s"
            repeatCount="indefinite"
          />
        )}
      </ellipse>

      {/* Halo / glow for ghost effect */}
      <ellipse cx="50" cy="15" rx="14" ry="3" fill={bodyColor} opacity="0.3" />
    </svg>
  );
}
