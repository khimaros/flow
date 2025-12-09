import React, { useState, useCallback, type ReactNode } from "react";
import { v4 as uuidv4 } from "uuid";
import { X } from "lucide-react";
import type { Toast, ToastType } from "../types";
import { ToastContext } from "./ToastContextTypes";

export const ToastProvider: React.FC<{ children: ReactNode }> = ({
  children,
}) => {
  const [toasts, setToasts] = useState<Toast[]>([]);

  const dismissToast = useCallback((id: string) => {
    setToasts((prev) =>
      prev.map((t) => (t.id === id ? { ...t, removing: true } : t)),
    );
    setTimeout(() => {
      setToasts((prev) => prev.filter((t) => t.id !== id));
    }, 300);
  }, []);

  const addToast = useCallback(
    (
      type: ToastType,
      title: string,
      message: string,
      customBorderColor?: string,
    ) => {
      const id = uuidv4();
      setToasts((prev) => [
        { id, type, title, message, borderColor: customBorderColor },
        ...prev,
      ]);

      // auto-dismiss
      setTimeout(() => {
        dismissToast(id);
      }, 5000);
    },
    [dismissToast],
  );

  return (
    <ToastContext.Provider value={{ addToast, dismissToast }}>
      {children}
      <div
        style={{
          position: "fixed",
          top: "80px",
          right: "20px",
          zIndex: 1000,
          display: "flex",
          flexDirection: "column",
          gap: "10px",
          pointerEvents: "none",
        }}
      >
        {toasts.map((toast) => {
          const isError = toast.type === "error";
          const borderColor =
            toast.borderColor ||
            (isError ? "var(--danger-color)" : "var(--primary-color)");
          return (
            <div
              key={toast.id}
              className="toast"
              style={{
                width: "400px",
                background: "var(--panel-bg)",
                border: `1px solid ${borderColor}`,
                borderRadius: "8px",
                boxShadow: "0 4px 12px rgba(0, 0, 0, 0.15)",
                overflow: "hidden",
                pointerEvents: "auto",
                opacity: toast.removing ? 0 : 1,
                transition: "opacity 300ms ease-out",
              }}
            >
              <div
                style={{
                  display: "flex",
                  justifyContent: "space-between",
                  alignItems: "center",
                  padding: "10px 12px",
                  background: isError
                    ? "color-mix(in srgb, var(--danger-color), var(--panel-bg) 85%)"
                    : "color-mix(in srgb, var(--primary-color), var(--panel-bg) 85%)",
                  borderBottom: `1px solid ${borderColor}`,
                }}
              >
                <span
                  style={{
                    fontWeight: 600,
                    fontSize: "13px",
                    color: borderColor,
                  }}
                >
                  {toast.title}
                </span>
                <button
                  onClick={() => dismissToast(toast.id)}
                  style={{
                    background: "transparent",
                    border: "none",
                    cursor: "pointer",
                    padding: "2px",
                    color: borderColor,
                    display: "flex",
                  }}
                >
                  <X size={16} />
                </button>
              </div>
              <div
                style={{
                  padding: "12px",
                  fontSize: "12px",
                  fontFamily: isError ? "monospace" : "inherit",
                  color: "var(--text-color)",
                  whiteSpace: "pre-wrap",
                  wordBreak: "break-word",
                  maxHeight: "200px",
                  overflow: "auto",
                }}
              >
                {toast.message}
              </div>
            </div>
          );
        })}
      </div>
    </ToastContext.Provider>
  );
};
