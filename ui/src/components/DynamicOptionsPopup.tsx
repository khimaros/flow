import React, {
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  useCallback,
} from "react";
import { createPortal } from "react-dom";
import { useViewport } from "reactflow";
import type { SelectOption } from "../types";

interface DynamicOptionsPopupProps {
  open: boolean;
  onClose: () => void;
  /** the trigger element this popup anchors to. used for positioning and
   *  for outside-click detection. */
  triggerRef: React.RefObject<HTMLElement | null>;
  options: SelectOption[];
  loading: boolean;
  error: string | null;
  /** "single" auto-closes after selection; "multi" stays open. visually:
   *  multi mode adds a checkbox prefix per row, single mode highlights the
   *  selected row. */
  mode: "single" | "multi";
  selectedValues: Set<string>;
  onPick: (value: string) => void;
  /** single-select only: invoked when the user presses Enter in the search
   *  input without clicking a row. enables free-form text entry. */
  onAcceptSearchText?: (text: string) => void;
  /** seed text for the search box on each open. */
  initialSearch?: string;
}

export const DynamicOptionsPopup: React.FC<DynamicOptionsPopupProps> = ({
  open,
  onClose,
  triggerRef,
  options,
  loading,
  error,
  mode,
  selectedValues,
  onPick,
  onAcceptSearchText,
  initialSearch = "",
}) => {
  const [search, setSearch] = useState(initialSearch);
  const [popupRect, setPopupRect] = useState<{
    top: number;
    left: number;
    width: number;
  } | null>(null);
  const popupRef = useRef<HTMLDivElement | null>(null);
  const searchRef = useRef<HTMLInputElement | null>(null);
  // ref-callback approach for auto-focus: a useEffect on [open] races with
  // popupRect being set asynchronously (the popup returns null until the rect
  // is measured, so the input doesn't exist when the effect first fires).
  // wantFocusRef flips when `open` becomes true; the search input's ref
  // callback consumes the flag the moment the element mounts.
  const wantFocusRef = useRef(false);
  const setSearchRef = useCallback((el: HTMLInputElement | null) => {
    searchRef.current = el;
    if (el && wantFocusRef.current) {
      el.focus();
      wantFocusRef.current = false;
    }
  }, []);

  // viewport drives re-measure on pan/zoom and zoom-scaling of the popup.
  const viewport = useViewport();
  const zoom = viewport.zoom;

  const updatePopupRect = useCallback(() => {
    const el = triggerRef.current;
    if (!el) return;
    const r = el.getBoundingClientRect();
    setPopupRect({ top: r.bottom + 2, left: r.left, width: r.width });
  }, [triggerRef]);

  // measure synchronously before paint so the popup never shows at a stale
  // position from the previous open.
  useLayoutEffect(() => {
    if (!open) {
      setPopupRect(null);
      return;
    }
    updatePopupRect();
  }, [open, updatePopupRect, viewport.x, viewport.y, viewport.zoom]);

  useEffect(() => {
    if (!open) return;
    const onResize = () => updatePopupRect();
    window.addEventListener("resize", onResize);
    window.addEventListener("scroll", onResize, true);
    return () => {
      window.removeEventListener("resize", onResize);
      window.removeEventListener("scroll", onResize, true);
    };
  }, [open, updatePopupRect, viewport.x, viewport.y, viewport.zoom]);

  useEffect(() => {
    if (!open) return;
    const onDocClick = (e: MouseEvent) => {
      const target = e.target as Element;
      if (triggerRef.current?.contains(target)) return;
      if (popupRef.current?.contains(target)) return;
      onClose();
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    document.addEventListener("mousedown", onDocClick);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDocClick);
      document.removeEventListener("keydown", onKey);
    };
  }, [open, onClose, triggerRef]);

  // reset search + request focus on each open. actual focusing happens in
  // setSearchRef once the input element actually mounts.
  useEffect(() => {
    if (open) {
      setSearch(initialSearch);
      wantFocusRef.current = true;
    } else {
      wantFocusRef.current = false;
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open]);

  const filteredOptions = useMemo(() => {
    const q = search.trim().toLowerCase();
    if (q === "") return options;
    return options.filter((o) => {
      if (o.value.toLowerCase().includes(q)) return true;
      if (o.label && o.label.toLowerCase().includes(q)) return true;
      return false;
    });
  }, [options, search]);

  const handlePick = (value: string) => {
    onPick(value);
    if (mode === "single") onClose();
  };

  const handleSearchKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key !== "Enter") return;
    e.preventDefault();
    // if exactly one option remains after filtering, pick it; otherwise
    // (single mode) accept the typed text as a free-form value.
    if (filteredOptions.length === 1) {
      handlePick(filteredOptions[0].value);
      return;
    }
    if (mode === "single" && onAcceptSearchText && search.length > 0) {
      onAcceptSearchText(search);
      onClose();
    }
  };

  if (!open || !popupRect) return null;

  return createPortal(
    <div
      ref={popupRef}
      className="nodrag nowheel"
      style={{
        position: "fixed",
        top: popupRect.top,
        left: popupRect.left,
        // unscaled width compensates so transform: scale(zoom) yields the
        // trigger's visual width on screen.
        width: popupRect.width / zoom,
        maxHeight: 280,
        transformOrigin: "top left",
        transform: `scale(${zoom})`,
        zIndex: 10000,
        background: "var(--panel-bg)",
        color: "var(--input-text)",
        border: "1px solid var(--panel-border)",
        borderRadius: "3px",
        boxShadow: "var(--node-shadow)",
        fontFamily: "monospace",
        fontSize: "12px",
        display: "flex",
        flexDirection: "column",
      }}
    >
      <div
        style={{
          padding: "4px 6px",
          borderBottom: "1px solid var(--panel-border)",
          flexShrink: 0,
        }}
      >
        <input
          ref={setSearchRef}
          type="text"
          className="nodrag"
          value={search}
          placeholder="search…"
          onChange={(e) => setSearch(e.target.value)}
          onKeyDown={handleSearchKeyDown}
          style={{
            width: "100%",
            boxSizing: "border-box",
            padding: "4px 6px",
            fontFamily: "monospace",
            fontSize: "12px",
            background: "var(--input-bg)",
            color: "var(--input-text)",
            border: "1px solid var(--input-border)",
            borderRadius: "3px",
            outline: "none",
          }}
        />
      </div>
      <div
        style={{
          overflowY: "auto",
          padding: "4px 8px",
          flex: 1,
          minHeight: 0,
        }}
      >
        {options.length === 0 ? (
          <div style={{ opacity: 0.6, padding: "4px 2px" }}>
            {loading
              ? "loading…"
              : error
                ? `error: ${error}`
                : "no options available — click refresh"}
          </div>
        ) : filteredOptions.length === 0 ? (
          <div style={{ opacity: 0.6, padding: "4px 2px" }}>
            no matches
            {mode === "single" && onAcceptSearchText && search.length > 0
              ? " — press Enter to use as custom value"
              : ""}
          </div>
        ) : (
          filteredOptions.map((opt) => {
            const isSelected = selectedValues.has(opt.value);
            const baseBg =
              mode === "single" && isSelected
                ? "var(--button-hover)"
                : "transparent";
            return (
              <label
                key={opt.value}
                onClick={() => {
                  if (mode === "single") handlePick(opt.value);
                }}
                onMouseEnter={(e) =>
                  (e.currentTarget.style.background = "var(--button-hover)")
                }
                onMouseLeave={(e) =>
                  (e.currentTarget.style.background = baseBg)
                }
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: "6px",
                  padding: "3px 4px",
                  cursor: "pointer",
                  borderRadius: "2px",
                  background: baseBg,
                  fontWeight:
                    mode === "single" && isSelected ? "bold" : undefined,
                  transition: "background 0.08s",
                }}
                title={
                  opt.label && opt.label !== opt.value ? opt.label : undefined
                }
              >
                {mode === "multi" && (
                  <input
                    type="checkbox"
                    checked={isSelected}
                    onChange={() => handlePick(opt.value)}
                    onClick={(e) => e.stopPropagation()}
                    style={{ margin: 0, flexShrink: 0 }}
                  />
                )}
                <span
                  style={{
                    flex: 1,
                    minWidth: 0,
                    overflow: "hidden",
                    textOverflow: "ellipsis",
                    whiteSpace: "nowrap",
                  }}
                >
                  {opt.value}
                </span>
              </label>
            );
          })
        )}
      </div>
    </div>,
    document.body,
  );
};
