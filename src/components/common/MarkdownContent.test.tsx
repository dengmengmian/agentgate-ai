import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { MarkdownContent } from "@/components/common/MarkdownContent";

describe("MarkdownContent", () => {
  it("renders GitHub-flavored tables", () => {
    render(
      <MarkdownContent
        content={"| 名称 | 状态 |\n| --- | --- |\n| Claude | 可用 |"}
      />
    );

    expect(screen.getByRole("table")).toBeInTheDocument();
    expect(screen.getByText("Claude")).toBeInTheDocument();
    expect(screen.getByText("可用")).toBeInTheDocument();
  });

  it("renders task lists", () => {
    render(<MarkdownContent content={"- [x] 已完成\n- [ ] 待处理"} />);

    const items = screen.getAllByRole("checkbox");
    expect(items[0]).toBeChecked();
    expect(items[1]).not.toBeChecked();
    expect(screen.getByText("已完成")).toBeInTheDocument();
    expect(screen.getByText("待处理")).toBeInTheDocument();
  });
});
