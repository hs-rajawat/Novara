import { useEffect, useRef, useState, type KeyboardEvent } from "react";
import { Link, useNavigate, useParams } from "react-router-dom";
import { open } from "@tauri-apps/plugin-dialog";
import clsx from "clsx";
import { api, onEvent } from "@/lib/ipc";
import { notify, reportError } from "@/lib/toast";
import { useConfirm } from "@/components/ConfirmDialog";
import type {
  Achievement,
  GameWithInstalls,
  Installation,
  PlaySession,
  SaveProfile,
} from "@/types";
import {
  formatBytes,
  formatPlaytime,
  formatRelative,
  formatSessionDay,
  formatSessionTime,
} from "@/lib/format";
import { parseGameFacts } from "@/lib/metadata";
import { Icon, type IconName } from "@/components/Icon";
import { GameArtwork } from "@/components/GameArtwork";
import { PlatformBadge } from "@/components/PlatformBadge";
import { toImgSrc } from "@/lib/image";

/**
 * Game Details — NOVARA's flagship page.
 *
 * Composition follows GAME_DETAILS_REDESIGN.md: one hero owns the first
 * viewport and carries the whole identity of the game (cover, logo, badges,
 * description, metadata, actions). Below it the page goes quiet.
 *
 * Everything here is presentation. The commands, events, routes and state
 * shape are exactly as they were — see the handlers below, which are unchanged.
 */

const RECENT_SESSIONS_LIMIT = 8;

/** Tiles in the Overview achievement strip before it collapses into "+N". */
const ACHIEVEMENT_STRIP = 8;

/** Genres support the title in the hero; About lists them all. */
const GENRE_BADGE_LIMIT = 2;

/** Languages are the longest value set; three plus a count is enough to scan. */
const LANGUAGE_LIMIT = 3;

/** Genres, capped so the row stays scannable. */
const GENRE_LIMIT = 3;

type TabKey = "overview" | "achievements" | "artwork" | "saves" | "mods" | "activity";

const TABS: { key: TabKey; label: string }[] = [
  { key: "overview", label: "Overview" },
  { key: "achievements", label: "Achievements" },
  { key: "artwork", label: "Artwork" },
  { key: "saves", label: "Saves" },
  { key: "mods", label: "Mods" },
  { key: "activity", label: "Activity" },
];

/** Library-integrity states, kept as data so all three stay symmetrical. */
const INSTALL_STATUS: Record<
  string,
  { label: string; tone: string; icon: "alert-triangle" | "alert-circle" | "info" }
> = {
  missing: { label: "Missing", tone: "is-danger", icon: "alert-triangle" },
  deleted: { label: "Not installed", tone: "is-danger", icon: "alert-circle" },
  offline: { label: "Drive offline", tone: "is-neutral", icon: "info" },
};

/** A value shown as a mark plus a short label. */
interface Chip {
  icon?: IconName;
  label: string;
}

const OS_ICONS: Record<string, IconName> = {
  Windows: "os-windows",
  macOS: "os-mac",
  Linux: "os-linux",
};

/**
 * The store categories worth showing, in display order.
 *
 * Steam ships twenty-odd categories per game, most of them plumbing ("Remote
 * Play on Phone", "Family Sharing", "Stats"). A player scanning this card wants
 * to know how many people can play and whether progress is tracked, so the row
 * is curated down to that rather than mirroring the provider.
 */
const FEATURES: { test: RegExp; icon: IconName; label: string }[] = [
  { test: /single-?player/i, icon: "user", label: "Single Player" },
  { test: /co-?op/i, icon: "users", label: "Co-op" },
  { test: /multi-?player|pvp/i, icon: "users", label: "Multiplayer" },
  { test: /achievement/i, icon: "trophy", label: "Achievements" },
  { test: /cloud/i, icon: "cloud", label: "Cloud Saves" },
];

/** Platform chips: an OS mark where one exists, plain text otherwise. */
function platformChips(platforms: string[], steamDeck: string | null): Chip[] {
  const chips: Chip[] = platforms.map((p) => ({ icon: OS_ICONS[p], label: p }));
  if (steamDeck) chips.push({ icon: "gamepad", label: "Steam Deck" });
  return chips;
}

