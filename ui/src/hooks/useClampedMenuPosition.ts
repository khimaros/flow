import { useLayoutEffect, useRef, useState } from "react";

// viewport edge padding for popup menus
const MENU_VIEWPORT_MARGIN = 8;

// clamp a menu's (x, y) so it stays inside the viewport, measuring after mount
export const useClampedMenuPosition = (x: number, y: number) => {
  const ref = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState({ x, y });
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const { width, height } = el.getBoundingClientRect();
    const maxX = window.innerWidth - width - MENU_VIEWPORT_MARGIN;
    const maxY = window.innerHeight - height - MENU_VIEWPORT_MARGIN;
    setPos({
      x: Math.max(MENU_VIEWPORT_MARGIN, Math.min(x, maxX)),
      y: Math.max(MENU_VIEWPORT_MARGIN, Math.min(y, maxY)),
    });
  }, [x, y]);
  return { ref, pos };
};
