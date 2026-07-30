/**
 * Read-only presentation helpers for `games.metadata_json`.
 *
 * The backend already stores the provider's payload verbatim in that column
 * (`GameMetadata::raw_json`, src-tauri/src/metadata/mod.rs) and flattens it onto
 * every `get_game` response. Nothing here fetches, writes or reshapes data — it
 * only reads a field the UI is already given, so the Game Details "About" panel
 * can show facts that have no dedicated column (genres, languages, controller
 * support…) without a new command or schema change.
 *
 * Every field is optional by design. Payload shape differs per provider (Steam's
 * `appdetails` is keyed by app id, others are flat) and older rows may predate
 * the column entirely, so each accessor is defensive and the caller renders only
 * what came back. A missing row is omitted rather than shown as "Unknown".
 */

export interface GameFacts {
  genres: string[];
  /** Store "categories" minus controller/Steam Deck entries — Single-player, Co-op… */
  features: string[];
  languages: string[];
  controllerSupport: string | null;
  steamDeck: string | null;
  engine: string | null;
  /** Provider display string ("21 Oct, 2015") — never reformatted, it isn't structured. */
  releaseDate: string | null;
  platforms: string[];
}

/** Frozen so the shared "nothing known" result can never be mutated by a caller. */
export const NO_FACTS: GameFacts = Object.freeze({
  genres: [],
  features: [],
  languages: [],
  controllerSupport: null,
  steamDeck: null,
  engine: null,
  releaseDate: null,
  platforms: [],
});

function asRecord(value: unknown): Record<string, unknown> | null {
  return typeof value === "object" && value !== null && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
}

function asText(value: unknown): string | null {
  if (typeof value === "string") {
    const trimmed = value.trim();
    return trimmed.length > 0 ? trimmed : null;
  }
  if (typeof value === "number" && Number.isFinite(value)) return String(value);
  return null;
}

/**
 * Locate the object holding the facts.
 *
 * Steam's `appdetails` body is `{ "<appid>": { success, data: {…} } }`, so the
 * interesting object is two levels down and under a key we don't know here.
 * Other providers store their own object flat.
 */
function payloadOf(root: unknown): Record<string, unknown> | null {
  const obj = asRecord(root);
  if (!obj) return null;

  const direct = asRecord(obj.data);
  if (direct) return direct;

  for (const value of Object.values(obj)) {
    const nested = asRecord(value);
    const data = nested && asRecord(nested.data);
    if (data) return data;
  }
  return obj;
}

/** Accepts both `[{ description }]` (Steam) and `["Action"]` (plain lists). */
function descriptions(value: unknown): string[] {
  if (!Array.isArray(value)) return [];
  const out: string[] = [];
  for (const item of value) {
    const text =
      asText(item) ?? asText(asRecord(item)?.description) ?? asText(asRecord(item)?.name);
    if (text && !out.includes(text)) out.push(text);
  }
  return out;
}

/**
 * Steam's `supported_languages` is a display string, not a list: markup, a
 * trailing footnote after a `<br>`, and asterisks marking full audio support.
 */
function languagesOf(value: unknown): string[] {
  const raw = asText(value);
  if (!raw) return [];
  const [listPart] = raw.split(/<br\s*\/?>/i);
  return listPart
    .replace(/<[^>]*>/g, "")
    .split(",")
    .map((entry) => entry.replace(/\*/g, "").trim())
    .filter((entry) => entry.length > 0)
    .filter((entry, index, all) => all.indexOf(entry) === index);
}

const PLATFORM_LABELS: [key: string, label: string][] = [
  ["windows", "Windows"],
  ["mac", "macOS"],
  ["linux", "Linux"],
];

export function parseGameFacts(rawJson: string | null | undefined): GameFacts {
  if (!rawJson) return NO_FACTS;

  let parsed: unknown;
  try {
    parsed = JSON.parse(rawJson);
  } catch {
    // A payload we can't read is the same as no payload — never a broken panel.
    return NO_FACTS;
  }

  const data = payloadOf(parsed);
  if (!data) return NO_FACTS;

  const categories = descriptions(data.categories);
  const controller = categories.find((c) => /controller/i.test(c)) ?? null;
  const steamDeck = categories.find((c) => /steam deck/i.test(c)) ?? null;

  const platforms = PLATFORM_LABELS.filter(
    ([key]) => asRecord(data.platforms)?.[key] === true
  ).map(([, label]) => label);

  return {
    genres: descriptions(data.genres),
    features: categories.filter((c) => c !== controller && c !== steamDeck),
    languages: languagesOf(data.supported_languages),
    controllerSupport: controller,
    steamDeck,
    engine: asText(data.engine) ?? asText(data.game_engine),
    releaseDate: asText(asRecord(data.release_date)?.date),
    platforms,
  };
}
