use serde::{Deserialize, Serialize};

/// Connection state for driver connections
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConnectionState {
    /// Not connected, no active connection attempt
    Disconnected,
    /// Currently attempting to establish connection
    Connecting,
    /// Successfully connected and operational
    Connected,
    /// Attempting to reconnect after a disconnection
    Reconnecting,
    /// Connection failed permanently (requires manual intervention)
    Failed,
}

impl ConnectionState {
    /// Check if state allows connection attempt
    pub fn can_connect(&self) -> bool {
        matches!(self, Self::Disconnected | Self::Failed)
    }

    /// Check if state allows reconnection attempt
    pub fn can_reconnect(&self) -> bool {
        matches!(self, Self::Disconnected | Self::Failed)
    }

    /// Check if currently connected
    pub fn is_connected(&self) -> bool {
        matches!(self, Self::Connected)
    }

    /// Check if in a transitional state
    pub fn is_transitioning(&self) -> bool {
        matches!(self, Self::Connecting | Self::Reconnecting)
    }

    /// Transition to connecting state
    pub fn to_connecting(&self) -> Result<Self, &'static str> {
        match self {
            Self::Disconnected | Self::Failed => Ok(Self::Connecting),
            _ => Err("Can only connect from Disconnected or Failed state"),
        }
    }

    /// Transition to connected state
    pub fn to_connected(&self) -> Result<Self, &'static str> {
        match self {
            Self::Connecting | Self::Reconnecting => Ok(Self::Connected),
            _ => Err("Can only complete connection from Connecting or Reconnecting state"),
        }
    }

    /// Transition to disconnected state
    pub fn to_disconnected(&self) -> Self {
        Self::Disconnected
    }

    /// Transition to reconnecting state
    pub fn to_reconnecting(&self) -> Result<Self, &'static str> {
        match self {
            Self::Connected | Self::Disconnected => Ok(Self::Reconnecting),
            _ => Err("Can only reconnect from Connected or Disconnected state"),
        }
    }

    /// Transition to failed state
    pub fn to_failed(&self) -> Self {
        Self::Failed
    }
}

impl Default for ConnectionState {
    fn default() -> Self {
        Self::Disconnected
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initial_state_is_disconnected() {
        let state = ConnectionState::default();
        assert_eq!(state, ConnectionState::Disconnected);
        assert!(state.can_connect());
        assert!(!state.is_connected());
    }

    #[test]
    fn test_transition_disconnected_to_connecting() {
        let state = ConnectionState::Disconnected;
        let next = state.to_connecting().unwrap();
        assert_eq!(next, ConnectionState::Connecting);
        assert!(next.is_transitioning());
    }

    #[test]
    fn test_transition_connecting_to_connected() {
        let state = ConnectionState::Connecting;
        let next = state.to_connected().unwrap();
        assert_eq!(next, ConnectionState::Connected);
        assert!(next.is_connected());
    }

    #[test]
    fn test_cannot_connect_from_connected() {
        let state = ConnectionState::Connected;
        let result = state.to_connecting();
        assert!(result.is_err());
    }

    #[test]
    fn test_reconnecting_from_connected() {
        let state = ConnectionState::Connected;
        let next = state.to_reconnecting().unwrap();
        assert_eq!(next, ConnectionState::Reconnecting);
        assert!(next.is_transitioning());
    }

    #[test]
    fn test_reconnecting_to_connected() {
        let state = ConnectionState::Reconnecting;
        let next = state.to_connected().unwrap();
        assert_eq!(next, ConnectionState::Connected);
    }

    #[test]
    fn test_to_disconnected_from_any_state() {
        assert_eq!(
            ConnectionState::Connected.to_disconnected(),
            ConnectionState::Disconnected
        );
        assert_eq!(
            ConnectionState::Connecting.to_disconnected(),
            ConnectionState::Disconnected
        );
        assert_eq!(
            ConnectionState::Failed.to_disconnected(),
            ConnectionState::Disconnected
        );
    }

    #[test]
    fn test_to_failed_from_any_state() {
        assert_eq!(
            ConnectionState::Connected.to_failed(),
            ConnectionState::Failed
        );
        assert_eq!(
            ConnectionState::Connecting.to_failed(),
            ConnectionState::Failed
        );
    }

    #[test]
    fn test_can_connect_only_from_valid_states() {
        assert!(ConnectionState::Disconnected.can_connect());
        assert!(ConnectionState::Failed.can_connect());
        assert!(!ConnectionState::Connected.can_connect());
        assert!(!ConnectionState::Connecting.can_connect());
        assert!(!ConnectionState::Reconnecting.can_connect());
    }
}
