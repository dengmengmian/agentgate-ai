import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent, act } from "@testing-library/react";
import { ToastContainer, toast } from "./Toast";

describe("Toast", () => {
  it("renders ToastContainer", () => {
    render(<ToastContainer />);
    expect(document.querySelector(".fixed.bottom-4")).toBeInTheDocument();
  });

  it("shows toast when toast() is called", () => {
    render(<ToastContainer />);
    act(() => {
      toast("success", "Operation completed");
    });
    expect(screen.getByText("Operation completed")).toBeInTheDocument();
  });

  it("shows error toast", () => {
    render(<ToastContainer />);
    act(() => {
      toast("error", "Something went wrong");
    });
    expect(screen.getByText("Something went wrong")).toBeInTheDocument();
  });

  it("shows warning toast", () => {
    render(<ToastContainer />);
    act(() => {
      toast("warning", "Please check your input");
    });
    expect(screen.getByText("Please check your input")).toBeInTheDocument();
  });

  it("dismisses toast when close button is clicked", () => {
    vi.useFakeTimers();
    render(<ToastContainer />);
    act(() => {
      toast("success", "Dismiss me");
    });
    expect(screen.getByText("Dismiss me")).toBeInTheDocument();

    const closeBtn = screen.getByRole("button");
    fireEvent.click(closeBtn);

    expect(screen.queryByText("Dismiss me")).not.toBeInTheDocument();
    vi.useRealTimers();
  });
});
