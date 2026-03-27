import { useQuery, UseQueryResult } from '@tanstack/react-query';
import { apiClient } from '../api/client';
import type { PaginatedResponse, SearchResult, SearchParams } from '../types';

export function useSearch(
  params: SearchParams,
  enabled = true
): UseQueryResult<PaginatedResponse<SearchResult>, Error> {
  return useQuery({
    queryKey: ['search', params],
    queryFn: () => apiClient.search(params),
    enabled,
    staleTime: 30000, // 30 seconds
    refetchOnWindowFocus: false,
  });
}

export function useSearchKeywords(
  query: string,
  enabled = true
): UseQueryResult<string[], Error> {
  return useQuery({
    queryKey: ['search-keywords', query],
    queryFn: () => apiClient.searchKeywords(query),
    enabled: enabled && query.length > 0,
    staleTime: 60000, // 1 minute
  });
}
