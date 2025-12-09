import { useState, useCallback, useMemo, memo } from "react";
import { Plus, Trash2, ChevronUp, ChevronDown } from "lucide-react";

interface ListEditorControlProps {
  value: string | string[];
  onChange: (value: string | string[]) => void;
  disabled?: boolean;
  onFocus?: () => void;
}

function stringify(item: unknown): string {
  if (item === null || item === undefined) return "";
  if (typeof item === "object") return JSON.stringify(item);
  return String(item);
}

// parse value as list of strings, handling both JSON strings and raw arrays
function parseItems(value: string | string[]): string[] {
  if (!value) return [];
  if (Array.isArray(value)) return value.map(stringify);
  if (typeof value !== "string") return [stringify(value)];

  try {
    const parsed = JSON.parse(value);
    if (Array.isArray(parsed)) {
      return parsed.map(stringify);
    }
  } catch {
    // not valid JSON, treat as single item if non-empty
    if (value.trim()) return [value];
  }
  return [];
}

const ListItem = memo(
  ({
    item,
    index,
    isEditing,
    editValue,
    disabled,
    isFirst,
    isLast,
    onStartEdit,
    onEditChange,
    onEditKeyDown,
    onCommitEdit,
    onMove,
    onRemove,
  }: {
    item: string;
    index: number;
    isEditing: boolean;
    editValue: string;
    disabled?: boolean;
    isFirst: boolean;
    isLast: boolean;
    onStartEdit: (index: number) => void;
    onEditChange: (value: string) => void;
    onEditKeyDown: (e: React.KeyboardEvent) => void;
    onCommitEdit: () => void;
    onMove: (index: number, direction: "up" | "down") => void;
    onRemove: (index: number) => void;
  }) => {
    const buttonStyle = {
      display: "flex",
      alignItems: "center",
      justifyContent: "center",
      width: "20px",
      height: "20px",
      border: "1px solid var(--input-border)",
      borderRadius: "4px",
      backgroundColor: "var(--button-bg)",
      color: "var(--text-color)",
      cursor: disabled ? "not-allowed" : "pointer",
      opacity: disabled ? 0.5 : 1,
      padding: 0,
    };

    return (
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: "4px",
          padding: "4px 6px",
          backgroundColor: "var(--node-bg)",
          borderRadius: "3px",
          fontSize: "12px",
        }}
      >
        {isEditing ? (
          <input
            className="nodrag"
            type="text"
            value={editValue}
            onChange={(e) => onEditChange(e.target.value)}
            onKeyDown={onEditKeyDown}
            onBlur={onCommitEdit}
            autoFocus
            style={{
              flex: 1,
              padding: "2px 4px",
              fontSize: "12px",
              fontFamily: "monospace",
              border: "1px solid var(--primary-color)",
              borderRadius: "2px",
              backgroundColor: "var(--input-bg)",
              color: "var(--text-color)",
              outline: "none",
            }}
          />
        ) : (
          <span
            onClick={() => !disabled && onStartEdit(index)}
            style={{
              flex: 1,
              overflow: "hidden",
              textOverflow: "ellipsis",
              whiteSpace: "nowrap",
              fontFamily: "monospace",
              cursor: disabled ? "default" : "text",
            }}
            title={disabled ? item : "Click to edit"}
          >
            {item}
          </span>
        )}
        {!disabled && (
          <>
            <button
              type="button"
              onClick={() => onMove(index, "up")}
              disabled={isFirst}
              style={{
                ...buttonStyle,
                opacity: isFirst ? 0.3 : 1,
              }}
              title="Move up"
            >
              <ChevronUp size={12} />
            </button>
            <button
              type="button"
              onClick={() => onMove(index, "down")}
              disabled={isLast}
              style={{
                ...buttonStyle,
                opacity: isLast ? 0.3 : 1,
              }}
              title="Move down"
            >
              <ChevronDown size={12} />
            </button>
            <button
              type="button"
              onClick={() => onRemove(index)}
              style={{
                ...buttonStyle,
                color: "var(--danger-color)",
              }}
              title="Remove item"
            >
              <Trash2 size={12} />
            </button>
          </>
        )}
      </div>
    );
  },
);

