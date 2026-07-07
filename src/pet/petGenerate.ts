// 宠物的"轻量 LLM 生成"能力:问候、状态播报、错误诊断、主动搭话、记忆提取
// 都复用 pet_chat(不加后端)。核心约定:**失败一律返回 null / {}**,由调用方
// 回落本地文案——网关没开 / 报错时宠物绝不哑掉。

import type { PetType } from "@/types/pet";
import { buildSystemPrompt } from "./personas";
import { buildMemoryString } from "./petMemory";
import { petChat } from "@/lib/api";

type Locale = "en" | "zh";

const langHint = (locale: Locale) =>
  locale === "zh" ? "用中文回复" : "Reply in English";

/// 单次生成:给一段情境指令,生成一句符合人格的短话。
/// 失败(网关不可用/超时/空回复)返回 null,调用方决定是否回落本地文案。
export async function petGenerate(
  petType: PetType,
  locale: Locale,
  instruction: string,
  memory: Record<string, string>
): Promise<string | null> {
  try {
    const reply = await petChat([
      {
        role: "system",
        content: buildSystemPrompt(petType, locale, buildMemoryString(memory)),
      },
      { role: "user", content: instruction },
    ]);
    const t = (reply ?? "").trim();
    return t && t !== "..." ? t : null;
  } catch {
    return null;
  }
}

const period = (hour: number): string => {
  if (hour < 6) return "凌晨/late night";
  if (hour < 12) return "上午/morning";
  if (hour < 18) return "下午/afternoon";
  if (hour < 23) return "晚上/evening";
  return "深夜/late night";
};

/// 开机问候指令。
export function buildGreetingInstruction(
  locale: Locale,
  hour: number,
  gwState: "running" | "stopped" | "active"
): string {
  return `情境:现在是${period(hour)},网关状态是「${gwState}」。用你的人格跟主人打个招呼,如果记得主人的信息可以自然带上。一句话,不超过30字,直接说话别解释。${langHint(locale)}。`;
}

export interface StatsCtx {
  requests?: number;
  errors?: number;
  cost?: number;
  tokens?: number;
}

/// 30 分钟统计播报指令(把冷冰冰的数字说成有态度的一句话)。
export function buildStatsInstruction(locale: Locale, s: StatsCtx): string {
  const facts: string[] = [];
  facts.push(`今日${s.requests ?? 0}个请求`);
  if (s.tokens) facts.push(`${s.tokens} tokens`);
  if (s.cost) facts.push(`花了$${s.cost.toFixed(2)}`);
  if (s.errors) facts.push(`${s.errors}个错误`);
  return `情境(今日网关数据):${facts.join(",")}。用你的人格对今天的用量说一句有态度的短评论,别只复述数字。不超过30字,直接说话。${langHint(locale)}。`;
}

/// 错误诊断指令:把技术错误翻成人话 + 给个建议。
export function buildErrorInstruction(
  locale: Locale,
  provider: string,
  message: string
): string {
  const p = provider ? `供应商「${provider}」` : "网关";
  return `情境:${p}刚报错,原始错误:「${message.slice(0, 200)}」。用你的人格把这个错误翻译成一句人话 + 一个简短建议(比如换供应商/检查key/稍后重试)。不超过40字,直接说,别贴原始错误。${langHint(locale)}。`;
}

export interface AmbientCtx {
  hour: number;
  gwState: "running" | "stopped" | "active";
  today?: StatsCtx;
  topic?: string;
}

/// 主动搭话指令:结合时间/状态/今日数据/最近话题,冒一句应景评论。
export function buildAmbientInstruction(locale: Locale, ctx: AmbientCtx): string {
  const facts: string[] = [`现在${period(ctx.hour)}`, `网关「${ctx.gwState}」`];
  if (ctx.today?.requests) facts.push(`今日${ctx.today.requests}个请求`);
  if (ctx.today?.errors) facts.push(`${ctx.today.errors}个错误`);
  if (ctx.today?.cost) facts.push(`花了$${ctx.today.cost.toFixed(2)}`);
  if (ctx.topic) facts.push(`主人最近在弄「${ctx.topic}」`);
  return `情境:${facts.join(",")}。用你的人格,针对这个情境主动说一句应景的话(关心/调侃/提醒都行),别复述数据。不超过30字,直接说话。${langHint(locale)}。`;
}

// ── 记忆提取 ──

const MEMORY_KEY = /^[a-z][a-z0-9_]{0,23}$/i;

/// 解析 LLM 提取的记忆 JSON,健壮兜底:NONE / 非法 / 脏输出都返回 {}。
/// 只保留合法 key(英文短标识、非内部 _ 前缀)、非空短 value,最多 3 条。
export function parseExtractedMemory(raw: string): Record<string, string> {
  const text = (raw ?? "").trim();
  if (!text || /^none\b/i.test(text)) return {};
  // 从可能夹带解释的输出里抠出第一个 JSON 对象
  const start = text.indexOf("{");
  const end = text.lastIndexOf("}");
  if (start === -1 || end <= start) return {};
  let obj: unknown;
  try {
    obj = JSON.parse(text.slice(start, end + 1));
  } catch {
    return {};
  }
  if (!obj || typeof obj !== "object") return {};
  const out: Record<string, string> = {};
  for (const [k, v] of Object.entries(obj as Record<string, unknown>)) {
    if (Object.keys(out).length >= 3) break;
    if (k.startsWith("_") || !MEMORY_KEY.test(k)) continue;
    const val = typeof v === "string" ? v.trim() : "";
    if (val && val.length <= 40) out[k] = val;
  }
  return out;
}

export function buildMemoryExtractionInstruction(
  locale: Locale,
  userMsg: string,
  reply: string
): string {
  return `以下是一轮对话。用户:「${userMsg.slice(0, 300)}」你回:「${reply.slice(0, 200)}」。
从用户的话里提取值得长期记住的稳定事实或偏好(如职业、喜好、正在做的项目、称呼)。
只输出一个 JSON 对象,key 用英文小写短标识,value 简短;没有值得记的就只输出 NONE。
不要解释,不要 name/topic(已单独处理)。${langHint(locale)} 不影响本次只输出 JSON 或 NONE。`;
}

/// 聊天后台调用:让 LLM 从这轮对话提取偏好。失败/无内容返回 null。
export async function extractMemoryLLM(
  petType: PetType,
  locale: Locale,
  userMsg: string,
  reply: string,
  memory: Record<string, string>
): Promise<Record<string, string> | null> {
  const raw = await petGenerate(
    petType,
    locale,
    buildMemoryExtractionInstruction(locale, userMsg, reply),
    memory
  );
  if (!raw) return null;
  const extracted = parseExtractedMemory(raw);
  return Object.keys(extracted).length ? extracted : null;
}
