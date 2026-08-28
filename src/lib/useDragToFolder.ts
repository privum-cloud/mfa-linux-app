import { useCallback, useRef, useState } from "react";

/**
 * Dragging an account onto a folder.
 *
 * Everything here is the HTML5 drag API, which works because the window sets
 * `dragDropEnabled: false`: Tauri then installs no drop handler of its own, and
 * dragging one element of the page onto another never involves the shell.
 */

/** How long a closed folder is hovered before it opens itself. */
const SPRING_OPEN_MS = 1000;

/** `null` is the folder-less section; a string is a folder's id. */
export type Target = string | null;

interface Options {
  /** Called only when the account is actually changing folder. */
  onMove: (accountId: string, folder: Target) => void;
  /** Open a closed folder that has been hovered long enough. */
  onSpringOpen: (folderId: string) => void;
  isCollapsed: (folderId: string) => boolean;
  /** A drag has begun out of this folder. */
  onBegin: (from: Target) => void;
  /** The drag is over, whether it moved anything or not. */
  onFinish: () => void;
  /** Dragging is using the app, and should not count towards the idle lock. */
  onActivity: () => void;
}

export function useDragToFolder({
  onMove,
  onSpringOpen,
  isCollapsed,
  onBegin,
  onFinish,
  onActivity,
}: Options) {
  // Two records of the same drag, and the split is not decoration.
  //
  // The ref is the one the handlers read. A drag event answers in the tick it
  // fires in, and state read from a closure is one render behind — a `dragover`
  // arriving before React has re-rendered would find no drag in progress and
  // skip its `preventDefault`, which is how a drop target quietly stops
  // accepting anything.
  //
  // The state exists only so the rows can be styled, and is set a tick late on
  // purpose. See the note in `onDragStart`.
  const dragging = useRef<string | null>(null);
  const [draggingId, setDraggingId] = useState<string | null>(null);
  const [over, setOver] = useState<Target | undefined>(undefined);
  // Where the dragged account started, so dropping it back where it already
  // lives asks the vault for nothing and the list does not blink.
  const origin = useRef<Target>(null);
  const springTimer = useRef<number | null>(null);

  const cancelSpring = useCallback(() => {
    if (springTimer.current !== null) {
      window.clearTimeout(springTimer.current);
      springTimer.current = null;
    }
  }, []);

  const stop = useCallback(() => {
    dragging.current = null;
    cancelSpring();
    setDraggingId(null);
    setOver(undefined);
    onFinish();
  }, [cancelSpring, onFinish]);

  const handleProps = (accountId: string, folder: Target) => ({
    draggable: true,
    onDragStart: (event: React.DragEvent) => {
      onActivity();
      origin.current = folder;
      dragging.current = accountId;

      // Deferred by a tick, and this is load-bearing on Windows. Setting state
      // here re-renders while WebView2 is still assembling the drag, and it
      // responds by abandoning it: `dragstart` fires, `dragend` follows, and no
      // `dragenter` or `dragover` is ever delivered anywhere on the page — a
      // drag that visibly picks the row up and then refuses every target.
      // Measured on the real thing, not reasoned about.
      //
      // Collapsing the other folders rides on the same delay for the same
      // reason, and it is why the folder being dragged out of stays open: a
      // row that is folded away is a row removed from the page, and the
      // browser abandons a drag whose source has gone.
      window.setTimeout(() => {
        setDraggingId(accountId);
        onBegin(folder);
      }, 0);
      event.dataTransfer.effectAllowed = "move";
      // Firefox refuses to start a drag without payload, and a plain-text id
      // is also the least surprising thing to hand anyone else.
      event.dataTransfer.setData("text/plain", accountId);

      // The grip is a 16px dot pattern; dragging its outline alone would say
      // nothing about what is moving. The row is the thing being moved.
      const row = event.currentTarget.closest("li");
      if (row instanceof HTMLElement) {
        event.dataTransfer.setDragImage(row, 24, row.clientHeight / 2);
      }
    },
    onDragEnd: stop,
  });

  const targetProps = (folder: Target) => ({
    onDragOver: (event: React.DragEvent) => {
      if (dragging.current === null) return;
      // Without this the drop never fires: the default is to refuse.
      event.preventDefault();
      event.dataTransfer.dropEffect = "move";
      if (over === folder) return;

      setOver(folder);
      cancelSpring();
      // A closed folder hides the subfolders someone may be aiming for, so
      // resting on it opens it rather than making them drop, expand and drag
      // a second time.
      if (folder !== null && isCollapsed(folder)) {
        springTimer.current = window.setTimeout(() => {
          onSpringOpen(folder);
          springTimer.current = null;
        }, SPRING_OPEN_MS);
      }
    },
    onDragLeave: (event: React.DragEvent) => {
      // dragleave also fires crossing into a child, which would flicker the
      // highlight off and on across every row inside the section.
      const goingTo = event.relatedTarget;
      if (goingTo instanceof Node && event.currentTarget.contains(goingTo)) {
        return;
      }
      cancelSpring();
      setOver((current) => (current === folder ? undefined : current));
    },
    onDrop: (event: React.DragEvent) => {
      event.preventDefault();
      const account = dragging.current;
      const from = origin.current;
      stop();
      if (account !== null && from !== folder) {
        onActivity();
        onMove(account, folder);
      }
    },
  });

  return {
    draggingId,
    isOver: (folder: Target) => draggingId !== null && over === folder,
    /** True while anything is being dragged, so empty drop targets can appear. */
    active: draggingId !== null,
    handleProps,
    targetProps,
  };
}
