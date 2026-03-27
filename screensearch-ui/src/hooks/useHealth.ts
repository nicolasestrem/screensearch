import { useQuery, UseQueryResult } from '@tanstack/react-query';
import { apiClient } from '../api/client';
import type { HealthStatus } from '../types';

export function useHealth(): UseQueryResult<HealthStatus, Error> {
  return useQuery({
    queryKey: ['health'],
    queryFn: () => apiClient.getHealth(),
    refetchInterval: 2000,
    staleTime: 0,
    retry: 3,
    retryDelay: 1000,
  });
}
