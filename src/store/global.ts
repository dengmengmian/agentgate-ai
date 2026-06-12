// 全局只读资源 store——4 个 slice 共享 providers / gatewaySettings / pricing /
// routeProfiles。这些资源跨页只读，原来每个组件各自 useState + useEffect + invoke
// 拉，造成 8 个组件每次 mount 都重新请求一遍。
//
// 设计原则：
// - 每个 slice 内部维护 items / loading / error；fetch() 防重入——已经 loading
//   时直接返回当前 in-flight promise，避免 dashboard 5s 轮询 + sidebar mount 同
//   时打两次 invoke。
// - refetch() 强制忽略 loading 重新拉一次（用户手动 refresh 场景）。
// - hook 直接 selector 整个对象——组件 destructure 拿 items / loading；订阅粒度
//   足够细，单 slice 内部 setState 不会让别的 slice consumer 重渲染。
//
// 不做的事：
// - 不维护 stale time / TTL，新数据由调用方在 mutation 后显式 refetch。
// - 不替代轮询逻辑，Providers/Routes 的 usePolling 仍然存在，只是把
//   useState + useEffect 那一对替换成 store.fetch()。

import { create } from "zustand";
import * as api from "@/lib/api";
import type {
  ProviderView,
  GatewaySettings,
  ModelPricing,
  RouteProfileView,
} from "@/lib/bindings";
import type { GatewayStatus } from "@/types/gateway";

/// 一个 slice 的通用形态——把 list/single-value 都套进 `items`/`value`
/// 仍然太异质，所以分别写两种基础类型：list 用 items: T[]，single 用
/// value: T | null。
interface ListSlice<T> {
  items: T[];
  loading: boolean;
  error: string | null;
  /// 首次拉取。已经 loading 时直接返回当前 in-flight promise，调用方拿到的
  /// 始终是同一次远端调用结果，避免 invoke 风暴。
  fetch: () => Promise<void>;
  /// 强制重新拉取（mutation 后调用）。会 reset error、设置 loading。
  refetch: () => Promise<void>;
}

interface ValueSlice<T> {
  value: T | null;
  loading: boolean;
  error: string | null;
  fetch: () => Promise<void>;
  refetch: () => Promise<void>;
}

// ── providers ──────────────────────────────────────────────────────

interface ProvidersStore extends ListSlice<ProviderView> {}

/// 防重入用的模块级 in-flight promise——zustand state 内放 promise 会触发不必要
/// 的订阅者重渲染，所以单独放在模块作用域。每个 slice 一个。
let providersInflight: Promise<void> | null = null;

export const useProviders = create<ProvidersStore>((set, get) => ({
  items: [],
  loading: false,
  error: null,
  fetch: async () => {
    if (providersInflight) return providersInflight;
    if (get().loading) return;
    providersInflight = (async () => {
      set({ loading: true, error: null });
      try {
        const items = await api.listProviders();
        set({ items, loading: false });
      } catch (err) {
        const message = (err as api.AppError)?.message ?? String(err);
        set({ loading: false, error: message });
      } finally {
        providersInflight = null;
      }
    })();
    return providersInflight;
  },
  refetch: async () => {
    // 取消 in-flight 也复用同一个 promise 链——避免并发 refetch 互相覆盖。
    if (providersInflight) {
      await providersInflight;
    }
    providersInflight = (async () => {
      set({ loading: true, error: null });
      try {
        const items = await api.listProviders();
        set({ items, loading: false });
      } catch (err) {
        const message = (err as api.AppError)?.message ?? String(err);
        set({ loading: false, error: message });
      } finally {
        providersInflight = null;
      }
    })();
    return providersInflight;
  },
}));

// ── gateway settings ───────────────────────────────────────────────

interface GatewaySettingsStore extends ValueSlice<GatewaySettings> {}

let gatewaySettingsInflight: Promise<void> | null = null;

export const useGatewaySettings = create<GatewaySettingsStore>((set, get) => ({
  value: null,
  loading: false,
  error: null,
  fetch: async () => {
    if (gatewaySettingsInflight) return gatewaySettingsInflight;
    if (get().loading) return;
    gatewaySettingsInflight = (async () => {
      set({ loading: true, error: null });
      try {
        const value = await api.getGatewaySettings();
        set({ value, loading: false });
      } catch (err) {
        const message = (err as api.AppError)?.message ?? String(err);
        set({ loading: false, error: message });
      } finally {
        gatewaySettingsInflight = null;
      }
    })();
    return gatewaySettingsInflight;
  },
  refetch: async () => {
    if (gatewaySettingsInflight) {
      await gatewaySettingsInflight;
    }
    gatewaySettingsInflight = (async () => {
      set({ loading: true, error: null });
      try {
        const value = await api.getGatewaySettings();
        set({ value, loading: false });
      } catch (err) {
        const message = (err as api.AppError)?.message ?? String(err);
        set({ loading: false, error: message });
      } finally {
        gatewaySettingsInflight = null;
      }
    })();
    return gatewaySettingsInflight;
  },
}));

// ── pricing ────────────────────────────────────────────────────────

