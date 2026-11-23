//! The files a package ships to the browser (07-themes-and-units.md,
//! "Shipped fonts").
//!
//! An asset is a [`FileKind::Asset`](crate::FileKind::Asset) file: a font
//! (`.woff2`, `.woff`) anywhere in the package. The analysis takes every
//! input as text, so a driver gives an asset's bytes as the text
//! [`text_of`] makes, one character per byte (U+0000–U+00FF), and code
//! generation reads them back with [`bytes_of`]. The encoding is lossless
//! and keeps an asset in the same channel as every other file: workspaces
//! mount it into the members that depend on its library, and a changed
//! file is a new revision like any other.
//!
//! ```
//! use k_sema::assets::{bytes_of, text_of};
//!
//! let bytes = [b'w', b'O', b'F', b'2', 0x00, 0xFF];
//! assert_eq!(bytes_of(&text_of(&bytes)).as_deref(), Some(&bytes[..]));
//! assert_eq!(bytes_of("€"), None);
//! ```

/// A font format a `FontFace` can ship.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FontFormat {
    /// WOFF 2.0, `font/woff2`.
    Woff2,
    /// WOFF 1.0, `font/woff`.
    Woff,
}

impl FontFormat {
    /// The format of a file named `name`, by its extension (any case).
    #[must_use]
    pub fn of(name: &str) -> Option<FontFormat> {
        let (_, ext) = name.rsplit_once('.')?;
        match ext.to_ascii_lowercase().as_str() {
            "woff2" => Some(FontFormat::Woff2),
            "woff" => Some(FontFormat::Woff),
            _ => None,
        }
    }

    /// The four bytes every file of the format starts with.
    #[must_use]
    pub const fn magic(self) -> &'static [u8; 4] {
        match self {
            FontFormat::Woff2 => b"wOF2",
            FontFormat::Woff => b"wOFF",
        }
    }

    /// The `format()` hint of an `@font-face` source.
    #[must_use]
    pub const fn css(self) -> &'static str {
        match self {
            FontFormat::Woff2 => "woff2",
            FontFormat::Woff => "woff",
        }
    }

    /// The file extension, without the dot.
    #[must_use]
    pub const fn extension(self) -> &'static str {
        self.css()
    }

    /// The media type, also the `type` of a preload link.
    #[must_use]
    pub const fn media_type(self) -> &'static str {
        match self {
            FontFormat::Woff2 => "font/woff2",
            FontFormat::Woff => "font/woff",
        }
    }

    /// The name of the format in a diagnostic.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            FontFormat::Woff2 => "WOFF2",
            FontFormat::Woff => "WOFF",
        }
    }
}

/// `true` for a file name that is an asset: a font Kobra ships.
#[must_use]
pub fn is_asset_name(name: &str) -> bool {
    FontFormat::of(name).is_some()
}

/// The text an asset's bytes are given as: one character per byte.
#[must_use]
pub fn text_of(bytes: &[u8]) -> String {
    bytes.iter().map(|&b| char::from(b)).collect()
}

/// The bytes of an asset given as `text` ([`text_of`]); `None` when `text`
/// holds a character above U+00FF, so it wasn't made by [`text_of`].
#[must_use]
pub fn bytes_of(text: &str) -> Option<Vec<u8>> {
    text.chars()
        .map(|c| u8::try_from(u32::from(c)).ok())
        .collect()
}

/// The text of a project file read from disk as `bytes`: an asset's in
/// the encoding of [`text_of`], any other file's as UTF-8.
///
/// # Errors
///
/// The UTF-8 error, for a file that isn't an asset and isn't UTF-8.
pub fn file_text(
    path: &crate::ProjectPath,
    bytes: Vec<u8>,
) -> Result<String, std::string::FromUtf8Error> {
    if path.kind() == crate::FileKind::Asset {
        Ok(text_of(&bytes))
    } else {
        String::from_utf8(bytes)
    }
}
