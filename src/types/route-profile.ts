// 从 bindings re-export,Create/Update/Add input 留 Partial。
import type {
  RouteProfileView,
  RouteProfileDetail,
  RouteProfileProviderView,
  CreateRouteProfileInput as WideCreate,
  UpdateRouteProfileInput as WideUpdate,
  AddProviderToRouteInput as WideAdd,
  ProviderRuntimeStatus,
  RouteProfileStats,
} from "@/lib/bindings";

export type {
  RouteProfileView,
  RouteProfileDetail,
  RouteProfileProviderView,
  ProviderRuntimeStatus,
  RouteProfileStats,
};

export type CreateRouteProfileInput = {
  [K in keyof WideCreate]?: WideCreate[K] | undefined;
} & Pick<WideCreate, "name" | "input_protocol">;

export type UpdateRouteProfileInput = {
  [K in keyof WideUpdate]?: WideUpdate[K] | undefined;
};

export type AddProviderToRouteInput = {
  [K in keyof WideAdd]?: WideAdd[K] | undefined;
};

// 历史保留的 RoutingConditions 类型(纯前端 JSON shape,Rust 端只存 String)。
export interface RoutingConditions {
  min_input_chars?: number | null;
  max_input_chars?: number | null;
  has_images?: boolean | null;
  has_tools?: boolean | null;
  system_keywords?: string[] | null;
  model_override?: string | null;
  // 请求 model 名匹配(子串,大小写不敏感)。仅依赖 model 名 → 所有客户端生效。
  model_name_match?: string[] | null;
}
