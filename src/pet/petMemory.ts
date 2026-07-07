// 宠物记忆的展示/编辑纯逻辑。记忆是 kv 对象存在 DB(pet_memory)。
// 下划线开头是内部元数据(如 _topic_at),不展示也不让用户编辑,但保存时要保留。

export type MemoryEntry = { key: string; value: string };

/// 可见记忆项(过滤内部 _ 字段),给编辑区渲染。
export function visibleEntries(memory: Record<string, string>): MemoryEntry[] {
  return Object.entries(memory)
    .filter(([k]) => !k.startsWith("_"))
    .map(([key, value]) => ({ key, value: String(value ?? "") }));
}

/// 把编辑后的可见项合并回记忆:保留原有内部 _ 字段,丢弃空 key/value,
/// 同名后者覆盖前者。返回新对象,不改入参。
export function mergeMemory(
  original: Record<string, string>,
  edited: MemoryEntry[]
): Record<string, string> {
  const next: Record<string, string> = {};
  // 先保留内部字段
  for (const [k, v] of Object.entries(original)) {
    if (k.startsWith("_")) next[k] = v;
  }
  for (const { key, value } of edited) {
    const k = key.trim();
    const v = value.trim();
    if (k && v && !k.startsWith("_")) next[k] = v;
  }
  return next;
}

/// 记忆对象 → prompt 字符串。下划线开头是内部元数据(如 _topic_at),不进 prompt。
export function buildMemoryString(memory: Record<string, string>): string {
  return Object.entries(memory)
    .filter(([k]) => !k.startsWith("_"))
    .map(([k, v]) => `${k}: ${v}`)
    .join("; ");
}

/// 已知记忆键的友好名,未知键直接用 key 本身。
export function memoryLabel(key: string, locale: "en" | "zh"): string {
  const zh = locale === "zh";
  switch (key) {
    case "name":
      return zh ? "名字" : "Name";
    case "topic":
      return zh ? "最近话题" : "Recent topic";
    default:
      return key;
  }
}
