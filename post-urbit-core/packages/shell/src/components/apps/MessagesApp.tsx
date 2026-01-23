import React, { useState, useCallback, useEffect } from 'react';
import {
  useInboxMessages,
  useSentMessages,
  useMessageStats,
  useMessageActions,
} from '../../api/hooks';
import { ApiClientError } from '../../api/client';
import { useAlert } from '../../context/AlertContext';
import Button from '../system7/Button';
import TextInput from '../system7/TextInput';
import type { Message, MessageFolder } from '../../api/types';

// View modes for the right pane
type ViewMode = 'list' | 'read' | 'compose';

// Folder configuration
const FOLDERS: { id: MessageFolder; label: string; icon: string }[] = [
  { id: 'inbox', label: 'Inbox', icon: '[]' },
  { id: 'sent', label: 'Sent', icon: '=>' },
];

// ============================================================================
// FolderList Component
// ============================================================================
interface FolderListProps {
  selectedFolder: MessageFolder;
  onSelectFolder: (folder: MessageFolder) => void;
  unreadCount: number;
  onCompose: () => void;
}

const FolderList = ({
  selectedFolder,
  onSelectFolder,
  unreadCount,
  onCompose,
}: FolderListProps) => {
  return (
    <div
      style={{
        width: '120px',
        borderRight: '1px solid black',
        background: 'white',
        display: 'flex',
        flexDirection: 'column',
      }}
    >
      <div style={{ padding: '8px', borderBottom: '1px solid #ccc' }}>
        <Button onClick={onCompose}>
          New
        </Button>
      </div>
      <div style={{ flex: 1, overflow: 'auto' }}>
        {FOLDERS.map((folder) => (
          <div
            key={folder.id}
            onClick={() => onSelectFolder(folder.id)}
            style={{
              padding: '8px 12px',
              cursor: 'pointer',
              background: selectedFolder === folder.id ? 'black' : 'white',
              color: selectedFolder === folder.id ? 'white' : 'black',
              fontFamily: 'var(--font-chicago)',
              fontSize: '11px',
              display: 'flex',
              justifyContent: 'space-between',
              alignItems: 'center',
              borderBottom: '1px solid #ddd',
            }}
          >
            <span>
              {folder.icon} {folder.label}
            </span>
            {folder.id === 'inbox' && unreadCount > 0 && (
              <span
                style={{
                  background: selectedFolder === folder.id ? 'white' : 'black',
                  color: selectedFolder === folder.id ? 'black' : 'white',
                  padding: '1px 5px',
                  borderRadius: '8px',
                  fontSize: '9px',
                  fontWeight: 'bold',
                }}
              >
                {unreadCount}
              </span>
            )}
          </div>
        ))}
      </div>
    </div>
  );
};

// ============================================================================
// MessageList Component
// ============================================================================
interface MessageListProps {
  messages: Message[];
  loading: boolean;
  error: ApiClientError | null;
  selectedMessageId: string | null;
  onSelectMessage: (message: Message) => void;
  onRetry: () => void;
  folder: MessageFolder;
}

