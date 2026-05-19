import { useState, useEffect, useRef, useCallback } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { listen } from "@tauri-apps/api/event";
import { getPetSettings, updatePetSettings, getPetGatewayState, getPetMemory, savePetMemory, petChat } from "@/lib/api";
import type { PetType, PetState, PetSettings, PetBubbleEvent } from "@/types/pet";
import { RobotPet } from "./pets/RobotPet";
import { PixelCat } from "./pets/PixelCat";
import { SlimePet } from "./pets/SlimePet";
import { FoxPet } from "./pets/FoxPet";
import { OctopusPet } from "./pets/OctopusPet";
import { GhostPet } from "./pets/GhostPet";
import { OxPet } from "./pets/OxPet";
import { SuperSoldierPet } from "./pets/SuperSoldierPet";
import { CoderPet } from "./pets/CoderPet";
import { Bubble, type BubbleType } from "./Bubble";
import "./pet.css";

const SLEEP_TIMEOUT = 5 * 60 * 1000;
const POLL_INTERVAL = 3000;
const ERROR_COOLDOWN = 10000;
const MAX_HISTORY = 10;

const PET_COMPONENTS: Record<PetType, React.ComponentType<{ state: PetState }>> = {
  robot: RobotPet, "pixel-cat": PixelCat, slime: SlimePet,
  fox: FoxPet, octopus: OctopusPet, ghost: GhostPet, ox: OxPet, soldier: SuperSoldierPet, coder: CoderPet,
};

interface BubbleData { text: string; type: BubbleType; key: number }

const SYSTEM_PROMPT = `You are a cute desktop pet assistant living on the user's screen. You are part of AgentGate, an AI gateway app.
Keep responses SHORT (1-2 sentences, under 50 chars if possible). Be friendly, playful, and use emoji occasionally.
If the user tells you their name or personal info, acknowledge it warmly.
Reply in the same language the user uses. If they write Chinese, reply in Chinese. If English, reply in English.`;

