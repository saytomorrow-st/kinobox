import type { MediaItemDto } from "../api";

export interface PosterRowProps {
  items: MediaItemDto[];
  focusedIndex: number | null;
  onSelect: (item: MediaItemDto) => void;
}

export function PosterRow({ items, focusedIndex, onSelect }: PosterRowProps) {
  return (
    <ul className="flex gap-3 overflow-x-auto pb-2 scroll-smooth">
      {items.map((item, index) => (
        <PosterCard
          key={item.id}
          item={item}
          focused={index === focusedIndex}
          onSelect={() => onSelect(item)}
        />
      ))}
    </ul>
  );
}

function PosterCard({
  item,
  focused,
  onSelect,
}: {
  item: MediaItemDto;
  focused: boolean;
  onSelect: () => void;
}) {
  return (
    <li className="shrink-0">
      <button
        type="button"
        onClick={onSelect}
        aria-label={item.title}
        className={`block w-36 outline-none transition-transform duration-200 focus-visible:ring-2 focus-visible:ring-accent/70 ${
          focused ? "scale-105" : ""
        }`}
      >
        <div
          className={`aspect-[2/3] w-full overflow-hidden rounded-card ${
            focused ? "ring-2 ring-accent" : ""
          }`}
        >
          {item.posterUrl ? (
            <img
              src={item.posterUrl}
              alt={item.title}
              loading="lazy"
              draggable={false}
              className="h-full w-full object-cover"
            />
          ) : (
            <div className="flex h-full w-full items-center justify-center bg-surface px-2">
              <span className="text-center text-card text-text-2">{item.title}</span>
            </div>
          )}
        </div>
        <span className={`mt-2 block truncate text-card ${focused ? "text-text" : "text-text-2"}`}>
          {item.title}
        </span>
      </button>
    </li>
  );
}
