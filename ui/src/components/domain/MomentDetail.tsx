// MomentDetail — one captured frame in full (UI_REFERENCE §4 Moment / §5). The
// image (intrinsic w/h → no layout shift), its capture context, the deferred vision
// analysis (or the on-demand "Tag with vision" entry point when it hasn't run yet —
// the partial state), the tags, and the recognized text. Text is shown as
// content_text (the default-retrieval layer) with raw_text always available via a
// disclosure — raw is preserved and viewable even though search defaults to content
// (03 §3b). Purely presentational: the route owns data fetching and wires
// `onQueueVision` to the enqueue_vision mutation. Tokens only.
import { Link } from "react-router-dom";

import type { UiFrameDetail } from "../../bindings/UiFrameDetail";
import type { CaptureTrigger } from "../../bindings/CaptureTrigger";
import type { TextSpan } from "../../bindings/TextSpan";
import { Button, Chip, Panel } from "../primitives";
import { FrameImage } from "./FrameImage";
import { FrameReconstruction } from "./FrameReconstruction";
import { IconSparkle, IconTag } from "../icons";
import { absoluteTime } from "../../lib/time";

/** Human-readable label for why a frame was captured (the capture trigger). */
const TRIGGER_LABEL: Record<CaptureTrigger, string> = {
  timer: "Timer",
  idle: "Idle",
  foreground_change: "App switch",
  clipboard_change: "Clipboard change",
  typing_pause: "Typing pause",
  click: "Click",
  scroll_stop: "Scroll stop",
  manual: "Manual",
};

export interface MomentDetailProps {
  detail: UiFrameDetail;
  /** Enqueue on-demand vision tagging for this frame. */
  onQueueVision: () => void;
  /** A vision job for this frame is in flight. */
  queueing: boolean;
  /** Stored text spans for the layout reconstruction — only needed (and fetched) when
   *  the screenshot has been retention-degraded (`detail.image_purged`). */
  spans?: TextSpan[];
}

/** A labelled context row; renders nothing when the value is absent. */
function Meta({
  label,
  value,
}: {
  label: string;
  value: string | null | undefined;
}) {
  if (!value) return null;
  return (
    <div className="flex flex-col gap-1">
      <span className="text-t-xs font-medium text-fg-secondary">{label}</span>
      <span className="break-words text-t-body text-fg-primary">{value}</span>
    </div>
  );
}

