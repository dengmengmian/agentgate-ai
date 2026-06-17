# UI Testing

AgentGate uses Vitest, React Testing Library, and jsdom for UI tests. Add browser E2E tests only for a few critical flows after component and page tests are stable.

## Commands

```bash
pnpm vitest run
pnpm vitest run src/pages/Providers.test.tsx
pnpm test:playwright
pnpm test:quickstart
pnpm test:ci-local
pnpm test:release-local
pnpm test:ui
```

`pnpm test:quickstart` runs the 5-minute quickstart smoke flow with a local mock OpenAI-compatible upstream: add provider, start the gateway, send one Chat Completions request, and verify the request log.

`pnpm test:release-local` is the release gate for local verification. It runs lint, build, all Vitest tests, download-doc checks, the quickstart smoke, and Docker preflight in order. Docker is required for a real release check; `AGENTGATE_SKIP_DOCKER_PREFLIGHT=1 pnpm test:release-local` is only for local debugging when Docker Hub is unreachable.

`pnpm test:ci-local` mirrors the non-Docker CI gate: lint, build, Playwright smoke, Vitest, docs check, and quickstart smoke.

`pnpm test:playwright` builds the production frontend and checks that the app shell plus core pages render in Chromium with mocked Tauri IPC. Keep this smoke shallow; business behavior belongs in Vitest and `test:quickstart`.

To smoke a real provider locally, pass secrets through environment variables only:

```bash
AGENTGATE_SMOKE_REAL=1 \
AGENTGATE_SMOKE_BASE_URL="https://..." \
AGENTGATE_SMOKE_API_KEY="$REAL_KEY" \
AGENTGATE_SMOKE_MODEL="..." \
pnpm test:quickstart
```

## What to Test

| Area | Test |
| --- | --- |
| Components | Visible state, disabled state, callbacks, empty states |
| Pages | API loading, rendered data, save/delete actions, error paths |
| Forms | User input, validation, submitted payload |
| Dialogs | Open, close, confirm, cancel |
| Routing | Navigation target and route-specific content |
| Tauri APIs | Mock plugin calls; do not call native APIs in jsdom |

## Default Pattern

```tsx
import { screen, fireEvent, waitFor } from "@testing-library/react";
import { vi } from "vitest";
import { renderWithProviders } from "@/components/test-utils";
import * as api from "@/lib/api";
import { Gateway } from "./Gateway";

vi.mock("@/lib/api");

it("saves gateway settings", async () => {
  vi.mocked(api.getGatewaySettings).mockResolvedValue(settings);
  vi.mocked(api.updateGatewaySettings).mockResolvedValue(settings);

  renderWithProviders(<Gateway />);

  fireEvent.change(await screen.findByDisplayValue("4141"), {
    target: { value: "8080" },
  });
  fireEvent.click(screen.getByText("gateway.save"));

  await waitFor(() => {
    expect(api.updateGatewaySettings).toHaveBeenCalledWith(
      expect.objectContaining({ port: 8080 }),
    );
  });
});
```

## Rules

- Prefer user-visible queries such as `getByRole`, `getByText`, and `findByText`.
- Mock `@/lib/api` at page boundaries.
- Reset global stores before tests that touch shared app state.
- Test behavior, not implementation details or CSS class names.
- Keep each test focused on one user outcome.
- Add Playwright later for only the main smoke flows: app loads, provider setup, and gateway controls.
