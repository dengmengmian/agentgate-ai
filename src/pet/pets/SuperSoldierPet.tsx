import type { PetState } from "@/types/pet";

interface Props {
  state: PetState;
}

export function SuperSoldierPet({ state }: Props) {
  const armorColor = "#4A6FA5";
  const armorDark = "#3A5580";
  const armorLight = "#6B8FC5";
  const visorColor =
    state === "error" ? "#F85149" : state === "active" ? "#3FB950" : "#7C8CFF";
  const capeColor = state === "error" ? "#C04040" : "#C0392B";
  const capeDark = state === "error" ? "#902020" : "#922B21";
  const skinColor = "#F0D5B8";

  return (
    <svg
      width="100"
      height="120"
      viewBox="0 0 100 120"
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
    >
      {/* Cape */}
      <path d="M28,50 Q18,70 22,100 Q30,105 38,98 L38,55Z" fill={capeColor}>
        {state === "active" && (
          <animate
            attributeName="d"
            values="M28,50 Q18,70 22,100 Q30,105 38,98 L38,55Z;M28,50 Q14,72 20,102 Q28,108 38,98 L38,55Z;M28,50 Q18,70 22,100 Q30,105 38,98 L38,55Z"
            dur="0.6s"
            repeatCount="indefinite"
          />
        )}
      </path>
      <path d="M62,55 L62,98 Q70,105 78,100 Q82,70 72,50Z" fill={capeColor}>
        {state === "active" && (
          <animate
            attributeName="d"
            values="M62,55 L62,98 Q70,105 78,100 Q82,70 72,50Z;M62,55 L62,98 Q72,108 80,102 Q86,72 72,50Z;M62,55 L62,98 Q70,105 78,100 Q82,70 72,50Z"
            dur="0.6s"
            repeatCount="indefinite"
          />
        )}
      </path>
      {/* Cape inner shadow */}
      <path
        d="M30,55 Q22,72 24,95 L34,92 L36,58Z"
        fill={capeDark}
        opacity="0.5"
      />
      <path
        d="M64,58 L66,92 L76,95 Q78,72 70,55Z"
        fill={capeDark}
        opacity="0.5"
      />

      {/* Helmet */}
      <path
        d="M30,18 Q30,4 50,4 Q70,4 70,18 L70,38 Q70,48 50,48 Q30,48 30,38Z"
        fill={armorColor}
      />
      {/* Helmet crest/ridge */}
      <rect x="46" y="2" width="8" height="16" rx="4" fill={armorDark} />
      {/* Helmet side panels */}
      <path d="M30,20 Q26,20 26,26 L26,32 Q26,36 30,36" fill={armorDark} />
      <path d="M70,20 Q74,20 74,26 L74,32 Q74,36 70,36" fill={armorDark} />

      {/* Visor */}
      <rect x="34" y="22" width="32" height="14" rx="3" fill="#111" />
      {/* Visor glow */}
      <rect
        x="36"
        y="24"
        width="28"
        height="10"
        rx="2"
        fill={visorColor}
        opacity="0.8"
      >
        {state === "active" && (
          <animate
            attributeName="opacity"
            values="0.8;0.4;0.8"
            dur="0.6s"
            repeatCount="indefinite"
          />
        )}
      </rect>
      {/* Visor eyes slit */}
      {state === "sleep" ? (
        <line
          x1="38"
          y1="29"
          x2="62"
          y2="29"
          stroke="#111"
          strokeWidth="3"
          strokeLinecap="round"
        />
      ) : (
        <>
          <rect
            x="38"
            y="26"
            width="8"
            height="6"
            rx="1"
            fill="#111"
            opacity="0.6"
          />
          <rect
            x="54"
            y="26"
            width="8"
            height="6"
            rx="1"
            fill="#111"
            opacity="0.6"
          />
        </>
      )}

      {/* Chin guard */}
      <path d="M36,38 Q50,50 64,38" fill={armorDark} />

      {/* Body — chest armor */}
      <rect x="30" y="48" width="40" height="32" rx="4" fill={armorColor} />
      {/* Chest plate lines */}
      <line
        x1="50"
        y1="48"
        x2="50"
        y2="80"
        stroke={armorDark}
        strokeWidth="1.5"
      />
      <path
        d="M35,56 L50,64 L65,56"
        stroke={armorDark}
        strokeWidth="1.5"
        fill="none"
      />

      {/* Chest emblem — star */}
      <polygon
        points="50,54 52,58 56,58 53,61 54,65 50,62 46,65 47,61 44,58 48,58"
        fill={visorColor}
      >
        {state === "active" && (
          <animate
            attributeName="opacity"
            values="1;0.5;1"
            dur="0.8s"
            repeatCount="indefinite"
          />
        )}
      </polygon>

      {/* Belt */}
      <rect x="30" y="76" width="40" height="6" rx="2" fill={armorDark} />
      <rect x="46" y="75" width="8" height="8" rx="2" fill="#D4AF37" />

      {/* Shoulder pads */}
      <ellipse
        cx="26"
        cy="52"
        rx="10"
        ry="7"
        fill={armorColor}
        stroke={armorDark}
        strokeWidth="1"
      />
      <ellipse
        cx="74"
        cy="52"
        rx="10"
        ry="7"
        fill={armorColor}
        stroke={armorDark}
        strokeWidth="1"
      />
      {/* Shoulder spikes */}
      <polygon points="20,48 16,42 24,46" fill={armorLight} />
      <polygon points="80,48 84,42 76,46" fill={armorLight} />

      {/* Arms */}
      <rect x="16" y="56" width="12" height="24" rx="5" fill={armorColor} />
      <rect x="72" y="56" width="12" height="24" rx="5" fill={armorColor} />
      {/* Gauntlets */}
      <rect x="16" y="72" width="12" height="10" rx="4" fill={armorDark} />
      <rect x="72" y="72" width="12" height="10" rx="4" fill={armorDark} />
      {/* Fists */}
      <ellipse cx="22" cy="84" rx="5" ry="4" fill={skinColor} />
      <ellipse cx="78" cy="84" rx="5" ry="4" fill={skinColor} />

      {/* Legs */}
      <rect x="34" y="82" width="12" height="20" rx="4" fill={armorDark} />
      <rect x="54" y="82" width="12" height="20" rx="4" fill={armorDark} />

      {/* Boots */}
      <rect x="32" y="98" width="16" height="8" rx="4" fill={armorColor} />
      <rect x="52" y="98" width="16" height="8" rx="4" fill={armorColor} />
      {/* Boot trim */}
      <rect x="32" y="98" width="16" height="3" rx="1.5" fill={armorLight} />
      <rect x="52" y="98" width="16" height="3" rx="1.5" fill={armorLight} />

      {/* Energy effect when active */}
      {state === "active" && (
        <>
          <circle
            cx="50"
            cy="60"
            r="20"
            fill="none"
            stroke={visorColor}
            strokeWidth="0.8"
            opacity="0.3"
          >
            <animate
              attributeName="r"
              values="20;28;20"
              dur="1s"
              repeatCount="indefinite"
            />
            <animate
              attributeName="opacity"
              values="0.3;0;0.3"
              dur="1s"
              repeatCount="indefinite"
            />
          </circle>
        </>
      )}

      {/* Damage cracks when error */}
      {state === "error" && (
        <>
          <path
            d="M42,58 L38,62 L41,64 L37,68"
            stroke="#F85149"
            strokeWidth="1"
            fill="none"
            strokeLinecap="round"
          />
          <path
            d="M60,56 L63,60 L59,63"
            stroke="#F85149"
            strokeWidth="1"
            fill="none"
            strokeLinecap="round"
          />
        </>
      )}
    </svg>
  );
}
