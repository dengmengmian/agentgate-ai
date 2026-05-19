import { useEffect, useState } from "react";

export type BubbleType = "info" | "success" | "error" | "chat";

interface Props {
  text: string;
  type: BubbleType;
  duration?: number;
  onDone: () => void;
}

export function Bubble({ text, type, duration = type === "chat" ? 10000 : 4000, onDone }: Props) {
  const [leaving, setLeaving] = useState(false);

  useEffect(() => {
    const fadeTimer = setTimeout(() => setLeaving(true), duration - 500);
    const doneTimer = setTimeout(onDone, duration);
    return () => {
      clearTimeout(fadeTimer);
      clearTimeout(doneTimer);
    };
  }, [duration, onDone]);

  const borderColor =
    type === "error" ? "#F85149"
    : type === "success" ? "#3FB950"
    : type === "chat" ? "#7C8CFF"
    : "#6B7280";

  return (
    <div className={`bubble ${leaving ? "bubble-out" : "bubble-in"}`}>
      <div
        className="bubble-content"
        style={{ borderColor }}
      >
        <span className="bubble-text">{text}</span>
      </div>
      <div className="bubble-arrow" style={{ borderTopColor: borderColor }} />
    </div>
  );
}
