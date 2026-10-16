import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import * as cmd from "./commands";
import { queryKeys } from "./queryKeys";
import { readTokenActionOptions } from "./apiReadTokenOptions";

/** Passive status contains only availability, and errors are safe before caching. */
export function useApiReadTokenStatus() {
  return useQuery({
    queryKey: queryKeys.apiReadTokenStatus,
    queryFn: async () => {
      try {
        return await cmd.getApiReadTokenStatus();
      } catch {
        throw new Error("Could not load read-token availability. Retry to check its status.");
      }
    },
    retry: false,
  });
}

/** An explicit action, never a background secret fetch. Mutation data is always void. */
export function useApiReadTokenAction(receiver: {
  reveal: (token: string) => void;
  copy: (token: string) => Promise<void>;
  isActive: () => boolean;
}) {
  const client = useQueryClient();
  return useMutation(readTokenActionOptions(client, cmd, receiver));
}
