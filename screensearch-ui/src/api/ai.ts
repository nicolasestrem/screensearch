import { API_BASE_URL } from '../lib/utils';

export interface AiConnectionRequest {
    provider_url: string;
    api_key?: string;
    model: string;
}

export interface AiConnectionResponse {
    success: boolean;
    message: string;
}

export interface AiReportRequest {
    provider_url: string;
    api_key?: string;
    model: string;
    start_time?: string;
    end_time?: string;
    prompt?: string;
}

export interface AiReportResponse {
    report: string;
    model_used: string;
    tokens_used?: number;
}

export const aiApi = {
    validateConnection: async (data: AiConnectionRequest): Promise<AiConnectionResponse> => {
        const response = await fetch(`${API_BASE_URL}/ai/validate`, {
            method: 'POST',
            headers: {
                'Content-Type': 'application/json',
            },
            body: JSON.stringify(data),
        });

        if (!response.ok) {
            throw new Error(`Connection validation failed: ${response.statusText}`);
        }

        return response.json();
    },

    generateReport: async (data: AiReportRequest): Promise<AiReportResponse> => {
        const response = await fetch(`${API_BASE_URL}/ai/generate`, {
            method: 'POST',
            headers: {
                'Content-Type': 'application/json',
            },
            body: JSON.stringify(data),
        });

        if (!response.ok) {
            const errorText = await response.text();
            let errorMessage = errorText;
            try {
                const errorJson: unknown = JSON.parse(errorText);
                if (
                    typeof errorJson === 'object' &&
                    errorJson !== null &&
                    'message' in errorJson &&
                    typeof errorJson.message === 'string'
                ) {
                    errorMessage = errorJson.message;
                }
            } catch {
                // The response was plain text.
            }
            throw new Error(`Report generation failed: ${response.statusText}. ${errorMessage}`);
        }

        return response.json();
    },
};
