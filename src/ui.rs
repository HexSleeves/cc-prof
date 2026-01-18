//! UI module for ccprof - centralized styling, color detection, tables, spinners.
//!
//! # No-color detection (in priority order):
//! 1. `--no-color` CLI flag (highest priority)
//! 2. `NO_COLOR` environment variable (any value)
//! 3. `TERM=dumb` environment variable
//! 4. Non-TTY stdout (detected via anstream)

use anstream::{eprintln, println};
use anstyle::{AnsiColor, Color, Style};
use comfy_table::{Cell, ContentArrangement, Table, presets};
use indicatif::{ProgressBar, ProgressStyle};
use std::io::IsTerminal;
use std::time::Duration;

/// Color mode for output
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorMode {
    /// Always emit ANSI colors
    Always,
    /// Emit colors only if TTY and not disabled
    #[default]
    Auto,
    /// Never emit ANSI colors
    Never,
}

impl std::str::FromStr for ColorMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "always" => Ok(Self::Always),
            "auto" => Ok(Self::Auto),
            "never" => Ok(Self::Never),
            _ => Err(format!("invalid color mode: {}", s)),
        }
    }
}

/// UI context holding resolved display settings
#[derive(Debug, Clone)]
pub struct Ui {
    /// Whether colors are enabled
    pub color_enabled: bool,
    /// Whether spinners are enabled (requires TTY + color)
    pub spinner_enabled: bool,
}

impl Default for Ui {
    fn default() -> Self {
        Self::new(ColorMode::Auto, false)
    }
}

impl Ui {
    /// Create a new UI context with color mode detection.
    ///
    /// Priority:
    /// 1. `force_no_color` (from --no-color flag)
    /// 2. `NO_COLOR` env var
    /// 3. `TERM=dumb`
    /// 4. TTY detection (for Auto mode)
    pub fn new(mode: ColorMode, force_no_color: bool) -> Self {
        let color_enabled = Self::resolve_color(mode, force_no_color);
        let is_tty = std::io::stdout().is_terminal();
        let spinner_enabled = color_enabled && is_tty;

        // Configure anstream's color choice globally
        if !color_enabled {
            anstream::ColorChoice::write_global(anstream::ColorChoice::Never);
        }

        Self {
            color_enabled,
            spinner_enabled,
        }
    }

    fn resolve_color(mode: ColorMode, force_no_color: bool) -> bool {
        // --no-color flag takes highest priority
        if force_no_color {
            return false;
        }

        // NO_COLOR env var (any value disables color per spec)
        if std::env::var("NO_COLOR").is_ok() {
            return false;
        }

        // TERM=dumb disables color
        if std::env::var("TERM").map(|t| t == "dumb").unwrap_or(false) {
            return false;
        }

        match mode {
            ColorMode::Always => true,
            ColorMode::Never => false,
            ColorMode::Auto => std::io::stdout().is_terminal(),
        }
    }

    // -------------------------------------------------------------------------
    // Styled label helpers
    // -------------------------------------------------------------------------

    fn style_label(&self, color: AnsiColor) -> Style {
        if self.color_enabled {
            Style::new().fg_color(Some(Color::Ansi(color))).bold()
        } else {
            Style::new()
        }
    }

    /// Print OK label (green) with message to stdout
    pub fn ok(&self, msg: impl AsRef<str>) {
        let label = self.style_label(AnsiColor::Green);
        println!("{label}OK{label:#} {}", msg.as_ref());
    }

    /// Print WARN label (yellow) with message to stdout
    pub fn warn(&self, msg: impl AsRef<str>) {
        let label = self.style_label(AnsiColor::Yellow);
        println!("{label}WARN{label:#} {}", msg.as_ref());
    }

    /// Print ERROR label (red) with message to stderr
    pub fn err(&self, msg: impl AsRef<str>) {
        let label = self.style_label(AnsiColor::Red);
        eprintln!("{label}ERROR{label:#} {}", msg.as_ref());
    }

    /// Print INFO label (cyan) with message to stdout
    pub fn info(&self, msg: impl AsRef<str>) {
        let label = self.style_label(AnsiColor::Cyan);
        println!("{label}INFO{label:#} {}", msg.as_ref());
    }

