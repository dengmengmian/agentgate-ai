import { useState, useEffect, useRef, useCallback, memo } from "react";
import { getCurrentWindow, LogicalPosition, LogicalSize } from "@tauri-apps/api/window";
import { events } from "@/lib/bindings";
import { getPetSettings, updatePetSettings, getPetGatewayState, getPetGatewayStateLite, getPetMemory, savePetMemory, petChat, getPetClickThrough, showPetContextMenu } from "@/lib/api";
import type { PetType, PetState } from "@/types/pet";
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
import { getGreeting } from "./greetings";
import { buildSystemPrompt, pickPokeReaction } from "./personas";
import "./pet.css";

const SLEEP_TIMEOUT = 5 * 60 * 1000;
// 兜底轮询——Rust 端 gateway start/stop/restart 会 emit `pet-gateway-state-changed`
// 让前端立即更新,这个轮询只为兜住偶发漏掉的 + 跟踪 active 状态(请求路径不发事件)。
const POLL_INTERVAL = 10000;
const ERROR_COOLDOWN = 10000;
const MAX_HISTORY = 10;
const STATS_INTERVAL = 30 * 60 * 1000; // show stats every 30 min
const DRAG_THRESHOLD = 4; // px before mousedown promotes to drag
const POKE_DURATION = 400; // matches pet.css @keyframes poke
// 气泡显示时窗口往上扩展,容下多行内容;消失还原。idle 时窗口保持小尺寸不挡底层。
const BUBBLE_EXPAND = 60;

// memo 一次拿 9 个版本——bubble / chatMode / clickThrough 等无关 state 变化时,
// 只要 state prop(idle/active/sleep/error/poke)没变就跳过 SVG reconcile。
const PET_COMPONENTS: Record<PetType, React.ComponentType<{ state: PetState }>> = {
  robot: memo(RobotPet), "pixel-cat": memo(PixelCat), slime: memo(SlimePet),
  fox: memo(FoxPet), octopus: memo(OctopusPet), ghost: memo(GhostPet),
  ox: memo(OxPet), soldier: memo(SuperSoldierPet), coder: memo(CoderPet),
};

interface BubbleData { text: string; type: BubbleType; key: number }

const compactNumber = (n: number) => {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return `${n}`;
};

