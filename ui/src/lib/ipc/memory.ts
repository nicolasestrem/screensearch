import {
  useMutation,
  useQuery,
  useQueryClient,
  type QueryClient,
} from "@tanstack/react-query";
import * as cmd from "./commands";
import type { MemorySettings } from "../../bindings/MemorySettings";
import type { MemoryStatus } from "../../bindings/MemoryStatus";
import { promotionKeys } from "./promotion";

export const memoryKeys = {
  all: ["memory"] as const,
  status: ["memory", "status"] as const,
  runs: ["memory", "runs"] as const,
  run: (id: string | null) => ["memory", "run", id] as const,
  deliveries: (id: string | null) => ["memory", "deliveries", id] as const,
};

export function useMemoryStatus() {
  return useQuery({ queryKey: memoryKeys.status, queryFn: cmd.getMemoryStatus });
}
export function useMemoryRuns() {
  return useQuery({ queryKey: memoryKeys.runs, queryFn: () => cmd.listMemoryRuns(100) });
}
export function useMemoryRun(id: string | null) {
  return useQuery({
    queryKey: memoryKeys.run(id),
    queryFn: () => cmd.getMemoryRun(id!),
    enabled: id !== null,
  });
}
export function useMemoryDeliveries(id: string | null) {
  return useQuery({
    queryKey: memoryKeys.deliveries(id),
    queryFn: () => cmd.getMemoryDeliveries(id!),
    enabled: id !== null,
  });
}
export function useMemorySchedulePreview(settings: MemorySettings | null) {
  return useQuery({
    queryKey: ["memory-preview", settings],
    queryFn: () => cmd.previewMemorySchedule(settings!),
    enabled: settings !== null && Boolean(settings.local_time && settings.timezone),
    retry: false,
  });
}
export function useMemoryMutation<T, V>(mutationFn: (variables: V) => Promise<T>) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn,
    gcTime: 0,
    onSuccess: () => qc.invalidateQueries({ queryKey: memoryKeys.all }),
  });
}

/**
 * Saving memory settings can move the output folder, which decides where the
 * single project note lives. The note-status query is keyed by a constant (the
 * backend resolves the folder), so it cannot refresh itself from the key: the
 * save must invalidate it explicitly. Both invalidations are awaited so the
 * mutation stays pending until mounted readers have refetched.
 */
export function memorySettingsMutationOptions(
  client: QueryClient,
  save: (settings: MemorySettings) => Promise<MemoryStatus> = cmd.setMemorySettings
) {
  return {
    mutationFn: save,
    gcTime: 0,
    onSuccess: () =>
      Promise.all([
        client.invalidateQueries({ queryKey: memoryKeys.all }),
        client.invalidateQueries({ queryKey: promotionKeys.noteStatus }),
      ]),
  };
}

export function useSetMemorySettings() {
  const qc = useQueryClient();
  return useMutation(memorySettingsMutationOptions(qc));
}

/** Backend errors are structured; unknown transport errors still remain readable. */
export function memoryError(error: unknown): string {
  if (error && typeof error === "object" && "message" in error) return String(error.message);
  return String(error);
}
