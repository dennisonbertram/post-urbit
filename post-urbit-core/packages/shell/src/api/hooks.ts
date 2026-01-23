// React hooks for Post-Urbit API
import { useState, useEffect, useCallback, useRef } from 'react';
import { apiClient, ApiClientError, UNAUTHORIZED_EVENT } from './client';
import type {
  HealthCheck,
  NodeStatus,
  Identity,
  App,
  AppRuntime,
  LoginRequest,
  LoginResponse,
  AppSecretsResponse,
  AppPermissions,
  AppPermissionsUpdate,
  AppActionResponse,
  AppUpdateResponse,
  LogsResponse,
  LogsQueryParams,
  Message,
  MessageFolder,
  MessageStats,
  MessageUpdate,
  PaginatedResponse,
  SendMessageRequest,
  SendMessageResponse,
} from './types';

interface UseApiState<T> {
  data: T | null;
  loading: boolean;
  isInitialLoading: boolean;
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
  const [isInitialLoading, setIsInitialLoading] = useState(true);
  const [error, setError] = useState<ApiClientError | null>(null);
  const requestIdRef = useRef(0);

  const fetchData = useCallback(async () => {
    const requestId = ++requestIdRef.current;
    setLoading(true);
    setError(null);
    try {
      const result = await fetchFn();
      // Only update state if this is still the current request
      if (requestId === requestIdRef.current) {
        setData(result);
      }
    } catch (err) {
      // Only update state if this is still the current request
      if (requestId === requestIdRef.current) {
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
      }
    } finally {
      // Only update state if this is still the current request
      if (requestId === requestIdRef.current) {
        setLoading(false);
        setIsInitialLoading(false);
      }
    }
  }, deps); // eslint-disable-line react-hooks/exhaustive-deps

  useEffect(() => {
    fetchData();
  }, [fetchData]);

  return { data, loading, isInitialLoading, error, refetch: fetchData };
}

