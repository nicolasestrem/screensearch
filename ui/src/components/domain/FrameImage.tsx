// FrameImage — a lazily-loaded, async-decoded capture image (UI_REFERENCE §5/§8).
// The stored `image_path` is relative; frameSrc resolves it to a Tauri asset URL
// (memoized) or `null` outside the shell (npm run dev) — in which case we render a
// calm placeholder rather than a broken image. `intrinsicWidth/Height`, when known
// (from FrameDetail), set the <img> width/height attributes so the browser reserves
// the right box before the pixels arrive (no layout shift, §8). The element is sized
// by the caller's `className`; only off-screen virtualization keeps a tile from ever
// mounting (and thus from resolving a URL).
import { useEffect, useState, type ImgHTMLAttributes } from "react";

import { frameSrc } from "../../lib/ipc/frameSrc";
import { cn } from "../../lib/cn";
import { IconImage, IconDatabase } from "../icons";

export interface FrameImageProps extends Omit<
  ImgHTMLAttributes<HTMLImageElement>,
  "src"
> {
  imagePath: string | null | undefined;
  /** When the screenshot has been retention-degraded — the image file is gone but the
   *  text proof remains. Renders a "text kept" state instead of fetching a dead path. */
  imagePurged?: boolean;
  /** Intrinsic pixel dimensions, when known — reserve space to avoid layout shift. */
  intrinsicWidth?: number;
  intrinsicHeight?: number;
  alt: string;
}

export function FrameImage({
  imagePath,
  imagePurged = false,
  intrinsicWidth,
  intrinsicHeight,
  alt,
  className,
  ...rest
}: FrameImageProps) {
  const [src, setSrc] = useState<string | null>(null);
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    // Degraded captures have no file to fetch — skip the resolve and show "text kept".
    if (imagePurged) {
      setSrc(null);
      setFailed(false);
      return;
    }
    let active = true;
    setFailed(false);
    setSrc(null);
    frameSrc(imagePath).then((u) => {
      if (active) setSrc(u);
    });
    return () => {
      active = false;
    };
  }, [imagePath, imagePurged]);

  // Retention-degraded: the screenshot was deleted on purpose to save space; the text
  // proof is preserved (and a full reconstruction shows in the Moment view).
  if (imagePurged) {
    return (
      <div
        className={cn(
          "flex flex-col items-center justify-center gap-1 bg-surface-raised text-fg-tertiary",
          className,
        )}
        role="img"
        aria-label={alt || "Screenshot expired; text kept"}
      >
        <IconDatabase size={20} />
        <span className="text-t-xs">Text kept</span>
      </div>
    );
  }

  // No resolvable URL (dev mode, missing path) or a failed decode → a placeholder
  // that still fills the caller's box, so layout never shifts when it resolves.
  if (!src || failed) {
    return (
      <div
        className={cn(
          "flex items-center justify-center bg-surface-raised text-fg-tertiary",
          className,
        )}
        role="img"
        aria-label={alt}
      >
        <IconImage size={24} />
      </div>
    );
  }

  return (
    <img
      src={src}
      alt={alt}
      loading="lazy"
      decoding="async"
      width={intrinsicWidth}
      height={intrinsicHeight}
      onError={() => setFailed(true)}
      className={className}
      {...rest}
    />
  );
}
