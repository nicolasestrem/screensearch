import axios, { AxiosError, AxiosInstance } from 'axios';
import type {
  PaginatedResponse,
  Frame,
  Tag,
  CreateTagRequest,
  SearchParams,
  SearchResult,
  AutomationClickRequest,
  AutomationTypeRequest,
  AutomationScrollRequest,
  AutomationKeyRequest,
  AutomationFindElementsRequest,
  UIElement,
  HealthStatus,
  Settings,
  UpdateSettingsRequest,
} from '../types';

class APIClient {
  private client: AxiosInstance;

  constructor(baseURL: string = '/api') {
    this.client = axios.create({
      baseURL,
      timeout: 30000,
      headers: {
        'Content-Type': 'application/json',
      },
    });

    // Request interceptor for logging
    this.client.interceptors.request.use(
      (config) => {
        console.log(`[API] ${config.method?.toUpperCase()} ${config.url}`);
        return config;
      },
      (error) => {
        console.error('[API] Request error:', error);
        return Promise.reject(error);
      }
    );

    // Response interceptor for error handling
    this.client.interceptors.response.use(
      (response) => response,
      (error: AxiosError) => {
        console.error('[API] Response error:', error.message);
        if (error.response) {
          console.error('[API] Error response:', error.response.data);
        }
        return Promise.reject(error);
      }
    );
  }

  // Health Check
  async getHealth(): Promise<HealthStatus> {
    const { data } = await this.client.get<HealthStatus>('/health');
    return data;
  }

  // Search Endpoints
  async search(params: SearchParams): Promise<PaginatedResponse<SearchResult>> {
    const { data } = await this.client.get<PaginatedResponse<SearchResult>>('/search', {
      params,
    });
    return data;
  }

  async searchKeywords(query: string, limit = 20): Promise<string[]> {
    const { data } = await this.client.get<string[]>('/search/keywords', {
      params: { keywords: query, limit },
    });
    return data;
  }

  // Frame Endpoints
  async getFrames(params?: {
    limit?: number;
    offset?: number;
    start_time?: string;
    end_time?: string;
    app_name?: string;
    tag_ids?: string;
    q?: string;
  }): Promise<PaginatedResponse<Frame>> {
    const { data } = await this.client.get<PaginatedResponse<Frame>>('/frames', {
      params,
    });
    return data;
  }

  async getFrame(id: number): Promise<Frame> {
    const { data } = await this.client.get<Frame>(`/frames/${id}`);
    return data;
  }

  async getFrameImage(id: number): Promise<string> {
    const { data } = await this.client.get<string>(`/frames/${id}/image`, {
      responseType: 'blob' as any,
    });
    return URL.createObjectURL(new Blob([data]));
  }

  // Tag Endpoints
  async getTags(): Promise<Tag[]> {
    const { data } = await this.client.get<Tag[]>('/tags');
    return data;
  }

  async createTag(tag: CreateTagRequest): Promise<Tag> {
    const { data } = await this.client.post<Tag>('/tags', tag);
    return data;
  }

  async updateTag(id: number, tag: Partial<CreateTagRequest>): Promise<Tag> {
    const { data } = await this.client.put<Tag>(`/tags/${id}`, tag);
    return data;
  }

  async deleteTag(id: number): Promise<void> {
    await this.client.delete(`/tags/${id}`);
  }

  async addTagToFrame(frameId: number, tagId: number): Promise<void> {
    await this.client.post(`/frames/${frameId}/tags`, { tag_id: tagId });
  }

  async removeTagFromFrame(frameId: number, tagId: number): Promise<void> {
    await this.client.delete(`/frames/${frameId}/tags/${tagId}`);
  }

  // Automation Endpoints
  async findElements(request: AutomationFindElementsRequest): Promise<UIElement[]> {
    const { data } = await this.client.post<UIElement[]>(
      '/automation/find-elements',
      request
    );
    return data;
  }

  async listElements(): Promise<UIElement[]> {
    const { data } = await this.client.post<UIElement[]>('/automation/list-elements');
    return data;
  }

  async click(request: AutomationClickRequest): Promise<void> {
    await this.client.post('/automation/click', request);
  }

  async type(request: AutomationTypeRequest): Promise<void> {
    await this.client.post('/automation/type', request);
  }

  async scroll(request: AutomationScrollRequest): Promise<void> {
    await this.client.post('/automation/scroll', request);
  }

  async pressKey(request: AutomationKeyRequest): Promise<void> {
    await this.client.post('/automation/press-key', request);
  }

  async getText(elementId: string): Promise<string> {
    const { data } = await this.client.post<{ text: string }>(
      '/automation/get-text',
      { element_id: elementId }
    );
    return data.text;
  }

  async openApp(appName: string): Promise<void> {
    await this.client.post('/automation/open-app', { app_name: appName });
  }

  async openUrl(url: string): Promise<void> {
    await this.client.post('/automation/open-url', { url });
  }

  // Settings Endpoints
  async getSettings(): Promise<Settings> {
    const { data } = await this.client.get<Settings>('/settings');
    return data;
  }

  async updateSettings(settings: UpdateSettingsRequest): Promise<Settings> {
    const { data } = await this.client.post<Settings>('/settings', settings);
    return data;
  }
}

// Export singleton instance
export const apiClient = new APIClient();

// Export class for testing
export default APIClient;
