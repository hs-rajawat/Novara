import clsx from "clsx";
import { Icon, type IconName } from "@/components/Icon";

interface Props {
  code: string | null | undefined;
  label: string | null | undefined;
  className?: string;
  /**
   * Lead with the launcher's mark instead of the tone dot.
   *
   * Opt-in rather than always-on: in a dense library grid the dot is the right
   * weight, but where the badge is the first thing identifying a game's home —
   * the Game Details hero — a glyph says "this lives in Steam" at a glance.
   */
  withIcon?: boolean;
  /** Mark size. Larger where the badge is an identity badge rather than a tag. */
  iconSize?: number;
}

/** Source glyph per store. Codes match the `sources` table seed. */
const SOURCE_ICONS: Record<string, IconName> = {
  steam: "src-steam",
  epic: "src-epic",
  gog: "src-gog",
  xbox: "src-xbox",
  ubisoft: "src-ubisoft",
  battle: "src-battle",
  emulator: "gamepad",
  manual: "src-manual",
};

const KNOWN_CODES = new Set(Object.keys(SOURCE_ICONS));

/**
 * Small platform chip: the store's mark or a tinted dot, plus its name.
 *
 * The marks are NOVARA's own simplified glyphs (see Icon.tsx), not vendor
 * logo files — same stroke grid as every other icon in the app, so a row of
 * mixed sources reads evenly, and nothing to fetch or miss when offline.
 */
export function PlatformBadge({
  code,
  label,
  className,
  withIcon = false,
  iconSize = 13,
}: Props) {
  if (!code || !label) return null;
  const tone = KNOWN_CODES.has(code) ? code : "default";
  const icon = withIcon ? SOURCE_ICONS[code] ?? "src-manual" : null;
  return (
    <span
      className={clsx("platform-badge", `tone-${tone}`, icon && "has-icon", className)}
    >
      {icon && <Icon name={icon} size={iconSize} />}
      {label}
    </span>
  );
}
