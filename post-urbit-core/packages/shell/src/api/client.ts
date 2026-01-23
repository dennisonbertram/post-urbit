// API Client for Post-Urbit HTTP API
import type { ApiError } from './types';

// Use relative URLs in dev mode (proxied by Vite), absolute in production
const DEFAULT_BASE_URL = import.meta.env.DEV ? '' : 'http://localhost:8080';
const AUTH_TOKEN_KEY = 'postnode_auth_token';
const CSRF_TOKEN_KEY = 'postnode_csrf_token';
export const UNAUTHORIZED_EVENT = 'postnode:unauthorized';

const notifyUnauthorized = () => {
  if (typeof window !== 'undefined') {
    window.dispatchEvent(new CustomEvent(UNAUTHORIZED_EVENT));
  }
};

export class ApiClient {
  private baseUrl: string;

  constructor(baseUrl: string = DEFAULT_BASE_URL) {
    this.baseUrl = baseUrl;
  }

  // Token management
  setAuthToken(token: string): void {
    localStorage.setItem(AUTH_TOKEN_KEY, token);
  }

  getAuthToken(): string | null {
    return localStorage.getItem(AUTH_TOKEN_KEY);
  }

  clearAuthToken(): void {
    localStorage.removeItem(AUTH_TOKEN_KEY);
    localStorage.removeItem(CSRF_TOKEN_KEY);
  }

  setCsrfToken(token: string): void {
    localStorage.setItem(CSRF_TOKEN_KEY, token);
  }

  getCsrfToken(): string | null {
    return localStorage.getItem(CSRF_TOKEN_KEY);
  }

  // Base fetch wrapper with auth and error handling
  private async request<T>(
    endpoint: string,
    options: RequestInit = {}
  ): Promise<T> {
    const url = `${this.baseUrl}${endpoint}`;
    const token = this.getAuthToken();
    const csrfToken = this.getCsrfToken();

    const headers: Record<string, string> = {
      'Content-Type': 'application/json',
    };

    // Add existing headers
    if (options.headers) {
      const existingHeaders = new Headers(options.headers);
      existingHeaders.forEach((value, key) => {
        headers[key] = value;
      });
    }

    // Add auth token if available
    if (token) {
      headers['Authorization'] = `Bearer ${token}`;
    }

    // Add CSRF token for state-changing requests
    if (
      csrfToken &&
      options.method &&
      ['POST', 'PUT', 'PATCH', 'DELETE'].includes(options.method)
    ) {
      headers['X-CSRF-Token'] = csrfToken;
    }

    try {
      const response = await fetch(url, {
        ...options,
        headers,
        credentials: 'include', // Include cookies for session auth
      });

      // Handle 401 - clear auth and redirect to login
      if (response.status === 401) {
        this.clearAuthToken();
        notifyUnauthorized();
        throw new ApiClientError('UNAUTHORIZED', 'Authentication required', 401);
      }

      // Handle non-OK responses
      if (!response.ok) {
        const errorData: ApiError = await response.json().catch(() => ({
          error: {
            code: 'UNKNOWN_ERROR',
            message: `HTTP ${response.status}: ${response.statusText}`,
          },
        }));

        throw new ApiClientError(
          errorData.error.code,
          errorData.error.message,
          response.status,
          errorData.error.details
        );
      }

      // Handle 204 No Content
      if (response.status === 204) {
        return undefined as T;
      }

      return await response.json();
    } catch (error) {
      if (error instanceof ApiClientError) {
        throw error;
      }

      // Network errors
      if (error instanceof TypeError) {
        throw new ApiClientError(
          'NETWORK_ERROR',
          'Unable to connect to the backend. Make sure the node is running.',
          0
        );
      }

      throw new ApiClientError(
        'UNKNOWN_ERROR',
        error instanceof Error ? error.message : 'An unknown error occurred',
        0
      );
    }
  }

  // HTTP method helpers
  async get<T>(endpoint: string): Promise<T> {
    return this.request<T>(endpoint, { method: 'GET' });
  }

  async post<T>(endpoint: string, data?: unknown): Promise<T> {
    return this.request<T>(endpoint, {
      method: 'POST',
      body: data ? JSON.stringify(data) : undefined,
    });
  }

  async put<T>(endpoint: string, data?: unknown): Promise<T> {
    return this.request<T>(endpoint, {
      method: 'PUT',
      body: data ? JSON.stringify(data) : undefined,
    });
  }

  async patch<T>(endpoint: string, data?: unknown): Promise<T> {
    return this.request<T>(endpoint, {
      method: 'PATCH',
      body: data ? JSON.stringify(data) : undefined,
    });
  }

  async delete<T>(endpoint: string): Promise<T> {
    return this.request<T>(endpoint, { method: 'DELETE' });
  }
}

// Custom error class for API errors
export class ApiClientError extends Error {
  constructor(
    public code: string,
    message: string,
    public status: number,
    public details?: unknown
  ) {
    super(message);
    this.name = 'ApiClientError';
  }
}

// Default client instance
export const apiClient = new ApiClient();
