import { useQuery, useMutation, useQueryClient, UseQueryResult } from '@tanstack/react-query';
import { apiClient } from '../api/client';
import type { Tag, CreateTagRequest } from '../types';
import toast from 'react-hot-toast';

export function useTags(): UseQueryResult<Tag[], Error> {
  return useQuery({
    queryKey: ['tags'],
    queryFn: () => apiClient.getTags(),
    staleTime: 60000, // 1 minute
  });
}

export function useCreateTag() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (tag: CreateTagRequest) => apiClient.createTag(tag),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['tags'] });
      toast.success('Tag created successfully');
    },
    onError: (error: Error) => {
      toast.error(`Failed to create tag: ${error.message}`);
    },
  });
}

export function useUpdateTag() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({ id, tag }: { id: number; tag: Partial<CreateTagRequest> }) =>
      apiClient.updateTag(id, tag),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['tags'] });
      toast.success('Tag updated successfully');
    },
    onError: (error: Error) => {
      toast.error(`Failed to update tag: ${error.message}`);
    },
  });
}

export function useDeleteTag() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (id: number) => apiClient.deleteTag(id),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['tags'] });
      queryClient.invalidateQueries({ queryKey: ['frames'] });
      toast.success('Tag deleted successfully');
    },
    onError: (error: Error) => {
      toast.error(`Failed to delete tag: ${error.message}`);
    },
  });
}

export function useAddTagToFrame() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({ frameId, tagId }: { frameId: number; tagId: number }) =>
      apiClient.addTagToFrame(frameId, tagId),
    onSuccess: (_, { frameId }) => {
      queryClient.invalidateQueries({ queryKey: ['frame', frameId] });
      queryClient.invalidateQueries({ queryKey: ['frames'] });
      toast.success('Tag added to frame');
    },
    onError: (error: Error) => {
      toast.error(`Failed to add tag: ${error.message}`);
    },
  });
}

export function useRemoveTagFromFrame() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({ frameId, tagId }: { frameId: number; tagId: number }) =>
      apiClient.removeTagFromFrame(frameId, tagId),
    onSuccess: (_, { frameId }) => {
      queryClient.invalidateQueries({ queryKey: ['frame', frameId] });
      queryClient.invalidateQueries({ queryKey: ['frames'] });
      toast.success('Tag removed from frame');
    },
    onError: (error: Error) => {
      toast.error(`Failed to remove tag: ${error.message}`);
    },
  });
}
