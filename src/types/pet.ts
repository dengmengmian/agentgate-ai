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
  last_error?: {
    message: string;
    provider?: string;
    timestamp: string;
  } | null;
  today?: {
    requests: number;
    cost: number;
  };
}

export interface PetBubbleEvent {
  text: string;
  text_zh?: string;
  type: "info" | "success" | "error" | "chat";
}
