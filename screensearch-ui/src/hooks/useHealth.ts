import { useQuery, UseQueryResult } from '@tanstack/react-query';
import { apiClient } from '../api/client';
import type { HealthStatus } from '../types';

export function useHealth(): UseQueryResult<HealthStatus, Error> {
  return useQuery({
    queryKey: ['health'],
    queryFn: () => apiClient.getHealth(),
    refetchInterval: 10000, // Refetch every 10 seconds
    retry: 3,
    retryDelay: 1000,
  });
}
