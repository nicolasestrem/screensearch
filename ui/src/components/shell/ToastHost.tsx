// ToastHost (new) — the shell's one toast surface, replacing the bare
// `ToastViewport` (UI_REFERENCE §5, 0.4.5 D9). It renders the same
// `useToastStore` queue (so `useLiveEvents` and every existing `toast.*` call site
// keep working unchanged) through the P2 `Toast` primitive, and adds the trailing
// action that `ToastItem` itself cannot carry: actions are registered beside the
// store by `actionToast.pushActionToast` and looked up per id here.
//
// Running an action also dismisses its toast — an undo that leaves "updated"
// on screen would be lying — and every close path (timer, dismiss button, action)
// drops the registered action so the map cannot grow.
import { Toast } from "../primitives";
import { useToastStore } from "../../state/toastStore";
import { clearToastAction, toastActionFor } from "./actionToast";

export function ToastHost() {
  const toasts = useToastStore((state) => state.toasts);
  const dismiss = useToastStore((state) => state.dismiss);

  const close = (id: number) => {
    clearToastAction(id);
    dismiss(id);
  };

  return (
    <div
      aria-live="polite"
      aria-relevant="additions"
      className="pointer-events-none fixed right-4 bottom-4 z-toast flex max-w-sm flex-col items-end gap-2"
    >
      {toasts.map((item) => {
        const registered = toastActionFor(item.id);
        return (
          <Toast
            key={item.id}
            toast={item}
            onClose={close}
            action={
              registered
                ? {
                    label: registered.label,
                    onAction: () => {
                      registered.onAction();
                      close(item.id);
                    },
                  }
                : undefined
            }
          />
        );
      })}
    </div>
  );
}
