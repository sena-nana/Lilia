use lilia_desktop_application::{
    DesktopTerminalScope, DesktopTerminalSessionId, DesktopTerminalSnapshot,
};

#[derive(Clone, Debug)]
pub enum TerminalViewMessage {
    InputChanged(DesktopTerminalSessionId, String),
    Submit(DesktopTerminalSessionId),
    Interrupt(DesktopTerminalSessionId),
    Eof(DesktopTerminalSessionId),
    CopyVisible(DesktopTerminalSessionId),
    Resize(DesktopTerminalSessionId, u16, u16),
    Scroll(DesktopTerminalSessionId, usize),
    Terminate(DesktopTerminalSessionId),
    Reveal(DesktopTerminalSessionId),
    NewSession(DesktopTerminalScope),
}

pub fn terminal_plain_text(snapshot: &DesktopTerminalSnapshot) -> String {
    terminal_rows_plain_text(&snapshot.screen)
}

fn terminal_rows_plain_text(rows: &[lilia_desktop_application::DesktopTerminalRow]) -> String {
    rows.iter()
        .map(|row| row.text.trim_end())
        .collect::<Vec<_>>()
        .join("\n")
        .trim_end_matches('\n')
        .to_owned()
}

#[cfg(test)]
mod tests {
    use lilia_desktop_application::DesktopTerminalRow;

    use super::terminal_rows_plain_text;

    #[test]
    fn copied_terminal_output_preserves_lines_and_omits_screen_padding() {
        let rows = vec![
            DesktopTerminalRow {
                text: "first   ".to_owned(),
                styles: Vec::new(),
            },
            DesktopTerminalRow {
                text: "second".to_owned(),
                styles: Vec::new(),
            },
            DesktopTerminalRow {
                text: "        ".to_owned(),
                styles: Vec::new(),
            },
        ];
        assert_eq!(terminal_rows_plain_text(&rows), "first\nsecond");
    }
}
