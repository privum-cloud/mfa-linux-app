import { useEffect, useLayoutEffect, useRef, useState } from "react";

/** Roughly what the menu measures, used before it has been laid out once. */
const ESTIMATED = { width: 190, height: 150 };
/** Kept this far from the window edge when it has to be nudged back in. */
const MARGIN = 8;

export interface FolderMenuAction {
  key: "add" | "rename" | "icon" | "remove";
  label: string;
  /** Separated from the rest, and coloured, because it destroys something. */
  destructive?: boolean;
}

const ACTIONS: FolderMenuAction[] = [
  { key: "add", label: "New subfolder" },
  { key: "rename", label: "Rename" },
  { key: "icon", label: "Change icon" },
  { key: "remove", label: "Delete folder", destructive: true },
];

interface Props {
  x: number;
  y: number;
  folderName: string;
  onPick: (key: FolderMenuAction["key"]) => void;
  onClose: () => void;
}

/**
 * What a folder can do, on right-click.
 *
 * Renaming, the icon and deleting already existed, but only inside the Folders
 * screen: changing a name meant leaving the list, finding the folder again and
 * coming back. They are the same actions, reachable where the folder is.
 */
export default function FolderMenu({ x, y, folderName, onPick, onClose }: Props) {
  const menu = useRef<HTMLDivElement>(null);
  const [at, setAt] = useState({ x, y });

  // Placed before paint, so it is never seen hanging off the window and then
  // jumping back. The window is 460px wide; a menu opened near the right edge
  // has to come back in rather than be clipped.
  useLayoutEffect(() => {
    const box = menu.current?.getBoundingClientRect();
    const width = box?.width || ESTIMATED.width;
    const height = box?.height || ESTIMATED.height;
    setAt({
      x: Math.max(MARGIN, Math.min(x, window.innerWidth - width - MARGIN)),
      y: Math.max(MARGIN, Math.min(y, window.innerHeight - height - MARGIN)),
    });
  }, [x, y]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    // Capture, so a click anywhere closes it before that click does anything
    // else — including a right-click on a different folder.
    const onDown = (e: MouseEvent) => {
      if (!menu.current?.contains(e.target as Node)) onClose();
    };
    document.addEventListener("keydown", onKey);
    document.addEventListener("mousedown", onDown, true);
    document.addEventListener("contextmenu", onDown, true);
    return () => {
      document.removeEventListener("keydown", onKey);
      document.removeEventListener("mousedown", onDown, true);
      document.removeEventListener("contextmenu", onDown, true);
    };
  }, [onClose]);

  return (
    <div
      ref={menu}
      className="menu"
      style={{ left: at.x, top: at.y }}
      role="menu"
      aria-label={folderName}
    >
      {ACTIONS.map((action) => (
        <button
          key={action.key}
          className={`menu__item${action.destructive ? " menu__item--danger" : ""}`}
          type="button"
          role="menuitem"
          onClick={() => onPick(action.key)}
        >
          {action.label}
        </button>
      ))}
    </div>
  );
}