    /// Return a styled string (dimmed/gray) - for inline use
    pub fn dim(&self, s: impl AsRef<str>) -> String {
        if self.color_enabled {
            let st = Style::new().fg_color(Some(Color::Ansi(AnsiColor::BrightBlack)));
            format!("{st}{}{st:#}", s.as_ref())
        } else {
            s.as_ref().to_string()
        }
    }

    /// Return a styled string (bold) - for inline use
    pub fn bold(&self, s: impl AsRef<str>) -> String {
        if self.color_enabled {
            let st = Style::new().bold();
            format!("{st}{}{st:#}", s.as_ref())
        } else {
            s.as_ref().to_string()
        }
    }

    /// Return a styled string with specific color - for inline use
    pub fn colored(&self, s: impl AsRef<str>, color: AnsiColor) -> String {
        if self.color_enabled {
            let st = Style::new().fg_color(Some(Color::Ansi(color)));
            format!("{st}{}{st:#}", s.as_ref())
        } else {
            s.as_ref().to_string()
        }
    }

    // -------------------------------------------------------------------------
    // Status icons (with fallback for no-color)
    // -------------------------------------------------------------------------

    pub fn icon_ok(&self) -> &'static str {
        if self.color_enabled { "✓" } else { "[OK]" }
    }

    pub fn icon_warn(&self) -> &'static str {
        if self.color_enabled { "⚠" } else { "[!]" }
    }

    pub fn icon_err(&self) -> &'static str {
        if self.color_enabled { "✗" } else { "[X]" }
    }

    pub fn icon_info(&self) -> &'static str {
        if self.color_enabled { "•" } else { "-" }
    }

    // -------------------------------------------------------------------------
    // Tables (comfy-table)
    // -------------------------------------------------------------------------

    /// Create a new table with sensible defaults
    pub fn table(&self) -> Table {
        let mut table = Table::new();
        table.set_content_arrangement(ContentArrangement::Dynamic);

        if self.color_enabled {
            table.load_preset(presets::UTF8_FULL_CONDENSED);
        } else {
            table.load_preset(presets::ASCII_MARKDOWN);
        }

        table
    }

    /// Create a simple table without borders (for lists)
    pub fn simple_table(&self) -> Table {
        let mut table = Table::new();
        table.set_content_arrangement(ContentArrangement::Dynamic);
        table.load_preset(presets::NOTHING);
        table
    }

    /// Create a styled cell
    pub fn cell(&self, content: impl Into<String>) -> Cell {
        Cell::new(content.into())
    }

    /// Create a styled header cell (bold when color enabled)
    pub fn header_cell(&self, content: impl Into<String>) -> Cell {
        let cell = Cell::new(content.into());
        if self.color_enabled {
            cell.add_attribute(comfy_table::Attribute::Bold)
        } else {
            cell
        }
    }

    /// Create a colored cell using comfy-table's native styling
    /// This avoids ANSI width calculation issues
    pub fn colored_cell(&self, content: impl Into<String>, color: AnsiColor) -> Cell {
        let cell = Cell::new(content.into());
        if self.color_enabled {
            cell.fg(ansi_to_comfy_color(color))
        } else {
            cell
        }
    }

    /// Create a cell with an icon prefix (properly styled)
    pub fn status_cell(&self, icon: &str, content: impl Into<String>) -> Cell {
        Cell::new(format!("{} {}", icon, content.into()))
    }

    // -------------------------------------------------------------------------
    // Spinners (indicatif)
    // -------------------------------------------------------------------------

    /// Create a spinner for longer operations.
    /// Returns a no-op spinner when disabled.
    pub fn spinner(&self, message: impl Into<std::borrow::Cow<'static, str>>) -> ProgressBar {
        if self.spinner_enabled {
            let pb = ProgressBar::new_spinner();
            pb.set_style(
                ProgressStyle::default_spinner()
                    .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏")
                    .template("{spinner:.cyan} {msg}")
                    .expect("valid template"),
            );
            pb.set_message(message);
            pb.enable_steady_tick(Duration::from_millis(80));
            pb
        } else {
            // Hidden/no-op progress bar
            let pb = ProgressBar::hidden();
            pb.set_message(message);
            pb
        }
    }

    /// Finish a spinner with a success message
    pub fn spinner_finish_ok(
        &self,
        pb: &ProgressBar,
        msg: impl Into<std::borrow::Cow<'static, str>>,
    ) {
        if self.spinner_enabled {
            pb.set_style(
                ProgressStyle::default_spinner()
                    .template("{msg}")
                    .expect("valid template"),
            );
            let icon = self.colored("✓", AnsiColor::Green);
            pb.finish_with_message(format!("{} {}", icon, msg.into()));
        } else {
            pb.finish_and_clear();
            self.ok(msg.into());
        }
    }

    /// Finish a spinner with an error message
    pub fn spinner_finish_err(
        &self,
        pb: &ProgressBar,
        msg: impl Into<std::borrow::Cow<'static, str>>,
    ) {
        if self.spinner_enabled {
            pb.set_style(
                ProgressStyle::default_spinner()
                    .template("{msg}")
                    .expect("valid template"),
            );
            let icon = self.colored("✗", AnsiColor::Red);
            pb.finish_with_message(format!("{} {}", icon, msg.into()));
        } else {
            pb.finish_and_clear();
            self.err(msg.into());
        }
    }

    // -------------------------------------------------------------------------
    // Println helpers (using anstream for proper tty handling)
    // -------------------------------------------------------------------------

    /// Print a line to stdout
    pub fn println(&self, msg: impl AsRef<str>) {
        println!("{}", msg.as_ref());
    }

    /// Print an empty line
    pub fn newline(&self) {
        println!();
    }

    /// Print a section header
    pub fn section(&self, title: impl AsRef<str>) {
        println!("{}", self.bold(title));
    }
}

