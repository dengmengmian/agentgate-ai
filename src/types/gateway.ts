// 从 bindings re-export。UpdateGatewaySettingsInput 用 Partial 兼容旧 partial 构造。
import type {
  GatewayStatus,
  GatewaySettings,
  UpdateGatewaySettingsInput as Wide,
} from "@/lib/bindings";

export type { GatewayStatus, GatewaySettings };
export type UpdateGatewaySettingsInput = {
  [K in keyof Wide]?: Wide[K] | undefined;
};