export function MomentDetail({
  detail,
  onQueueVision,
  queueing,
  spans,
}: MomentDetailProps) {
  const { vision } = detail;
  // A negative confidence is the "unknown" sentinel (the model gave no usable score);
  // surface it as n/a rather than a misleading -100%.
  const confidenceChip = vision ? (
    vision.confidence >= 0 ? (
      <Chip tone="accent">{Math.round(vision.confidence * 100)}%</Chip>
    ) : (
      <Chip tone="neutral">n/a</Chip>
    )
  ) : undefined;

  return (
    <div className="grid grid-cols-1 gap-4 lg:grid-cols-[1.6fr_1fr]">
      {/* Image (or, once the screenshot has expired, a text+layout reconstruction).
          min-w-0: a grid item defaults to min-width:auto, so wide recognized text
          would force this track past its 1.6fr share and shove the context rail
          off-screen; min-w-0 lets it honour the track and wrap/scroll inside (D9). */}
      <div className="flex min-w-0 flex-col gap-4">
        {detail.image_purged ? (
          <div className="flex flex-col gap-2">
            <FrameReconstruction
              spans={spans ?? []}
              width={detail.width}
              height={detail.height}
              className="h-auto"
            />
            <p className="text-t-xs text-fg-tertiary">
              The screenshot expired to save space — this is a reconstruction
              from the recognized text at its on-screen positions. The full text
              is below.
            </p>
          </div>
        ) : (
          <FrameImage
            imagePath={detail.image_path}
            intrinsicWidth={detail.width}
            intrinsicHeight={detail.height}
            alt={`Capture from ${absoluteTime(detail.captured_at)}`}
            className="h-auto w-full rounded-md border border-line-subtle bg-surface-raised object-contain"
          />
        )}
        <Panel
          title="Recognized text"
          action={
            <Chip tone="neutral">
              {detail.text_source === "uia" ? "UIA" : "OCR"}
            </Chip>
          }
        >
          {detail.content_text ? (
            // Preformatted capture (tables / code / terminal): preserve columns and
            // scroll this block horizontally rather than wrap them into an unreadable
            // stack — a scoped D9 exemption for recognized text (UI_REFERENCE §8).
            <pre className="overflow-x-auto whitespace-pre text-t-body text-fg-secondary font-mono">
              {detail.content_text}
            </pre>
          ) : (
            <p className="text-t-body text-fg-tertiary">
              No text was recognized in this frame.
            </p>
          )}
          {detail.raw_text && (
            <details className="mt-3 border-t border-line-subtle pt-3">
              <summary className="cursor-pointer select-none text-t-xs font-medium text-fg-secondary">
                Raw text
                {detail.raw_text !== detail.content_text
                  ? " (includes app chrome)"
                  : ""}
              </summary>
              <pre className="mt-2 overflow-x-auto whitespace-pre text-t-body text-fg-secondary font-mono">
                {detail.raw_text}
              </pre>
            </details>
          )}
        </Panel>
      </div>

      {/* Context, vision, tags. */}
      <div className="flex min-w-0 flex-col gap-4">
        <Panel title="Context">
          <div className="flex flex-col gap-3">
            <Meta label="Captured" value={absoluteTime(detail.captured_at)} />
            <Meta
              label="Captured via"
              value={
                detail.capture_trigger
                  ? TRIGGER_LABEL[detail.capture_trigger]
                  : null
              }
            />
            <Meta label="App" value={detail.app_hint} />
            <Meta label="Window" value={detail.window_title} />
            <Meta label="URL" value={detail.browser_url} />
            {detail.session ? (
              <div className="flex flex-col gap-1">
                <span className="text-t-xs font-medium text-fg-secondary">
                  Part of session
                </span>
                <Link
                  to={`/timeline/session/${detail.session.id}`}
                  className="inline-flex min-h-hit-min items-center break-words text-t-body text-fg-primary underline decoration-line-subtle underline-offset-4 hover:text-accent"
                >
                  {detail.session.title?.trim() ||
                    `Session ${detail.session.id}`}
                </Link>
              </div>
            ) : null}
            <Meta
              label="Monitor"
              value={`#${detail.monitor_index} · ${detail.width}×${detail.height}`}
            />
          </div>
        </Panel>

        <Panel title="Vision" action={confidenceChip}>
          {vision ? (
            <div className="flex flex-col gap-3">
              <p className="text-t-body text-fg-primary">
                {vision.description}
              </p>
              <div className="flex flex-wrap items-center gap-2">
                {vision.activity_type && (
                  <Chip tone="neutral">{vision.activity_type}</Chip>
                )}
                <Chip tone="neutral">{vision.model}</Chip>
              </div>
            </div>
          ) : (
            // Partial state: vision hasn't run — the on-demand entry point (`03 §5`).
            <div className="flex flex-col items-start gap-3">
              <p className="text-t-body text-fg-secondary">
                This frame hasn't been tagged by vision yet.
              </p>
              <Button
                variant="primary"
                leadingIcon={<IconSparkle size={16} />}
                onClick={onQueueVision}
                disabled={queueing}
              >
                {queueing ? "Queuing…" : "Tag with vision"}
              </Button>
            </div>
          )}
        </Panel>

        {detail.tags.length > 0 && (
          <Panel title="Tags">
            <div className="flex flex-wrap gap-2">
              {detail.tags.map((t) => (
                <Chip key={t} tone="neutral">
                  <IconTag size={12} />
                  {t}
                </Chip>
              ))}
            </div>
          </Panel>
        )}
      </div>
    </div>
  );
}