export function PetApp() {
  const [petType, setPetType] = useState<PetType>("robot");
  const [gatewayState, setGatewayState] = useState<"running" | "stopped" | "active">("stopped");
  const [isSleeping, setIsSleeping] = useState(false);
  const [isError, setIsError] = useState(false);
  const [bubble, setBubble] = useState<BubbleData | null>(null);
  const [chatMode, setChatMode] = useState(false);
  const [chatInput, setChatInput] = useState("");
  const [chatLoading, setChatLoading] = useState(false);
  const lastActivityRef = useRef(Date.now());
  const sleepTimerRef = useRef<ReturnType<typeof setTimeout>>(undefined);
  const lastErrorTsRef = useRef("");
  const bubbleKeyRef = useRef(0);
  const chatHistoryRef = useRef<Array<{ role: string; content: string }>>([]);
  const memoryRef = useRef<Record<string, string>>({});
  const inputRef = useRef<HTMLInputElement>(null);
  const chatInputRef = useRef("");

  const locale = navigator.language.startsWith("zh") ? "zh" as const : "en" as const;

  const showBubble = useCallback((text: string, type: BubbleType, duration?: number) => {
    bubbleKeyRef.current += 1;
    setBubble({ text, type, key: bubbleKeyRef.current });
    if (duration !== undefined) {
      // Bubble component handles its own timeout, but we can pass it via key
    }
  }, []);
  const dismissBubble = useCallback(() => setBubble(null), []);

  // Load pet type
  useEffect(() => {
    getPetSettings().then((s) => setPetType(s.pet_type as PetType)).catch(() => {});
  }, []);

  // Listen for settings changes
  useEffect(() => {
    const unlisten = listen<PetSettings>("pet-settings-changed", (e) => setPetType(e.payload.pet_type as PetType));
    return () => { unlisten.then((fn) => fn()); };
  }, []);

  // Listen for bubble events
  useEffect(() => {
    const unlisten = listen<PetBubbleEvent>("pet-bubble", (e) => {
      const text = locale === "zh" && e.payload.text_zh ? e.payload.text_zh : e.payload.text;
      showBubble(text, e.payload.type);
    });
    return () => { unlisten.then((fn) => fn()); };
  }, [locale, showBubble]);

  // Load memory
  useEffect(() => {
    getPetMemory().then((raw) => {
      try { memoryRef.current = JSON.parse(raw); } catch { memoryRef.current = {}; }
    }).catch(() => {});
  }, []);

  // Poll gateway state + errors
  useEffect(() => {
    const poll = () => {
      getPetGatewayState().then((info) => {
        setGatewayState((prev) => {
          if (prev !== info.state) {
            // Wake up on any state change — especially "active"
            setIsSleeping(false);
            lastActivityRef.current = Date.now();
            if (sleepTimerRef.current) clearTimeout(sleepTimerRef.current);
            sleepTimerRef.current = setTimeout(() => setIsSleeping(true), SLEEP_TIMEOUT);
          }
          return info.state;
        });
        if (info.last_error && info.last_error.timestamp !== lastErrorTsRef.current) {
          const age = Date.now() - new Date(info.last_error.timestamp).getTime();
          if (age < ERROR_COOLDOWN) {
            lastErrorTsRef.current = info.last_error.timestamp;
            const p = info.last_error.provider || "";
            const m = info.last_error.message.length > 40 ? info.last_error.message.slice(0, 40) + "..." : info.last_error.message;
            showBubble(p ? `${p}: ${m}` : m, "error");
            setIsError(true);
            setTimeout(() => setIsError(false), 3000);
          }
        }
      }).catch(() => {});
    };
    poll();
    const id = setInterval(poll, POLL_INTERVAL);
    return () => clearInterval(id);
  }, [showBubble]);

  // Sleep timer
  const resetSleepTimer = useCallback(() => {
    setIsSleeping(false);
    lastActivityRef.current = Date.now();
    if (sleepTimerRef.current) clearTimeout(sleepTimerRef.current);
    sleepTimerRef.current = setTimeout(() => setIsSleeping(true), SLEEP_TIMEOUT);
  }, []);

  useEffect(() => {
    resetSleepTimer();
    return () => { if (sleepTimerRef.current) clearTimeout(sleepTimerRef.current); };
  }, [resetSleepTimer]);

  // Pet state
  // active requests and errors always override sleep
  const petState: PetState = isError ? "error" : gatewayState === "active" ? "active" : isSleeping ? "sleep" : "idle";

  // Drag
  const handleMouseDown = useCallback((e: React.MouseEvent) => {
    if (e.button === 0 && !chatMode) getCurrentWindow().startDragging();
  }, [chatMode]);

  // Click: toggle chat mode or show greeting
  const handleClick = useCallback(() => {
    if (chatMode) return; // don't interfere when chat is open
    resetSleepTimer();
    setChatMode(true);
    setTimeout(() => inputRef.current?.focus(), 100);
  }, [chatMode, resetSleepTimer]);

  // Send chat message
  const handleChatSubmit = useCallback(async () => {
    const msg = chatInputRef.current.trim();
    if (!msg) return;

    setChatInput("");
    chatInputRef.current = "";
    setChatLoading(true);
    setBubble(null);

    // Build messages with system prompt + memory + history
    const memStr = Object.entries(memoryRef.current)
      .map(([k, v]) => `${k}: ${v}`)
      .join("; ");
    const sysContent = SYSTEM_PROMPT + (memStr ? `\n\nYou remember about the user: ${memStr}` : "");

    chatHistoryRef.current.push({ role: "user", content: msg });
    if (chatHistoryRef.current.length > MAX_HISTORY) {
      chatHistoryRef.current = chatHistoryRef.current.slice(-MAX_HISTORY);
    }

    const messages = [
      { role: "system", content: sysContent },
      ...chatHistoryRef.current,
    ];

    try {
      const reply = await petChat(messages);
      chatHistoryRef.current.push({ role: "assistant", content: reply });
      setChatLoading(false);
      showBubble(reply, "chat");

      // Simple memory extraction
      const namePatterns = [
        /my name is\s+(\S+)/i, /i'?m\s+(\S+)/i, /call me\s+(\S+)/i,
        /我叫(.{1,10})/, /我是(.{1,10})/, /叫我(.{1,10})/,
      ];
      for (const pat of namePatterns) {
        const m = msg.match(pat);
        if (m) {
          memoryRef.current.name = m[1].trim();
          savePetMemory(JSON.stringify(memoryRef.current)).catch(() => {});
          break;
        }
      }
    } catch (err: unknown) {
      setChatLoading(false);
      const errMsg = err && typeof err === "object" && "message" in err
        ? String((err as { message: string }).message)
        : "Connection failed";
      const short = errMsg.length > 30 ? errMsg.slice(0, 30) + "..." : errMsg;
      showBubble(short, "error");
    }
  }, [showBubble]);

  // Close chat on Escape or click outside
  useEffect(() => {
    if (!chatMode) return;
    const handleKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setChatMode(false);
    };
    window.addEventListener("keydown", handleKey);
    return () => window.removeEventListener("keydown", handleKey);
  }, [chatMode]);

  // Save position
  useEffect(() => {
    const win = getCurrentWindow();
    const unlisten = win.onMoved(({ payload }) => {
      updatePetSettings({ pos_x: payload.x, pos_y: payload.y }).catch(() => {});
    });
    return () => { unlisten.then((fn) => fn()); };
  }, []);

  const PetComponent = PET_COMPONENTS[petType];

  return (
    <div
      className="pet-container"
      onMouseDown={handleMouseDown}
      onClick={handleClick}
    >
      <div className={chatMode ? "" : `pet-${petState}`} style={{ position: "relative" }}>
        {/* Bubble */}
        {bubble && (
          <Bubble key={bubble.key} text={bubble.text} type={bubble.type} onDone={dismissBubble} />
        )}

        {/* Loading indicator */}
        {chatLoading && (
          <div className="bubble bubble-in">
            <div className="bubble-content" style={{ borderColor: "#7C8CFF" }}>
              <div className="chat-loading">
                <span /><span /><span />
              </div>
            </div>
            <div className="bubble-arrow" style={{ borderTopColor: "#7C8CFF" }} />
          </div>
        )}

        <PetComponent state={petState} />
        {petState === "sleep" && !chatMode && <span className="zzz">z</span>}

        {/* Chat input */}
        {chatMode && (
          <div className="chat-input-wrap" onClick={(e) => e.stopPropagation()}>
            <input
              ref={inputRef}
              className="chat-input"
              value={chatInput}
              onChange={(e) => { setChatInput(e.target.value); chatInputRef.current = e.target.value; }}
              onKeyDown={(e) => {
                if (e.key === "Enter") handleChatSubmit();
                e.stopPropagation();
              }}
              onMouseDown={(e) => e.stopPropagation()}
              placeholder={locale === "zh" ? "跟我聊天..." : "Chat with me..."}
              disabled={chatLoading}
            />
          </div>
        )}
      </div>
    </div>
  );
}
