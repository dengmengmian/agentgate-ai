import { memo } from "react";
import type { PetType, PetState } from "@/types/pet";
import { RobotPet } from "./pets/RobotPet";
import { PixelCat } from "./pets/PixelCat";
import { SlimePet } from "./pets/SlimePet";
import { FoxPet } from "./pets/FoxPet";
import { OctopusPet } from "./pets/OctopusPet";
import { GhostPet } from "./pets/GhostPet";
import { OxPet } from "./pets/OxPet";
import { SuperSoldierPet } from "./pets/SuperSoldierPet";
import { CoderPet } from "./pets/CoderPet";

// type → SVG 组件。宠物窗口(PetApp)和主窗口的聊天页共用这一份映射。
// memo 一次:无关 state 变化时,只要 state prop 没变就跳过 SVG reconcile。
export const PET_COMPONENTS: Record<
  PetType,
  React.ComponentType<{ state: PetState }>
> = {
  robot: memo(RobotPet),
  "pixel-cat": memo(PixelCat),
  slime: memo(SlimePet),
  fox: memo(FoxPet),
  octopus: memo(OctopusPet),
  ghost: memo(GhostPet),
  ox: memo(OxPet),
  soldier: memo(SuperSoldierPet),
  coder: memo(CoderPet),
};