const MessageList = ({
  messages,
  loading,
  error,
  selectedMessageId,
  onSelectMessage,
  onRetry,
  folder,
}: MessageListProps) => {
  if (loading) {
    return (
      <div style={{ flex: 1, padding: '12px', color: '#666' }}>
        Loading messages...
      </div>
    );
  }

  if (error) {
    return (
      <div
        style={{
          flex: 1,
          padding: '12px',
          display: 'flex',
          flexDirection: 'column',
          alignItems: 'center',
          justifyContent: 'center',
          gap: '12px',
        }}
      >
        <div
          style={{
            color: '#c00',
            fontFamily: 'var(--font-geneva)',
            fontSize: '11px',
            textAlign: 'center',
          }}
        >
          {error.status === 404 ? (
            <>
              Messaging not available
              <br />
              <span style={{ fontSize: '10px', color: '#666' }}>
                The messaging API is not implemented on this node yet.
              </span>
            </>
          ) : (
            <>
              Failed to load messages
              <br />
              {error.message}
            </>
          )}
        </div>
        {error.status !== 404 && <Button onClick={onRetry}>Retry</Button>}
      </div>
    );
  }

  const formatDate = (dateStr: string) => {
    const date = new Date(dateStr);
    const now = new Date();
    const isToday = date.toDateString() === now.toDateString();
    if (isToday) {
      return date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
    }
    return date.toLocaleDateString([], { month: 'short', day: 'numeric' });
  };

  const truncateIid = (iid: string) => {
    if (iid.length > 12) {
      return iid.slice(0, 6) + '...' + iid.slice(-4);
    }
    return iid;
  };

  return (
    <div
      style={{
        flex: 1,
        minWidth: '220px',
        borderRight: '1px solid black',
        background: 'white',
        overflow: 'auto',
        display: 'flex',
        flexDirection: 'column',
      }}
    >
      {/* Header */}
      <div
        style={{
          display: 'grid',
          gridTemplateColumns: '1fr 70px',
          padding: '4px 8px',
          borderBottom: '1px solid black',
          background: '#f0f0f0',
          fontFamily: 'var(--font-chicago)',
          fontSize: '10px',
          fontWeight: 'bold',
        }}
      >
        <span>{folder === 'sent' ? 'To' : 'From'} / Subject</span>
        <span style={{ textAlign: 'right' }}>Date</span>
      </div>

      {/* Messages */}
      <div style={{ flex: 1, overflow: 'auto' }}>
        {messages.length === 0 ? (
          <div
            style={{ padding: '20px', color: '#666', textAlign: 'center', fontSize: '11px' }}
          >
            No messages
          </div>
        ) : (
          messages.map((message) => (
            <div
              key={message.id}
              onClick={() => onSelectMessage(message)}
              style={{
                display: 'grid',
                gridTemplateColumns: '1fr 70px',
                padding: '6px 8px',
                cursor: 'pointer',
                background:
                  selectedMessageId === message.id ? 'black' : 'white',
                color: selectedMessageId === message.id ? 'white' : 'black',
                borderBottom: '1px solid #ddd',
                fontFamily: 'var(--font-geneva)',
                fontSize: '11px',
              }}
            >
              <div style={{ overflow: 'hidden' }}>
                <div
                  style={{
                    fontWeight: message.read ? 'normal' : 'bold',
                    whiteSpace: 'nowrap',
                    overflow: 'hidden',
                    textOverflow: 'ellipsis',
                  }}
                >
                  {folder === 'sent'
                    ? truncateIid(message.recipient_iid)
                    : truncateIid(message.sender_iid)}
                </div>
                <div
                  style={{
                    fontSize: '10px',
                    color:
                      selectedMessageId === message.id ? '#ccc' : '#666',
                    whiteSpace: 'nowrap',
                    overflow: 'hidden',
                    textOverflow: 'ellipsis',
                    fontWeight: message.read ? 'normal' : 'bold',
                  }}
                >
                  {message.subject || '(No Subject)'}
                </div>
              </div>
              <div style={{ fontSize: '10px', textAlign: 'right' }}>
                {formatDate(message.sent_at)}
              </div>
            </div>
          ))
        )}
      </div>
    </div>
  );
};

// ============================================================================
// MessageView Component
// ============================================================================
interface MessageViewProps {
  message: Message;
  onDelete: () => void;
  onReply: () => void;
}

