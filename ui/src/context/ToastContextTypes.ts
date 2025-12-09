import { createContext } from "react";
import type { ToastType } from "../types";

export interface ToastContextType {
  addToast: (
    type: ToastType,
    title: string,
    message: string,
    customBorderColor?: string,
  ) => void;
  dismissToast: (id: string) => void;
}

export const ToastContext = createContext<ToastContextType | undefined>(
  undefined,
);
