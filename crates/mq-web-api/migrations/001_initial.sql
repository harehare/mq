-- Create rate_limits table for rate limiting functionality
CREATE TABLE rate_limits (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    identifier TEXT NOT NULL,
    window_start INTEGER NOT NULL,
    request_count INTEGER NOT NULL DEFAULT 1,
    created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
    expires_at INTEGER NOT NULL
);

-- Create indexes for better performance
CREATE INDEX idx_rate_limits_identifier_window
ON rate_limits(identifier, window_start);

CREATE INDEX idx_rate_limits_expires_at
ON rate_limits(expires_at);