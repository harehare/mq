import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { ToastContainer, type ToastItem } from "../../src/components/Toast";

describe("ToastContainer", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("renders nothing when there are no toasts", () => {
    const { container } = render(
      <ToastContainer toasts={[]} onClose={() => {}} />,
    );
    expect(container).toBeEmptyDOMElement();
  });

  it("renders a message for each toast", () => {
    const toasts: ToastItem[] = [
      { id: "1", message: "Saved", type: "success" },
      { id: "2", message: "Something broke", type: "error" },
    ];
    render(<ToastContainer toasts={toasts} onClose={() => {}} />);

    expect(screen.getByText("Saved")).toBeInTheDocument();
    expect(screen.getByText("Something broke")).toBeInTheDocument();
  });

  it("calls onClose when the close button is clicked", () => {
    const onClose = vi.fn();
    const toasts: ToastItem[] = [
      { id: "1", message: "Saved", type: "success" },
    ];
    render(<ToastContainer toasts={toasts} onClose={onClose} />);

    fireEvent.click(screen.getByRole("button"));
    expect(onClose).toHaveBeenCalledWith("1");
  });

  it("auto-dismisses after the default duration", () => {
    const onClose = vi.fn();
    const toasts: ToastItem[] = [
      { id: "1", message: "Saved", type: "success" },
    ];
    render(<ToastContainer toasts={toasts} onClose={onClose} />);

    expect(onClose).not.toHaveBeenCalled();
    vi.advanceTimersByTime(3000);
    expect(onClose).toHaveBeenCalledWith("1");
  });

  it("honors a custom duration", () => {
    const onClose = vi.fn();
    const toasts: ToastItem[] = [
      { id: "1", message: "Saved", type: "info", duration: 1000 },
    ];
    render(<ToastContainer toasts={toasts} onClose={onClose} />);

    vi.advanceTimersByTime(999);
    expect(onClose).not.toHaveBeenCalled();
    vi.advanceTimersByTime(1);
    expect(onClose).toHaveBeenCalledWith("1");
  });
});
