// Read-credential mutations deliberately resolve to void: neither the secret nor
// a backend error can enter TanStack's mutation state (including global callbacks).
import type { QueryClient } from "@tanstack/react-query";
import type * as commands from "./commands";
import { queryKeys } from "./queryKeys.ts";

export type ReadTokenAction = "create" | "rotate" | "revoke" | "reveal" | "copy";
type CredentialCommands = Pick<typeof commands,
  "createApiReadToken" | "rotateApiReadToken" | "revokeApiReadToken" | "revealApiReadToken">;

const failureMessages: Record<ReadTokenAction, string> = {
  create: "Could not create the read token. Check its status, then retry.",
  rotate: "Could not rotate the read token. The previous credential may still be in use; check its status, then retry.",
  revoke: "Could not revoke the read token. Do not assume access was removed; check its status, then retry.",
  reveal: "Could not reveal the read token. Check its status, then retry.",
  copy: "Could not copy the read token. Check clipboard access and retry, or reveal it to copy manually.",
};

export function readTokenActionOptions(
  client: QueryClient,
  api: CredentialCommands,
  receiver: {
    reveal: (token: string) => void;
    copy: (token: string) => Promise<void>;
    isActive: () => boolean;
  },
) {
  return {
    gcTime: 0,
    retry: false as const,
    onMutate: () => client.cancelQueries({ queryKey: queryKeys.apiReadTokenStatus }),
    mutationFn: async (action: ReadTokenAction): Promise<void> => {
      try {
        if (action === "reveal" || action === "copy") {
          // Only local component state or an explicit clipboard write receives this
          // value. It is not a mutation argument, result, context or cache entry.
          const secret = await api.revealApiReadToken();
          if (!receiver.isActive()) return;
          if (action === "reveal") receiver.reveal(secret.token);
          else await receiver.copy(secret.token);
        } else {
          const status = await (action === "create" ? api.createApiReadToken()
            : action === "rotate" ? api.rotateApiReadToken() : api.revokeApiReadToken());
          client.setQueryData(queryKeys.apiReadTokenStatus, status);
        }
      } catch {
        // Never cache/echo raw IPC or clipboard errors, even as Error.cause.
        throw new Error(failureMessages[action]);
      }
    },
    onSettled: () => {
      void client.invalidateQueries({ queryKey: queryKeys.apiReadTokenStatus });
    },
  };
}
