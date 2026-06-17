import type { ReactElement, ReactNode } from "react";
import type { RenderOptions } from "@testing-library/react";
import { render } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { I18nProvider } from "@/lib/i18n";

interface ProvidersRenderOptions extends Omit<RenderOptions, "wrapper"> {
  route?: string;
}

export function renderWithProviders(
  ui: ReactElement,
  { route = "/", ...options }: ProvidersRenderOptions = {}
) {
  function Wrapper({ children }: { children: ReactNode }) {
    return (
      <I18nProvider>
        <MemoryRouter initialEntries={[route]}>{children}</MemoryRouter>
      </I18nProvider>
    );
  }

  return render(ui, { wrapper: Wrapper, ...options });
}
