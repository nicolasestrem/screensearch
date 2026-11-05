// Action toasts (undo, retry) ride the existing toast store so `useLiveEvents`
// keeps working, while the trailing action lives beside it: `ToastItem` carries
// only `{id, level, message}` and the store is not this phase's file. The shell's
// `ToastHost` renders every queued toast with the P2 `Toast` primitive and passes
// the registered action for ids that have one.
import type { ToastLevel } from "../../bindings/ToastLevel";
import type { ToastAction } from "../primitives";
import { useToastStore } from "../../state/toastStore";

/** Actions keyed by toast id; entries are dropped when their toast is dismissed. */
const actions = new Map<number, ToastAction>();

/** Enqueue a toast and attach a trailing action to it. Returns the toast id. */
export function pushActionToast(
  level: ToastLevel,
  message: string,
  action: ToastAction,
): number {
  const id = useToastStore.getState().push(level, message);
  actions.set(id, action);
  return id;
}

/** The action registered for a queued toast, if any. */
export function toastActionFor(id: number): ToastAction | undefined {
  return actions.get(id);
}

/** Drop the registered action after the toast is dismissed or acted on. */
export function clearToastAction(id: number): void {
  actions.delete(id);
}
