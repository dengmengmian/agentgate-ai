import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, act, waitFor, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";

vi.mock("@/lib/api");

import * as api from "@/lib/api";
import { Logs } from "./Logs";
import { __resetGlobalStoresForTest } from "@/store/global";

function logItem(id: string, model: string) {
  return {
    id,
    request_id: id,
    timestamp: "2026-06-13T00:00:00Z",
    client: null,
    provider: null,
    model,
    route: null,
    status_code: 200,
    latency_ms: 1,
    error_message: null,
    source: "gateway",
    session_id: null,
  } as any;
}

describe("Logs", () => {
  beforeEach(() => {
    __resetGlobalStoresForTest();
    vi.mocked(api.listRequestLogs).mockResolvedValue([] as any);
    vi.mocked(api.countRequestLogs).mockResolvedValue(0);
    vi.mocked(api.listLogModels).mockResolvedValue([]);
    vi.mocked(api.listProviders).mockResolvedValue([] as any);
    vi.mocked(api.listRouteProfiles).mockResolvedValue([] as any);
  });

  it("慢返回的旧请求结果不覆盖新请求", async () => {
    // listRequestLogs 每次调用返回受控 promise，按序号入队，
    // 由测试决定 resolve 顺序——模拟旧请求比新请求晚返回。
    const resolvers: Array<(v: any) => void> = [];
    vi.mocked(api.listRequestLogs).mockImplementation(
      () => new Promise<any>((resolve) => resolvers.push(resolve))
    );

    const { container } = render(
      <MemoryRouter>
        <Logs />
      </MemoryRouter>
    );

    await waitFor(() => expect(resolvers.length).toBe(1));

    const statusSelect = container.querySelector("select")!;
    await act(async () => {
      statusSelect.value = "error";
      statusSelect.dispatchEvent(new Event("change", { bubbles: true }));
    });
    await waitFor(() => expect(resolvers.length).toBe(2));

    // 新请求先返回
    await act(async () => {
      resolvers[1]([logItem("new", "NEW-MODEL")]);
    });
    expect(await screen.findByText("NEW-MODEL")).toBeInTheDocument();

    // 旧请求后返回——绝不能覆盖新结果
    await act(async () => {
      resolvers[0]([logItem("old", "OLD-MODEL")]);
    });
    expect(screen.queryByText("OLD-MODEL")).not.toBeInTheDocument();
    expect(screen.getByText("NEW-MODEL")).toBeInTheDocument();
  });

  it("翻页后滚动回顶部", async () => {
    vi.mocked(api.listRequestLogs).mockResolvedValue([
      logItem("a", "M"),
    ] as any);
    vi.mocked(api.countRequestLogs).mockResolvedValue(250); // 250/100 → 3 页，出现翻页

    const scrollSpy = vi.fn();
    (Element.prototype as any).scrollIntoView = scrollSpy;

    render(
      <MemoryRouter>
        <Logs />
      </MemoryRouter>
    );

    const next = await screen.findByText("logs.page_next");
    scrollSpy.mockClear();
    await act(async () => {
      next.click();
    });
    expect(scrollSpy).toHaveBeenCalled();
  });
});
