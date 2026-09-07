//! Text helpers shared by every screen.

use unicode_width::UnicodeWidthStr;

use crate::domain::storage::{ByteSize, SizeBase};

pub const SPINNER_FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

pub fn spinner(tick: usize) -> &'static str {
    SPINNER_FRAMES[tick % SPINNER_FRAMES.len()]
}

pub fn size(value: ByteSize, base: SizeBase) -> String {
    value.format(base)
}

/// `1234567` becomes `1,234,567`.
pub fn count(value: u64) -> String {
    let digits = value.to_string();
    let mut result = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, character) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index) % 3 == 0 {
            result.push(',');
        }
        result.push(character);
    }
    result
}

/// Short form such as `1.2M` for tight columns.
pub fn compact_count(value: u64) -> String {
    match value {
        0..=9_999 => value.to_string(),
        10_000..=999_999 => format!("{:.0}K", value as f64 / 1_000.0),
        1_000_000..=999_999_999 => format!("{:.1}M", value as f64 / 1_000_000.0),
        _ => format!("{:.1}B", value as f64 / 1_000_000_000.0),
    }
}

pub fn percent(ratio: f64) -> String {
    let value = (ratio * 100.0).clamp(0.0, 100.0);
    if value >= 10.0 {
        format!("{value:.0}%")
    } else if value >= 0.1 {
        format!("{value:.1}%")
    } else if value > 0.0 {
        "<0.1%".to_owned()
    } else {
        "0%".to_owned()
    }
}

/// A horizontal bar of `width` cells filled proportionally to `ratio`, with eighth blocks
/// for the partial cell.
pub fn bar(ratio: f64, width: usize) -> String {
    const PARTIALS: [&str; 8] = ["", "▏", "▎", "▍", "▌", "▋", "▊", "▉"];
    if width == 0 {
        return String::new();
    }
    let ratio = ratio.clamp(0.0, 1.0);
    let filled_eighths = (ratio * width as f64 * 8.0).round() as usize;
    let full = filled_eighths / 8;
    let partial = filled_eighths % 8;
    let mut result = String::with_capacity(width * 3);
    for _ in 0..full.min(width) {
        result.push('█');
    }
    if full < width && partial > 0 {
        result.push_str(PARTIALS[partial]);
    }
    let used = full.min(width) + usize::from(full < width && partial > 0);
    for _ in used..width {
        result.push(' ');
    }
    result
}

/// Cuts `text` so it fits `width` cells, keeping the end when `keep_end` is set (useful
/// for paths) and marking the cut with an ellipsis.
pub fn fit(text: &str, width: usize, keep_end: bool) -> String {
    if width == 0 {
        return String::new();
    }
    if UnicodeWidthStr::width(text) <= width {
        return text.to_owned();
    }
    if width == 1 {
        return "…".to_owned();
    }
    let budget = width - 1;
    if keep_end {
        let mut collected: Vec<char> = Vec::new();
        let mut used = 0;
        for character in text.chars().rev() {
            let character_width = unicode_width::UnicodeWidthChar::width(character).unwrap_or(0);
            if used + character_width > budget {
                break;
            }
            used += character_width;
            collected.push(character);
        }
        let mut result = String::from("…");
        result.extend(collected.into_iter().rev());
        result
    } else {
        let mut result = String::new();
        let mut used = 0;
        for character in text.chars() {
            let character_width = unicode_width::UnicodeWidthChar::width(character).unwrap_or(0);
            if used + character_width > budget {
                break;
            }
            used += character_width;
            result.push(character);
        }
        result.push('…');
        result
    }
}

pub fn duration(seconds: f64) -> String {
    if seconds < 1.0 {
        format!("{:.0} ms", seconds * 1000.0)
    } else if seconds < 60.0 {
        format!("{seconds:.1} s")
    } else {
        let minutes = (seconds / 60.0).floor();
        format!("{minutes:.0} min {:.0} s", seconds - minutes * 60.0)
    }
}

/// Shortens a path with `~` for the home directory.
pub fn home_relative(path: &std::path::Path, home: Option<&std::path::Path>) -> String {
    if let Some(home) = home {
        if path == home {
            return "~".to_owned();
        }
        if let Ok(relative) = path.strip_prefix(home) {
            return format!("~/{}", relative.display());
        }
    }
    path.display().to_string()
}