interface PricingStore extends ListSlice<ModelPricing> {
  /// pricing 在 Settings 里需要本地 mutate（add / update / delete 即时反映），
  /// 暴露一个 setter 让组件 mutation 后无需再次 refetch。
  setItems: (items: ModelPricing[]) => void;
}

let pricingInflight: Promise<void> | null = null;

export const usePricing = create<PricingStore>((set, get) => ({
  items: [],
  loading: false,
  error: null,
  setItems: (items) => set({ items }),
  fetch: async () => {
    if (pricingInflight) return pricingInflight;
    if (get().loading) return;
    pricingInflight = (async () => {
      set({ loading: true, error: null });
      try {
        const items = await api.listModelPricing();
        set({ items, loading: false });
      } catch (err) {
        const message = (err as api.AppError)?.message ?? String(err);
        set({ loading: false, error: message });
      } finally {
        pricingInflight = null;
      }
    })();
    return pricingInflight;
  },
  refetch: async () => {
    if (pricingInflight) {
      await pricingInflight;
    }
    pricingInflight = (async () => {
      set({ loading: true, error: null });
      try {
        const items = await api.listModelPricing();
        set({ items, loading: false });
      } catch (err) {
        const message = (err as api.AppError)?.message ?? String(err);
        set({ loading: false, error: message });
      } finally {
        pricingInflight = null;
      }
    })();
    return pricingInflight;
  },
}));

// ── route profiles ─────────────────────────────────────────────────

interface RouteProfilesStore extends ListSlice<RouteProfileView> {}

let routeProfilesInflight: Promise<void> | null = null;

export const useRouteProfiles = create<RouteProfilesStore>((set, get) => ({
  items: [],
  loading: false,
  error: null,
  fetch: async () => {
    if (routeProfilesInflight) return routeProfilesInflight;
    if (get().loading) return;
    routeProfilesInflight = (async () => {
      set({ loading: true, error: null });
      try {
        const items = await api.listRouteProfiles();
        set({ items, loading: false });
      } catch (err) {
        const message = (err as api.AppError)?.message ?? String(err);
        set({ loading: false, error: message });
      } finally {
        routeProfilesInflight = null;
      }
    })();
    return routeProfilesInflight;
  },
  refetch: async () => {
    if (routeProfilesInflight) {
      await routeProfilesInflight;
    }
    routeProfilesInflight = (async () => {
      set({ loading: true, error: null });
      try {
        const items = await api.listRouteProfiles();
        set({ items, loading: false });
      } catch (err) {
        const message = (err as api.AppError)?.message ?? String(err);
        set({ loading: false, error: message });
      } finally {
        routeProfilesInflight = null;
      }
    })();
    return routeProfilesInflight;
  },
}));

// ── gateway status ─────────────────────────────────────────────────
// Topbar（常驻）是唯一轮询源，Dashboard 等页面只订阅；start/stop/restart
// 返回的新状态通过 setValue 直接写入，所有订阅者即时更新。

interface GatewayStatusStore extends ValueSlice<GatewayStatus> {
  /// start/stop/restart 命令的返回值直接写入，不用再发一次查询。
  setValue: (value: GatewayStatus) => void;
}

let gatewayStatusInflight: Promise<void> | null = null;

export const useGatewayStatus = create<GatewayStatusStore>((set, get) => ({
  value: null,
  loading: false,
  error: null,
  setValue: (value) => set({ value }),
  fetch: async () => {
    if (gatewayStatusInflight) return gatewayStatusInflight;
    if (get().loading) return;
    gatewayStatusInflight = (async () => {
      set({ loading: true, error: null });
      try {
        const value = await api.getGatewayStatus();
        set({ value, loading: false });
      } catch (err) {
        const message = (err as api.AppError)?.message ?? String(err);
        set({ loading: false, error: message });
      } finally {
        gatewayStatusInflight = null;
      }
    })();
    return gatewayStatusInflight;
  },
  refetch: async () => {
    if (gatewayStatusInflight) {
      await gatewayStatusInflight;
    }
    gatewayStatusInflight = (async () => {
      set({ loading: true, error: null });
      try {
        const value = await api.getGatewayStatus();
        set({ value, loading: false });
      } catch (err) {
        const message = (err as api.AppError)?.message ?? String(err);
        set({ loading: false, error: message });
      } finally {
        gatewayStatusInflight = null;
      }
    })();
    return gatewayStatusInflight;
  },
}));

/// 测试辅助：清空所有 store——单测之间互不污染。生产代码不应调用。
export function __resetGlobalStoresForTest() {
  providersInflight = null;
  gatewaySettingsInflight = null;
  pricingInflight = null;
  routeProfilesInflight = null;
  gatewayStatusInflight = null;
  useProviders.setState({ items: [], loading: false, error: null });
  useGatewaySettings.setState({ value: null, loading: false, error: null });
  usePricing.setState({ items: [], loading: false, error: null });
  useRouteProfiles.setState({ items: [], loading: false, error: null });
  useGatewayStatus.setState({ value: null, loading: false, error: null });
}
