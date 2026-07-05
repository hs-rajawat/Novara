import { useEffect, useState } from "react";
import clsx from "clsx";
import { toImgSrc } from "@/lib/image";
import { coverGradient } from "@/lib/color";

interface Props {
  /** Raw stored path (or http/data URI) — this component converts it. */
  src: string | null | undefined;
  title: string;
  kind: "cover" | "hero";
  className?: string;
  alt?: string;
  /** Opt out of native lazy-loading for the single above-the-fold hero image. */
  eager?: boolean;
}

/**
 * Single source of truth for rendering game artwork: converts the stored
 * path via Tauri's asset protocol, shows a deterministic gradient
 * immediately, blurs the image in as it loads, and falls back to the
 * gradient (+ initials, for covers) if the path is missing or fails to load.
 */
export function GameArtwork({ src, title, kind, className, alt, eager = false }: Props) {
  const url = toImgSrc(src);
  const [loaded, setLoaded] = useState(false);
  const [errored, setErrored] = useState(false);

  // A card can be recycled for a different game (list re-sort, carousel
  // reuse) without unmounting — reset the transition state per source.
  useEffect(() => {
    setLoaded(false);
    setErrored(false);
  }, [url]);

  const showImage = !!url && !errored;
  const initials =
    kind === "cover"
      ? title
          .split(/\s+/)
          .slice(0, 2)
          .map((s) => s[0])
          .join("")
          .toUpperCase()
      : undefined;

  return (
    <div
      className={clsx("artwork", `artwork-${kind}`, className)}
      style={{ background: coverGradient(title) }}
    >
      {showImage && (
        <img
          className={clsx("artwork-img", loaded && "is-loaded")}
          src={url}
          alt={alt ?? title}
          loading={eager ? "eager" : "lazy"}
          decoding="async"
          onLoad={() => setLoaded(true)}
          onError={() => setErrored(true)}
        />
      )}
      {initials && (
        <span className={clsx("artwork-placeholder", loaded && showImage && "is-hidden")}>
          {initials}
        </span>
      )}
    </div>
  );
}
