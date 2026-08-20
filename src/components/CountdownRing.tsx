const SIZE = 26;
const STROKE = 2.5;
const RADIUS = (SIZE - STROKE) / 2;
const CIRCUMFERENCE = 2 * Math.PI * RADIUS;

/** Below this many seconds the arc turns amber. */
const WARN_AT = 5;

interface Props {
  secondsRemaining: number;
  period: number;
}

/**
 * How long the current code has left.
 *
 * Two rules that are easy to get wrong:
 *
 * `secondsRemaining === period` is the instant a fresh code begins, so it draws
 * a *full* ring. Read naively it looks like the maximum and gets rendered as
 * about-to-expire, and the ring flickers once every period.
 *
 * The warning is an amber arc *and* a shrinking one, so it never rests on
 * colour alone.
 */
export default function CountdownRing({ secondsRemaining, period }: Props) {
  const fraction = period > 0 ? Math.min(secondsRemaining / period, 1) : 0;
  const warning = secondsRemaining > 0 && secondsRemaining <= WARN_AT;

  return (
    <svg
      className="ring"
      width={SIZE}
      height={SIZE}
      viewBox={`0 0 ${SIZE} ${SIZE}`}
      aria-hidden="true"
    >
      <circle
        className="ring__track"
        cx={SIZE / 2}
        cy={SIZE / 2}
        r={RADIUS}
        fill="none"
        strokeWidth={STROKE}
      />
      <circle
        className={`ring__arc${warning ? " ring__arc--warn" : ""}`}
        cx={SIZE / 2}
        cy={SIZE / 2}
        r={RADIUS}
        fill="none"
        strokeWidth={STROKE}
        strokeDasharray={CIRCUMFERENCE}
        strokeDashoffset={CIRCUMFERENCE * (1 - fraction)}
      />
    </svg>
  );
}