const MessageView = ({ message, onDelete, onReply }: MessageViewProps) => {
  const formatFullDate = (dateStr: string) => {
    return new Date(dateStr).toLocaleString();
  };

  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%' }}>
      {/* Header */}
      <div
        style={{
          padding: '8px 12px',
          borderBottom: '1px solid black',
          background: '#f0f0f0',
          fontFamily: 'var(--font-geneva)',
          fontSize: '11px',
        }}
      >
        <div style={{ marginBottom: '4px' }}>
          <strong>From:</strong>{' '}
          <span style={{ fontFamily: 'Monaco, monospace', fontSize: '10px' }}>
            {message.sender_iid}
          </span>
        </div>
        <div style={{ marginBottom: '4px' }}>
          <strong>To:</strong>{' '}
          <span style={{ fontFamily: 'Monaco, monospace', fontSize: '10px' }}>
            {message.recipient_iid}
          </span>
        </div>
        <div style={{ marginBottom: '4px' }}>
          <strong>Subject:</strong> {message.subject || '(No Subject)'}
        </div>
        <div style={{ fontSize: '10px', color: '#666' }}>
          {formatFullDate(message.sent_at)}
        </div>
      </div>

      {/* Body */}
      <div
        style={{
          flex: 1,
          padding: '12px',
          overflow: 'auto',
          whiteSpace: 'pre-wrap',
          fontFamily: 'var(--font-geneva)',
          fontSize: '12px',
          background: 'white',
          lineHeight: '1.4',
        }}
      >
        {message.body || '(No content)'}
      </div>

      {/* Actions */}
      <div
        style={{
          padding: '8px 12px',
          borderTop: '1px solid black',
          display: 'flex',
          gap: '8px',
          justifyContent: 'flex-end',
          background: '#f0f0f0',
        }}
      >
        <Button onClick={onDelete}>Delete</Button>
        <Button onClick={onReply}>Reply</Button>
      </div>
    </div>
  );
};

// ============================================================================
// ComposeMessage Component
// ============================================================================
interface ComposeMessageProps {
  onSend: (recipient: string, subject: string, body: string) => Promise<void>;
  onCancel: () => void;
  replyTo?: { recipient: string; subject: string };
}

const ComposeMessage = ({ onSend, onCancel, replyTo }: ComposeMessageProps) => {
  // Helper to normalize subject and prevent Re: Re: Re: stacking
  const normalizeSubject = (subject: string) => {
    const cleanSubject = subject.trim();
    if (cleanSubject.toLowerCase().startsWith('re:')) {
      return cleanSubject;
    }
    return `Re: ${cleanSubject}`;
  };

  const [recipient, setRecipient] = useState(replyTo?.recipient || '');
  const [subject, setSubject] = useState(
    replyTo?.subject ? normalizeSubject(replyTo.subject) : ''
  );
  const [body, setBody] = useState('');
  const [sending, setSending] = useState(false);

  // Sync state when replyTo changes
  useEffect(() => {
    if (replyTo) {
      setRecipient(replyTo.recipient);
      setSubject(replyTo.subject ? normalizeSubject(replyTo.subject) : '');
    }
  }, [replyTo]);

  const handleSend = async () => {
    if (!recipient.trim()) return;
    setSending(true);
    try {
      await onSend(recipient.trim(), subject, body);
    } finally {
      setSending(false);
    }
  };

  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%' }}>
      {/* Header */}
      <div
        style={{
          padding: '8px 12px',
          borderBottom: '1px solid black',
          background: '#f0f0f0',
        }}
      >
        <h3
          style={{
            margin: '0 0 8px 0',
            fontFamily: 'var(--font-chicago)',
            fontSize: '12px',
          }}
        >
          New Message
        </h3>
      </div>

      {/* Form Fields */}
      <div
        style={{
          padding: '12px',
          borderBottom: '1px solid #ccc',
          background: 'white',
        }}
      >
        <div style={{ marginBottom: '8px' }}>
          <label
            style={{
              display: 'block',
              marginBottom: '4px',
              fontSize: '11px',
              fontFamily: 'var(--font-chicago)',
            }}
          >
            To:
          </label>
          <TextInput
            value={recipient}
            onChange={(e) => setRecipient(e.target.value)}
            placeholder="Enter recipient IID"
            style={{ width: '100%', fontFamily: 'Monaco, monospace', fontSize: '10px' }}
          />
        </div>
        <div>
          <label
            style={{
              display: 'block',
              marginBottom: '4px',
              fontSize: '11px',
              fontFamily: 'var(--font-chicago)',
            }}
          >
            Subject:
          </label>
          <TextInput
            value={subject}
            onChange={(e) => setSubject(e.target.value)}
            placeholder="Enter subject"
            style={{ width: '100%' }}
          />
        </div>
      </div>

      {/* Body */}
      <div style={{ flex: 1, padding: '12px', background: 'white' }}>
        <textarea
          value={body}
          onChange={(e) => setBody(e.target.value)}
          placeholder="Type your message..."
          style={{
            width: '100%',
            height: '100%',
            resize: 'none',
            fontFamily: 'var(--font-geneva)',
            fontSize: '12px',
            padding: '8px',
            border: '2px inset #888',
            boxSizing: 'border-box',
            lineHeight: '1.4',
          }}
        />
      </div>

      {/* Actions */}
      <div
        style={{
          padding: '8px 12px',
          borderTop: '1px solid black',
          display: 'flex',
          gap: '8px',
          justifyContent: 'flex-end',
          background: '#f0f0f0',
        }}
      >
        <Button onClick={onCancel} disabled={sending}>
          Cancel
        </Button>
        <Button
          isDefault
          onClick={handleSend}
          disabled={sending || !recipient.trim()}
        >
          {sending ? 'Sending...' : 'Send'}
        </Button>
      </div>
    </div>
  );
};

