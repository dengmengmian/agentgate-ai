// 从 bindings re-export。
//
// 5 个客户端 ApplyConfigResult / ToggleResult 在 Rust 端是 5 个不同 struct,
// bindings 用 ClientApplyConfigResult / ClientToggleResult 这种带前缀名字防冲突。
// 这里把 5 份合并成一个 union 让历史调用方继续用通用名。
import type {
  GatewayAuthSettings,
  CodexConfigStatus,
  ClaudeCodeEnvStatus,
  ProfileDetection,
  OpenCodeConfigStatus,
  GeminiCliConfigStatus,
  AtomCodeConfigStatus,
  ClaudeDesktopStatus,
  ClaudeDesktopApplyResult,
  CodexApplyConfigResult,
  ClaudeCodeApplyConfigResult,
  OpenCodeApplyConfigResult,
  GeminiCliApplyConfigResult,
  AtomCodeApplyConfigResult,
  CodexToggleResult,
  ClaudeCodeToggleResult,
  GeminiCliToggleResult,
  AtomCodeToggleResult,
} from "@/lib/bindings";

export type {
  GatewayAuthSettings,
  CodexConfigStatus,
  ClaudeCodeEnvStatus,
  ProfileDetection,
  OpenCodeConfigStatus,
  GeminiCliConfigStatus,
  AtomCodeConfigStatus,
  ClaudeDesktopStatus,
  ClaudeDesktopApplyResult,
};

export type ApplyConfigResult =
  | CodexApplyConfigResult
  | ClaudeCodeApplyConfigResult
  | OpenCodeApplyConfigResult
  | GeminiCliApplyConfigResult
  | AtomCodeApplyConfigResult;

export type ToggleResult =
  | CodexToggleResult
  | ClaudeCodeToggleResult
  | GeminiCliToggleResult
  | AtomCodeToggleResult;
