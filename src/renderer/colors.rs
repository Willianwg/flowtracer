use crossterm::style::{Attribute, Color, SetAttribute, SetForegroundColor};
use std::fmt;
use std::io::Write;

/// Controls whether ANSI color/style escape sequences are emitted.
#[derive(Debug, Clone)]
pub struct ColorConfig {
    pub enabled: bool,
}

#[allow(dead_code)]
impl ColorConfig {
    pub fn new(enabled: bool) -> Self {
        Self { enabled }
    }

    pub fn write_error<W: Write + ?Sized>(&self, w: &mut W, text: &str) -> fmt::Result {
        self.write_styled(w, text, Color::Red, true)
    }

    pub fn write_success<W: Write + ?Sized>(&self, w: &mut W, text: &str) -> fmt::Result {
        self.write_styled(w, text, Color::Green, false)
    }

    pub fn write_dim<W: Write + ?Sized>(&self, w: &mut W, text: &str) -> fmt::Result {
        self.write_styled(w, text, Color::DarkGrey, false)
    }

    pub fn write_warning<W: Write + ?Sized>(&self, w: &mut W, text: &str) -> fmt::Result {
        self.write_styled(w, text, Color::Yellow, false)
    }

    pub fn write_header<W: Write + ?Sized>(&self, w: &mut W, text: &str) -> fmt::Result {
        if self.enabled {
            write!(
                w,
                "{}{}{}{}{}",
                SetAttribute(Attribute::Bold),
                SetAttribute(Attribute::Underlined),
                text,
                SetAttribute(Attribute::NoUnderline),
                SetAttribute(Attribute::Reset),
            )
            .map_err(|_| fmt::Error)
        } else {
            write!(w, "{}", text).map_err(|_| fmt::Error)
        }
    }

    pub fn write_bold<W: Write + ?Sized>(&self, w: &mut W, text: &str) -> fmt::Result {
        if self.enabled {
            write!(
                w,
                "{}{}{}",
                SetAttribute(Attribute::Bold),
                text,
                SetAttribute(Attribute::Reset),
            )
            .map_err(|_| fmt::Error)
        } else {
            write!(w, "{}", text).map_err(|_| fmt::Error)
        }
    }

    pub fn write_duration<W: Write + ?Sized>(&self, w: &mut W, text: &str) -> fmt::Result {
        self.write_styled(w, text, Color::DarkGrey, false)
    }

    pub fn write_error_icon<W: Write + ?Sized>(&self, w: &mut W) -> fmt::Result {
        if self.enabled {
            write!(
                w,
                "{}{}❌{}",
                SetForegroundColor(Color::Red),
                SetAttribute(Attribute::Bold),
                SetAttribute(Attribute::Reset),
            )
            .map_err(|_| fmt::Error)
        } else {
            write!(w, "❌").map_err(|_| fmt::Error)
        }
    }

    pub fn write_success_icon<W: Write + ?Sized>(&self, w: &mut W) -> fmt::Result {
        if self.enabled {
            write!(
                w,
                "{}✅{}",
                SetForegroundColor(Color::Green),
                SetAttribute(Attribute::Reset),
            )
            .map_err(|_| fmt::Error)
        } else {
            write!(w, "✅").map_err(|_| fmt::Error)
        }
    }

    pub fn write_lightning<W: Write + ?Sized>(&self, w: &mut W, text: &str) -> fmt::Result {
        if self.enabled {
            write!(
                w,
                "{}{}⚡ {}{}",
                SetForegroundColor(Color::Red),
                SetAttribute(Attribute::Bold),
                text,
                SetAttribute(Attribute::Reset),
            )
            .map_err(|_| fmt::Error)
        } else {
            write!(w, "⚡ {}", text).map_err(|_| fmt::Error)
        }
    }

    fn write_styled<W: Write + ?Sized>(
        &self,
        w: &mut W,
        text: &str,
        color: Color,
        bold: bool,
    ) -> fmt::Result {
        if self.enabled {
            if bold {
                write!(
                    w,
                    "{}{}{}{}",
                    SetForegroundColor(color),
                    SetAttribute(Attribute::Bold),
                    text,
                    SetAttribute(Attribute::Reset),
                )
                .map_err(|_| fmt::Error)
            } else {
                write!(
                    w,
                    "{}{}{}",
                    SetForegroundColor(color),
                    text,
                    SetAttribute(Attribute::Reset),
                )
                .map_err(|_| fmt::Error)
            }
        } else {
            write!(w, "{}", text).map_err(|_| fmt::Error)
        }
    }
}

impl Default for ColorConfig {
    fn default() -> Self {
        Self { enabled: true }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_config_disabled_produces_plain_text() {
        let config = ColorConfig::new(false);
        let mut buf = Vec::new();

        config.write_error(&mut buf, "fail").unwrap();
        assert_eq!(String::from_utf8(buf).unwrap(), "fail");
    }

    #[test]
    fn color_config_enabled_produces_escape_sequences() {
        let config = ColorConfig::new(true);
        let mut buf = Vec::new();

        config.write_error(&mut buf, "fail").unwrap();
        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("fail"));
        assert!(output.contains("\x1b[")); // ANSI escape
    }

    #[test]
    fn default_has_colors_enabled() {
        let config = ColorConfig::default();
        assert!(config.enabled);
    }

    #[test]
    fn dim_style_no_color() {
        let config = ColorConfig::new(false);
        let mut buf = Vec::new();
        config.write_dim(&mut buf, "meta").unwrap();
        assert_eq!(String::from_utf8(buf).unwrap(), "meta");
    }

    #[test]
    fn bold_style_no_color() {
        let config = ColorConfig::new(false);
        let mut buf = Vec::new();
        config.write_bold(&mut buf, "title").unwrap();
        assert_eq!(String::from_utf8(buf).unwrap(), "title");
    }
}
