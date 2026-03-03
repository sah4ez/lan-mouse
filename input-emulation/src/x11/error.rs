use thiserror::Error;

/// X11 input emulation error
#[derive(Debug, Error, PartialEq)]
#[allow(dead_code)] // Will be used in future steps
pub enum X11EmulationError {
    #[error("failed to open X11 display: {display}")]
    DisplayConnection { display: String },

    #[error("emulation error: {operation}: {detail}")]
    EmulationFailed { operation: String, detail: String },

    #[error("invalid coordinates: {0}")]
    InvalidCoordinates(String),

    #[error("X11 protocol error: {0}")]
    ProtocolError(String),

    #[error("XRandR error: {0}")]
    XRandRError(String),

    #[error("XInput2 error: {0}")]
    XInput2Error(String),

    #[error("invalid scroll axis")]
    InvalidScrollAxis,

    #[error("timeout: {0}")]
    Timeout(String),

    #[error("display is invalid or closed")]
    InvalidDisplay,

    #[error("XTest extension is not available on this X server")]
    XTestNotAvailable,
}

/// Result type for X11 operations
pub type X11Result<T> = Result<T, X11EmulationError>;

/// Trait for adding context to errors
#[allow(dead_code)] // Will be used in future steps
pub trait ErrorContext<T> {
    fn with_context<F>(self, f: F) -> X11Result<T>
    where
        F: FnOnce() -> String;
}

impl<T> ErrorContext<T> for X11Result<T> {
    fn with_context<F>(self, f: F) -> X11Result<T>
    where
        F: FnOnce() -> String,
    {
        self.map_err(|e| match e {
            X11EmulationError::EmulationFailed { operation, detail } => {
                X11EmulationError::EmulationFailed {
                    operation,
                    detail: format!("{}: {}", f(), detail),
                }
            }
            _ => e,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_context_adds_context() {
        let result: X11Result<()> = Err(X11EmulationError::EmulationFailed {
            operation: "test_operation".to_string(),
            detail: "original detail".to_string(),
        });

        let result = result.with_context(|| "additional context".to_string());

        assert!(result.is_err());
        if let Err(X11EmulationError::EmulationFailed { detail, .. }) = result {
            assert!(detail.contains("additional context"));
            assert!(detail.contains("original detail"));
        } else {
            panic!("Expected EmulationFailed error");
        }
    }

    #[test]
    fn test_error_context_preserves_other_errors() {
        let result: X11Result<()> = Err(X11EmulationError::InvalidDisplay);

        let result = result.with_context(|| "additional context".to_string());

        assert!(matches!(result, Err(X11EmulationError::InvalidDisplay)));
    }

    #[test]
    fn test_error_context_preserves_ok() {
        let result: X11Result<i32> = Ok(42);

        let result = result.with_context(|| "additional context".to_string());

        assert_eq!(result, Ok(42));
    }
}
