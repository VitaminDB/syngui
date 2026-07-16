use std::fmt;

#[derive(Debug)]
pub enum MguiError {
    RenderError(String),
    LayoutError(String),
    GpuError(String),
    IoError(std::io::Error),
}

impl fmt::Display for MguiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MguiError::RenderError(msg) => write!(f, "Render error: {}", msg),
            MguiError::LayoutError(msg) => write!(f, "Layout error: {}", msg),
            MguiError::GpuError(msg) => write!(f, "GPU error: {}", msg),
            MguiError::IoError(e) => write!(f, "IO error: {}", e),
        }
    }
}

impl std::error::Error for MguiError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            MguiError::IoError(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for MguiError {
    fn from(e: std::io::Error) -> Self {
        MguiError::IoError(e)
    }
}

pub type Result<T> = std::result::Result<T, MguiError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_error_display() {
        let e = MguiError::RenderError("shader failed".into());
        assert_eq!(format!("{}", e), "Render error: shader failed");
    }

    #[test]
    fn layout_error_display() {
        let e = MguiError::LayoutError("overflow".into());
        assert_eq!(format!("{}", e), "Layout error: overflow");
    }

    #[test]
    fn gpu_error_display() {
        let e = MguiError::GpuError("out of memory".into());
        assert_eq!(format!("{}", e), "GPU error: out of memory");
    }

    #[test]
    fn io_error_display() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let e = MguiError::IoError(io_err);
        assert!(format!("{}", e).contains("IO error"));
        assert!(format!("{}", e).contains("file not found"));
    }

    #[test]
    fn io_error_source() {
        let io_err = std::io::Error::new(std::io::ErrorKind::Other, "disk full");
        let e = MguiError::IoError(io_err);
        assert!(std::error::Error::source(&e).is_some());
    }

    #[test]
    fn non_io_error_source_is_none() {
        let e = MguiError::RenderError("test".into());
        assert!(std::error::Error::source(&e).is_none());

        let e = MguiError::LayoutError("test".into());
        assert!(std::error::Error::source(&e).is_none());

        let e = MguiError::GpuError("test".into());
        assert!(std::error::Error::source(&e).is_none());
    }

    #[test]
    fn from_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "access denied");
        let e: MguiError = io_err.into();
        match e {
            MguiError::IoError(ref inner) => {
                assert_eq!(inner.kind(), std::io::ErrorKind::PermissionDenied);
            }
            _ => panic!("expected IoError"),
        }
    }

    #[test]
    fn result_type_alias() {
        let ok: Result<i32> = Ok(42);
        assert_eq!(ok.unwrap(), 42);

        let err: Result<i32> = Err(MguiError::RenderError("fail".into()));
        assert!(err.is_err());
    }

    #[test]
    fn error_is_debug() {
        let e = MguiError::RenderError("test".into());
        let debug = format!("{:?}", e);
        assert!(debug.contains("RenderError"));
    }
}
