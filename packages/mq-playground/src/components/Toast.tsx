import { useEffect } from "react";
import { VscCheck, VscClose, VscInfo } from "react-icons/vsc";
import "./Toast.css";

export type ToastItem = {
  id: string;
  message: string;
  type: "success" | "error" | "info";
  duration?: number;
};

type ToastProps = {
  toast: ToastItem;
  onClose: (id: string) => void;
};

const Toast = ({ toast, onClose }: ToastProps) => {
  useEffect(() => {
    const timer = setTimeout(() => onClose(toast.id), toast.duration ?? 3000);
    return () => clearTimeout(timer);
  }, [toast, onClose]);

  return (
    <div className={`toast toast-${toast.type}`}>
      {toast.type === "success" && <VscCheck size={14} />}
      {toast.type === "info" && <VscInfo size={14} />}
      <span className="toast-message">{toast.message}</span>
      <button className="toast-close" onClick={() => onClose(toast.id)}>
        <VscClose size={12} />
      </button>
    </div>
  );
};

type ToastContainerProps = {
  toasts: ToastItem[];
  onClose: (id: string) => void;
};

export const ToastContainer = ({ toasts, onClose }: ToastContainerProps) => {
  if (toasts.length === 0) return null;

  return (
    <div className="toast-container">
      {toasts.map((toast) => (
        <Toast key={toast.id} toast={toast} onClose={onClose} />
      ))}
    </div>
  );
};
