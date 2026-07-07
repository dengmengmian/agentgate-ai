import { useState, useEffect, useRef, useCallback } from "react";
import {
  getCurrentWindow,
  currentMonitor,
  LogicalPosition,
  LogicalSize,
} from "@tauri-apps/api/window";
import { events } from "@/lib/bindings";
import {
  getPetSettings,
  updatePetSettings,
  getPetGatewayState,
  getPetGatewayStateLite,
  getPetMemory,
  getPetChatHistory,
  getPetClickThrough,
  showPetContextMenu,
} from "@/lib/api";
import type { PetType, PetState } from "@/types/pet";
import { PET_COMPONENTS } from "./petComponents";
import { Bubble, type BubbleType } from "./Bubble";
import { getGreeting } from "./greetings";
import {
  pickPokeReaction,
  pickAngryReaction,
  pickSulkReaction,
} from "./personas";
import {
  activityTier,
  pokeMood,
  getDateBadge,
  isOverBudget,
  topicGreeting,
  type PokeMood,
} from "./petLogic";
import { sendPetMessage, parseHistory, type ChatMessage } from "./chatCore";
import {
  petGenerate,
  buildGreetingInstruction,
  buildStatsInstruction,
  buildErrorInstruction,
  buildAmbientInstruction,
} from "./petGenerate";
import "./pet.css";

const SLEEP_TIMEOUT = 5 * 60 * 1000;
// 兜底轮询——Rust 端 gateway start/stop/restart 会 emit `pet-gateway-state-changed`
// 让前端立即更新,这个轮询只为兜住偶发漏掉的 + 跟踪 active 状态(请求路径不发事件)。
const POLL_INTERVAL = 10000;
const ERROR_COOLDOWN = 10000;
const STATS_INTERVAL = 30 * 60 * 1000; // show stats every 30 min
const DRAG_THRESHOLD = 4; // px before mousedown promotes to drag
const POKE_DURATION = 400; // matches pet.css @keyframes poke
// 气泡显示时窗口往上扩展,容下多行内容;消失还原。idle 时窗口保持小尺寸不挡底层。
const BUBBLE_EXPAND = 60;
// ── 趣味行为参数 ──
const POKE_STREAK_WINDOW = 4000; // 两次戳间隔小于这个才算"连戳"
const ANGRY_DURATION = 1500;
const SULK_DURATION = 3000;
const CC_WORKING_TIMEOUT = 3 * 60 * 1000; // hook 漏发 done 时徽章兜底自动消失
const CC_DONE_LINGER = 6000;
const CELEBRATE_DURATION = 1200; // 对应 pet.css celebrate-jump 0.55s × 2
const DATE_BADGE_REFRESH = 30 * 60 * 1000;
// 贴边挂靠:窗口拖到距屏幕左右边缘 DOCK_TRIGGER 逻辑 px 内时吸附,只露 DOCK_PEEK 宽。
const DOCK_TRIGGER = 14;
const DOCK_PEEK = 70;
const PET_WIN_WIDTH = 140; // 与 Rust 侧 PET_WIDTH 一致
// 主动搭话:每 AMBIENT_TICK 检查一次,满足条件且距上次搭话 > AMBIENT_MIN_GAP 才说。
const AMBIENT_TICK = 2 * 60 * 1000;
const AMBIENT_MIN_GAP = 8 * 60 * 1000;

interface BubbleData {
  text: string;
  type: BubbleType;
  key: number;
  isCc?: boolean;
}

const compactNumber = (n: number) => {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return `${n}`;
};

