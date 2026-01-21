// React hooks for Post-Urbit API
import { useState, useEffect, useCallback } from 'react';
import { apiClient, ApiClientError } from './client';
import type {
  HealthCheck,
  NodeStatus,
  Identity,
  App,
  AppRuntime,
  LoginRequest,
  LoginResponse,
} from './types';

interface UseApiState<T> {
  data: T | null;
  loading: boolean;
  error: ApiClientError | null;
  refetch: () => Promise<void>;
}

// Generic hook for API calls with loading/error states
function useApi<T>(
  fetchFn: () => Promise<T>,
  deps: unknown[] = []
): UseApiState<T> {
  const [data, setData] = useState<T | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<ApiClientError | null>(null);

  const fetchData = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const result = await fetchFn();
      setData(result);
    } catch (err) {
      if (err instanceof ApiClientError) {
        setError(err);
      } else {
        setError(
          new ApiClientError(
            'UNKNOWN_ERROR',
            err instanceof Error ? err.message : 'Unknown error',
            0
          )
        );
      }
    } finally {
      setLoading(false);
    }
  }, deps); // eslint-disable-line react-hooks/exhaustive-deps

  useEffect(() => {
    fetchData();
  }, [fetchData]);

  return { data, loading, error, refetch: fetchData };
}

// Health check hook - polls every 30 seconds
export function useHealth(pollInterval: number = 30000): UseApiState<HealthCheck> {
  const [data, setData] = useState<HealthCheck | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<ApiClientError | null>(null);

  const fetchData = useCallback(async () => {
    try {
      const result = await apiClient.get<HealthCheck>('/health');
      setData(result);
      setError(null);
    } catch (err) {
      if (err instanceof ApiClientError) {
        setError(err);
      }
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchData();
    const interval = setInterval(fetchData, pollInterval);
    return () => clearInterval(interval);
  }, [fetchData, pollInterval]);

  return { data, loading, error, refetch: fetchData };
}

// Node status hook
export function useNodeStatus(): UseApiState<NodeStatus> {
  return useApi(() => apiClient.get<NodeStatus>('/admin/v1/status'));
}

// Identity hook
export function useIdentity(): UseApiState<Identity> {
  return useApi(() => apiClient.get<Identity>('/admin/v1/identity'));
}

// Apps list hook
export function useApps(): UseApiState<App[]> {
  return useApi(() => apiClient.get<App[]>('/admin/v1/apps'));
}

// Single app runtime hook
export function useAppRuntime(appId: string): UseApiState<AppRuntime> {
  return useApi(
    () => apiClient.get<AppRuntime>(`/admin/v1/apps/${appId}/runtime`),
    [appId]
  );
}

// Auth hooks
export function useAuth() {
  const [isAuthenticated, setIsAuthenticated] = useState(
    () => !!apiClient.getAuthToken()
  );

  const login = async (password: string, rememberDevice = true) => {
    try {
      const response = await apiClient.post<LoginResponse>('/admin/v1/auth/login', {
        password,
        remember_device: rememberDevice,
      } as LoginRequest);

      // Store CSRF token from response
      apiClient.setCsrfToken(response.csrf_token);
      setIsAuthenticated(true);

      return { success: true, error: null };
    } catch (err) {
      return {
        success: false,
        error: err instanceof ApiClientError ? err : new ApiClientError(
          'LOGIN_ERROR',
          'Login failed',
          0
        ),
      };
    }
  };

  const logout = async () => {
    try {
      await apiClient.post('/admin/v1/auth/logout');
    } catch (err) {
      // Ignore errors on logout
    } finally {
      apiClient.clearAuthToken();
      setIsAuthenticated(false);
    }
  };

  return { isAuthenticated, login, logout };
}

// Check if backend is reachable
export function useBackendStatus() {
  const [isReachable, setIsReachable] = useState(false);
  const [checking, setChecking] = useState(true);

  useEffect(() => {
    const checkBackend = async () => {
      try {
        await apiClient.get('/health');
        setIsReachable(true);
      } catch (err) {
        setIsReachable(false);
      } finally {
        setChecking(false);
      }
    };

    checkBackend();
    const interval = setInterval(checkBackend, 10000); // Check every 10s
    return () => clearInterval(interval);
  }, []);

  return { isReachable, checking };
}
