import { useLayoutEffect, useRef, useState } from "react";
import clsx from "clsx";
import { toImgSrc } from "@/lib/image";
import { coverGradient } from "@/lib/color";

interface Props {
  /** Raw stored path (or http/data URI) — this component converts it. */
  src: string | null | undefined;
  title: string;
  kind: "cover" | "hero" | "logo";
  className?: string;
  alt?: string;
  /** Opt out of native lazy-loading for the single above-the-fold hero image. */
  eager?: boolean;
}

type Status = "pending" | "loaded" | "errored";

/** What the element itself reports, which is authoritative. */
function observe(img: HTMLImageElement | null): Status {
  if (!img || !img.complete) return "pending";
  // `complete` is also true for a failed load; zero decoded width is the only way
  // to tell those apart.
  return img.naturalWidth > 0 ? "loaded" : "errored";
}

/**
 * Single source of truth for rendering game artwork: converts the stored
 * path via Tauri's asset protocol, shows a deterministic gradient
 * immediately, blurs the image in as it loads, and falls back to the
 * gradient (+ initials, for covers) if the path is missing or fails to load.
 */
export function GameArtwork({ src, title, kind, className, alt, eager = false }: Props) {
  const url = toImgSrc(src);
  const imgRef = useRef<HTMLImageElement | null>(null);
  // Tagged with the source it describes, so a card recycled for another game
  // (list re-sort, carousel reuse) can never show the previous image's state.
  const [state, setState] = useState<{ url: string | undefined; status: Status }>({
    url,
    status: "pending",
  });

  // Reconcile with the element on every source change, rather than relying only on
  // the `load` event.
  //
  // The image is invisible until `is-loaded` is applied (`.artwork-img` is
  // `opacity: 0`), and that class used to be driven solely by `onLoad`. When the
  // asset is already in the webview's cache — the normal case for artwork just seen
  // on the Dashboard or in the Library — decoding can finish before React attaches
  // the handler, so `load` fires with nothing listening and `onLoad` never runs.
  // The card then stayed blank for good: initials for a cover, an empty box for a
  // hero, until some unrelated re-render happened to win the race. `complete` is
  // the element's own record of that, true whether the event was missed or never
  // needed.
  //
  // This is also the per-source reset, and the two must stay in one effect: as two,
  // the passive reset ran *after* this layout effect and overwrote the correction
  // it had just made, which is how the blank card survived a first attempt at a fix.
  //
  // A layout effect so this settles before paint; otherwise a cached image still
  // flashes its placeholder for a frame.
  useLayoutEffect(() => {
    const observed = observe(imgRef.current);
    setState((prev) =>
      prev.url === url && prev.status === observed ? prev : { url, status: observed },
    );
  }, [url]);

  // The effect has not run yet on the render where `url` changes, so trust the tag
  // rather than the stored status. This also keeps the `<img>` mounted through that
  // render, so the effect above has an element to consult.
  const status = state.url === url ? state.status : "pending";
  const loaded = status === "loaded";
  const showImage = !!url && status !== "errored";
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
          ref={imgRef}
          className={clsx("artwork-img", loaded && "is-loaded")}
          src={url}
          alt={alt ?? title}
          loading={eager ? "eager" : "lazy"}
          decoding="async"
          onLoad={() => setState({ url, status: "loaded" })}
          onError={() => setState({ url, status: "errored" })}
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
