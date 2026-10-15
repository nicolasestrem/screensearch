import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import * as cmd from "./commands";
import type { MemorySettings } from "../../bindings/MemorySettings";

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

/** Backend errors are structured; unknown transport errors still remain readable. */
export function memoryError(error: unknown): string {
  if (error && typeof error === "object" && "message" in error) return String(error.message);
  return String(error);
}
