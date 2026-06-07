export type PetType = "robot" | "pixel-cat" | "slime" | "fox" | "octopus" | "ghost" | "ox" | "soldier" | "coder";

export type PetState = "idle" | "active" | "error" | "sleep" | "poke";

export interface PetSettings {
  pet_type: PetType;
  visible: boolean;
  pos_x: number;
  pos_y: number;
}

export interface UpdatePetSettingsInput {
  pet_type?: string;
  visible?: boolean;
  pos_x?: number;
  pos_y?: number;
}

export interface PetGatewayInfo {
  state: "running" | "stopped" | "active";
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

export interface PetBubbleEvent {
  text: string;
  text_zh?: string;
  type: "info" | "success" | "error" | "chat";
}