// -----------------------------------------------------------------------------
// Helper: convert anstyle::AnsiColor to comfy_table::Color
// -----------------------------------------------------------------------------

fn ansi_to_comfy_color(color: AnsiColor) -> comfy_table::Color {
    match color {
        AnsiColor::Black => comfy_table::Color::Black,
        AnsiColor::Red => comfy_table::Color::Red,
        AnsiColor::Green => comfy_table::Color::Green,
        AnsiColor::Yellow => comfy_table::Color::Yellow,
        AnsiColor::Blue => comfy_table::Color::Blue,
        AnsiColor::Magenta => comfy_table::Color::Magenta,
        AnsiColor::Cyan => comfy_table::Color::Cyan,
        AnsiColor::White => comfy_table::Color::White,
        AnsiColor::BrightBlack => comfy_table::Color::DarkGrey,
        AnsiColor::BrightRed => comfy_table::Color::Red,
        AnsiColor::BrightGreen => comfy_table::Color::Green,
        AnsiColor::BrightYellow => comfy_table::Color::Yellow,
        AnsiColor::BrightBlue => comfy_table::Color::Blue,
        AnsiColor::BrightMagenta => comfy_table::Color::Magenta,
        AnsiColor::BrightCyan => comfy_table::Color::Cyan,
        AnsiColor::BrightWhite => comfy_table::Color::White,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_color_mode_parse() {
        assert_eq!("always".parse::<ColorMode>().unwrap(), ColorMode::Always);
        assert_eq!("auto".parse::<ColorMode>().unwrap(), ColorMode::Auto);
        assert_eq!("never".parse::<ColorMode>().unwrap(), ColorMode::Never);
        assert!("invalid".parse::<ColorMode>().is_err());
    }

    #[test]
    fn test_ui_force_no_color() {
        let ui = Ui::new(ColorMode::Always, true);
        assert!(!ui.color_enabled);
    }

    #[test]
    fn test_ui_never_mode() {
        let ui = Ui::new(ColorMode::Never, false);
        assert!(!ui.color_enabled);
    }

    #[test]
    fn test_icons_no_color() {
        let ui = Ui::new(ColorMode::Never, false);
        assert_eq!(ui.icon_ok(), "[OK]");
        assert_eq!(ui.icon_err(), "[X]");
        assert_eq!(ui.icon_warn(), "[!]");
    }

    #[test]
    fn test_dim_no_color() {
        let ui = Ui::new(ColorMode::Never, false);
        assert_eq!(ui.dim("test"), "test");
    }

    #[test]
    fn test_table_creation() {
        let ui = Ui::new(ColorMode::Never, false);
        let table = ui.table();
        // Just verify it doesn't panic
        drop(table);
    }

    #[test]
    fn test_spinner_disabled() {
        let ui = Ui::new(ColorMode::Never, false);
        assert!(!ui.spinner_enabled);
        let pb = ui.spinner("test");
        pb.finish();
    }
}
