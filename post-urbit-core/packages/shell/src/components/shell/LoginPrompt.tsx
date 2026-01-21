import React, { useState } from 'react';
import Dialog from '../system7/Dialog';
import TextInput from '../system7/TextInput';
import Button from '../system7/Button';
import Alert from '../system7/Alert';
import type { ApiClientError } from '../../api/client';

interface LoginPromptProps {
  onLogin: (password: string) => Promise<{ success: boolean; error: ApiClientError | null }>;
}

const LoginPrompt: React.FC<LoginPromptProps> = ({ onLogin }) => {
  const [password, setPassword] = useState('');
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!password) {
      setError('Password is required');
      return;
    }

    setLoading(true);
    setError(null);

    const result = await onLogin(password);

    if (!result.success) {
      setError(result.error?.message || 'Login failed');
      setLoading(false);
    }
  };

  return (
    <div style={{
      position: 'fixed',
      top: 0,
      left: 0,
      right: 0,
      bottom: 0,
      background: 'rgba(0, 0, 0, 0.3)',
      display: 'flex',
      alignItems: 'center',
      justifyContent: 'center',
      zIndex: 1000,
    }}>
      <div style={{ width: '400px' }}>
        <Dialog title="Post-Urbit Login">
          <form onSubmit={handleSubmit}>
            <div style={{ padding: '20px' }}>
              <p style={{ marginBottom: '16px' }}>
                Enter your admin password to access the shell.
              </p>

              <div style={{ marginBottom: '16px' }}>
                <TextInput
                  type="password"
                  placeholder="Admin password"
                  value={password}
                  onChange={(e) => setPassword(e.target.value)}
                  disabled={loading}
                  autoFocus
                />
              </div>

              {error && (
                <div style={{ marginBottom: '16px' }}>
                  <Alert type="stop" title="Login Failed" message={error} />
                </div>
              )}

              <div style={{ display: 'flex', gap: '8px', justifyContent: 'flex-end' }}>
                <Button
                  label={loading ? 'Logging in...' : 'Login'}
                  variant="default"
                  disabled={loading}
                  onClick={handleSubmit}
                />
              </div>
            </div>
          </form>
        </Dialog>
      </div>
    </div>
  );
};

export default LoginPrompt;