// ============================================================================
// MessagesApp Main Component
// ============================================================================
const MessagesApp = () => {
  // State
  const [selectedFolder, setSelectedFolder] = useState<MessageFolder>('inbox');
  const [selectedMessageId, setSelectedMessageId] = useState<string | null>(
    null
  );
  const [viewMode, setViewMode] = useState<ViewMode>('list');
  const [previousViewMode, setPreviousViewMode] = useState<ViewMode>('list');
  const [replyTo, setReplyTo] = useState<{
    recipient: string;
    subject: string;
  } | null>(null);

  // Data hooks
  const {
    data: inboxData,
    loading: inboxLoading,
    error: inboxError,
    refetch: refetchInbox,
  } = useInboxMessages();
  const {
    data: sentData,
    loading: sentLoading,
    error: sentError,
    refetch: refetchSent,
  } = useSentMessages();
  const { data: stats, error: statsError, refetch: refetchStats } = useMessageStats();
  const { markAsRead, deleteMessage, sendMessage } = useMessageActions();
  const { showAlert } = useAlert();

  // Get messages for current folder
  const messages =
    selectedFolder === 'inbox'
      ? inboxData?.items || []
      : selectedFolder === 'sent'
      ? sentData?.items || []
      : [];

  const loading =
    selectedFolder === 'inbox'
      ? inboxLoading
      : selectedFolder === 'sent'
      ? sentLoading
      : false;

  const error =
    selectedFolder === 'inbox'
      ? inboxError
      : selectedFolder === 'sent'
      ? sentError
      : null;

  const selectedMessage = messages.find((m) => m.id === selectedMessageId);

  // Handlers
  const handleSelectFolder = useCallback((folder: MessageFolder) => {
    setSelectedFolder(folder);
    setSelectedMessageId(null);
    setViewMode('list');
  }, []);

  const handleSelectMessage = useCallback(
    async (message: Message) => {
      setSelectedMessageId(message.id);
      setPreviousViewMode(viewMode);
      setViewMode('read');
      setReplyTo(null);

      // Mark as read if it's an unread inbox message
      if (!message.read && selectedFolder === 'inbox') {
        try {
          await markAsRead(message.id);
          refetchInbox();
          refetchStats();
        } catch (err) {
          // Silently fail - message will remain unread
        }
      }
    },
    [selectedFolder, viewMode, markAsRead, refetchInbox, refetchStats]
  );

  const handleCompose = useCallback(() => {
    setSelectedMessageId(null);
    setPreviousViewMode(viewMode);
    setViewMode('compose');
    setReplyTo(null);
  }, [viewMode]);

  const handleReply = useCallback(() => {
    if (selectedMessage) {
      // CRITICAL FIX: For Sent folder, reply to recipient, not sender
      const replyRecipient = selectedFolder === 'sent'
        ? selectedMessage.recipient_iid
        : selectedMessage.sender_iid;

      setReplyTo({
        recipient: replyRecipient,
        subject: selectedMessage.subject,
      });
      setPreviousViewMode(viewMode);
      setViewMode('compose');
    }
  }, [selectedMessage, selectedFolder, viewMode]);

  const handleSend = useCallback(
    async (recipient: string, subject: string, body: string) => {
      try {
        await sendMessage({ recipient_iid: recipient, subject, body });
        showAlert('note', 'Message Sent', 'Your message has been sent.');
        setViewMode('list');
        setReplyTo(null);
        refetchSent();
        refetchInbox(); // In case sending to self
        refetchStats();
      } catch (err) {
        showAlert(
          'stop',
          'Error',
          err instanceof Error ? err.message : 'Failed to send message.'
        );
      }
    },
    [sendMessage, showAlert, refetchSent, refetchInbox, refetchStats]
  );

  const handleDelete = useCallback(async () => {
    if (!selectedMessage) return;

    // HIGH FIX: Add confirmation before deleting
    const confirmed = window.confirm(
      `Are you sure you want to delete this message from ${selectedMessage.sender_iid}?`
    );
    if (!confirmed) return;

    try {
      await deleteMessage(selectedMessage.id);
      showAlert('note', 'Deleted', 'Message has been deleted.');
      setSelectedMessageId(null);
      setViewMode('list');
      refetchInbox();
      refetchSent();
      refetchStats();
    } catch (err) {
      showAlert(
        'stop',
        'Error',
        err instanceof Error ? err.message : 'Failed to delete message.'
      );
    }
  }, [
    selectedMessage,
    deleteMessage,
    showAlert,
    refetchInbox,
    refetchSent,
    refetchStats,
  ]);

  const handleCancelCompose = useCallback(() => {
    // HIGH FIX: Return to previous view instead of always going to 'list'
    setViewMode(previousViewMode);
    setReplyTo(null);
  }, [previousViewMode]);

  const handleRetryFetch = useCallback(() => {
    if (selectedFolder === 'inbox') {
      refetchInbox();
    } else if (selectedFolder === 'sent') {
      refetchSent();
    }
    refetchStats();
  }, [selectedFolder, refetchInbox, refetchSent, refetchStats]);

  return (
    <div
      style={{
        display: 'flex',
        height: '100%',
        minWidth: '650px',
        minHeight: '400px',
        background: 'white',
      }}
    >
      {/* Left Pane - Folders */}
      <FolderList
        selectedFolder={selectedFolder}
        onSelectFolder={handleSelectFolder}
        unreadCount={stats?.unread_count || 0}
        onCompose={handleCompose}
      />

      {/* Middle Pane - Message List */}
      <MessageList
        messages={messages}
        loading={loading}
        error={error}
        selectedMessageId={selectedMessageId}
        onSelectMessage={handleSelectMessage}
        onRetry={handleRetryFetch}
        folder={selectedFolder}
      />

      {/* Right Pane - Message View or Compose */}
      <div
        style={{
          flex: 2,
          display: 'flex',
          flexDirection: 'column',
          borderLeft: '1px solid black',
          minWidth: '300px',
        }}
      >
        {viewMode === 'compose' && (
          <ComposeMessage
            onSend={handleSend}
            onCancel={handleCancelCompose}
            replyTo={replyTo || undefined}
          />
        )}
        {viewMode === 'read' && selectedMessage && (
          <MessageView
            message={selectedMessage}
            onDelete={handleDelete}
            onReply={handleReply}
          />
        )}
        {/* CRITICAL FIX: Show fallback when message is no longer available */}
        {viewMode === 'read' && !selectedMessage && (
          <div
            style={{
              flex: 1,
              display: 'flex',
              flexDirection: 'column',
              alignItems: 'center',
              justifyContent: 'center',
              color: '#666',
              fontFamily: 'var(--font-geneva)',
              fontSize: '12px',
              gap: '12px',
            }}
          >
            <div>Message no longer available</div>
            <Button onClick={() => setViewMode('list')}>Back to List</Button>
          </div>
        )}
        {viewMode === 'list' && (
          <div
            style={{
              flex: 1,
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'center',
              color: '#666',
              fontFamily: 'var(--font-geneva)',
              fontSize: '12px',
            }}
          >
            Select a message to read
          </div>
        )}
      </div>
    </div>
  );
};

export default MessagesApp;
