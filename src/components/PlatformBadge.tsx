import clsx from "clsx";

interface Props {
  code: string | null | undefined;
  label: string | null | undefined;
  className?: string;
}

const KNOWN_CODES = new Set([
  "steam",
  "epic",
  "gog",
  "xbox",
  "ubisoft",
  "battle",
  "emulator",
  "manual",
]);

/** Small platform chip. Text + a tinted dot only — no invented brand logos. */
export function PlatformBadge({ code, label, className }: Props) {
  if (!code || !label) return null;
  const tone = KNOWN_CODES.has(code) ? code : "default";
  return <span className={clsx("platform-badge", `tone-${tone}`, className)}>{label}</span>;
}
