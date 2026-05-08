//! TIDAL cover art helpers.

use riptide_core::CoverArtId;
use url::Url;

use crate::{TidalError, TidalResult};

/// Requested square cover art size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CoverArtSize(u16);

impl CoverArtSize {
    /// Creates a cover size capped at TIDAL's 1280px maximum.
    pub const fn new(size: u16) -> Self {
        Self(if size > 1280 { 1280 } else { size })
    }

    /// Returns the size in pixels.
    pub const fn get(self) -> u16 {
        self.0
    }
}

impl Default for CoverArtSize {
    fn default() -> Self {
        Self(1280)
    }
}

/// Constructs the TIDAL resources URL for a cover art ID.
pub fn cover_url(id: &CoverArtId, size: CoverArtSize) -> TidalResult<Url> {
    let formatted_id = id.as_str().replace('-', "/");
    Url::parse(&format!(
        "https://resources.tidal.com/images/{formatted_id}/{size}x{size}.jpg",
        size = size.get()
    ))
    .map_err(|error| TidalError::InvalidValue {
        field: "cover_url",
        message: error.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cover_url_matches_python_format() -> Result<(), Box<dyn std::error::Error>> {
        let id = CoverArtId::new("abcdef12-3456-7890-abcd-ef1234567890")?;
        let url = cover_url(&id, CoverArtSize::new(300))?;
        assert_eq!(
            url.as_str(),
            "https://resources.tidal.com/images/abcdef12/3456/7890/abcd/ef1234567890/300x300.jpg"
        );
        Ok(())
    }

    #[test]
    fn cover_size_caps_at_tidal_max() {
        assert_eq!(CoverArtSize::new(2000).get(), 1280);
    }
}