/** Curated feature chips, deduplicated by label and capped. */
function featureChips(categories: string[]): Chip[] {
  const chips: Chip[] = [];
  for (const { test, icon, label } of FEATURES) {
    if (chips.length >= 4) break;
    if (categories.some((c) => test.test(c))) chips.push({ icon, label });
  }
  return chips;
}

/** "Full controller support" → a mark plus title-cased label. */
function controllerChip(support: string | null): Chip | null {
  if (!support) return null;
  if (/full/i.test(support)) {
    return { icon: "gamepad", label: "Full Controller Support" };
  }
  if (/partial/i.test(support)) {
    return { icon: "gamepad", label: "Partial Controller Support" };
  }
  return { icon: "gamepad", label: support };
}

export function GameDetails() {
  const { id = "" } = useParams();
  const navigate = useNavigate();
  const [game, setGame] = useState<GameWithInstalls | null>(null);
  const [notes, setNotes] = useState("");
  const [savingNotes, setSavingNotes] = useState(false);
  const [launching, setLaunching] = useState(false);
  const [refreshing, setRefreshing] = useState(false);
  const [sessions, setSessions] = useState<PlaySession[]>([]);
  const [achievements, setAchievements] = useState<Achievement[]>([]);
  const [saveProfiles, setSaveProfiles] = useState<SaveProfile[] | null>(null);
  const [tab, setTab] = useState<TabKey>("overview");
  // A logo that fails to decode must fall back to the text title rather than
  // leaving the page with no name on it.
  const [logoBroken, setLogoBroken] = useState(false);
  const tabRefs = useRef<(HTMLButtonElement | null)[]>([]);
  const { confirm, dialog } = useConfirm();

  useEffect(() => {
    api.getGame(id).then((g) => {
      setGame(g);
      setNotes(g?.user_notes ?? "");
    });
  }, [id]);

  useEffect(() => {
    api.listSessions(id, RECENT_SESSIONS_LIMIT).then(setSessions);
  }, [id]);

  // Read-only, for the Overview progress summary. Unlocking still happens on the
  // dedicated Achievements page, which this page links to.
  useEffect(() => {
    api
      .listAchievements(id)
      .then(setAchievements)
      .catch((e) => reportError(e, "load achievements for this game"));
  }, [id]);

  // Deferred until the Saves tab is actually opened — the panel is its only
  // consumer, so an unopened tab costs nothing.
  useEffect(() => {
    if (tab !== "saves" || saveProfiles !== null) return;
    api
      .listSaveProfiles(id)
      .then(setSaveProfiles)
      .catch((e) => reportError(e, "load save profiles for this game"));
  }, [tab, saveProfiles, id]);

  // A session that ends, or the Library Integrity System changing this
  // game's status (periodic sweep, a launch-time recheck, Locate
  // Executable), should show up without a manual refresh.
  useEffect(() => {
    let unlisten: (() => void) | null = null;
    let cancelled = false;
    onEvent((ev) => {
      if (ev.type === "session_ended" && ev.game_id === id) {
        api
          .listSessions(id, RECENT_SESSIONS_LIMIT)
          .then(setSessions)
          .catch((e) => reportError(e, "refresh recent sessions"));
      }
      if (ev.type === "game_updated" && ev.game_id === id) {
        api
          .getGame(id)
          .then(setGame)
          .catch((e) => reportError(e, "refresh this game"));
      }
    }).then((fn) => {
      // GameDetails unmounts on every navigation, so without this guard a
      // listener registered after unmount is never detached and they
      // accumulate for the whole session.
      if (cancelled) fn();
      else unlisten = fn;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [id]);

  if (!game) {
    // Occupies the hero's footprint so the page does not jump when it arrives.
    return <div className="gd-skeleton" />;
  }

  async function fav() {
    try {
      await api.setFavorite(id, !game!.is_favorite);
      setGame(await api.getGame(id));
    } catch (err) {
      reportError(err, "update your favourites");
    }
  }

  async function saveNotes() {
    setSavingNotes(true);
    try {
      await api.updateNotes(id, notes || null);
    } catch (err) {
      reportError(err, "save your notes");
    } finally {
      setSavingNotes(false);
    }
  }

  async function launch() {
    setLaunching(true);
    try {
      await api.launchGame(id);
    } catch (err) {
      // The backend emits a Notice for anticipated conditions; anything else
      // used to disappear here, so Play appeared to do nothing at all.
      reportError(err, "launch this game");
    } finally {
      setLaunching(false);
    }
  }

  async function toggleHidden() {
    if (!game) return;
    const hidden = !game.is_hidden;
    if (
      hidden &&
      !(await confirm({
        title: `Remove "${game.title}" from your library?`,
        description:
          "This keeps its playtime, sessions, and achievements — you can bring it back any time.",
        confirmLabel: "Remove",
        tone: "danger",
        icon: "trash",
      }))
    ) {
      return;
    }
    try {
      await api.setHidden(id, hidden);
      if (hidden) {
        navigate("/library");
      } else {
        setGame(await api.getGame(id));
      }
    } catch (err) {
      reportError(
        err,
        hidden ? "remove this game from your library" : "restore this game"
      );
    }
  }

  async function browseExe(installationId: string) {
    try {
      const picked = await open({
        multiple: false,
        filters: [
          { name: "Executables", extensions: ["exe", "bat", "sh", "AppImage"] },
        ],
      });
      if (!picked || Array.isArray(picked)) return;
      await api.setInstallationExecutable(installationId, picked as string);
      setGame(await api.getGame(id));
    } catch (err) {
      reportError(err, "set the executable for this installation");
    }
  }

  async function refreshMetadata() {
    setRefreshing(true);
    try {
      const result = await api.refreshMetadata(id);
      setGame(await api.getGame(id));

      // The result used to be discarded, so the button spun and then nothing
      // observable happened — identical whether the refresh found new data,
      // found nothing, or never ran because the feature is disabled.
      const changed =
        (result.text_updated ? 1 : 0) + result.artwork_updated;
      if (changed > 0) {
        const parts: string[] = [];
        if (result.text_updated) parts.push("details");
        if (result.artwork_updated > 0) {
          parts.push(
            `${result.artwork_updated} artwork image${
              result.artwork_updated === 1 ? "" : "s"
            }`
          );
        }
        notify(`Updated ${parts.join(" and ")}`, "success");
      } else if (!result.network_allowed) {
        notify(
          "Automatic metadata is turned off — enable it in Settings to fetch online artwork and details.",
          "warning"
        );
      } else {
        notify("No new metadata was found for this game", "info");
      }
    } catch (err) {
      reportError(err, "refresh metadata for this game");
    } finally {
      setRefreshing(false);
    }
  }

  /** Roving focus across the tab strip (DESIGN.md §24.4). */
  function onTabKeyDown(e: KeyboardEvent<HTMLButtonElement>, index: number) {
    let next = index;
    if (e.key === "ArrowRight") next = (index + 1) % TABS.length;
    else if (e.key === "ArrowLeft") next = (index - 1 + TABS.length) % TABS.length;
    else if (e.key === "Home") next = 0;
    else if (e.key === "End") next = TABS.length - 1;
    else return;
    e.preventDefault();
    setTab(TABS[next].key);
    tabRefs.current[next]?.focus();
  }

  const facts = parseGameFacts(game.metadata_json);
  const unlocked = achievements.filter((a) => a.is_unlocked).length;
  const achievementPct = achievements.length
    ? Math.round((unlocked / achievements.length) * 100)
    : 0;
  const stripRest = Math.max(0, achievements.length - ACHIEVEMENT_STRIP);
  const status = INSTALL_STATUS[game.primary_install_status ?? ""];
  const launchable = canLaunch(game.installations);
  const showLogo = !!game.logo_path && !logoBroken;
  const completion = Math.round(game.completion_pct);
  const platform =
    facts.platforms.length > 0
      ? facts.platforms.join(", ")
      : game.primary_source_label;
  // About, derived once: chips beat prose for scanning, and each row is capped so
  // the card stays readable in a couple of seconds.
  const releaseDate = facts.releaseDate ?? game.release_year;
  const platforms = platformChips(facts.platforms, facts.steamDeck);
  const features = featureChips(facts.features);
  const controller = controllerChip(facts.controllerSupport);
  const genres = facts.genres.slice(0, GENRE_LIMIT);
  // Whether About has anything to say at all — an empty card is worse than a
  // sentence explaining why it is empty.
  const hasAbout =
    !!releaseDate ||
    genres.length > 0 ||
    platforms.length > 0 ||
    features.length > 0 ||
    facts.languages.length > 0 ||
    !!controller ||
    !!platform;

  return (
    <>
      {dialog}

      {/* ══ HERO — the artwork is the page ══════════════════════════════ */}
      <section className="gd-hero">
        {/* No `artwork-fill` here: the hero art is sized to the hero's width and
            takes its height from the source's own ratio, so it must not be
            stretched to the box (see `.gd-hero-art`). */}
        <GameArtwork
          src={game.hero_path}
          title={game.title}
          kind="hero"
          className="gd-hero-art"
          alt={`${game.title} artwork`}
          eager
        />
        <div className="gd-hero-scrim" aria-hidden />

        <Link to="/library" className="gd-hero-back">
          <Icon name="arrow-left" size={14} />
          Library
        </Link>

        <div className="gd-hero-inner">
          {/* Poster and text share a row; the action row below is their sibling,
              so it spans the whole composition and starts under the poster
              rather than indented past it. */}
          <div className="gd-hero-top">
            <div className="gd-hero-cover">
              <GameArtwork
                src={game.cover_path}
                title={game.title}
                kind="cover"
                alt={`${game.title} cover`}
                eager
              />
            </div>

            <div className="gd-hero-id">
              {/* The logo is preferred when the game has one; the heading stays
                  in the accessibility tree either way. */}
              <h1 className={clsx("gd-hero-title", showLogo && "gd-visually-hidden")}>
                {game.title}
              </h1>
              {showLogo && (
                <img
                  className="gd-hero-logo"
                  src={toImgSrc(game.logo_path)}
                  alt=""
                  onError={() => setLogoBroken(true)}
                />
              )}

              {/* Source → genre → completion. Colour carries the hierarchy: the
                  launcher reads neutral, genres take the brand accent, progress
                  takes amber. Release year and completion state are not badges —
                  the year belongs to About and the state is not on this page. */}
              <div className="gd-hero-badges">
                <PlatformBadge
                  code={game.primary_source_code}
                  label={game.primary_source_label}
                  className="gd-pill"
                  withIcon
                  iconSize={15}
                />
                {facts.genres.slice(0, GENRE_BADGE_LIMIT).map((genre) => (
                  <span key={genre} className="gd-pill is-genre">
                    {genre}
                  </span>
                ))}
                <span className="gd-pill is-progress">{completion}%</span>
                {/* The exception to "no duplicate facts": an unavailable install
                    is a warning, not a restatement, and it appears only when
                    something is actually wrong. All three states, because the
                    Installations panel distinguishes them too (§23.4). */}
                {status && (
                  <span className={clsx("gd-pill", status.tone)}>
                    <Icon name={status.icon} size={11} />
                    {status.label}
                  </span>
                )}
              </div>

              {game.description && <p className="gd-hero-desc">{game.description}</p>}

              <div className="gd-hero-meta">
                <Meta label="Developer" value={game.developer} />
                <Meta label="Publisher" value={game.publisher} />
                <Meta label="Released" value={facts.releaseDate ?? game.release_year} />
                <Meta
                  label="Last played"
                  value={game.last_played_at ? formatRelative(game.last_played_at) : "Never"}
                />
                <Meta
                  label="Playtime"
                  value={formatPlaytime(game.total_playtime_seconds)}
                />
              </div>
            </div>
          </div>

          {/* Three groups, not seven buttons: Play, the four game actions, then
              the two utilities. The grouping is structural so the spacing comes
              from one rule rather than from hand-placed spacers. */}
          <div className="gd-hero-actions">
            <div className="gd-action-group">
              <button
                className="gd-btn gd-btn-play"
                onClick={launch}
                disabled={launching || !launchable}
                title={
                  launchable
                    ? "Launch game"
                    : "No launchable installation found — locate its executable under Overview"
                }
              >
                <Icon name="play" size={16} />
                {launching ? "Launching…" : "Play"}
              </button>
            </div>

            <div className="gd-action-group">
              <button
                className={clsx("gd-btn", game.is_favorite && "is-fav")}
                onClick={fav}
                title={game.is_favorite ? "Remove from favorites" : "Add to favorites"}
                aria-pressed={!!game.is_favorite}
              >
                <Icon name="star" size={16} />
                {game.is_favorite ? "Favorited" : "Favorite"}
              </button>
              <Link to={`/library/${id}/achievements`} className="gd-btn">
                <Icon name="trophy" size={16} />
                Achievements
              </Link>
              <Link to={`/library/${id}/saves`} className="gd-btn">
                <Icon name="save" size={16} />
                Saves
              </Link>
              <Link to={`/library/${id}/mods`} className="gd-btn">
                <Icon name="package" size={16} />
                Mods
              </Link>
            </div>

            <div className="gd-action-group">
              <button
                className="gd-btn gd-btn-icon"
                onClick={refreshMetadata}
                disabled={refreshing}
                title="Refresh description, genres, and artwork from available sources"
                aria-label="Refresh metadata"
              >
                <Icon
                  name="refresh"
                  size={16}
                  className={refreshing ? "spin" : undefined}
                />
              </button>
              {/* Library membership, not an uninstaller: the game's files are
                  never touched and its history survives. */}
              <button
                className="gd-btn gd-btn-icon gd-btn-danger"
                onClick={toggleHidden}
                title={
                  game.is_hidden
                    ? "Restore this game to your library"
                    : "Remove from library — files, playtime and achievements are kept"
                }
                aria-label={game.is_hidden ? "Restore to library" : "Remove from library"}
              >
                <Icon name={game.is_hidden ? "rotate-ccw" : "trash"} size={16} />
              </button>
            </div>
          </div>
        </div>
      </section>

      {/* ══ TABS ═══════════════════════════════════════════════════════ */}
      <div className="gd-tabs" role="tablist" aria-label="Game sections">
        {TABS.map((t, i) => (
          <button
            key={t.key}
            ref={(el) => {
              tabRefs.current[i] = el;
            }}
            role="tab"
            id={`gd-tab-${t.key}`}
            aria-selected={tab === t.key}
            aria-controls={`gd-panel-${t.key}`}
            tabIndex={tab === t.key ? 0 : -1}
            className={clsx("gd-tab", tab === t.key && "is-active")}
            onClick={() => setTab(t.key)}
            onKeyDown={(e) => onTabKeyDown(e, i)}
          >
            {t.label}
          </button>
        ))}
      </div>

      <div
        className="gd-panel"
        key={tab}
        role="tabpanel"
        id={`gd-panel-${tab}`}
        aria-labelledby={`gd-tab-${tab}`}
        tabIndex={-1}
      >
        {tab === "overview" && (
          <div className="gd-grid">
            <div className="gd-col">
              {/* Achievement data only. The library completion state (Playing,
                  Backlog, …) lives on the tab strip: those are states the user
                  assigns to manage their library, not progress the game reports.
                  
                  The layout renders identically whether or not achievements exist
                  yet — no empty-state message, because the Overview should look
                  finished before the achievement backend is. */}
              <section className="gd-card">
                <header className="gd-card-head">
                  <h2>Achievements</h2>
                  <button
                    className="gd-btn-outline"
                    onClick={() => setTab("achievements")}
                    disabled={achievements.length === 0}
                  >
                    View All
                    <Icon name="chevron-right" size={13} />
                  </button>
                </header>

                <div className="gd-metric">
                  <span className="gd-metric-value">{achievementPct}%</span>
                  <span className="gd-metric-caption">Complete</span>
                </div>
                <div className="gd-bar">
                  <div
                    className="gd-bar-fill"
                    style={{ width: `${achievementPct}%` }}
                  />
                </div>

                {/* Always a real count. "0 / 0" is true when nothing is tracked —
                    unlike an invented total, which would be indistinguishable from
                    real data to someone reading their own library. */}
                <div className="gd-ach-count">
                  {unlocked} / {achievements.length} unlocked
                </div>

                {/* Locked tiles are the normal state of this strip, so they are
                    styled as collectibles waiting to be earned rather than as
                    absences. Before any data exists the strip shows a full shelf
                    of them, which is what an unplayed game looks like anyway.
                    
                    aria-hidden: restates the count line above, and eight
                    unlabelled icons would be noise in a screen reader. */}
                <div className="gd-ach-strip" aria-hidden>
                  {achievements.length === 0
                    ? Array.from({ length: ACHIEVEMENT_STRIP }, (_, i) => (
                        <span key={i} className="gd-ach-tile">
                          <Icon name="lock" size={18} />
                        </span>
                      ))
                    : achievements.slice(0, ACHIEVEMENT_STRIP).map((a) => (
                        <span
                          key={a.id}
                          className={clsx("gd-ach-tile", a.is_unlocked && "is-unlocked")}
                          title={a.name}
                        >
                          <Icon name={a.is_unlocked ? "trophy" : "lock"} size={18} />
                        </span>
                      ))}
                  {stripRest > 0 && (
                    <span className="gd-ach-tile is-more">+{stripRest}</span>
                  )}
                </div>
              </section>

              <section className="gd-card">
                <header className="gd-card-head">
                  <h2>Notes</h2>
                  <button
                    className="gd-btn-outline"
                    onClick={saveNotes}
                    disabled={savingNotes}
                  >
                    <Icon name="check" size={13} />
                    {savingNotes ? "Saving…" : "Save"}
                  </button>
                </header>
                <textarea
                  rows={5}
                  style={{ width: "100%" }}
                  value={notes}
                  onChange={(e) => setNotes(e.target.value)}
                  placeholder="Where am I in the story? Build I'm using? Side-quests left…"
                />
              </section>
            </div>

            <div className="gd-col">
              <section className="gd-card">
                <header className="gd-card-head">
                  <h2>About</h2>
                </header>
                {/* Six short rows a player can read in a couple of seconds, not
                    everything the provider sent. Developer and publisher are in
                    the hero; the release date was deliberately taken out of the
                    badge row and belongs here. Engine and the provider's own name
                    are not information a player came for. `metadata_json` is still
                    read whole — only the display is edited. */}
                {hasAbout ? (
                  <dl className="gd-facts">
                    <Fact label="Release date" value={releaseDate} />
                    <Fact
                      label="Platform"
                      chips={platforms.length > 0 ? platforms : undefined}
                      value={platforms.length === 0 ? platform : undefined}
                    />
                    <Fact label="Genre" tags={genres} />
                    <Fact label="Features" chips={features} />
                    <Fact
                      label="Languages"
                      tags={facts.languages}
                      limit={LANGUAGE_LIMIT}
                    />
                    <Fact label="Controller" chips={controller ? [controller] : []} />
                  </dl>
                ) : (
                  <p className="gd-empty">
                    No details yet. Refresh metadata from the hero to fetch genres,
                    features and language support.
                  </p>
                )}
              </section>

              <section className="gd-card">
                <header className="gd-card-head">
                  <h2>Installations</h2>
                  <span className="gd-card-note">
                    {game.installations.length}{" "}
                    {game.installations.length === 1 ? "location" : "locations"}
                  </span>
                </header>
                {game.installations.length === 0 ? (
                  <p className="gd-empty">
                    No install location on record for this game.
                  </p>
                ) : (
                  <div className="gd-rows">
                    {game.installations.map((i) => (
                      <div key={i.id} className="gd-row gd-row-stack">
                        <div className="gd-row-line">
                          <span className="gd-row-path">{i.install_dir}</span>
                          <InstallFlag installation={i} />
                        </div>
                        <div className="gd-row-line">
                          <span className="gd-row-path is-muted">
                            {i.executable ?? "no executable detected"}
                            {i.executable && i.executable_override ? (
                              <span className="gd-flag is-accent" style={{ marginLeft: 8 }}>
                                Manual
                              </span>
                            ) : null}
                          </span>
                          <button
                            className="gd-link"
                            onClick={() => browseExe(i.id)}
                            title={
                              i.status === "missing"
                                ? "Point NOVARA at this game's executable to restore it"
                                : "Choose a different executable for this installation"
                            }
                          >
                            <Icon name="folder" size={13} />
                            {i.status === "missing" ? "Locate" : "Change"}
                          </button>
                        </div>
                      </div>
                    ))}
                  </div>
                )}
              </section>
            </div>
          </div>
        )}

        {tab === "achievements" && (
          <section className="gd-card">
            <header className="gd-card-head">
              <h2>Achievements</h2>
              <Link to={`/library/${id}/achievements`} className="gd-btn-outline">
                Manage
                <Icon name="chevron-right" size={13} />
              </Link>
            </header>
            {achievements.length === 0 ? (
              <p className="gd-empty">
                Nothing tracked yet. Create your own on the Achievements page — or
                import a community template later.
              </p>
            ) : (
              <>
                <div className="gd-metric">
                  <span className="gd-metric-value">{achievementPct}%</span>
                  <span className="gd-metric-caption">Complete</span>
                </div>
                <div className="gd-bar">
                  <div className="gd-bar-fill" style={{ width: `${achievementPct}%` }} />
                </div>
                <div className="gd-ach-count">
                  {unlocked} / {achievements.length} unlocked
                </div>
                <div className="gd-ach-list">
                  {achievements.map((a) => (
                    <AchievementTile key={a.id} achievement={a} />
                  ))}
                </div>
              </>
            )}
          </section>
        )}

        {tab === "artwork" && (
          <section className="gd-card">
            <header className="gd-card-head">
              <h2>Artwork</h2>
              <span className="gd-card-note">Stored locally</span>
            </header>
            <div className="gd-art-row">
              <figure className="gd-art">
                <div className="gd-art-frame gd-art-cover">
                  <GameArtwork
                    src={game.cover_path}
                    title={game.title}
                    kind="cover"
                    className="artwork-fill"
                    alt="Cover"
                  />
                </div>
                <figcaption>Cover</figcaption>
              </figure>
              <figure className="gd-art">
                <div className="gd-art-frame gd-art-hero">
                  <GameArtwork
                    src={game.hero_path}
                    title={game.title}
                    kind="hero"
                    className="artwork-fill"
                    alt="Hero"
                  />
                </div>
                <figcaption>Hero</figcaption>
              </figure>
              <figure className="gd-art">
                <div className="gd-art-frame gd-art-logo">
                  <GameArtwork
                    src={game.logo_path}
                    title={game.title}
                    kind="logo"
                    className="artwork-fill"
                    alt="Logo"
                  />
                </div>
                <figcaption>Logo</figcaption>
              </figure>
            </div>
          </section>
        )}

        {tab === "saves" && (
          <section className="gd-card">
            <header className="gd-card-head">
              <h2>Save profiles</h2>
              <Link to={`/library/${id}/saves`} className="gd-btn-outline">
                Manage
                <Icon name="chevron-right" size={13} />
              </Link>
            </header>
            {saveProfiles === null ? (
              <div className="shimmer" style={{ height: 62, borderRadius: 14 }} />
            ) : saveProfiles.length === 0 ? (
              <p className="gd-empty">
                No save folders tracked yet. Open the Save Manager to detect this
                game's save location and start taking backups.
              </p>
            ) : (
              <div className="gd-rows">
                {saveProfiles.map((p) => (
                  <div key={p.id} className="gd-row">
                    <span className="gd-row-mark">
                      <Icon name="save" size={15} />
                    </span>
                    <div className="gd-row-main">
                      <div className="gd-row-title">{p.label}</div>
                      <div className="gd-row-path is-muted">{p.source_dir}</div>
                    </div>
                    {p.auto_backup ? (
                      <span className="gd-flag is-info">Auto</span>
                    ) : null}
                    {p.is_manual_override ? (
                      <span className="gd-flag is-accent">Manual</span>
                    ) : null}
                  </div>
                ))}
              </div>
            )}
          </section>
        )}

        {tab === "mods" && (
          <section className="gd-card">
            <header className="gd-card-head">
              <h2>Mods</h2>
              <Link to={`/library/${id}/mods`} className="gd-btn-outline">
                Open
                <Icon name="chevron-right" size={13} />
              </Link>
            </header>
            <p className="gd-empty">
              NOVARA will list the mods installed for this game here, so you can
              enable or disable them and set their load order. This feature is still
              in development.
            </p>
          </section>
        )}

        {tab === "activity" && (
          <section className="gd-card">
            <header className="gd-card-head">
              <h2>Recent sessions</h2>
              <Link to="/timeline" className="gd-btn-outline">
                Full timeline
                <Icon name="chevron-right" size={13} />
              </Link>
            </header>
            {sessions.length === 0 ? (
              <p className="gd-empty">
                No sessions recorded yet — play this game and NOVARA will start
                tracking your time here.
              </p>
            ) : (
              <div className="gd-rows">
                {sessions.map((s) => (
                  <div key={s.id} className="gd-row">
                    <span className="gd-row-mark">
                      <Icon name="zap" size={15} />
                    </span>
                    <div className="gd-row-main">
                      <div className="gd-row-title">{formatSessionDay(s.started_at)}</div>
                      <div className="gd-row-sub">
                        {formatSessionTime(s.started_at)}
                        {s.idle_seconds > 0
                          ? ` · idle ${formatPlaytime(s.idle_seconds)}`
                          : ""}
                      </div>
                    </div>
                    <span className="gd-flag">
                      {formatPlaytime(s.duration_seconds)}
                    </span>
                  </div>
                ))}
              </div>
            )}
          </section>
        )}
      </div>
    </>
  );
}

/** Hero metadata row. Omitted entirely when there is nothing to say. */
function Meta({ label, value }: { label: string; value?: string | number | null }) {
  if (value === null || value === undefined || value === "" || value === "—") {
    return null;
  }
  return (
    <div className="gd-meta-item">
      <span className="gd-meta-label">{label}</span>
      <span className="gd-meta-value">{value}</span>
    </div>
  );
}

function InstallFlag({ installation }: { installation: Installation }) {
  if (installation.status === "missing" || installation.status === "deleted") {
    return (
      <span className="gd-flag is-danger">
        <Icon name="alert-triangle" size={11} />
        {installation.status === "missing" ? "Missing" : "Deleted"}
      </span>
    );
  }
  if (installation.status === "offline") {
    return (
      <span className="gd-flag is-neutral">
        <Icon name="info" size={11} />
        Drive offline
      </span>
    );
  }
  return (
    <span className="gd-flag">{formatBytes(installation.install_size_bytes ?? 0)}</span>
  );
}

/** Named tile for the full achievement list on the Achievements tab. */
function AchievementTile({ achievement }: { achievement: Achievement }) {
  const unlocked = achievement.is_unlocked === 1;
  return (
    <div
      className={clsx("gd-ach", unlocked && "is-unlocked")}
      title={achievement.description ?? achievement.name}
    >
      <span className="gd-ach-mark">
        <Icon name={unlocked ? "trophy" : "lock"} size={16} />
      </span>
      <div className="gd-ach-text">
        <div className="gd-ach-name">{achievement.name}</div>
        <div className="gd-ach-when">
          {unlocked ? formatRelative(achievement.unlocked_at) : "Locked"}
        </div>
      </div>
    </div>
  );
}

/**
 * Tag values with graceful truncation.
 *
 * Steam ships up to 28 supported languages and a comparable pile of store
 * categories; rendered whole they turn About into a wall of chips taller than
 * everything else on the page. The full set is kept — it is just disclosed on
 * request rather than by default.
 */
function TagRow({ items, limit }: { items: string[]; limit: number }) {
  const [expanded, setExpanded] = useState(false);
  const hidden = items.length - limit;
  const shown = expanded ? items : items.slice(0, limit);
  return (
    <dd className="gd-fact-tags">
      {shown.map((t) => (
        <span key={t} className="gd-tag">
          {t}
        </span>
      ))}
      {hidden > 0 && (
        <button
          className="gd-tag gd-tag-more"
          onClick={() => setExpanded((v) => !v)}
          aria-expanded={expanded}
        >
          {expanded ? "Show less" : `+${hidden} more`}
        </button>
      )}
    </dd>
  );
}

/**
 * One About row. Renders nothing when there is no value, so the panel never
 * shows an empty label or a placeholder dash.
 */
function Fact({
  label,
  value,
  tags,
  chips,
  limit,
}: {
  label: string;
  value?: string | number | null;
  tags?: string[];
  chips?: Chip[];
  limit?: number;
}) {
  if (chips) {
    if (chips.length === 0) return null;
    return (
      <div className="gd-fact">
        <dt>{label}</dt>
        <dd className="gd-fact-tags">
          {chips.map((c) => (
            <span key={c.label} className="gd-tag is-strong">
              {c.icon && <Icon name={c.icon} size={13} />}
              {c.label}
            </span>
          ))}
        </dd>
      </div>
    );
  }
  if (tags) {
    if (tags.length === 0) return null;
    return (
      <div className="gd-fact">
        <dt>{label}</dt>
        {limit !== undefined && tags.length > limit ? (
          <TagRow items={tags} limit={limit} />
        ) : (
          <dd className="gd-fact-tags">
            {tags.map((t) => (
              <span key={t} className="gd-tag">
                {t}
              </span>
            ))}
          </dd>
        )}
      </div>
    );
  }
  if (value === null || value === undefined || value === "") return null;
  return (
    <div className="gd-fact">
      <dt>{label}</dt>
      <dd>{value}</dd>
    </div>
  );
}

function canLaunch(installs: Installation[]): boolean {
  return installs.some((i) => i.status === "installed");
}
