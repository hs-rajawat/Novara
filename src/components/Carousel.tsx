import { useEffect, useRef, useState, type ReactNode } from "react";
import { Link } from "react-router-dom";
import { Icon, type IconName } from "@/components/Icon";

interface Props {
  title: string;
  icon?: IconName;
  viewAllHref?: string;
  children: ReactNode;
}

/** Horizontal shelf with scroll-snap and arrow navigation. No dependency —
 * native overflow scroll does the heavy lifting; the arrows are a
 * convenience for mouse/trackpad users. */
export function Carousel({ title, icon, viewAllHref, children }: Props) {
  const trackRef = useRef<HTMLDivElement>(null);
  const [canPrev, setCanPrev] = useState(false);
  const [canNext, setCanNext] = useState(false);

  useEffect(() => {
    const el = trackRef.current;
    if (!el) return;

    function update() {
      if (!el) return;
      setCanPrev(el.scrollLeft > 4);
      setCanNext(el.scrollLeft + el.clientWidth < el.scrollWidth - 4);
    }

    update();
    el.addEventListener("scroll", update, { passive: true });
    const ro = new ResizeObserver(update);
    ro.observe(el);
    return () => {
      el.removeEventListener("scroll", update);
      ro.disconnect();
    };
  }, [children]);

  function scrollBy(dir: 1 | -1) {
    const el = trackRef.current;
    if (!el) return;
    el.scrollBy({ left: dir * el.clientWidth * 0.82, behavior: "smooth" });
  }

  return (
    <section className="carousel">
      <div className="section-header">
        <h2>
          {icon && <Icon name={icon} size={15} />}
          {title}
        </h2>
        <div className="row" style={{ gap: 6 }}>
          {viewAllHref && (
            <Link to={viewAllHref} className="sub back-link">
              View all
              <Icon name="chevron-right" size={13} />
            </Link>
          )}
          <button
            type="button"
            className="carousel-arrow"
            onClick={() => scrollBy(-1)}
            disabled={!canPrev}
            aria-label="Scroll left"
          >
            <Icon name="chevron-left" size={15} />
          </button>
          <button
            type="button"
            className="carousel-arrow"
            onClick={() => scrollBy(1)}
            disabled={!canNext}
            aria-label="Scroll right"
          >
            <Icon name="chevron-right" size={15} />
          </button>
        </div>
      </div>
      <div className="carousel-track" ref={trackRef}>
        {children}
      </div>
    </section>
  );
}
