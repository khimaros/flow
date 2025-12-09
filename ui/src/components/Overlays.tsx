import { useEffect } from "react";
import { useClampedMenuPosition } from "../hooks/useClampedMenuPosition";

export const ContextMenu = ({
  x,
  y,
  options,
  onClose,
}: {
  x: number;
  y: number;
  options: { label: string; onClick: () => void }[];
  onClose: () => void;
}) => {
  // close on click outside
  useEffect(() => {
    const handleClick = () => onClose();
    window.addEventListener("click", handleClick);
    return () => window.removeEventListener("click", handleClick);
  }, [onClose]);

  const { ref, pos } = useClampedMenuPosition(x, y);

  return (
    <div
      ref={ref}
      style={{
        position: "fixed",
        top: pos.y,
        left: pos.x,
        background: "var(--panel-bg)",
        border: "1px solid var(--panel-border)",
        borderRadius: "6px",
        boxShadow: "0 4px 6px -1px rgba(0,0,0,0.1)",
        zIndex: 1000,
        padding: "4px 0",
        minWidth: "150px",
      }}
    >
      {options.map((opt, i) => (
        <div
          key={i}
          onClick={opt.onClick}
          style={{
            padding: "8px 12px",
            cursor: "pointer",
            fontSize: "13px",
            color: "var(--text-color)",
          }}
          className="context-menu-item"
        >
          {opt.label}
        </div>
      ))}
    </div>
  );
};

export const NodeSelector = ({
  x,
  y,
  onClose,
  onSelect,
  options = [],
}: {
  x: number;
  y: number;
  onClose: () => void;
  onSelect: (type: string) => void;
  options?: { name: string; title: string; description: string }[];
}) => {
  useEffect(() => {
    const handleClick = () => onClose();
    // delay adding the listener to avoid the immediate click event that opened the menu
    const timer = setTimeout(() => {
      window.addEventListener("click", handleClick);
    }, 100);

    return () => {
      clearTimeout(timer);
      window.removeEventListener("click", handleClick);
    };
  }, [onClose]);

  const { ref, pos } = useClampedMenuPosition(x, y);

  return (
    <div
      ref={ref}
      style={{
        position: "fixed",
        top: pos.y,
        left: pos.x,
        background: "var(--panel-bg)",
        border: "1px solid var(--panel-border)",
        borderRadius: "6px",
        boxShadow: "0 4px 6px -1px rgba(0,0,0,0.1)",
        zIndex: 1000,
        padding: "0 0 4px 0",
        minWidth: "150px",
        maxHeight: "400px",
        overflowY: "auto",
      }}
    >
      <div
        style={{
          padding: "8px 12px",
          fontSize: "11px",
          fontWeight: 600,
          color: "var(--node-text)",
          borderBottom: "1px solid var(--panel-border)",
          position: "sticky",
          top: 0,
          background: "var(--panel-bg)",
          borderRadius: "6px 6px 0 0",
        }}
      >
        Add Node
      </div>
      {options.map((opt) => (
        <div
          key={opt.name}
          onClick={() => onSelect(opt.name)}
          style={{
            padding: "8px 12px",
            cursor: "pointer",
            fontSize: "13px",
            color: "var(--text-color)",
          }}
          className="context-menu-item"
          title={opt.description}
        >
          {opt.title}
        </div>
      ))}
    </div>
  );
};
