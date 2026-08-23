/**
 * Folder icons, drawn inline.
 *
 * Emoji were the first choice and the wrong one: they depend on a font the
 * machine may not have, and a folder icon that renders as an empty box on
 * someone else's desktop is worse than no icon at all. These ship inside the
 * application, cost no request, satisfy a `default-src 'self'` policy, and take
 * their colour from the text around them.
 *
 * The vault stores the identifier, not the drawing. Changing the artwork later
 * then costs nothing, where storing a glyph would have frozen it.
 */

export const FOLDER_ICONS = [
  "folder",
  "building",
  "bank",
  "briefcase",
  "lock",
  "cloud",
  "desktop",
  "globe",
  "cart",
  "plane",
  "game",
  "mail",
  "gear",
  "person",
  "key",
  "star",
] as const;

export type FolderIconId = (typeof FOLDER_ICONS)[number];

/** What each icon is called out loud, for screen readers and tooltips. */
export const FOLDER_ICON_LABELS: Record<string, string> = {
  folder: "Folder",
  building: "Company",
  bank: "Bank",
  briefcase: "Work",
  lock: "Secure",
  cloud: "Cloud",
  desktop: "Computer",
  globe: "Web",
  cart: "Shopping",
  plane: "Travel",
  game: "Games",
  mail: "Mail",
  gear: "Tools",
  person: "Personal",
  key: "Keys",
  star: "Favourites",
};

/** Path data on a 24×24 grid, stroked rather than filled so one set reads well
 *  at any size and inherits the surrounding colour. */
const PATHS: Record<string, string> = {
  folder: "M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2Z",
  building: "M4 21V4a1 1 0 0 1 1-1h9a1 1 0 0 1 1 1v17M15 9h4a1 1 0 0 1 1 1v11M2 21h20M7 7h2M7 11h2M7 15h2M18 13h.01M18 17h.01",
  bank: "M3 10h18M5 10v8M9 10v8M15 10v8M19 10v8M2 21h20M12 3 3 8h18Z",
  briefcase: "M3 9a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2ZM9 7V5a1 1 0 0 1 1-1h4a1 1 0 0 1 1 1v2M3 13h18",
  lock: "M5 11h14a1 1 0 0 1 1 1v8a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1v-8a1 1 0 0 1 1-1ZM8 11V7a4 4 0 0 1 8 0v4",
  cloud: "M7 18a4 4 0 0 1-.6-7.96A5.5 5.5 0 0 1 17.3 9.2 3.9 3.9 0 0 1 17 18Z",
  desktop: "M3 5a1 1 0 0 1 1-1h16a1 1 0 0 1 1 1v10a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1ZM8 20h8M12 16v4",
  globe: "M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18ZM3 12h18M12 3c2.5 2.4 3.8 5.5 3.8 9S14.5 18.6 12 21c-2.5-2.4-3.8-5.5-3.8-9S9.5 5.4 12 3Z",
  cart: "M3 4h2l2.4 10.4a1 1 0 0 0 1 .8h7.9a1 1 0 0 0 1-.75L20 7H6M9 20h.01M17 20h.01",
  plane: "M10.5 13.5 3 11V9l7.5.9L13 4h2l-1.2 6.4 6.2.7v2l-6.2.7L15 20h-2Z",
  game: "M7 8h10a4 4 0 0 1 4 4v2a3 3 0 0 1-5.2 2L14 15h-4l-1.8 1A3 3 0 0 1 3 14v-2a4 4 0 0 1 4-4ZM8 11v2M7 12h2M16 11h.01M18 13h.01",
  mail: "M3 7a1 1 0 0 1 1-1h16a1 1 0 0 1 1 1v10a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1ZM3 7l9 6 9-6",
  gear: "M12 9a3 3 0 1 0 0 6 3 3 0 0 0 0-6ZM12 2v2.5M12 19.5V22M22 12h-2.5M4.5 12H2M19.1 4.9l-1.8 1.8M6.7 17.3l-1.8 1.8M19.1 19.1l-1.8-1.8M6.7 6.7 4.9 4.9",
  person: "M12 4a4 4 0 1 0 0 8 4 4 0 0 0 0-8ZM4 21a8 8 0 0 1 16 0",
  key: "M15.5 3a5.5 5.5 0 1 0-4.4 8.8L4 19v3h3l1-1v-2h2v-2h2l1.1-1.1A5.5 5.5 0 0 0 15.5 3ZM16.5 7.5h.01",
  star: "m12 3.5 2.7 5.6 6.1.9-4.4 4.3 1 6.1-5.4-2.9-5.4 2.9 1-6.1L3.2 10l6.1-.9Z",
};

interface Props {
  /** An identifier from FOLDER_ICONS. Anything unknown falls back to a folder,
   *  so a vault written by a future version never renders a hole. */
  icon?: string | null;
  size?: number;
  className?: string;
}

export default function FolderIcon({ icon, size = 16, className }: Props) {
  const path = PATHS[icon ?? ""] ?? PATHS.folder;

  return (
    <svg
      className={className}
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.7}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      focusable="false"
    >
      <path d={path} />
    </svg>
  );
}
