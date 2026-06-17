import { describe, it, expect } from "vitest";
import { render } from "@testing-library/react";
import { RobotPet } from "./pets/RobotPet";
import { PixelCat } from "./pets/PixelCat";
import { SlimePet } from "./pets/SlimePet";
import { FoxPet } from "./pets/FoxPet";
import { OctopusPet } from "./pets/OctopusPet";
import { GhostPet } from "./pets/GhostPet";
import { OxPet } from "./pets/OxPet";
import { SuperSoldierPet } from "./pets/SuperSoldierPet";
import { CoderPet } from "./pets/CoderPet";
import type { PetState } from "@/types/pet";

const PET_COMPONENTS = [
  { name: "RobotPet", Component: RobotPet },
  { name: "PixelCat", Component: PixelCat },
  { name: "SlimePet", Component: SlimePet },
  { name: "FoxPet", Component: FoxPet },
  { name: "OctopusPet", Component: OctopusPet },
  { name: "GhostPet", Component: GhostPet },
  { name: "OxPet", Component: OxPet },
  { name: "SuperSoldierPet", Component: SuperSoldierPet },
  { name: "CoderPet", Component: CoderPet },
];

const STATES: PetState[] = ["idle", "active", "error", "sleep", "poke"];

describe("pet SVG components", () => {
  for (const { name, Component } of PET_COMPONENTS) {
    describe(name, () => {
      for (const state of STATES) {
        it(`renders for state ${state}`, () => {
          const { container } = render(<Component state={state} />);
          expect(container.querySelector("svg")).toBeInTheDocument();
        });
      }
    });
  }
});
