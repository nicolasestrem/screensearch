import { useQuery, useMutation, useQueryClient, UseQueryResult, UseMutationResult } from '@tanstack/react-query';
import { apiClient } from '../api/client';
import type { Settings, UpdateSettingsRequest } from '../types';
import toast from 'react-hot-toast';

export function useSettings(
  enabled = true
): UseQueryResult<Settings, Error> {
  return useQuery({
    queryKey: ['settings'],
    queryFn: () => apiClient.getSettings(),
    enabled,
    staleTime: 60000, // 1 minute
  });
}

export function useUpdateSettings(): UseMutationResult<Settings, Error, UpdateSettingsRequest> {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (settings: UpdateSettingsRequest) => apiClient.updateSettings(settings),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['settings'] });
      toast.success('Settings updated successfully');
    },
    onError: (error: Error) => {
      toast.error(`Failed to update settings: ${error.message}`);
    },
  });
}
