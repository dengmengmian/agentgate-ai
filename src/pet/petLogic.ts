// 宠物趣味行为的纯逻辑,全部无副作用,便于单测。
// UI 侧(PetApp/CSS)只消费这里的判定结果。

/// 网关活跃强度分档:并发请求越多,弹跳越欢。
export function activityTier(activeCount: number): 1 | 2 | 3 {
  if (activeCount >= 3) return 3;
  if (activeCount === 2) return 2;
  return 1;
}

/// 连续戳的心情:戳 1-2 下正常反应,3-4 下生气,5 下以上背过身生闷气。
export type PokeMood = "normal" | "angry" | "sulk";
export function pokeMood(streak: number): PokeMood {
  if (streak >= 5) return "sulk";
  if (streak >= 3) return "angry";
  return "normal";
}

// 春节是农历,只能查表。到期后补新年份即可,缺表年份自然没彩蛋,无副作用。
const LUNAR_NEW_YEAR: Record<number, [number, number]> = {
  2026: [2, 17],
  2027: [2, 6],
  2028: [1, 26],
};

/// 日期彩蛋:节日 > 深夜 > 周五,无匹配返回 null。
export function getDateBadge(now: Date): string | null {
  const month = now.getMonth() + 1;
  const day = now.getDate();
  const hour = now.getHours();

  if (month === 12 && (day === 24 || day === 25)) return "🎄";
  if (month === 10 && day === 31) return "🎃";
  if (month === 1 && day === 1) return "🎉";
  const cny = LUNAR_NEW_YEAR[now.getFullYear()];
  if (cny && month === cny[0] && day === cny[1]) return "🧧";

  if (hour < 6) return "🌙";
  if (now.getDay() === 5) return "✨";
  return null;
}

const TOPIC_PATTERNS = [
  /(?:正在|在)(?:做|弄|搞|写|修|开发|调)([^,，。!！?？]{2,30})/,
  /working on ([^,.!?]{3,40})/i,
];

/// 从聊天内容提取"在做的事",作为话题记忆。提不出返回 null。
export function extractTopic(msg: string): string | null {
  for (const pat of TOPIC_PATTERNS) {
    const m = msg.match(pat);
    if (m) {
      const topic = m[1].trim().replace(/[。.!！?？~]+$/, "");
      if (topic.length >= 2) return topic.slice(0, 20);
    }
  }
  return null;
}

const DEFAULT_BUDGET = 10;

/// 今日花费是否超过"吃撑"阈值。用户配置了 cost_alert 用配置值,否则默认 $10。
export function isOverBudget(
  cost: number,
  alert?: { enabled?: boolean; threshold?: number | null } | null
): boolean {
  if (cost <= 0) return false;
  const threshold =
    alert?.enabled && alert.threshold && alert.threshold > 0
      ? alert.threshold
      : DEFAULT_BUDGET;
  return cost >= threshold;
}

/// 带话题的问候语:"上次你说在弄 xxx,搞定了吗?"
export function topicGreeting(topic: string, locale: "en" | "zh"): string {
  return locale === "zh"
    ? `上次你说在弄「${topic}」,搞定了吗?`
    : `Last time you were working on ${topic} — done yet?`;
}
