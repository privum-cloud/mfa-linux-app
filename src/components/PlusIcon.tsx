/**
 * Add an account.
 *
 * Drawn rather than typed. A `+` character is placed on the x-height, not in
 * the middle of its box, so centring the box leaves the mark sitting above the
 * centre — it reads as crooked next to two icons that are centred properly.
 * Two strokes in the same 24-unit grid as its neighbours put it where the eye
 * expects, and give it their weight instead of the font's.
 *
 * A shade heavier than the others on purpose: two lines with nothing else
 * around them look thinner than a drawing of the same stroke.
 */
export default function PlusIcon({ size = 17 }: { size?: number }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.9}
      strokeLinecap="round"
      aria-hidden="true"
      focusable="false"
    >
      <path d="M12 5v14M5 12h14" />
    </svg>
  );
}