export function PetApp() {
  const [petType, setPetType] = useState<PetType>("robot");
  const [gatewayState, setGatewayState] = useState<"running" | "stopped" | "active">("stopped");
  const [isSleeping, setIsSleeping] = useState(false);
  const [isError, setIsError] = useState(false);
  const [bubble, setBubble] = useState<BubbleData | null>(null);
  const [chatMode, setChatMode] = useState(false);
  const [chatInput, setChatInput] = useState("");
  const [chatLoading, setChatLoading] = useState(false);
  const [isPoked, setIsPoked] = useState(false);
  const [clickThrough, setClickThroughLocal] = useState<boolean>(false);
  const petTypeRef = useRef<PetType>("robot");
  const dragRef = useRef<{ x: number; y: number; dragging: boolean } | null>(null);
  const pokeTimerRef = useRef<ReturnType<typeof setTimeout>>(undefined);
  // 当前窗口"展开偏移量"。onMoved 写 DB 时反向加回去,
  // 保证持久化的位置永远是 idle(未扩展)窗口的左上角。
  const expansionRef = useRef(0);
  const lastActivityRef = useRef(Date.now());
  const sleepTimerRef = useRef<ReturnType<typeof setTimeout>>(undefined);
  const lastErrorTsRef = useRef("");
  const bubbleKeyRef = useRef(0);
  const chatHistoryRef = useRef<Array<{ role: string; content: string }>>([]);
  const memoryRef = useRef<Record<string, string>>({});
  const inputRef = useRef<HTMLInputElement>(null);
  const chatInputRef = useRef("");
  const lastStatsRef = useRef(0);
  const gatewayStateRef = useRef<"running" | "stopped" | "active">("stopped");
  const petRef = useRef<HTMLDivElement>(null);
  const petStateRef = useRef<PetState>("idle");
  const chatModeRef = useRef(false);

  const locale = navigator.language.startsWith("zh") ? "zh" as const : "en" as const;

  // ── Helpers ──

  const showBubble = useCallback((text: string, type: BubbleType) => {
    bubbleKeyRef.current += 1;
    setBubble({ text, type, key: bubbleKeyRef.current });
  }, []);

  const dismissBubble = useCallback(() => setBubble(null), []);

  // ── Init: load settings + memory + startup greeting ──

  useEffect(() => { petTypeRef.current = petType; }, [petType]);

  useEffect(() => {
    getPetSettings().then((s) => setPetType(s.pet_type as PetType)).catch(() => {});

    getPetMemory().then((raw) => {
      try { memoryRef.current = JSON.parse(raw); } catch { memoryRef.current = {}; }
      // Startup greeting (delayed so window renders first)
      setTimeout(() => {
        const name = memoryRef.current.name;
        if (name) {
          showBubble(
            locale === "zh" ? `${name}，${getGreeting("stopped", "zh")}` : `Hey ${name}! ${getGreeting("stopped", "en")}`,
            "chat"
          );
        } else {
          showBubble(getGreeting("stopped", locale), "chat");
        }
      }, 1500);
    }).catch(() => {});
  }, [locale, showBubble]);

  // ── Event listeners ──

  useEffect(() => {
    const unlisten = events.petSettingsChanged.listen((e) => setPetType(e.payload.pet_type as PetType));
    return () => { unlisten.then((fn) => fn()); };
  }, []);

  useEffect(() => {
    const unlisten = events.petBubble.listen((e) => {
      const text = locale === "zh" && e.payload.text_zh ? e.payload.text_zh : e.payload.text;
      showBubble(text, e.payload.type as "info" | "success" | "error" | "chat");
    });
    return () => { unlisten.then((fn) => fn()); };
  }, [locale, showBubble]);

  // ── Poll gateway state + errors (10s, lite) ──
  // 只取 state + last_error,不读全表 stats(那个走单独的 30 分钟 timer)。
  useEffect(() => {
    const poll = () => {
      if (document.hidden) return;
      getPetGatewayStateLite().then((info) => {
        setGatewayState((prev) => {
          if (prev !== info.state) {
            setIsSleeping(false);
            lastActivityRef.current = Date.now();
            if (sleepTimerRef.current) clearTimeout(sleepTimerRef.current);
            sleepTimerRef.current = setTimeout(() => setIsSleeping(true), SLEEP_TIMEOUT);
          }
          return info.state;
        });
        gatewayStateRef.current = info.state;

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
    const onVisible = () => { if (!document.hidden) poll(); };
    document.addEventListener("visibilitychange", onVisible);
    const unEvent = events.petGatewayStateChanged.listen(() => poll());
    return () => {
      clearInterval(id);
      document.removeEventListener("visibilitychange", onVisible);
      unEvent.then((fn) => fn());
    };
  }, [showBubble, locale]);

  // ── Stats bubble (30 min,跑全表聚合) ──
  // 拆出来后只在真正要弹气泡时调一次 full,平时不烧 DB。
  useEffect(() => {
    const tick = () => {
      if (document.hidden) return;
      if (gatewayStateRef.current === "stopped") return;
      if (Date.now() - lastStatsRef.current < STATS_INTERVAL) return;
      getPetGatewayState().then((info) => {
        if (!info.today || info.today.requests <= 0) return;
        lastStatsRef.current = Date.now();
        const tokens = (info.today.input_tokens ?? 0) + (info.today.output_tokens ?? 0);
        const cost = info.today.cost > 0 ? ` | $${info.today.cost.toFixed(2)}` : "";
        const errorText = (info.today.errors ?? 0) > 0
          ? locale === "zh" ? ` | ${info.today.errors} 错误` : ` | ${info.today.errors} err`
          : "";
        const tokenText = tokens > 0
          ? locale === "zh" ? ` | ${compactNumber(tokens)} tokens` : ` | ${compactNumber(tokens)} tok`
          : "";
        showBubble(
          locale === "zh"
            ? `今日: ${info.today.requests} 请求${tokenText}${cost}${errorText}`
            : `Today: ${info.today.requests} req${tokenText}${cost}${errorText}`,
          "info"
        );
      }).catch(() => {});
    };
    const id = setInterval(tick, STATS_INTERVAL);
    return () => clearInterval(id);
  }, [locale, showBubble]);

  // ── Sleep timer ──

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

  // ── State ──

  const petState: PetState = isPoked ? "poke" : isError ? "error" : gatewayState === "active" ? "active" : isSleeping ? "sleep" : "idle";

  // ── Poke ──

  const triggerPoke = useCallback(() => {
    if (pokeTimerRef.current) clearTimeout(pokeTimerRef.current);
    setIsPoked(true);
    pokeTimerRef.current = setTimeout(() => setIsPoked(false), POKE_DURATION);
    showBubble(pickPokeReaction(petTypeRef.current, locale), "chat");
    resetSleepTimer();
  }, [locale, resetSleepTimer, showBubble]);

  // ── Drag (threshold) + click → poke ──

  const handleMouseDown = useCallback((e: React.MouseEvent) => {
    if (e.button !== 0 || chatMode) return;
    dragRef.current = { x: e.clientX, y: e.clientY, dragging: false };
  }, [chatMode]);

  // 合并:drag 检测 + eye-follow,一个 mousemove handler。
  // eye-follow 只在 idle && !chatMode 时算,用 rAF 节流到 60fps,
  // 通过 petRef 直接写 DOM transform,不走 React state(原来 setLookAngle 每次 mousemove 都 re-render)。
  useEffect(() => {
    let pendingX = 0;
    let pendingY = 0;
    let rafScheduled = false;

    const handleMove = (e: MouseEvent) => {
      if (document.hidden) return;

      // drag detection
      const s = dragRef.current;
      if (s && !s.dragging) {
        if (Math.abs(e.clientX - s.x) > DRAG_THRESHOLD || Math.abs(e.clientY - s.y) > DRAG_THRESHOLD) {
          s.dragging = true;
          getCurrentWindow().startDragging();
        }
      }

      // eye-follow: 仅 idle / 非聊天状态。rAF 节流 + 直写 transform。
      if (petStateRef.current === "idle" && !chatModeRef.current) {
        pendingX = e.clientX;
        pendingY = e.clientY;
        if (!rafScheduled) {
          rafScheduled = true;
          requestAnimationFrame(() => {
            rafScheduled = false;
            if (!petRef.current) return;
            const cx = window.innerWidth / 2;
            const cy = window.innerHeight / 2;
            const dx = (pendingX - cx) / cx;
            const dy = (pendingY - cy) / cy;
            petRef.current.style.transform = `rotateY(${(dx * 3).toFixed(2)}deg) rotateX(${(-dy * 2).toFixed(2)}deg)`;
          });
        }
      }
    };

    const handleUp = () => {
      const s = dragRef.current;
      if (s && !s.dragging) triggerPoke();
      dragRef.current = null;
    };

    window.addEventListener("mousemove", handleMove);
    window.addEventListener("mouseup", handleUp);
    return () => {
      window.removeEventListener("mousemove", handleMove);
      window.removeEventListener("mouseup", handleUp);
    };
  }, [triggerPoke]);

  // 同步 ref(供合并的 mousemove handler 用 — 不依赖 React state closure)
  useEffect(() => { petStateRef.current = petState; }, [petState]);
  useEffect(() => { chatModeRef.current = chatMode; }, [chatMode]);

  // 非 idle / 聊天时清掉 inline transform,让 CSS class 动画接管
  useEffect(() => {
    if (petState !== "idle" || chatMode) {
      if (petRef.current) petRef.current.style.transform = "";
    }
  }, [petState, chatMode]);

  // ── Double-click: open chat ──

  const handleDoubleClick = useCallback(() => {
    if (chatMode) return;
    if (pokeTimerRef.current) { clearTimeout(pokeTimerRef.current); setIsPoked(false); }
    dragRef.current = null;
    resetSleepTimer();
    setChatMode(true);
    setTimeout(() => inputRef.current?.focus(), 100);
  }, [chatMode, resetSleepTimer]);

  // ── Right-click: OS 原生菜单 ──
  // Rust 侧构造 popup,事件统一在 lib.rs on_menu_event 里处理。
  // 不再画 HTML 菜单 → 宠物窗口不需要 resize → 菜单展开不挡底层。
  const handleContextMenu = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    dragRef.current = null;
    showPetContextMenu().catch(() => {});
  }, []);

  // 「清空记忆」由原生菜单触发,Rust 写 DB + emit。前端清本地缓存 + 提示气泡。
  useEffect(() => {
    const un = events.petMemoryReset.listen(() => {
      memoryRef.current = {};
      chatHistoryRef.current = [];
      showBubble(locale === "zh" ? "记忆已清空 ✨" : "Memory cleared ✨", "info");
    });
    return () => { un.then((fn) => fn()); };
  }, [locale, showBubble]);

  // ── Click-through (鼠标穿透到下方应用) ──
  //
  // 真值在 Rust AppState,Pet 启动时拉一遍 + 监听 changed 事件。
  // 右键菜单 / Settings / tray 三处都改 Rust,这里只是镜像 + 应用窗口设置。
  const applyClickThrough = useCallback(async (on: boolean) => {
    try { await getCurrentWindow().setIgnoreCursorEvents(on); } catch { /* ignore */ }
    setClickThroughLocal(on);
  }, []);

  useEffect(() => {
    getPetClickThrough().then(applyClickThrough).catch(() => {});
    const un = events.petClickThroughChanged.listen((e) => applyClickThrough(e.payload));
    return () => { un.then((fn) => fn()); };
  }, [applyClickThrough]);


  // ── Chat submit with AI fallback ──

  const handleChatSubmit = useCallback(async () => {
    const msg = chatInputRef.current.trim();
    if (!msg) return;

    setChatInput("");
    chatInputRef.current = "";
    setChatLoading(true);
    setBubble(null);

    // Memory extraction (do before sending so AI also gets it)
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

    // Build messages
    const memStr = Object.entries(memoryRef.current).map(([k, v]) => `${k}: ${v}`).join("; ");
    const sysContent = buildSystemPrompt(petTypeRef.current, locale, memStr);

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
      setChatMode(false);
      showBubble(reply, "chat");
    } catch (err) {
      setChatLoading(false);
      setChatMode(false);
      const e = err as { code?: string; message?: string };
      let msg: string;
      if (e?.code === "GATEWAY_NOT_RUNNING") {
        msg = locale === "zh" ? "先启动网关哦" : "Start the gateway first";
      } else if (e?.code === "ACTIVE_PROVIDER_NOT_FOUND") {
        msg = locale === "zh" ? "先选个可用供应商" : "Pick an active provider first";
      } else if (e?.code === "PROVIDER_API_KEY_MISSING") {
        msg = locale === "zh" ? "供应商缺 API Key" : "Provider API key is missing";
      } else if (e?.code === "GATEWAY_AUTH_INVALID" || e?.code === "GATEWAY_AUTH_MISSING") {
        msg = locale === "zh" ? "网关 token 不对" : "Gateway token needs attention";
      } else {
        const short = (e?.message || "request failed").slice(0, 60);
        msg = locale === "zh" ? `调不通: ${short}` : `Call failed: ${short}`;
      }
      showBubble(msg, "error");
    }
  }, [showBubble, locale]);

  // ── Close chat on Escape / outside-click ──

  useEffect(() => {
    if (!chatMode) return;
    const handleKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setChatMode(false);
    };
    const handleDown = (e: MouseEvent) => {
      const t = e.target as Element | null;
      // 点 input 或者它的包装层不关——其他任何地方都关。
      if (t && t.closest(".chat-input-wrap")) return;
      if (!chatLoading) setChatMode(false);
    };
    window.addEventListener("keydown", handleKey);
    window.addEventListener("mousedown", handleDown);
    return () => {
      window.removeEventListener("keydown", handleKey);
      window.removeEventListener("mousedown", handleDown);
    };
  }, [chatMode, chatLoading]);

  // ── Save position (debounced) ──
  //
  // 用户拖窗时 onMoved 每帧触发(60Hz),原来每帧都 IPC + SQLite write + emit。
  // debounce 300ms,等用户停手再写一次。气泡展开/收回触发的 setPosition 也被一并吸收。
  useEffect(() => {
    const win = getCurrentWindow();
    let timer: ReturnType<typeof setTimeout> | undefined;
    let pending: { x: number; y: number } | null = null;
    const unlisten = win.onMoved(({ payload }) => {
      pending = { x: payload.x, y: payload.y + expansionRef.current };
      if (timer) clearTimeout(timer);
      timer = setTimeout(() => {
        if (!pending) return;
        updatePetSettings({ pos_x: pending.x, pos_y: pending.y }).catch(() => {});
        pending = null;
      }, 300);
    });
    return () => {
      if (timer) clearTimeout(timer);
      unlisten.then((fn) => fn());
    };
  }, []);

  // 气泡显示 → 窗口往上撑 BUBBLE_EXPAND;气泡消失 → 还原。
  // 聊天 loading 三个点也是气泡形态,合并到同一开关,避免回复来时窗口"咯噔"一下。
  const hasBubble = bubble !== null || chatLoading;
  useEffect(() => {
    const target = hasBubble ? BUBBLE_EXPAND : 0;
    const current = expansionRef.current;
    if (target === current) return;
    const delta = target - current; // 正 = 扩张,负 = 收回
    (async () => {
      try {
        const win = getCurrentWindow();
        const scale = await win.scaleFactor();
        const size = await win.outerSize();
        const pos = await win.outerPosition();
        const w = size.width / scale;
        const h = size.height / scale;
        const x = pos.x / scale;
        const y = pos.y / scale;
        expansionRef.current = target; // 先更新,再 setPosition 触发 onMoved 时反向加回正确
        await win.setSize(new LogicalSize(w, h + delta));
        await win.setPosition(new LogicalPosition(x, y - delta));
      } catch { /* ignore */ }
    })();
  }, [hasBubble]);

  // ── Render ──

  const PetComponent = PET_COMPONENTS[petType];

  return (
    <div
      className="pet-container"
      onMouseDown={handleMouseDown}
      onDoubleClick={handleDoubleClick}
      onContextMenu={handleContextMenu}
    >
      <div
        ref={petRef}
        className={chatMode ? "" : `pet-${petState}`}
        style={{
          position: "relative",
          transition: "transform 0.3s ease-out, opacity 0.2s",
          opacity: clickThrough ? 0.45 : 1,
        }}
      >
        {bubble && (
          <Bubble key={bubble.key} text={bubble.text} type={bubble.type} onDone={dismissBubble} />
        )}

        {chatLoading && (
          <div className="bubble bubble-in">
            <div className="bubble-content" style={{ borderColor: "var(--color-accent, #E89850)" }}>
              <div className="chat-loading">
                <span /><span /><span />
              </div>
            </div>
            <div className="bubble-arrow" style={{ borderTopColor: "var(--color-accent, #E89850)" }} />
          </div>
        )}

        <PetComponent state={petState} />
        {petState === "sleep" && !chatMode && <span className="zzz">z</span>}

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
              onBlur={() => { if (!chatLoading) setChatMode(false); }}
              placeholder={locale === "zh" ? "跟我聊天..." : "Chat with me..."}
              disabled={chatLoading}
            />
          </div>
        )}

      </div>
    </div>
  );
}
