// 宠物聊天的共享核心:宠物窗口(双击气泡聊天)和主窗口的宠物聊天页
// 都用这里的逻辑,保证同一条 pet_chat 链路、同一份记忆和人格。
//
// 聊天记录持久化在 Rust(app_settings.pet_chat_history),save 时广播
// PetChatUpdated,两个窗口 listen 后各自刷新——DB 是唯一真值源。

import type { PetType } from "@/types/pet";
import { buildSystemPrompt } from "./personas";
import { extractTopic } from "./petLogic";
import { buildMemoryString } from "./petMemory";
import { extractMemoryLLM } from "./petGenerate";
import { petChat, savePetMemory, savePetChatHistory } from "@/lib/api";

export { buildMemoryString };

export interface ChatMessage {
  role: "user" | "assistant";
  content: string;
  ts: number;
}

/// 发给 LLM 的历史窗口(条数)。持久化上限在 Rust 侧(50),这里只控上下文长度。
export const MAX_CHAT_CONTEXT = 10;

const NAME_PATTERNS = [
  /my name is\s+(\S+)/i,
  /i'?m\s+(\S+)/i,
  /call me\s+(\S+)/i,
  /我叫(.{1,10})/,
  /我是(.{1,10})/,
  /叫我(.{1,10})/,
];

export function extractName(msg: string): string | null {
  for (const pat of NAME_PATTERNS) {
    const m = msg.match(pat);
    if (m) return m[1].trim();
  }
  return null;
}

export function mapChatError(
  e: { code?: string; message?: string } | null | undefined,
  locale: "en" | "zh"
): string {
  const zh = locale === "zh";
  switch (e?.code) {
    case "GATEWAY_NOT_RUNNING":
      return zh ? "先启动网关哦" : "Start the gateway first";
    case "ACTIVE_PROVIDER_NOT_FOUND":
      return zh ? "先选个可用供应商" : "Pick an active provider first";
    case "PROVIDER_API_KEY_MISSING":
      return zh ? "供应商缺 API Key" : "Provider API key is missing";
    case "GATEWAY_AUTH_INVALID":
    case "GATEWAY_AUTH_MISSING":
      return zh ? "网关 token 不对" : "Gateway token needs attention";
    default: {
      const short = (e?.message || "request failed").slice(0, 60);
      return zh ? `调不通: ${short}` : `Call failed: ${short}`;
    }
  }
}

/// 取最近 N 条并剥掉 ts,组装成 LLM messages。
export function recentContext(
  history: ChatMessage[],
  limit = MAX_CHAT_CONTEXT
): Array<{ role: string; content: string }> {
  return history
    .slice(-limit)
    .map(({ role, content }) => ({ role, content }));
}

/// 从用户消息里抽取 name / topic 写入记忆,变了就持久化。返回是否更新过。
export function updateMemoryFromMessage(
  memory: Record<string, string>,
  msg: string
): boolean {
  let changed = false;
  const name = extractName(msg);
  if (name) {
    memory.name = name;
    changed = true;
  }
  const topic = extractTopic(msg);
  if (topic) {
    memory.topic = topic;
    memory._topic_at = new Date().toISOString();
    changed = true;
  }
  if (changed) savePetMemory(JSON.stringify(memory)).catch(() => {});
  return changed;
}

export type SendResult =
  | { ok: true; reply: string; history: ChatMessage[] }
  | { ok: false; errorText: string; history: ChatMessage[] };

/// 发一条消息:抽记忆 → 落库用户消息 → 调 LLM → 落库回复。
/// 每次 save 都会广播 PetChatUpdated,两个窗口据此刷新列表,不用手动同步。
export async function sendPetMessage(params: {
  userMsg: string;
  petType: PetType;
  locale: "en" | "zh";
  history: ChatMessage[];
  memory: Record<string, string>;
}): Promise<SendResult> {
  const { userMsg, petType, locale, history, memory } = params;

  updateMemoryFromMessage(memory, userMsg);

  const withUser: ChatMessage[] = [
    ...history,
    { role: "user", content: userMsg, ts: Date.now() },
  ];
  // 先落库用户消息:两个窗口立刻看到"我说的话",再等回复
  await savePetChatHistory(JSON.stringify(withUser)).catch(() => {});

  const messages = [
    { role: "system", content: buildSystemPrompt(petType, locale, buildMemoryString(memory)) },
    ...recentContext(withUser),
  ];

  try {
    const reply = await petChat(messages);
    const withReply: ChatMessage[] = [
      ...withUser,
      { role: "assistant", content: reply, ts: Date.now() },
    ];
    await savePetChatHistory(JSON.stringify(withReply)).catch(() => {});
    // 更聪明的记忆:后台让 LLM 从这轮对话提取用户偏好/事实,不阻塞回复。
    // 提取到就 merge 进记忆并广播(savePetMemory → PetMemoryChanged 同步两窗口)。
    void extractMemoryLLM(petType, locale, userMsg, reply, memory).then(
      (extracted) => {
        if (!extracted) return;
        Object.assign(memory, extracted);
        savePetMemory(JSON.stringify(memory)).catch(() => {});
      }
    );
    return { ok: true, reply, history: withReply };
  } catch (err) {
    return {
      ok: false,
      errorText: mapChatError(
        err as { code?: string; message?: string },
        locale
      ),
      history: withUser,
    };
  }
}

/// 解析持久化的历史 JSON,坏数据当空历史(不炸页面)。
export function parseHistory(raw: string): ChatMessage[] {
  try {
    const arr = JSON.parse(raw);
    if (!Array.isArray(arr)) return [];
    return arr.filter(
      (m): m is ChatMessage =>
        m &&
        (m.role === "user" || m.role === "assistant") &&
        typeof m.content === "string"
    );
  } catch {
    return [];
  }
}
