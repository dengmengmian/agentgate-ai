import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { Bubble, type BubbleType } from "./Bubble";

describe("Bubble", () => {
  it("renders the provided text", () => {
    render(<Bubble text="Hello!" type="chat" onDone={vi.fn()} />);
    expect(screen.getByText("Hello!")).toBeInTheDocument();
  });

  it.each<BubbleType>(["info", "success", "error", "chat"])(
    "renders %s bubbles",
    (type) => {
      render(<Bubble text={`${type} bubble`} type={type} onDone={vi.fn()} />);
      expect(screen.getByText(`${type} bubble`)).toBeInTheDocument();
    }
  );

  it("calls onDone when clicked", () => {
    const onDone = vi.fn();
    render(<Bubble text="Dismiss me" type="info" onDone={onDone} />);
    fireEvent.click(screen.getByText("Dismiss me"));
    expect(onDone).toHaveBeenCalledTimes(1);
  });
});