// Health check hook - polls every 30 seconds
export function useHealth(pollInterval: number = 30000): UseApiState<HealthCheck> {
  const [data, setData] = useState<HealthCheck | null>(null);
  const [loading, setLoading] = useState(true);
  const [isInitialLoading, setIsInitialLoading] = useState(true);
  const [error, setError] = useState<ApiClientError | null>(null);
  const requestIdRef = useRef(0);

  const fetchData = useCallback(async () => {
    const requestId = ++requestIdRef.current;
    setLoading(true);
    setError(null);
    try {
      const result = await apiClient.get<HealthCheck>('/health');
      // Only update state if this is still the current request
      if (requestId === requestIdRef.current) {
        setData(result);
        setError(null);
      }
    } catch (err) {
      // Only update state if this is still the current request
      if (requestId === requestIdRef.current) {
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
      }
    } finally {
      // Only update state if this is still the current request
      if (requestId === requestIdRef.current) {
        setLoading(false);
        setIsInitialLoading(false);
      }
    }
  }, []);

  useEffect(() => {
    fetchData();
    const interval = setInterval(fetchData, pollInterval);
    return () => clearInterval(interval);
  }, [fetchData, pollInterval]);

  return { data, loading, isInitialLoading, error, refetch: fetchData };
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
  const sessionKey = 'postnode_session_active';
  const [isAuthenticated, setIsAuthenticated] = useState(
    () => !!apiClient.getAuthToken() || localStorage.getItem(sessionKey) === 'true'
  );
  const [checking, setChecking] = useState(true);

  const refreshSession = useCallback(async () => {
    if (apiClient.getAuthToken()) {
      setIsAuthenticated(true);
      return;
    }

    try {
      await apiClient.post('/admin/v1/auth/refresh');
      localStorage.setItem(sessionKey, 'true');
      setIsAuthenticated(true);
    } catch (err) {
      if (err instanceof ApiClientError && err.status === 401) {
        apiClient.clearAuthToken();
        localStorage.removeItem(sessionKey);
        setIsAuthenticated(false);
      }
    }
  }, [sessionKey]);

  useEffect(() => {
    let isActive = true;

    const check = async () => {
      await refreshSession();
      if (isActive) {
        setChecking(false);
      }
    };

    check();

    return () => {
      isActive = false;
    };
  }, [refreshSession]);

  useEffect(() => {
    const handleUnauthorized = () => {
      localStorage.removeItem(sessionKey);
      setIsAuthenticated(false);
    };

    window.addEventListener(UNAUTHORIZED_EVENT, handleUnauthorized);
    return () => {
      window.removeEventListener(UNAUTHORIZED_EVENT, handleUnauthorized);
    };
  }, [sessionKey]);

  const login = async (password: string, rememberDevice = true) => {
    try {
      const response = await apiClient.post<LoginResponse>('/admin/v1/auth/login', {
        password,
        remember_device: rememberDevice,
      } as LoginRequest);

      // Store CSRF token from response
      apiClient.setCsrfToken(response.csrf_token);
      localStorage.setItem(sessionKey, 'true');
      setIsAuthenticated(true);
      setChecking(false);

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
      localStorage.removeItem(sessionKey);
      setIsAuthenticated(false);
      setChecking(false);
    }
  };

  return { isAuthenticated, checking, login, logout };
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

// App secrets hook
export function useAppSecrets(appId: string): UseApiState<AppSecretsResponse> {
  return useApi(
    () => apiClient.get<AppSecretsResponse>(`/admin/v1/apps/${appId}/secrets`),
    [appId]
  );
}

// App permissions hook
export function useAppPermissions(appId: string): UseApiState<AppPermissions> {
  return useApi(
    () => apiClient.get<AppPermissions>(`/admin/v1/apps/${appId}/permissions`),
    [appId]
  );
}

// App settings hook
export function useAppSettings(appId: string): UseApiState<Record<string, unknown>> {
  return useApi(
    () => apiClient.get<Record<string, unknown>>(`/admin/v1/apps/${appId}/settings`),
    [appId]
  );
}

// App actions hook
export function useAppActions() {
  const startApp = async (appId: string): Promise<AppActionResponse> => {
    try {
      return await apiClient.post<AppActionResponse>(`/admin/v1/apps/${appId}/start`);
    } catch (err) {
      if (err instanceof ApiClientError) {
        throw err;
      }
      throw new ApiClientError(
        'START_APP_ERROR',
        err instanceof Error ? err.message : 'Failed to start app',
        0
      );
    }
  };

  const stopApp = async (appId: string): Promise<AppActionResponse> => {
    try {
      return await apiClient.post<AppActionResponse>(`/admin/v1/apps/${appId}/stop`);
    } catch (err) {
      if (err instanceof ApiClientError) {
        throw err;
      }
      throw new ApiClientError(
        'STOP_APP_ERROR',
        err instanceof Error ? err.message : 'Failed to stop app',
        0
      );
    }
  };

  const restartApp = async (appId: string): Promise<AppActionResponse> => {
    try {
      return await apiClient.post<AppActionResponse>(`/admin/v1/apps/${appId}/restart`);
    } catch (err) {
      if (err instanceof ApiClientError) {
        throw err;
      }
      throw new ApiClientError(
        'RESTART_APP_ERROR',
        err instanceof Error ? err.message : 'Failed to restart app',
        0
      );
    }
  };

  const updateApp = async (appId: string): Promise<AppUpdateResponse> => {
    try {
      return await apiClient.post<AppUpdateResponse>(`/admin/v1/apps/${appId}/update`);
    } catch (err) {
      if (err instanceof ApiClientError) {
        throw err;
      }
      throw new ApiClientError(
        'UPDATE_APP_ERROR',
        err instanceof Error ? err.message : 'Failed to update app',
        0
      );
    }
  };

  const clearAppData = async (appId: string): Promise<void> => {
    try {
      await apiClient.post(`/admin/v1/apps/${appId}/clear-data`);
    } catch (err) {
      if (err instanceof ApiClientError) {
        throw err;
      }
      throw new ApiClientError(
        'CLEAR_DATA_ERROR',
        err instanceof Error ? err.message : 'Failed to clear app data',
        0
      );
    }
  };

  const updatePermissions = async (
    appId: string,
    update: AppPermissionsUpdate
  ): Promise<AppPermissions> => {
    try {
      return await apiClient.patch<AppPermissions>(
        `/admin/v1/apps/${appId}/permissions`,
        update
      );
    } catch (err) {
      if (err instanceof ApiClientError) {
        throw err;
      }
      throw new ApiClientError(
        'UPDATE_PERMISSIONS_ERROR',
        err instanceof Error ? err.message : 'Failed to update permissions',
        0
      );
    }
  };

  return {
    startApp,
    stopApp,
    restartApp,
    updateApp,
    clearAppData,
    updatePermissions,
  };
}

// Helper to build query string from params
function buildQueryString(params: LogsQueryParams): string {
  const searchParams = new URLSearchParams();

  if (params.limit !== undefined) {
    searchParams.append('limit', String(params.limit));
  }
  if (params.cursor !== undefined) {
    searchParams.append('cursor', params.cursor);
  }
  if (params.level !== undefined) {
    searchParams.append('level', params.level);
  }
  if (params.target !== undefined) {
    searchParams.append('target', params.target);
  }
  if (params.search !== undefined) {
    searchParams.append('search', params.search);
  }
  if (params.since !== undefined) {
    searchParams.append('since', params.since);
  }
  if (params.until !== undefined) {
    searchParams.append('until', params.until);
  }

  const query = searchParams.toString();
  return query ? `?${query}` : '';
}

// Logs hook with optional polling
interface UseLogsResult {
  data: LogsResponse | null;
  loading: boolean;
  error: ApiClientError | null;
  refetch: () => Promise<void>;
  hasMore: boolean;
}

export function useLogs(
  params: LogsQueryParams = {},
  pollInterval: number = 0
): UseLogsResult {
  const [data, setData] = useState<LogsResponse | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<ApiClientError | null>(null);
  const abortControllerRef = useRef<AbortController | null>(null);
  const isMountedRef = useRef(true);

  const fetchData = useCallback(async () => {
    // Cancel any pending request
    if (abortControllerRef.current) {
      abortControllerRef.current.abort();
    }

    const controller = new AbortController();
    abortControllerRef.current = controller;
    setLoading(true);
    setError(null);

    try {
      const queryString = buildQueryString(params);
      const result = await apiClient.get<LogsResponse>(
        `/admin/v1/logs${queryString}`
      );

      if (!controller.signal.aborted && isMountedRef.current) {
        setData(result);
      }
    } catch (err) {
      if (!controller.signal.aborted && isMountedRef.current) {
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
      }
    } finally {
      if (!controller.signal.aborted && isMountedRef.current) {
        setLoading(false);
      }
    }
  }, [params.limit, params.cursor, params.level, params.target, params.search, params.since, params.until]); // eslint-disable-line react-hooks/exhaustive-deps

  useEffect(() => {
    isMountedRef.current = true;
    fetchData();

    // Set up polling if interval is specified
    let interval: ReturnType<typeof setInterval> | null = null;
    if (pollInterval > 0) {
      interval = setInterval(fetchData, pollInterval);
    }

    // Cleanup on unmount
    return () => {
      isMountedRef.current = false;
      if (interval) {
        clearInterval(interval);
      }
      if (abortControllerRef.current) {
        abortControllerRef.current.abort();
      }
    };
  }, [fetchData, pollInterval]);

  return {
    data,
    loading,
    error,
    refetch: fetchData,
    hasMore: data?.has_more ?? false,
  };
}

// Polling node status hook with Page Visibility API support
export function usePollingNodeStatus(pollInterval: number = 30000): UseApiState<NodeStatus> {
  const [data, setData] = useState<NodeStatus | null>(null);
  const [loading, setLoading] = useState(true);
  const [isInitialLoading, setIsInitialLoading] = useState(true);
  const [error, setError] = useState<ApiClientError | null>(null);
  const [isPaused, setIsPaused] = useState(false);
  const requestIdRef = useRef(0);

  const fetchData = useCallback(async () => {
    const requestId = ++requestIdRef.current;
    setLoading(true);
    setError(null);
    try {
      const result = await apiClient.get<NodeStatus>('/admin/v1/status');
      // Only update state if this is still the current request
      if (requestId === requestIdRef.current) {
        setData(result);
      }
    } catch (err) {
      // Only update state if this is still the current request
      if (requestId === requestIdRef.current) {
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
      }
    } finally {
      // Only update state if this is still the current request
      if (requestId === requestIdRef.current) {
        setLoading(false);
        setIsInitialLoading(false);
      }
    }
  }, []);

  useEffect(() => {
    // Handle Page Visibility API to pause polling when document is hidden
    const handleVisibilityChange = () => {
      setIsPaused(document.hidden);
    };

    document.addEventListener('visibilitychange', handleVisibilityChange);

    return () => {
      document.removeEventListener('visibilitychange', handleVisibilityChange);
    };
  }, []);

  useEffect(() => {
    // Initial fetch
    fetchData();

    // Set up polling interval
    const interval = setInterval(() => {
      if (!isPaused) {
        fetchData();
      }
    }, pollInterval);

    return () => clearInterval(interval);
  }, [fetchData, pollInterval, isPaused]);

  return { data, loading, isInitialLoading, error, refetch: fetchData };
}

// ============================================================================
// Message Hooks
// ============================================================================

// List inbox messages
export function useInboxMessages(
  params: { limit?: number; offset?: number } = {}
): UseApiState<PaginatedResponse<Message>> {
  const { limit = 50, offset = 0 } = params;
  const queryString = new URLSearchParams();
  if (limit) queryString.append('limit', String(limit));
  if (offset) queryString.append('offset', String(offset));
  const query = queryString.toString();

  return useApi(
    () =>
      apiClient.get<PaginatedResponse<Message>>(
        `/admin/v1/messages${query ? '?' + query : ''}`
      ),
    [limit, offset]
  );
}

// List sent messages
export function useSentMessages(
  params: { limit?: number; offset?: number } = {}
): UseApiState<PaginatedResponse<Message>> {
  const { limit = 50, offset = 0 } = params;
  const queryString = new URLSearchParams();
  if (limit) queryString.append('limit', String(limit));
  if (offset) queryString.append('offset', String(offset));
  const query = queryString.toString();

  return useApi(
    () =>
      apiClient.get<PaginatedResponse<Message>>(
        `/admin/v1/messages/sent${query ? '?' + query : ''}`
      ),
    [limit, offset]
  );
}

// Get single message
export function useMessage(messageId: string): UseApiState<Message> {
  return useApi(
    () => apiClient.get<Message>(`/admin/v1/messages/${messageId}`),
    [messageId]
  );
}

// Get message stats (for unread badge)
export function useMessageStats(): UseApiState<MessageStats> {
  return useApi(() => apiClient.get<MessageStats>('/admin/v1/messages/stats'));
}

// Message actions hook
export function useMessageActions() {
  const sendMessage = async (
    request: SendMessageRequest
  ): Promise<SendMessageResponse> => {
    try {
      return await apiClient.post<SendMessageResponse>(
        '/admin/v1/messages',
        request
      );
    } catch (err) {
      if (err instanceof ApiClientError) {
        throw err;
      }
      throw new ApiClientError(
        'SEND_MESSAGE_ERROR',
        err instanceof Error ? err.message : 'Failed to send message',
        0
      );
    }
  };

  const markAsRead = async (messageId: string): Promise<Message> => {
    try {
      return await apiClient.patch<Message>(`/admin/v1/messages/${messageId}`, {
        read: true,
      } as MessageUpdate);
    } catch (err) {
      if (err instanceof ApiClientError) {
        throw err;
      }
      throw new ApiClientError(
        'MARK_READ_ERROR',
        err instanceof Error ? err.message : 'Failed to mark message as read',
        0
      );
    }
  };

  const markAsUnread = async (messageId: string): Promise<Message> => {
    try {
      return await apiClient.patch<Message>(`/admin/v1/messages/${messageId}`, {
        read: false,
      } as MessageUpdate);
    } catch (err) {
      if (err instanceof ApiClientError) {
        throw err;
      }
      throw new ApiClientError(
        'MARK_UNREAD_ERROR',
        err instanceof Error ? err.message : 'Failed to mark message as unread',
        0
      );
    }
  };

  const deleteMessage = async (messageId: string): Promise<void> => {
    try {
      await apiClient.delete(`/admin/v1/messages/${messageId}`);
    } catch (err) {
      if (err instanceof ApiClientError) {
        throw err;
      }
      throw new ApiClientError(
        'DELETE_MESSAGE_ERROR',
        err instanceof Error ? err.message : 'Failed to delete message',
        0
      );
    }
  };

  const moveToFolder = async (
    messageId: string,
    folder: MessageFolder
  ): Promise<Message> => {
    try {
      return await apiClient.patch<Message>(`/admin/v1/messages/${messageId}`, {
        folder,
      } as MessageUpdate);
    } catch (err) {
      if (err instanceof ApiClientError) {
        throw err;
      }
      throw new ApiClientError(
        'MOVE_MESSAGE_ERROR',
        err instanceof Error ? err.message : 'Failed to move message',
        0
      );
    }
  };

  return {
    sendMessage,
    markAsRead,
    markAsUnread,
    deleteMessage,
    moveToFolder,
  };
}
