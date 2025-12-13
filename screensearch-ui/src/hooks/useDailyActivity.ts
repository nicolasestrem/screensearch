import { useQuery } from '@tanstack/react-query';
import axios from 'axios';
import { FrameResponse } from '../types';
import { useStore } from '../store/useStore';
import { startOfDay, endOfDay } from 'date-fns';

interface DailyActivityResponse {
    data: FrameResponse[];
    pagination: {
        total: number;
        limit: number;
        offset: number;
    };
};
}

/**
 * Hook to fetch daily activity stats for the timeline graph.
 * Unlike useFrames, this fetches a large dataset (up to 5000 items) for a single day
 * to populate the density visualization, bypassing standard pagination limits.
 */
export function useDailyActivity() {
    const { filters } = useStore();
    const date = filters.dateRange.start || new Date(); // Default to today if no date selected

    const queryParams = {
        start_time: startOfDay(date).toISOString(),
        end_time: endOfDay(date).toISOString(),
        limit: 5000, // Fetch a large number to get mostly everything for the graph
        // We can add other filters here if we want the graph to filter too
        ...(filters.applications[0] && {
            app: filters.applications[0],
        }),
        ...(filters.searchQuery && {
            q: filters.searchQuery,
        }),
    };

    return useQuery({
        queryKey: ['daily-activity', date, filters.applications, filters.searchQuery],
        queryFn: async () => {
            // Use configured provider URL or default to localhost:3131 (API)
            // Note: aiConfig is for LLM, we need the API base URL. Assuming localhost:3131 for now based on other files.
            const baseUrl = 'http://localhost:3131';
            const { data } = await axios.get<DailyActivityResponse>(`${baseUrl}/frames`, {
                params: queryParams,
            });
            return data.data; // Return just the frames array
        },
        staleTime: 1000 * 60 * 5, // 5 minutes
    });
}
