//! Owns the terminal for the lifetime of the interface, and hands it back temporarily when
//! another program (sudo) needs it.

use std::io;

use ratatui::DefaultTerminal;

pub struct TerminalSession {
    terminal: DefaultTerminal,
}

impl TerminalSession {
    pub fn open() -> io::Result<Self> {
        install_panic_hook();
        let terminal = ratatui::try_init()?;
        Ok(Self { terminal })
    }

    pub fn terminal(&mut self) -> &mut DefaultTerminal {
        &mut self.terminal
    }

    /// Leaves the alternate screen and raw mode, runs `action`, then takes the terminal
    /// back and forces a full redraw.
    pub fn suspend<R>(&mut self, action: impl FnOnce() -> R) -> io::Result<R> {
        ratatui::restore();
        let result = action();
        println!();
        println!("  Press Enter to return to tree-cleaner…");
        let mut line = String::new();
        let _ = io::stdin().read_line(&mut line);
        self.terminal = ratatui::try_init()?;
        self.terminal.clear()?;
        Ok(result)
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        ratatui::restore();
    }
}

/// Makes sure a panic never leaves the user with a broken terminal.
fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        ratatui::restore();
        previous(info);
    }));
}
