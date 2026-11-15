import type { MediaItemDto } from "../api";

export interface HeroBannerProps {
  item: MediaItemDto;
}

export function HeroBanner({ item }: HeroBannerProps) {
  return (
    <section aria-label="Избранное" className="relative overflow-hidden rounded-card bg-surface">
      <div className="relative aspect-video w-full">
        {item.posterUrl ? (
          <img
            src={item.posterUrl}
            alt={item.title}
            draggable={false}
            className="h-full w-full object-cover"
          />
        ) : (
          <div className="flex h-full w-full items-center justify-center bg-surface">
            <span className="px-8 text-center text-section text-text-2">{item.title}</span>
          </div>
        )}
        <div className="absolute inset-0 bg-gradient-to-t from-black/90 via-black/40 to-transparent" />
        <div className="absolute inset-x-0 bottom-0 flex flex-col gap-2 p-8">
          <h1 className="text-hero text-text">{item.title}</h1>
          <p className="text-card text-text-2">
            {[item.year, item.rating !== null ? `★ ${item.rating}` : null]
              .filter((part) => part !== null)
              .join(" · ")}
          </p>
        </div>
      </div>
    </section>
  );
}
