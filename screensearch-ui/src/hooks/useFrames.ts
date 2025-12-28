import { useQuery, UseQueryResult } from '@tanstack/react-query';
import { apiClient } from '../api/client';
import type { PaginatedResponse, Frame } from '../types';

interface UseFramesParams {
  limit?: number;
  offset?: number;
  start_time?: string;
  end_time?: string;
  app_name?: string;
  tag_ids?: string;
  q?: string;
  mode?: 'fts' | 'semantic' | 'hybrid';
}

export function useFrames(
  params?: UseFramesParams,
  enabled = true
): UseQueryResult<PaginatedResponse<Frame>, Error> {
  return useQuery({
    queryKey: ['frames', params],
    queryFn: () => apiClient.getFrames(params),
    enabled,
    staleTime: 10000, // 10 seconds
    refetchInterval: 30000, // Refetch every 30 seconds
  });
}

export function useFrame(
  id: number,
  enabled = true
): UseQueryResult<Frame, Error> {
  return useQuery({
    queryKey: ['frame', id],
    queryFn: () => apiClient.getFrame(id),
    enabled,
    staleTime: 60000, // 1 minute
  });
}

export function useFrameImage(
  id: number,
  enabled = true
): UseQueryResult<string, Error> {
  return useQuery({
    queryKey: ['frame-image', id],
    queryFn: () => apiClient.getFrameImage(id),
    enabled,
    staleTime: 300000, // 5 minutes
    gcTime: 600000, // 10 minutes
  });
}