export const ListEditorControl = ({
  value,
  onChange,
  disabled,
  onFocus,
}: ListEditorControlProps) => {
  const [newItem, setNewItem] = useState("");
  const [editingIndex, setEditingIndex] = useState<number | null>(null);
  const [editValue, setEditValue] = useState("");
  const items = useMemo(() => parseItems(value), [value]);

  const updateItems = useCallback(
    (newItems: string[]) => {
      onChange(newItems);
    },
    [onChange],
  );

  const addItem = useCallback(() => {
    if (!newItem.trim()) return;
    updateItems([...items, newItem.trim()]);
    setNewItem("");
  }, [items, newItem, updateItems]);

  const removeItem = useCallback(
    (index: number) => {
      updateItems(items.filter((_, i) => i !== index));
    },
    [items, updateItems],
  );

  const moveItem = useCallback(
    (index: number, direction: "up" | "down") => {
      const newIndex = direction === "up" ? index - 1 : index + 1;
      if (newIndex < 0 || newIndex >= items.length) return;
      const newItems = [...items];
      [newItems[index], newItems[newIndex]] = [
        newItems[newIndex],
        newItems[index],
      ];
      updateItems(newItems);
    },
    [items, updateItems],
  );

  const startEditing = useCallback(
    (index: number) => {
      setEditingIndex(index);
      setEditValue(items[index]);
    },
    [items],
  );

  const commitEdit = useCallback(() => {
    if (editingIndex === null) return;
    if (editValue.trim()) {
      const newItems = [...items];
      newItems[editingIndex] = editValue.trim();
      updateItems(newItems);
    }
    setEditingIndex(null);
    setEditValue("");
  }, [editingIndex, editValue, items, updateItems]);

  const cancelEdit = useCallback(() => {
    setEditingIndex(null);
    setEditValue("");
  }, []);

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter") {
      e.preventDefault();
      addItem();
    }
  };

  const handleEditKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === "Enter") {
        e.preventDefault();
        commitEdit();
      } else if (e.key === "Escape") {
        e.preventDefault();
        cancelEdit();
      }
    },
    [commitEdit, cancelEdit],
  );

  const buttonStyle = {
    display: "flex",
    alignItems: "center",
    justifyContent: "center",
    width: "24px",
    height: "24px",
    border: "1px solid var(--input-border)",
    borderRadius: "4px",
    backgroundColor: "var(--button-bg)",
    color: "var(--text-color)",
    cursor: disabled ? "not-allowed" : "pointer",
    opacity: disabled ? 0.5 : 1,
    padding: 0,
  };

  if (disabled) {
    return (
      <div
        className="nodrag"
        onFocus={onFocus}
        style={{
          display: "flex",
          flexDirection: "column",
          gap: "4px",
          padding: "6px",
          backgroundColor: "var(--input-bg)",
          borderRadius: "4px",
          border: "1px solid var(--input-border)",
          flex: 1,
          flexShrink: 1,
          height: "100%",
          maxHeight: "100%",
          minHeight: 0,
          overflow: "hidden",
          boxSizing: "border-box",
        }}
      >
        <div
          style={{
            display: "flex",
            flexDirection: "column",
            gap: "2px",
            overflowY: "auto",
            flex: 1,
            minHeight: 0,
          }}
        >
          {items.map((item, index) => (
            <div
              key={index}
              style={{
                padding: "4px 6px",
                backgroundColor: "var(--node-bg)",
                borderRadius: "2px",
                fontSize: "11px",
                fontFamily: "monospace",
                whiteSpace: "nowrap",
                overflow: "hidden",
                textOverflow: "ellipsis",
                color: "var(--text-color)",
                opacity: 0.9,
                minHeight: "1.5em",
                flexShrink: 0,
              }}
              title={item}
            >
              {item}
            </div>
          ))}
          {items.length === 0 && (
            <span
              style={{
                fontSize: "11px",
                color: "var(--text-muted)",
                fontStyle: "italic",
                padding: "4px",
              }}
            >
              empty list
            </span>
          )}
        </div>
      </div>
    );
  }

  return (
    <div
      className="nodrag"
      onFocus={onFocus}
      style={{
        display: "flex",
        flexDirection: "column",
        gap: "6px",
        padding: "8px",
        backgroundColor: "var(--input-bg)",
        borderRadius: "4px",
        border: "1px solid var(--input-border)",
        flex: 1,
        flexShrink: 1,
        height: "100%",
        maxHeight: "100%",
        minHeight: 0,
        overflow: "hidden",
        boxSizing: "border-box",
      }}
    >
      {/* item list */}
      <div
        style={{
          display: "flex",
          flexDirection: "column",
          gap: "4px",
          overflowY: "auto",
          flex: 1,
          minHeight: 0,
          maxHeight: "100%",
        }}
      >
        {items.map((item, index) => (
          <ListItem
            key={index}
            item={item}
            index={index}
            isEditing={editingIndex === index}
            editValue={editValue}
            disabled={disabled}
            isFirst={index === 0}
            isLast={index === items.length - 1}
            onStartEdit={startEditing}
            onEditChange={setEditValue}
            onEditKeyDown={handleEditKeyDown}
            onCommitEdit={commitEdit}
            onMove={moveItem}
            onRemove={removeItem}
          />
        ))}
        {items.length === 0 && (
          <span
            style={{
              fontSize: "11px",
              color: "var(--text-muted)",
              fontStyle: "italic",
              padding: "4px",
            }}
          >
            no items
          </span>
        )}
      </div>

      {/* add new item */}
      <div style={{ display: "flex", gap: "4px" }}>
        <input
          type="text"
          value={newItem}
          onChange={(e) => setNewItem(e.target.value)}
          onKeyDown={handleKeyDown}
          disabled={disabled}
          placeholder="add item..."
          style={{
            flex: 1,
            padding: "4px 8px",
            fontSize: "12px",
            fontFamily: "monospace",
            border: "1px solid var(--input-border)",
            borderRadius: "4px",
            backgroundColor: "var(--input-bg)",
            color: "var(--text-color)",
          }}
        />
        <button
          type="button"
          onClick={addItem}
          disabled={disabled || !newItem.trim()}
          style={{
            ...buttonStyle,
            backgroundColor: newItem.trim()
              ? "var(--primary-color)"
              : "var(--button-bg)",
            color: newItem.trim() ? "white" : "var(--text-color)",
            opacity: disabled || !newItem.trim() ? 0.5 : 1,
          }}
          title="Add item"
        >
          <Plus size={14} />
        </button>
      </div>
    </div>
  );
};
