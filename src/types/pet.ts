// narrow union types 保留手抄(Rust 端字段是 String,bindings 给 string)。
// PetSettings 在 bindings 里 pet_type 是 string,这里 narrow 成 PetType。
import type {
  PetSettings as Wide,
  UpdatePetSettingsInput as WideUpdate,
} from "@/lib/bindings";

export type PetType =
  | "robot"
  | "pixel-cat"
  | "slime"
  | "fox"
  | "octopus"
  | "ghost"
  | "ox"
  | "soldier"
  | "coder";
export type PetState = "idle" | "active" | "error" | "sleep" | "poke";

export type PetSettings = Omit<Wide, "pet_type"> & { pet_type: PetType };

export type UpdatePetSettingsInput = {
  [K in keyof WideUpdate]?: WideUpdate[K] | undefined;
};

export interface PetGatewayInfo {
  state: "running" | "stopped" | "active";
  /** 并发请求数(lite 接口返回),活跃强度分级用 */
  active_count?: number;
  /** 用户的花费预警配置,宠物"吃撑"判定用 */
  cost_alert?: { enabled?: boolean; threshold?: number | null } | null;
  running?: boolean;
  host?: string;
  port?: number;
  active_provider?: {
    id: string;
    name: string;
    default_model?: string | null;
  } | null;
  latest_model?: string | null;
  last_error?: {
    message: string;
    provider?: string;
    timestamp: string;
  } | null;
  today?: {
    requests: number;
    errors?: number;
    input_tokens?: number;
    output_tokens?: number;
    cache_read_tokens?: number;
    cache_write_tokens?: number;
    cost: number;
  };
}

// PetBubbleEvent 已下放到 bindings.ts(由 Rust app::events::PetBubble 反射),
// `type` 窄字段从 `bindings.PetBubble.type` 转 `BubbleType` 是 listener 边界
// cast。手抄类型删除。
