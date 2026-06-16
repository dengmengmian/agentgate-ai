import { useEffect, useRef, useState } from "react";

export type BubbleType = "info" | "success" | "error" | "chat";

interface Props {
  text: string;
  type: BubbleType;
  duration?: number;
  onDone: () => void;
}

export function Bubble({
  text,
  type,
  duration = type === "chat" ? 10000 : 4000,
  onDone,
}: Props) {
  const [leaving, setLeaving] = useState(false);
  const pausedRef = useRef(false);
  const leavingRef = useRef(false);

  useEffect(() => {
    let elapsed = 0;
    let last = performance.now();
    let rafId = 0;
    const tick = (now: number) => {
      if (!pausedRef.current) elapsed += now - last;
      last = now;
      if (elapsed >= duration) {
        onDone();
        return;
      }
      if (!leavingRef.current && elapsed >= duration - 500) {
        leavingRef.current = true;
        setLeaving(true);
      }
      rafId = requestAnimationFrame(tick);
    };
    rafId = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(rafId);
  }, [duration, onDone]);

  const borderColor =
    type === "error"
      ? "#F85149"
      : type === "success"
        ? "#3FB950"
        : type === "chat"
          ? "#7C8CFF"
          : "#6B7280";

  return (
    <div
      className={`bubble ${leaving ? "bubble-out" : "bubble-in"}`}
      onMouseEnter={() => {
        pausedRef.current = true;
        leavingRef.current = false;
        setLeaving(false);
      }}
      onMouseLeave={() => {
        pausedRef.current = false;
      }}
      onMouseDown={(e) => e.stopPropagation()}
      onClick={(e) => {
        e.stopPropagation();
        onDone();
      }}
    >
      <div className="bubble-content" style={{ borderColor }}>
        <span className="bubble-text">{text}</span>
      </div>
      <div className="bubble-arrow" style={{ borderTopColor: borderColor }} />
    </div>
  );
}