export function PetApp() {
  const [petType, setPetType] = useState<PetType>("robot");
  const [gatewayState, setGatewayState] = useState<
    "running" | "stopped" | "active"
  >("stopped");
  const [isSleeping, setIsSleeping] = useState(false);
  const [isError, setIsError] = useState(false);
  const [bubble, setBubble] = useState<BubbleData | null>(null);
  const [chatMode, setChatMode] = useState(false);
  const [chatInput, setChatInput] = useState("");
  const [chatLoading, setChatLoading] = useState(false);
  const [isPoked, setIsPoked] = useState(false);
  const [clickThrough, setClickThroughLocal] = useState<boolean>(false);
  // ── 趣味行为 state ──
  const [ccBadge, setCcBadge] = useState<"working" | "waiting" | "done" | null>(
    null
  );
  const [celebrate, setCelebrate] = useState(false);
  const [activeTier, setActiveTier] = useState<1 | 2 | 3>(1);
  const [mood, setMood] = useState<PokeMood>("normal");
  const [chubby, setChubby] = useState(false);
  const [dateBadge, setDateBadge] = useState<string | null>(() =>
    getDateBadge(new Date())
  );
  const petTypeRef = useRef<PetType>("robot");
  const dragRef = useRef<{ x: number; y: number; dragging: boolean } | null>(
    null
  );
  const pokeTimerRef = useRef<ReturnType<typeof setTimeout>>(undefined);
  // 当前窗口"展开偏移量"。onMoved 写 DB 时反向加回去,
  // 保证持久化的位置永远是 idle(未扩展)窗口的左上角。
  const expansionRef = useRef(0);
  const sleepTimerRef = useRef<ReturnType<typeof setTimeout>>(undefined);
  const lastErrorTsRef = useRef("");
  const bubbleKeyRef = useRef(0);
  const chatHistoryRef = useRef<ChatMessage[]>([]);
  const memoryRef = useRef<Record<string, string>>({});
  const inputRef = useRef<HTMLInputElement>(null);
  const chatInputRef = useRef("");
  const lastStatsRef = useRef(0);
  const gatewayStateRef = useRef<"running" | "stopped" | "active">("stopped");
  const petRef = useRef<HTMLDivElement>(null);
  const petStateRef = useRef<PetState>("idle");
  const chatModeRef = useRef(false);
  // ── 趣味行为 refs ──
  const moodRef = useRef<PokeMood>("normal");
  const moodTimerRef = useRef<ReturnType<typeof setTimeout>>(undefined);
  const pokeStreakRef = useRef(0);
  const lastPokeTsRef = useRef(0);
  const ccBadgeTimerRef = useRef<ReturnType<typeof setTimeout>>(undefined);
  const celebrateTimerRef = useRef<ReturnType<typeof setTimeout>>(undefined);
  const dockedRef = useRef<"left" | "right" | null>(null);
  // ── 主动搭话 refs ──(interval 闭包读 ref,避免陈旧 state)
  const bubbleActiveRef = useRef(false);
  const clickThroughRef = useRef(false);
  const lastAmbientAtRef = useRef(0);

  const locale = navigator.language.startsWith("zh")
    ? ("zh" as const)
    : ("en" as const);

  // ── Helpers ──

  const showBubble = useCallback(
    (text: string, type: BubbleType, isCc = false) => {
      // 挂靠半隐时气泡会被屏幕边缘裁掉,不弹
      if (dockedRef.current) return;
      setBubble((prev) => {
        // CC 提醒优先:正在显示的 CC 气泡不被普通气泡顶掉(它会自己到点消失)。
        if (!isCc && prev?.isCc) return prev;
        bubbleKeyRef.current += 1;
        return { text, type, key: bubbleKeyRef.current, isCc };
      });
    },
    []
  );

  const dismissBubble = useCallback(() => setBubble(null), []);

  // ── Init: load settings + memory + startup greeting ──

  useEffect(() => {
    petTypeRef.current = petType;
  }, [petType]);

  useEffect(() => {
    getPetSettings()
      .then((s) => setPetType(s.pet_type as PetType))
      .catch(() => {});

    // 载入持久化的聊天历史,重启后气泡聊天仍带上下文,也和主窗口聊天页共享
    getPetChatHistory()
      .then((raw) => {
        chatHistoryRef.current = parseHistory(raw);
      })
      .catch(() => {});

    getPetMemory()
      .then((raw) => {
        try {
          memoryRef.current = JSON.parse(raw);
        } catch {
          memoryRef.current = {};
        }
        // Startup greeting (delayed so window renders first)
        setTimeout(async () => {
          const name = memoryRef.current.name;
          const topic = memoryRef.current.topic;
          const topicAt = Date.parse(memoryRef.current._topic_at ?? "");
          const topicFresh =
            !Number.isNaN(topicAt) &&
            Date.now() - topicAt < 7 * 24 * 60 * 60 * 1000;
          // 本地兜底问候:LLM 不可用(网关没开等)时用它。
          const base =
            topic && topicFresh && Math.random() < 0.6
              ? topicGreeting(topic, locale)
              : getGreeting(gatewayStateRef.current, locale);
          const fallback = name
            ? locale === "zh"
              ? `${name}，${base}`
              : `Hey ${name}! ${base}`
            : base;
          // 优先用 LLM 生成结合记忆/时间/状态的问候,失败回落本地文案
          const llm = await petGenerate(
            petTypeRef.current,
            locale,
            buildGreetingInstruction(
              locale,
              new Date().getHours(),
              gatewayStateRef.current
            ),
            memoryRef.current
          );
          showBubble(llm ?? fallback, "chat");
        }, 1500);
      })
      .catch(() => {});
  }, [locale, showBubble]);

  // ── Event listeners ──

  useEffect(() => {
    const unlisten = events.petSettingsChanged.listen((e) =>
      setPetType(e.payload.pet_type as PetType)
    );
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  // CC 状态 → 头顶徽章:working 转齿轮,waiting 跳动,done 庆祝 + 徽章停留几秒。
  const applyCcStatus = useCallback((ccType: string) => {
    if (ccBadgeTimerRef.current) clearTimeout(ccBadgeTimerRef.current);
    if (ccType === "cc-working") {
      setCcBadge("working");
      // hook 漏发 done 时兜底,别让齿轮永远转
      ccBadgeTimerRef.current = setTimeout(
        () => setCcBadge(null),
        CC_WORKING_TIMEOUT
      );
    } else if (ccType === "cc-waiting") {
      setCcBadge("waiting");
      ccBadgeTimerRef.current = setTimeout(
        () => setCcBadge(null),
        CC_WORKING_TIMEOUT
      );
    } else if (ccType === "cc-done") {
      setCcBadge("done");
      setCelebrate(true);
      if (celebrateTimerRef.current) clearTimeout(celebrateTimerRef.current);
      celebrateTimerRef.current = setTimeout(
        () => setCelebrate(false),
        CELEBRATE_DURATION
      );
      ccBadgeTimerRef.current = setTimeout(
        () => setCcBadge(null),
        CC_DONE_LINGER
      );
    }
  }, []);

  useEffect(() => {
    const unlisten = events.petBubble.listen((e) => {
      const text =
        locale === "zh" && e.payload.text_zh
          ? e.payload.text_zh
          : e.payload.text;
      // CC 提醒用 type="cc" 作来源标记:映射成 info 样式显示,并标记 isCc 走优先级保护。
      const isCc = e.payload.type.startsWith("cc");
      if (isCc) applyCcStatus(e.payload.type);
      if (e.payload.type === "cc-working") {
        return;
      } // working 太频繁,不弹气泡(徽章动画已表达)
      showBubble(
        text,
        isCc
          ? "info"
          : (e.payload.type as "info" | "success" | "error" | "chat"),
        isCc
      );
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [locale, showBubble, applyCcStatus]);

  // ── Poll gateway state + errors (10s, lite) ──
  // 只取 state + last_error,不读全表 stats(那个走单独的 30 分钟 timer)。
  useEffect(() => {
    const poll = () => {
      if (document.hidden) return;
      getPetGatewayStateLite()
        .then((info) => {
          setGatewayState((prev) => {
            if (prev !== info.state) {
              setIsSleeping(false);
              if (sleepTimerRef.current) clearTimeout(sleepTimerRef.current);
              sleepTimerRef.current = setTimeout(
                () => setIsSleeping(true),
                SLEEP_TIMEOUT
              );
            }
            return info.state;
          });
          gatewayStateRef.current = info.state;
          setActiveTier(activityTier(info.active_count ?? 0));

          if (
            info.last_error &&
            info.last_error.timestamp !== lastErrorTsRef.current
          ) {
            const age =
              Date.now() - new Date(info.last_error.timestamp).getTime();
            if (age < ERROR_COOLDOWN) {
              lastErrorTsRef.current = info.last_error.timestamp;
              const p = info.last_error.provider || "";
              const rawMsg = info.last_error.message;
              const fallback =
                (p ? `${p}: ` : "") +
                (rawMsg.length > 40 ? rawMsg.slice(0, 40) + "..." : rawMsg);
              setIsError(true);
              setTimeout(() => setIsError(false), 3000);
              // 先弹本地兜底(即时),LLM 诊断返回后换成人话+建议
              showBubble(fallback, "error");
              petGenerate(
                petTypeRef.current,
                locale,
                buildErrorInstruction(locale, p, rawMsg),
                memoryRef.current
              ).then((diag) => {
                if (
                  diag &&
                  lastErrorTsRef.current === info.last_error!.timestamp
                )
                  showBubble(diag, "error");
              });
            }
          }
        })
        .catch(() => {});
    };
    poll();
    const id = setInterval(poll, POLL_INTERVAL);
    const onVisible = () => {
      if (!document.hidden) poll();
    };
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
      getPetGatewayState()
        .then(async (info) => {
          if (!info.today || info.today.requests <= 0) return;
          lastStatsRef.current = Date.now();
          const tokens =
            (info.today.input_tokens ?? 0) + (info.today.output_tokens ?? 0);
          const cost =
            info.today.cost > 0 ? ` | $${info.today.cost.toFixed(2)}` : "";
          const errorText =
            (info.today.errors ?? 0) > 0
              ? locale === "zh"
                ? ` | ${info.today.errors} 错误`
                : ` | ${info.today.errors} err`
              : "";
          const tokenText =
            tokens > 0
              ? locale === "zh"
                ? ` | ${compactNumber(tokens)} tokens`
                : ` | ${compactNumber(tokens)} tok`
              : "";
          // 花费拟人化:超过预警阈值(没配置就 $10)宠物吃撑变圆,统计变成打嗝吐槽
          const over = isOverBudget(info.today.cost ?? 0, info.cost_alert);
          setChubby(over);
          const burp = over
            ? locale === "zh"
              ? "饱嗝~ 吃撑了 🫃 "
              : "Burp~ so full 🫃 "
            : "";
          const fallback =
            locale === "zh"
              ? `${burp}今日: ${info.today.requests} 请求${tokenText}${cost}${errorText}`
              : `${burp}Today: ${info.today.requests} req${tokenText}${cost}${errorText}`;
          // LLM 把数据说成有态度的一句话,失败回落上面的模板文案
          const llm = await petGenerate(
            petTypeRef.current,
            locale,
            buildStatsInstruction(locale, {
              requests: info.today.requests,
              errors: info.today.errors,
              cost: info.today.cost,
              tokens,
            }),
            memoryRef.current
          );
          showBubble(llm ? `${burp}${llm}` : fallback, "info");
        })
        .catch(() => {});
    };
    const id = setInterval(tick, STATS_INTERVAL);
    return () => clearInterval(id);
  }, [locale, showBubble]);

  // ── Sleep timer ──

  const resetSleepTimer = useCallback(() => {
    setIsSleeping(false);
    if (sleepTimerRef.current) clearTimeout(sleepTimerRef.current);
    sleepTimerRef.current = setTimeout(
      () => setIsSleeping(true),
      SLEEP_TIMEOUT
    );
  }, []);

  useEffect(() => {
    resetSleepTimer();
    return () => {
      if (sleepTimerRef.current) clearTimeout(sleepTimerRef.current);
    };
  }, [resetSleepTimer]);

  // ── State ──

  const petState: PetState = isPoked
    ? "poke"
    : isError
      ? "error"
      : gatewayState === "active"
        ? "active"
        : isSleeping
          ? "sleep"
          : "idle";

  // ── Poke(连戳有脾气)──

  const setMoodTimed = useCallback((m: PokeMood, duration: number) => {
    setMood(m);
    moodRef.current = m;
    if (moodTimerRef.current) clearTimeout(moodTimerRef.current);
    moodTimerRef.current = setTimeout(() => {
      setMood("normal");
      moodRef.current = "normal";
      pokeStreakRef.current = 0;
    }, duration);
  }, []);

  const triggerPoke = useCallback(() => {
    const now = Date.now();
    pokeStreakRef.current =
      now - lastPokeTsRef.current < POKE_STREAK_WINDOW
        ? pokeStreakRef.current + 1
        : 1;
    lastPokeTsRef.current = now;
    const m = pokeMood(pokeStreakRef.current);

    if (pokeTimerRef.current) clearTimeout(pokeTimerRef.current);
    setIsPoked(true);
    pokeTimerRef.current = setTimeout(() => setIsPoked(false), POKE_DURATION);

    if (m === "sulk") {
      // 背过身:进入时冷冷丢一句,之后再戳不理人,只刷新生闷气时长
      if (moodRef.current !== "sulk") {
        showBubble(pickSulkReaction(petTypeRef.current, locale), "chat");
      }
      setMoodTimed("sulk", SULK_DURATION);
    } else if (m === "angry") {
      showBubble(pickAngryReaction(petTypeRef.current, locale), "chat");
      setMoodTimed("angry", ANGRY_DURATION);
    } else {
      showBubble(pickPokeReaction(petTypeRef.current, locale), "chat");
    }
    resetSleepTimer();
  }, [locale, resetSleepTimer, showBubble, setMoodTimed]);

  // ── 贴边挂靠 ──
  // 拖到屏幕左右边缘附近 → 窗口吸附到只露 DOCK_PEEK 宽(半个宠物探出);
  // 单击弹回屏幕内。挂靠状态不持久化,重启由 Rust 越界校正拉回可见区。

  const undock = useCallback(async () => {
    const side = dockedRef.current;
    if (!side) return;
    dockedRef.current = null;
    try {
      const win = getCurrentWindow();
      const mon = await currentMonitor();
      if (!mon) return;
      const scale = mon.scaleFactor;
      const pos = await win.outerPosition();
      const x =
        side === "left"
          ? mon.position.x / scale + 8
          : (mon.position.x + mon.size.width) / scale - PET_WIN_WIDTH - 8;
      await win.setPosition(new LogicalPosition(x, pos.y / scale));
    } catch {
      /* ignore */
    }
  }, []);

  const snapToEdgeIfNear = useCallback(async (physX: number, physY: number) => {
    try {
      const mon = await currentMonitor();
      if (!mon) return;
      const scale = mon.scaleFactor;
      const left = mon.position.x;
      const right = mon.position.x + mon.size.width;
      const winW = PET_WIN_WIDTH * scale;
      if (dockedRef.current) {
        // 已挂靠:被拖离边缘就解除标记(位置已是用户拖的新位置)
        if (physX > left + 4 && physX + winW < right - 4) {
          dockedRef.current = null;
        }
        return;
      }
      const win = getCurrentWindow();
      if (physX <= left + DOCK_TRIGGER * scale) {
        dockedRef.current = "left";
        await win.setPosition(
          new LogicalPosition(
            left / scale - (PET_WIN_WIDTH - DOCK_PEEK),
            physY / scale
          )
        );
      } else if (physX + winW >= right - DOCK_TRIGGER * scale) {
        dockedRef.current = "right";
        await win.setPosition(
          new LogicalPosition(right / scale - DOCK_PEEK, physY / scale)
        );
      }
    } catch {
      /* ignore */
    }
  }, []);

  // ── Drag (threshold) + click → poke ──

  const handleMouseDown = useCallback(
    (e: React.MouseEvent) => {
      if (e.button !== 0 || chatMode) return;
      dragRef.current = { x: e.clientX, y: e.clientY, dragging: false };
    },
    [chatMode]
  );

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
        if (
          Math.abs(e.clientX - s.x) > DRAG_THRESHOLD ||
          Math.abs(e.clientY - s.y) > DRAG_THRESHOLD
        ) {
          s.dragging = true;
          getCurrentWindow().startDragging();
        }
      }

      // eye-follow: 仅 idle / 非聊天 / 心情正常时。rAF 节流 + 直写 transform。
      // (sulk 用 class transform 背过身,inline transform 会把它顶掉,必须让位)
      if (
        petStateRef.current === "idle" &&
        !chatModeRef.current &&
        moodRef.current === "normal"
      ) {
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
      // 挂靠时单击是"钻出来",不是戳
      if (s && !s.dragging) {
        if (dockedRef.current) undock();
        else triggerPoke();
      }
      dragRef.current = null;
    };

    window.addEventListener("mousemove", handleMove);
    window.addEventListener("mouseup", handleUp);
    return () => {
      window.removeEventListener("mousemove", handleMove);
      window.removeEventListener("mouseup", handleUp);
    };
  }, [triggerPoke, undock]);

  // 同步 ref(供合并的 mousemove handler 用 — 不依赖 React state closure)
  useEffect(() => {
    petStateRef.current = petState;
  }, [petState]);
  useEffect(() => {
    chatModeRef.current = chatMode;
  }, [chatMode]);

  // 非 idle / 聊天 / 有心情时清掉 inline transform,让 CSS class 动画接管
  useEffect(() => {
    if (petState !== "idle" || chatMode || mood !== "normal") {
      if (petRef.current) petRef.current.style.transform = "";
    }
  }, [petState, chatMode, mood]);

  // ── Double-click: open chat ──

  const handleDoubleClick = useCallback(() => {
    if (chatMode || dockedRef.current) return;
    if (pokeTimerRef.current) {
      clearTimeout(pokeTimerRef.current);
      setIsPoked(false);
    }
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

  // 聊天历史跨窗口同步:任一窗口发消息 → Rust 落库 + 广播 → 这里刷新 ref,
  // 让气泡聊天的上下文始终等于 DB 真值(避免用陈旧 ref 覆盖掉对方的消息)。
  useEffect(() => {
    const un = events.petChatUpdated.listen((e) => {
      chatHistoryRef.current = parseHistory(e.payload);
    });
    return () => {
      un.then((fn) => fn());
    };
  }, []);

  // 记忆跨窗口同步:聊天页手动编辑 / 聊天里自动提取都会广播,
  // 这里更新 memoryRef,否则宠物聊天还会用旧名字/旧话题。
  useEffect(() => {
    const un = events.petMemoryChanged.listen((e) => {
      try {
        const parsed = JSON.parse(e.payload);
        if (parsed && typeof parsed === "object") memoryRef.current = parsed;
      } catch {
        /* ignore bad payload */
      }
    });
    return () => {
      un.then((fn) => fn());
    };
  }, []);

  // 「清空记忆」由原生菜单触发,Rust 写 DB + emit。前端清本地缓存 + 提示气泡。
  // 只清记忆,不动聊天历史(两者独立清空)。
  useEffect(() => {
    const un = events.petMemoryReset.listen(() => {
      memoryRef.current = {};
      showBubble(
        locale === "zh" ? "记忆已清空 ✨" : "Memory cleared ✨",
        "info"
      );
    });
    return () => {
      un.then((fn) => fn());
    };
  }, [locale, showBubble]);

  // ── Click-through (鼠标穿透到下方应用) ──
  //
  // 真值在 Rust AppState,Pet 启动时拉一遍 + 监听 changed 事件。
  // 右键菜单 / Settings / tray 三处都改 Rust,这里只是镜像 + 应用窗口设置。
  const applyClickThrough = useCallback(async (on: boolean) => {
    try {
      await getCurrentWindow().setIgnoreCursorEvents(on);
    } catch {
      /* ignore */
    }
    setClickThroughLocal(on);
  }, []);

  useEffect(() => {
    getPetClickThrough()
      .then(applyClickThrough)
      .catch(() => {});
    const un = events.petClickThroughChanged.listen((e) =>
      applyClickThrough(e.payload)
    );
    return () => {
      un.then((fn) => fn());
    };
  }, [applyClickThrough]);

  // ── Chat submit with AI fallback ──

  const handleChatSubmit = useCallback(async () => {
    const msg = chatInputRef.current.trim();
    if (!msg) return;

    setChatInput("");
    chatInputRef.current = "";
    setChatLoading(true);
    setBubble(null);

    // 走共享聊天核心:记忆抽取 + 落库 + LLM 调用 + 错误映射都在里面。
    // 落库会广播 PetChatUpdated,chatHistoryRef 由下面的 listener 统一同步。
    const result = await sendPetMessage({
      userMsg: msg,
      petType: petTypeRef.current,
      locale,
      history: chatHistoryRef.current,
      memory: memoryRef.current,
    });
    setChatLoading(false);
    setChatMode(false);
    if (result.ok) {
      showBubble(result.reply, "chat");
    } else {
      showBubble(result.errorText, "error");
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
    let rawY = 0; // 物理坐标原值,贴边判定用(pending.y 混了 expansion 偏移)
    const unlisten = win.onMoved(({ payload }) => {
      pending = { x: payload.x, y: payload.y + expansionRef.current };
      rawY = payload.y;
      if (timer) clearTimeout(timer);
      timer = setTimeout(() => {
        if (!pending) return;
        updatePetSettings({ pos_x: pending.x, pos_y: pending.y }).catch(
          () => {}
        );
        snapToEdgeIfNear(pending.x, rawY);
        pending = null;
      }, 300);
    });
    return () => {
      if (timer) clearTimeout(timer);
      unlisten.then((fn) => fn());
    };
  }, [snapToEdgeIfNear]);

  // 日期彩蛋:跨零点/跨节日时半小时内刷新
  useEffect(() => {
    const id = setInterval(
      () => setDateBadge(getDateBadge(new Date())),
      DATE_BADGE_REFRESH
    );
    return () => clearInterval(id);
  }, []);

  // 趣味行为定时器统一清理
  useEffect(() => {
    return () => {
      if (moodTimerRef.current) clearTimeout(moodTimerRef.current);
      if (ccBadgeTimerRef.current) clearTimeout(ccBadgeTimerRef.current);
      if (celebrateTimerRef.current) clearTimeout(celebrateTimerRef.current);
    };
  }, []);

  // 镜像 state 到 ref,供主动搭话的 interval 闭包读取(避免陈旧闭包)
  useEffect(() => {
    bubbleActiveRef.current = bubble !== null || chatLoading;
  }, [bubble, chatLoading]);
  useEffect(() => {
    clickThroughRef.current = clickThrough;
  }, [clickThrough]);

  // ── 主动搭话:低频 tick,情境合适时用 LLM 冒一句应景评论 ──
  // 条件严格(空闲/无气泡/网关在跑/非穿透/间隔够久),避免打扰 + 控制 token。
  useEffect(() => {
    const tick = async () => {
      if (document.hidden) return;
      if (bubbleActiveRef.current || chatModeRef.current) return;
      if (dockedRef.current || clickThroughRef.current) return;
      if (gatewayStateRef.current === "stopped") return;
      if (moodRef.current !== "normal") return;
      if (petStateRef.current !== "idle" && petStateRef.current !== "active")
        return;
      if (Date.now() - lastAmbientAtRef.current < AMBIENT_MIN_GAP) return;
      lastAmbientAtRef.current = Date.now();
      const info = await getPetGatewayState().catch(() => null);
      const line = await petGenerate(
        petTypeRef.current,
        locale,
        buildAmbientInstruction(locale, {
          hour: new Date().getHours(),
          gwState: gatewayStateRef.current,
          today: info?.today
            ? {
                requests: info.today.requests,
                errors: info.today.errors,
                cost: info.today.cost,
              }
            : undefined,
          topic: memoryRef.current.topic,
        }),
        memoryRef.current
      );
      // 生成期间用户可能开始交互了,再确认一次条件才弹
      if (
        line &&
        !bubbleActiveRef.current &&
        !chatModeRef.current &&
        !dockedRef.current
      ) {
        showBubble(line, "chat");
      }
    };
    const id = setInterval(tick, AMBIENT_TICK);
    return () => clearInterval(id);
  }, [locale, showBubble]);

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
      } catch {
        /* ignore */
      }
    })();
  }, [hasBubble]);

  // ── Render ──

  const PetComponent = PET_COMPONENTS[petType];

  // 基础动画类互斥,优先级:庆祝 > 生闷气 > 生气 > 常规状态。
  // 修饰类(强度档位/吃撑)叠加在基础类之上。
  const baseClass = celebrate
    ? "pet-celebrate"
    : mood === "sulk"
      ? "pet-sulk"
      : mood === "angry"
        ? "pet-angry"
        : `pet-${petState}`;
  const wrapperClass = chatMode
    ? ""
    : [
        baseClass,
        petState === "active" && mood === "normal" && activeTier > 1
          ? `pet-active-${activeTier}`
          : "",
        chubby ? "pet-chubby" : "",
      ]
        .filter(Boolean)
        .join(" ");

  return (
    <div
      className="pet-container"
      onMouseDown={handleMouseDown}
      onDoubleClick={handleDoubleClick}
      onContextMenu={handleContextMenu}
    >
      <div
        ref={petRef}
        className={wrapperClass}
        style={{
          position: "relative",
          transition: "transform 0.3s ease-out, opacity 0.2s",
          // 内联 opacity 优先级高于 class,生闷气的变暗也在这里合并
          opacity: clickThrough ? 0.45 : mood === "sulk" ? 0.7 : 1,
        }}
      >
        {bubble && (
          <Bubble
            key={bubble.key}
            text={bubble.text}
            type={bubble.type}
            onDone={dismissBubble}
          />
        )}

        {chatLoading && (
          <div className="bubble bubble-in">
            <div
              className="bubble-content"
              style={{ borderColor: "var(--color-accent, #E89850)" }}
            >
              <div className="chat-loading">
                <span />
                <span />
                <span />
              </div>
            </div>
            <div
              className="bubble-arrow"
              style={{ borderTopColor: "var(--color-accent, #E89850)" }}
            />
          </div>
        )}

        <PetComponent state={petState} />
        {petState === "sleep" && !chatMode && <span className="zzz">z</span>}
        {!chatMode && ccBadge && (
          <span className={`cc-badge cc-badge-${ccBadge}`}>
            {ccBadge === "working" ? "⚙️" : ccBadge === "waiting" ? "⏳" : "✅"}
          </span>
        )}
        {!chatMode &&
          petState === "active" &&
          mood === "normal" &&
          activeTier === 3 && <span className="sweat">💦</span>}
        {!chatMode && mood === "angry" && (
          <span className="angry-mark">💢</span>
        )}
        {!chatMode && dateBadge && (
          <span className="date-badge">{dateBadge}</span>
        )}

        {chatMode && (
          <div className="chat-input-wrap" onClick={(e) => e.stopPropagation()}>
            <input
              ref={inputRef}
              className="chat-input"
              value={chatInput}
              onChange={(e) => {
                setChatInput(e.target.value);
                chatInputRef.current = e.target.value;
              }}
              onKeyDown={(e) => {
                if (e.key === "Enter") handleChatSubmit();
                e.stopPropagation();
              }}
              onMouseDown={(e) => e.stopPropagation()}
              onBlur={() => {
                if (!chatLoading) setChatMode(false);
              }}
              placeholder={locale === "zh" ? "跟我聊天..." : "Chat with me..."}
              disabled={chatLoading}
            />
          </div>
        )}
      </div>
    </div>
  );
}
