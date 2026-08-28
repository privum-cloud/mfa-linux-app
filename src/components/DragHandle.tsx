const SIZE = 16;
const RADIUS = 1.15;
const COLUMNS = [5.5, 10.5];
const ROWS = [4, 8, 12];

/**
 * The grip that starts a drag.
 *
 * It is a sibling of the row button rather than a child of it, which is the
 * whole point: the row keeps its own click, and copying a code — the thing
 * this screen exists for — is never asked to share a gesture with anything.
 */
export default function DragHandle() {
  return (
    <svg
      className="grip"
      width={SIZE}
      height={SIZE}
      viewBox={`0 0 ${SIZE} ${SIZE}`}
      aria-hidden="true"
    >
      {ROWS.flatMap((y) =>
        COLUMNS.map((x) => (
          <circle key={`${x}-${y}`} cx={x} cy={y} r={RADIUS} fill="currentColor" />
        )),
      )}
    </svg>
  );
}
