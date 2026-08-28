const SIZE = 15;
const STROKE = 2;
const RADIUS = (SIZE - STROKE) / 2;
const CIRCUMFERENCE = 2 * Math.PI * RADIUS;
/** How much of the ring is drawn. A quarter reads as turning; a half does not. */
const ARC = 0.25;

/**
 * Something is happening and has not finished.
 *
 * Reading a picture is usually instant, but a screenshot that has to be
 * enlarged before the code comes out takes a couple of seconds, and a window
 * that sits perfectly still for that long reads as one that ignored the click.
 *
 * It inherits `currentColor`, so it belongs to whatever it is placed inside
 * rather than carrying a colour of its own. Where motion is unwelcome the
 * stylesheet stops it turning and the label beside it carries the meaning.
 */
export default function Spinner() {
  return (
    <svg
      className="spinner"
      width={SIZE}
      height={SIZE}
      viewBox={`0 0 ${SIZE} ${SIZE}`}
      aria-hidden="true"
    >
      <circle
        className="spinner__track"
        cx={SIZE / 2}
        cy={SIZE / 2}
        r={RADIUS}
        fill="none"
        strokeWidth={STROKE}
      />
      <circle
        className="spinner__arc"
        cx={SIZE / 2}
        cy={SIZE / 2}
        r={RADIUS}
        fill="none"
        strokeWidth={STROKE}
        strokeDasharray={CIRCUMFERENCE}
        strokeDashoffset={CIRCUMFERENCE * (1 - ARC)}
      />
    </svg>
  );
}
